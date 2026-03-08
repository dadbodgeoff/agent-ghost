//! ghost skill — compiled skill catalog management.

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
                "  {:<20}  {:<10}  {:<10}  {:<8}  {}",
                "NAME", "VERSION", "STATE", "MODE", "POLICY"
            );
            println!("  {}", "─".repeat(75));
            for s in &self.installed {
                println!(
                    "  {:<20}  {:<10}  {:<10}  {:<8}  {}",
                    &s.name[..s.name.len().min(20)],
                    &s.version[..s.version.len().min(10)],
                    &s.state[..s.state.len().min(8)],
                    &s.execution_mode[..s.execution_mode.len().min(8)],
                    s.policy_capability
                );
                if !s.privileges.is_empty() {
                    println!("       privileges: {}", s.privileges.join(" | "));
                }
            }
        }

        if !self.available.is_empty() {
            if !self.installed.is_empty() {
                println!();
            }
            println!("Available Skills ({}):", self.available.len());
            println!(
                "  {:<20}  {:<10}  {:<10}  {}",
                "NAME", "VERSION", "STATE", "DESCRIPTION"
            );
            println!("  {}", "─".repeat(60));
            for s in &self.available {
                let desc = &s.description[..s.description.len().min(40)];
                println!(
                    "  {:<20}  {:<10}  {:<10}  {}",
                    &s.name[..s.name.len().min(20)],
                    &s.version[..s.version.len().min(10)],
                    &s.state[..s.state.len().min(10)],
                    desc
                );
                if !s.privileges.is_empty() {
                    println!("       privileges: {}", s.privileges.join(" | "));
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
struct InstallResult {
    name: String,
    version: String,
    status: String,
}

impl TableDisplay for InstallResult {
    fn print_table(&self) {
        println!(
            "Installed skill '{}' v{}: {}",
            self.name, self.version, self.status
        );
    }
}

/// Run `ghost skill install <path>`.
pub async fn run_install(args: SkillInstallArgs, backend: &CliBackend) -> Result<(), CliError> {
    let client = match backend {
        CliBackend::Http { client } => client,
        CliBackend::Direct { .. } => return Err(direct_mode_not_supported()),
    };

    // The path argument is the compiled skill name exposed by the gateway catalog.
    let path = format!("/api/skills/{}/install", args.path);
    let resp = client.post(&path, &serde_json::json!({})).await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse install response: {e}")))?;

    let result = InstallResult {
        name: body["name"].as_str().unwrap_or(&args.path).to_string(),
        version: body["version"].as_str().unwrap_or("unknown").to_string(),
        status: "installed".to_string(),
    };

    print_output(&result, args.output);
    Ok(())
}

// ─── ghost skill inspect ─────────────────────────────────────────────────────

pub struct SkillInspectArgs {
    pub name: String,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct SkillDetail {
    name: String,
    version: String,
    description: String,
    source: String,
    execution_mode: String,
    policy_capability: String,
    privileges: Vec<String>,
    state: String,
    capabilities: Vec<String>,
}

impl TableDisplay for SkillDetail {
    fn print_table(&self) {
        println!("Skill: {}", self.name);
        println!("  Version:      {}", self.version);
        println!("  Description:  {}", self.description);
        println!("  Source:       {}", self.source);
        println!("  Mode:         {}", self.execution_mode);
        println!("  State:        {}", self.state);
        println!("  Policy:       {}", self.policy_capability);
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
                        name: s.name.clone(),
                        version: s.version.clone(),
                        description: s.description.clone(),
                        source: s.source.clone(),
                        execution_mode: s.execution_mode.clone(),
                        policy_capability: s.policy_capability.clone(),
                        privileges: s.privileges.clone(),
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
    async fn direct_backend_is_rejected_for_list_install_and_inspect() {
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
    }
}
