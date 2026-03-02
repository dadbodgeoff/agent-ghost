//! ghost skill — WASM skill management (T-4.2.1–T-4.2.3, §4.1).

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

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
    pub capabilities: Vec<String>,
    pub source: String,
    pub state: String,
}

#[derive(Serialize)]
struct SkillListResponse {
    installed: Vec<SkillEntry>,
    available: Vec<SkillEntry>,
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
                "NAME", "VERSION", "SOURCE", "STATE", "CAPABILITIES"
            );
            println!("  {}", "─".repeat(75));
            for s in &self.installed {
                println!(
                    "  {:<20}  {:<10}  {:<10}  {:<8}  {}",
                    &s.name[..s.name.len().min(20)],
                    &s.version[..s.version.len().min(10)],
                    &s.source[..s.source.len().min(10)],
                    &s.state[..s.state.len().min(8)],
                    s.capabilities.join(", ")
                );
            }
        }

        if !self.available.is_empty() {
            if !self.installed.is_empty() {
                println!();
            }
            println!("Available Skills ({}):", self.available.len());
            println!(
                "  {:<20}  {:<10}  {}",
                "NAME", "VERSION", "DESCRIPTION"
            );
            println!("  {}", "─".repeat(60));
            for s in &self.available {
                let desc = &s.description[..s.description.len().min(40)];
                println!(
                    "  {:<20}  {:<10}  {}",
                    &s.name[..s.name.len().min(20)],
                    &s.version[..s.version.len().min(10)],
                    desc
                );
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
        CliBackend::Direct { .. } => {
            // Direct: read from ~/.ghost/skills/ directory.
            let skills_dir = crate::bootstrap::shellexpand_tilde("~/.ghost/skills");
            let mut installed = Vec::new();
            if let Ok(dir) = std::fs::read_dir(&skills_dir) {
                for entry in dir.flatten() {
                    let path = entry.path();
                    // Look for manifest.json files in skill directories.
                    let manifest_path = if path.is_dir() {
                        path.join("manifest.json")
                    } else {
                        continue;
                    };
                    if manifest_path.exists() {
                        if let Ok(data) = std::fs::read_to_string(&manifest_path) {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                                installed.push(SkillEntry {
                                    id: v["name"].as_str().unwrap_or("").to_string(),
                                    name: v["name"].as_str().unwrap_or("").to_string(),
                                    version: v["version"].as_str().unwrap_or("0.0.0").to_string(),
                                    description: v["description"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    capabilities: v["capabilities"]
                                        .as_array()
                                        .map(|a| {
                                            a.iter()
                                                .filter_map(|c| c.as_str().map(String::from))
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    source: "user".to_string(),
                                    state: "loaded".to_string(),
                                });
                            }
                        }
                    }
                }
            }
            (installed, Vec::new())
        }
    };

    print_output(&SkillListResponse { installed, available }, args.output);
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
        println!("Installed skill '{}' v{}: {}", self.name, self.version, self.status);
    }
}

/// Run `ghost skill install <path>`.
pub async fn run_install(args: SkillInstallArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    // The path argument is the skill ID for the bundled skills API.
    let path = format!("/api/skills/{}/install", args.path);
    let resp = client.post(&path, &serde_json::json!({})).await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse install response: {e}")))?;

    let result = InstallResult {
        name: body["name"]
            .as_str()
            .unwrap_or(&args.path)
            .to_string(),
        version: body["version"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
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
    capabilities: Vec<String>,
    source: String,
    state: String,
}

impl TableDisplay for SkillDetail {
    fn print_table(&self) {
        println!("Skill: {}", self.name);
        println!("  Version:      {}", self.version);
        println!("  Description:  {}", self.description);
        println!("  Source:        {}", self.source);
        println!("  State:         {}", self.state);
        println!("  Capabilities: {}", self.capabilities.join(", "));
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
                        capabilities: s.capabilities.clone(),
                        source: s.source.clone(),
                        state: s.state.clone(),
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
        CliBackend::Direct { .. } => {
            // Direct: try to read from ~/.ghost/skills/<name>/manifest.json.
            let manifest_path = format!(
                "{}/{}/manifest.json",
                crate::bootstrap::shellexpand_tilde("~/.ghost/skills"),
                args.name
            );
            let data = std::fs::read_to_string(&manifest_path).map_err(|_| {
                CliError::NotFound(format!("skill '{}' not found in ~/.ghost/skills/", args.name))
            })?;
            let v: serde_json::Value = serde_json::from_str(&data)
                .map_err(|e| CliError::Internal(format!("parse manifest: {e}")))?;

            let detail = SkillDetail {
                name: v["name"].as_str().unwrap_or("").to_string(),
                version: v["version"].as_str().unwrap_or("0.0.0").to_string(),
                description: v["description"].as_str().unwrap_or("").to_string(),
                capabilities: v["capabilities"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| c.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                source: "user".to_string(),
                state: "loaded".to_string(),
            };
            print_output(&detail, args.output);
        }
    }

    Ok(())
}
