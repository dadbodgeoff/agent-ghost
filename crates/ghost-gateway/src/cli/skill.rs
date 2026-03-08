//! ghost skill — mixed-source skill catalog management.

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

// ─── ghost skill list ────────────────────────────────────────────────────────

pub struct SkillListArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub removable: bool,
    pub installable: bool,
    pub execution_mode: String,
    pub policy_capability: String,
    pub privileges: Vec<String>,
    pub requested_capabilities: Vec<String>,
    pub install_state: String,
    pub verification_status: String,
    pub quarantine_state: String,
    pub runtime_visible: bool,
    pub artifact_digest: Option<String>,
    pub publisher: Option<String>,
    pub source_uri: Option<String>,
    pub signer_key_id: Option<String>,
    pub signer_publisher: Option<String>,
    pub quarantine_reason: Option<String>,
    pub quarantine_revision: Option<i64>,
    pub state: String,
    pub capabilities: Vec<String>,
}

#[derive(Serialize)]
struct SkillListResponse {
    installed: Vec<SkillEntry>,
    available: Vec<SkillEntry>,
}

fn direct_mode_not_supported() -> CliError {
    CliError::Usage(
        "skill catalog commands require a running gateway; direct manifest-backed skill mode is no longer supported"
            .into(),
    )
}

impl TableDisplay for SkillListResponse {
    fn print_table(&self) {
        if self.installed.is_empty() && self.available.is_empty() {
            println!("No skills found.");
            return;
        }

        if !self.installed.is_empty() {
            println!("Installed Skills ({}):", self.installed.len());
            println!(
                "  {:<20}  {:<10}  {:<10}  {:<12}  {}",
                "NAME", "VERSION", "STATE", "VERIFY", "POLICY"
            );
            println!("  {}", "─".repeat(92));
            for s in &self.installed {
                println!(
                    "  {:<20}  {:<10}  {:<10}  {:<12}  {}",
                    &s.name[..s.name.len().min(20)],
                    &s.version[..s.version.len().min(10)],
                    &s.state[..s.state.len().min(8)],
                    &s.verification_status[..s.verification_status.len().min(12)],
                    s.policy_capability
                );
                println!(
                    "       id: {} | source: {} | install: {} | runtime_visible: {}",
                    s.id, s.source, s.install_state, s.runtime_visible
                );
                if !s.privileges.is_empty() {
                    println!("       privileges: {}", s.privileges.join(" | "));
                }
                if let Some(reason) = &s.quarantine_reason {
                    println!("       quarantine: {}", reason);
                }
            }
        }

        if !self.available.is_empty() {
            if !self.installed.is_empty() {
                println!();
            }
            println!("Available Skills ({}):", self.available.len());
            println!(
                "  {:<20}  {:<10}  {:<10}  {:<12}  {}",
                "NAME", "VERSION", "STATE", "VERIFY", "DESCRIPTION"
            );
            println!("  {}", "─".repeat(84));
            for s in &self.available {
                let desc = &s.description[..s.description.len().min(40)];
                println!(
                    "  {:<20}  {:<10}  {:<10}  {:<12}  {}",
                    &s.name[..s.name.len().min(20)],
                    &s.version[..s.version.len().min(10)],
                    &s.state[..s.state.len().min(10)],
                    &s.verification_status[..s.verification_status.len().min(12)],
                    desc
                );
                println!(
                    "       id: {} | source: {} | install: {} | runtime_visible: {}",
                    s.id, s.source, s.install_state, s.runtime_visible
                );
                if !s.privileges.is_empty() {
                    println!("       privileges: {}", s.privileges.join(" | "));
                }
                if let Some(reason) = &s.quarantine_reason {
                    println!("       quarantine: {}", reason);
                }
            }
        }
    }
}

/// Run `ghost skill list`.
pub async fn run_list(args: SkillListArgs, backend: &CliBackend) -> Result<(), CliError> {
    let (installed, available) = match backend {
        CliBackend::Http { client } => {
            let resp = client.get("/api/skills").await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse skills: {e}")))?;
            let installed: Vec<SkillEntry> =
                serde_json::from_value(body["installed"].clone()).unwrap_or_default();
            let available: Vec<SkillEntry> =
                serde_json::from_value(body["available"].clone()).unwrap_or_default();
            (installed, available)
        }
        CliBackend::Direct { .. } => return Err(direct_mode_not_supported()),
    };

    print_output(
        &SkillListResponse {
            installed,
            available,
        },
        args.output,
    );
    Ok(())
}

// ─── ghost skill install ─────────────────────────────────────────────────────

pub struct SkillInstallArgs {
    pub path: String,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct SkillMutationResult {
    action: String,
    id: String,
    name: String,
    version: String,
    state: String,
    install_state: String,
    verification_status: String,
    quarantine_state: String,
    runtime_visible: bool,
    quarantine_revision: Option<i64>,
}

impl TableDisplay for SkillMutationResult {
    fn print_table(&self) {
        println!(
            "Skill '{}' ({}) v{}: action={} state={} install={} verify={} quarantine={} runtime_visible={}",
            self.name,
            self.id,
            self.version,
            self.action,
            self.state,
            self.install_state,
            self.verification_status,
            self.quarantine_state,
            self.runtime_visible
        );
        if let Some(revision) = self.quarantine_revision {
            println!("  quarantine_revision={revision}");
        }
    }
}

fn parse_skill_mutation_result(
    body: serde_json::Value,
    fallback_id: &str,
    action: &str,
) -> SkillMutationResult {
    SkillMutationResult {
        action: action.to_string(),
        id: body["id"].as_str().unwrap_or(fallback_id).to_string(),
        name: body["name"].as_str().unwrap_or(fallback_id).to_string(),
        version: body["version"].as_str().unwrap_or("unknown").to_string(),
        state: body["state"].as_str().unwrap_or("unknown").to_string(),
        install_state: body["install_state"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        verification_status: body["verification_status"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        quarantine_state: body["quarantine_state"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        runtime_visible: body["runtime_visible"].as_bool().unwrap_or(false),
        quarantine_revision: body["quarantine_revision"].as_i64(),
    }
}

/// Run `ghost skill install <path>`.
pub async fn run_install(args: SkillInstallArgs, backend: &CliBackend) -> Result<(), CliError> {
    let client = match backend {
        CliBackend::Http { client } => client,
        CliBackend::Direct { .. } => return Err(direct_mode_not_supported()),
    };

    // The path argument is a catalog identifier: compiled skill name or external artifact id.
    let path = format!("/api/skills/{}/install", args.path);
    let resp = client.post(&path, &serde_json::json!({})).await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse install response: {e}")))?;

    let result = parse_skill_mutation_result(body, &args.path, "installed");

    print_output(&result, args.output);
    Ok(())
}

pub struct SkillQuarantineArgs {
    pub id: String,
    pub reason: String,
    pub output: OutputFormat,
}

pub async fn run_quarantine(
    args: SkillQuarantineArgs,
    backend: &CliBackend,
) -> Result<(), CliError> {
    let client = match backend {
        CliBackend::Http { client } => client,
        CliBackend::Direct { .. } => return Err(direct_mode_not_supported()),
    };

    let path = format!("/api/skills/{}/quarantine", args.id);
    let resp = client
        .post(&path, &serde_json::json!({ "reason": args.reason }))
        .await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse quarantine response: {e}")))?;

    print_output(
        &parse_skill_mutation_result(body, &args.id, "quarantined"),
        args.output,
    );
    Ok(())
}

pub struct SkillResolveQuarantineArgs {
    pub id: String,
    pub expected_revision: i64,
    pub output: OutputFormat,
}

pub async fn run_resolve_quarantine(
    args: SkillResolveQuarantineArgs,
    backend: &CliBackend,
) -> Result<(), CliError> {
    let client = match backend {
        CliBackend::Http { client } => client,
        CliBackend::Direct { .. } => return Err(direct_mode_not_supported()),
    };

    let path = format!("/api/skills/{}/quarantine/resolve", args.id);
    let resp = client
        .post(
            &path,
            &serde_json::json!({
                "expected_quarantine_revision": args.expected_revision,
            }),
        )
        .await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse resolve response: {e}")))?;

    print_output(
        &parse_skill_mutation_result(body, &args.id, "quarantine_resolved"),
        args.output,
    );
    Ok(())
}

pub struct SkillReverifyArgs {
    pub id: String,
    pub output: OutputFormat,
}

pub async fn run_reverify(args: SkillReverifyArgs, backend: &CliBackend) -> Result<(), CliError> {
    let client = match backend {
        CliBackend::Http { client } => client,
        CliBackend::Direct { .. } => return Err(direct_mode_not_supported()),
    };

    let path = format!("/api/skills/{}/reverify", args.id);
    let resp = client.post(&path, &serde_json::json!({})).await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse reverify response: {e}")))?;

    print_output(
        &parse_skill_mutation_result(body, &args.id, "reverified"),
        args.output,
    );
    Ok(())
}

// ─── ghost skill inspect ─────────────────────────────────────────────────────

pub struct SkillInspectArgs {
    pub name: String,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct SkillDetail {
    id: String,
    name: String,
    version: String,
    description: String,
    source: String,
    execution_mode: String,
    policy_capability: String,
    privileges: Vec<String>,
    requested_capabilities: Vec<String>,
    install_state: String,
    verification_status: String,
    quarantine_state: String,
    runtime_visible: bool,
    artifact_digest: Option<String>,
    publisher: Option<String>,
    source_uri: Option<String>,
    signer_key_id: Option<String>,
    signer_publisher: Option<String>,
    quarantine_reason: Option<String>,
    quarantine_revision: Option<i64>,
    state: String,
    capabilities: Vec<String>,
}

impl TableDisplay for SkillDetail {
    fn print_table(&self) {
        println!("Skill: {}", self.name);
        println!("  Id:           {}", self.id);
        println!("  Version:      {}", self.version);
        println!("  Description:  {}", self.description);
        println!("  Source:       {}", self.source);
        println!("  Mode:         {}", self.execution_mode);
        println!("  State:        {}", self.state);
        println!("  Install:      {}", self.install_state);
        println!("  Verify:       {}", self.verification_status);
        println!("  Quarantine:   {}", self.quarantine_state);
        println!("  Runtime:      {}", self.runtime_visible);
        println!("  Policy:       {}", self.policy_capability);
        if let Some(digest) = &self.artifact_digest {
            println!("  Digest:       {}", digest);
        }
        if let Some(publisher) = &self.publisher {
            println!("  Publisher:    {}", publisher);
        }
        if let Some(source_uri) = &self.source_uri {
            println!("  Source URI:   {}", source_uri);
        }
        if let Some(key_id) = &self.signer_key_id {
            println!("  Signer Key:   {}", key_id);
        }
        if let Some(publisher) = &self.signer_publisher {
            println!("  Signer Org:   {}", publisher);
        }
        if let Some(reason) = &self.quarantine_reason {
            println!("  Quarantine Reason: {}", reason);
        }
        if let Some(revision) = self.quarantine_revision {
            println!("  Quarantine Revision: {}", revision);
        }
        if !self.requested_capabilities.is_empty() {
            println!(
                "  Requested Capabilities: {}",
                self.requested_capabilities.join(", ")
            );
        }
        if self.privileges.is_empty() {
            println!("  Privileges:   none declared");
        } else {
            println!("  Privileges:");
            for privilege in &self.privileges {
                println!("    - {}", privilege);
            }
        }
    }
}

/// Run `ghost skill inspect <name>`.
pub async fn run_inspect(args: SkillInspectArgs, backend: &CliBackend) -> Result<(), CliError> {
    match backend {
        CliBackend::Http { client } => {
            // List all skills and find the matching one.
            let resp = client.get("/api/skills").await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse skills: {e}")))?;

            let installed: Vec<SkillEntry> =
                serde_json::from_value(body["installed"].clone()).unwrap_or_default();
            let available: Vec<SkillEntry> =
                serde_json::from_value(body["available"].clone()).unwrap_or_default();

            let skill = installed
                .iter()
                .chain(available.iter())
                .find(|s| s.name == args.name || s.id == args.name);

            match skill {
                Some(s) => {
                    let detail = SkillDetail {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        version: s.version.clone(),
                        description: s.description.clone(),
                        source: s.source.clone(),
                        execution_mode: s.execution_mode.clone(),
                        policy_capability: s.policy_capability.clone(),
                        privileges: s.privileges.clone(),
                        requested_capabilities: s.requested_capabilities.clone(),
                        install_state: s.install_state.clone(),
                        verification_status: s.verification_status.clone(),
                        quarantine_state: s.quarantine_state.clone(),
                        runtime_visible: s.runtime_visible,
                        artifact_digest: s.artifact_digest.clone(),
                        publisher: s.publisher.clone(),
                        source_uri: s.source_uri.clone(),
                        signer_key_id: s.signer_key_id.clone(),
                        signer_publisher: s.signer_publisher.clone(),
                        quarantine_reason: s.quarantine_reason.clone(),
                        quarantine_revision: s.quarantine_revision,
                        state: s.state.clone(),
                        capabilities: s.capabilities.clone(),
                    };
                    print_output(&detail, args.output);
                }
                None => {
                    return Err(CliError::NotFound(format!(
                        "skill '{}' not found",
                        args.name
                    )));
                }
            }
        }
        CliBackend::Direct { .. } => return Err(direct_mode_not_supported()),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_backend() -> CliBackend {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db = crate::db_pool::create_pool(tmp_dir.path().join("cli-skill.db")).unwrap();
        CliBackend::Direct {
            config: crate::config::GhostConfig::default(),
            db,
        }
    }

    #[tokio::test]
    async fn direct_backend_is_rejected_for_skill_http_only_commands() {
        let backend = direct_backend();

        let list_error = run_list(
            SkillListArgs {
                output: OutputFormat::Json,
            },
            &backend,
        )
        .await
        .unwrap_err();
        assert!(matches!(list_error, CliError::Usage(_)));

        let install_error = run_install(
            SkillInstallArgs {
                path: "note_take".into(),
                output: OutputFormat::Json,
            },
            &backend,
        )
        .await
        .unwrap_err();
        assert!(matches!(install_error, CliError::Usage(_)));

        let inspect_error = run_inspect(
            SkillInspectArgs {
                name: "note_take".into(),
                output: OutputFormat::Json,
            },
            &backend,
        )
        .await
        .unwrap_err();
        assert!(matches!(inspect_error, CliError::Usage(_)));

        let quarantine_error = run_quarantine(
            SkillQuarantineArgs {
                id: "digest".into(),
                reason: "manual review".into(),
                output: OutputFormat::Json,
            },
            &backend,
        )
        .await
        .unwrap_err();
        assert!(matches!(quarantine_error, CliError::Usage(_)));

        let resolve_error = run_resolve_quarantine(
            SkillResolveQuarantineArgs {
                id: "digest".into(),
                expected_revision: 1,
                output: OutputFormat::Json,
            },
            &backend,
        )
        .await
        .unwrap_err();
        assert!(matches!(resolve_error, CliError::Usage(_)));

        let reverify_error = run_reverify(
            SkillReverifyArgs {
                id: "digest".into(),
                output: OutputFormat::Json,
            },
            &backend,
        )
        .await
        .unwrap_err();
        assert!(matches!(reverify_error, CliError::Usage(_)));
    }
}
