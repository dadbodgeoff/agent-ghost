//! ghost safety — kill switch and safety management (CLI§12).
//!
//! All subcommands delegate to existing `/api/safety/*` endpoints.
//! `kill-all` requires double confirmation (--yes + interactive prompt) unless --force.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::confirm::confirm;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyStatus {
    pub kill_switch_active: bool,
    pub paused_agents: Vec<String>,
    pub quarantined_agents: Vec<String>,
}

impl TableDisplay for SafetyStatus {
    fn print_table(&self) {
        println!("Safety Status");
        println!("─────────────");
        println!("  Kill switch: {}", if self.kill_switch_active { "ACTIVE" } else { "off" });
        if self.paused_agents.is_empty() {
            println!("  Paused:      none");
        } else {
            println!("  Paused:      {}", self.paused_agents.join(", "));
        }
        if self.quarantined_agents.is_empty() {
            println!("  Quarantined: none");
        } else {
            println!("  Quarantined: {}", self.quarantined_agents.join(", "));
        }
    }
}

pub async fn run_status(backend: &CliBackend, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let resp = backend.http().get("/api/safety/status").await?;
    let status: SafetyStatus = resp.json().await.map_err(|e| {
        CliError::Http(format!("failed to parse safety status: {e}"))
    })?;
    print_output(&status, output);
    Ok(())
}

/// Kill all agents. Requires double confirmation:
/// 1. `--yes` flag (or interactive y/n)
/// 2. Type "KILL" to confirm (unless `--force`)
pub async fn run_kill_all(backend: &CliBackend, yes: bool, force: bool, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;

    // First confirmation
    if !confirm("Kill ALL agents? This will immediately stop all agent activity. [y/N]", yes) {
        return Err(CliError::Cancelled);
    }

    // Second confirmation (unless --force)
    if !force {
        eprint!("Type KILL to confirm: ");
        let _ = io::stderr().flush();
        let stdin = io::stdin();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            return Err(CliError::Cancelled);
        }
        if line.trim() != "KILL" {
            eprintln!("Aborted.");
            return Err(CliError::Cancelled);
        }
    }

    backend.http().post("/api/safety/kill-all", &serde_json::json!({})).await?;

    if matches!(output, OutputFormat::Table) {
        println!("Kill switch activated. All agents stopped.");
    } else {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"kill_switch": "activated"})).unwrap_or_default());
    }
    Ok(())
}

pub async fn run_clear(backend: &CliBackend, yes: bool, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    if !confirm("Clear kill switch and resume normal operation? [y/N]", yes) {
        return Err(CliError::Cancelled);
    }

    backend.http().post("/api/safety/clear", &serde_json::json!({})).await?;

    if matches!(output, OutputFormat::Table) {
        println!("Kill switch cleared. Normal operation resumed.");
    } else {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"kill_switch": "cleared"})).unwrap_or_default());
    }
    Ok(())
}
