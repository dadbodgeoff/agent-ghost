//! ghost policy — corporate policy management (T-3.4.1–T-3.4.3).

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use uuid::Uuid;

use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

/// Resolve the CORP_POLICY.md path: `~/.ghost/config/CORP_POLICY.md`.
fn policy_path() -> PathBuf {
    crate::bootstrap::ghost_home()
        .join("config")
        .join("CORP_POLICY.md")
}

// ─── ghost policy show (T-3.4.1) ────────────────────────────────────────────

pub struct PolicyShowArgs {
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct PolicyShowResult {
    path: String,
    denied_tools: Vec<String>,
    raw_content: Option<String>,
}

impl TableDisplay for PolicyShowResult {
    fn print_table(&self) {
        println!("CORP_POLICY.md ({})", self.path);
        println!();
        if self.denied_tools.is_empty() {
            if let Some(content) = &self.raw_content {
                println!("{content}");
            } else {
                println!("  No denied tools defined.");
            }
        } else {
            println!("Denied tools ({}):", self.denied_tools.len());
            for tool in &self.denied_tools {
                println!("  ✗ {tool}");
            }
        }
    }
}

pub fn run_show(args: PolicyShowArgs) -> Result<(), CliError> {
    let path = policy_path();

    if !path.exists() {
        eprintln!("CORP_POLICY.md not found at {}", path.display());
        eprintln!("Create one with a `## Denied Tools` section listing tools to block.");
        return Err(CliError::Config("CORP_POLICY.md not found".into()));
    }

    // Try structured parsing first.
    match ghost_policy::corp_policy::CorpPolicy::load(&path) {
        Ok(policy) => {
            let mut denied: Vec<String> = policy.denied_tools().iter().cloned().collect();
            denied.sort();

            let result = PolicyShowResult {
                path: path.display().to_string(),
                denied_tools: denied,
                raw_content: None,
            };
            print_output(&result, args.output);
        }
        Err(_) => {
            // Graceful fallback: show raw content.
            let content = std::fs::read_to_string(&path)
                .map_err(|e| CliError::Config(format!("failed to read CORP_POLICY.md: {e}")))?;
            let result = PolicyShowResult {
                path: path.display().to_string(),
                denied_tools: vec![],
                raw_content: Some(content),
            };
            print_output(&result, args.output);
        }
    }

    Ok(())
}

// ─── ghost policy check (T-3.4.2) ───────────────────────────────────────────

pub struct PolicyCheckArgs {
    pub tool_name: String,
    pub agent_id: Option<String>,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct PolicyCheckResult {
    tool_name: String,
    decision: String,
    reason: Option<String>,
    constraint: Option<String>,
    alternatives: Vec<String>,
}

impl TableDisplay for PolicyCheckResult {
    fn print_table(&self) {
        match self.decision.as_str() {
            "permit" => println!("✓ Tool '{}' is PERMITTED.", self.tool_name),
            "deny" => {
                println!("✗ Tool '{}' is DENIED.", self.tool_name);
                if let Some(reason) = &self.reason {
                    println!("  Reason: {reason}");
                }
                if let Some(constraint) = &self.constraint {
                    println!("  Constraint: {constraint}");
                }
                if !self.alternatives.is_empty() {
                    println!("  Alternatives:");
                    for alt in &self.alternatives {
                        println!("    - {alt}");
                    }
                }
            }
            "escalate" => {
                println!("⚠ Tool '{}' requires ESCALATION.", self.tool_name);
                if let Some(reason) = &self.reason {
                    println!("  Reason: {reason}");
                }
            }
            _ => println!("? Tool '{}': {}", self.tool_name, self.decision),
        }
    }
}

pub fn run_check(args: PolicyCheckArgs) -> Result<(), CliError> {
    let path = policy_path();

    // Load policy (empty policy if file doesn't exist — permits everything).
    let corp_policy = if path.exists() {
        ghost_policy::corp_policy::CorpPolicy::load(&path)
            .unwrap_or_else(|_| ghost_policy::corp_policy::CorpPolicy::new())
    } else {
        ghost_policy::corp_policy::CorpPolicy::new()
    };

    let mut engine = ghost_policy::engine::PolicyEngine::new(corp_policy);

    // Parse agent ID.
    let agent_id = args
        .agent_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    // Grant a synthetic capability so the capability check doesn't mask the
    // corp-policy result. We want to test corp-policy denial specifically.
    engine.grant_capability(agent_id, args.tool_name.clone());

    // Build synthetic tool call.
    let call = ghost_policy::context::ToolCall {
        tool_name: args.tool_name.clone(),
        arguments: serde_json::Value::Null,
        capability: args.tool_name.clone(),
        is_compaction_flush: false,
    };

    let ctx = ghost_policy::context::PolicyContext {
        agent_id,
        session_id: Uuid::nil(),
        intervention_level: 0,
        session_duration: Duration::ZERO,
        session_denial_count: 0,
        is_compaction_flush: false,
        session_reflection_count: 0,
    };

    let decision = engine.evaluate(&call, &ctx);

    let result = match decision {
        ghost_policy::engine::PolicyDecision::Permit => PolicyCheckResult {
            tool_name: args.tool_name,
            decision: "permit".into(),
            reason: None,
            constraint: None,
            alternatives: vec![],
        },
        ghost_policy::engine::PolicyDecision::Deny(feedback) => PolicyCheckResult {
            tool_name: args.tool_name,
            decision: "deny".into(),
            reason: Some(feedback.reason),
            constraint: Some(feedback.constraint),
            alternatives: feedback.suggested_alternatives,
        },
        ghost_policy::engine::PolicyDecision::Escalate(reason) => PolicyCheckResult {
            tool_name: args.tool_name,
            decision: "escalate".into(),
            reason: Some(reason),
            constraint: None,
            alternatives: vec![],
        },
    };

    print_output(&result, args.output);
    Ok(())
}

// ─── ghost policy lint (T-3.4.3) ────────────────────────────────────────────

pub struct PolicyLintArgs {}

pub fn run_lint(_args: PolicyLintArgs) -> Result<(), CliError> {
    let path = policy_path();
    let mut errors = 0u32;
    let mut checks = 0u32;

    // Check 1: File exists.
    checks += 1;
    if !path.exists() {
        println!("✗ CORP_POLICY.md not found at {}", path.display());
        errors += 1;
        println!();
        println!("{checks} check(s), {errors} error(s).");
        return Err(CliError::Config("CORP_POLICY.md not found".into()));
    }
    println!("✓ CORP_POLICY.md found at {}", path.display());

    let content = std::fs::read_to_string(&path)
        .map_err(|e| CliError::Config(format!("failed to read CORP_POLICY.md: {e}")))?;

    // Check 2: Non-empty.
    checks += 1;
    if content.trim().is_empty() {
        println!("✗ File is empty.");
        errors += 1;
    } else {
        println!("✓ File is non-empty ({} bytes).", content.len());
    }

    // Check 3: Contains a ## Denied Tools heading.
    checks += 1;
    let has_denied_heading = content.lines().any(|l| {
        let t = l.trim();
        t.eq_ignore_ascii_case("## denied tools") || t.eq_ignore_ascii_case("## denied-tools")
    });
    if has_denied_heading {
        println!("✓ Contains `## Denied Tools` heading.");
    } else {
        println!("✗ Missing `## Denied Tools` heading.");
        errors += 1;
    }

    // Check 4: Deny-list entries are valid format.
    checks += 1;
    if has_denied_heading {
        let policy = ghost_policy::corp_policy::CorpPolicy::load(&path);
        match policy {
            Ok(p) => {
                let count = p.denied_tools().len();
                if count > 0 {
                    println!("✓ {count} denied tool(s) parsed successfully.");
                } else {
                    println!(
                        "✗ `## Denied Tools` section has no entries. Add lines starting with `- `."
                    );
                    errors += 1;
                }
            }
            Err(e) => {
                println!("✗ Failed to parse policy: {e}");
                errors += 1;
            }
        }
    }

    // Check 5: Signature comment present (informational only).
    checks += 1;
    let has_signature = content.contains("<!-- SIGNATURE:");
    if has_signature {
        println!("✓ Signature comment found.");
    } else {
        println!("~ Signature comment not present (optional — use `ghost identity sign` to add).");
    }

    println!();
    if errors == 0 {
        println!("{checks} check(s) passed.");
    } else {
        println!("{checks} check(s), {errors} error(s).");
    }

    if errors > 0 {
        Err(CliError::Config(format!("{errors} lint error(s)")))
    } else {
        Ok(())
    }
}
