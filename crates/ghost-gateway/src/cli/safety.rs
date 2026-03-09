//! ghost safety — kill switch and safety management (CLI§12).
//!
//! All subcommands delegate to existing `/api/safety/*` endpoints.
//! `kill-all` requires double confirmation (--yes + interactive prompt) unless --force.

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::backend::CliBackend;
use super::confirm::confirm;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

#[derive(Debug, Clone, Serialize)]
pub struct SafetyStatus {
    pub kill_switch_active: bool,
    pub paused_agents: Vec<String>,
    pub quarantined_agents: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct SafetyStatusWire {
    #[serde(default)]
    kill_switch_active: bool,
    #[serde(default)]
    platform_killed: bool,
    #[serde(default)]
    paused_agents: Vec<String>,
    #[serde(default)]
    quarantined_agents: Vec<String>,
    #[serde(default)]
    per_agent: BTreeMap<String, SafetyAgentStatus>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    platform_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SafetyAgentStatus {
    level: String,
}

impl SafetyStatus {
    fn from_api_value(value: Value) -> Result<Self, serde_json::Error> {
        let wire: SafetyStatusWire = serde_json::from_value(value)?;
        let mut paused_agents = wire.paused_agents;
        let mut quarantined_agents = wire.quarantined_agents;

        for (agent_id, agent_state) in wire.per_agent {
            match agent_state.level.as_str() {
                "Pause" => paused_agents.push(agent_id),
                "Quarantine" => quarantined_agents.push(agent_id),
                _ => {}
            }
        }

        paused_agents.sort();
        paused_agents.dedup();
        quarantined_agents.sort();
        quarantined_agents.dedup();

        let platform_active = wire.kill_switch_active
            || wire.platform_killed
            || wire
                .platform_level
                .as_deref()
                .is_some_and(is_non_normal_level)
            || wire.state.as_deref().is_some_and(is_non_normal_level);

        Ok(Self {
            kill_switch_active: platform_active
                || !paused_agents.is_empty()
                || !quarantined_agents.is_empty(),
            paused_agents,
            quarantined_agents,
        })
    }
}

fn is_non_normal_level(level: &str) -> bool {
    !level.is_empty() && level != "Normal"
}

impl TableDisplay for SafetyStatus {
    fn print_table(&self) {
        println!("Safety Status");
        println!("─────────────");
        println!(
            "  Kill switch: {}",
            if self.kill_switch_active {
                "ACTIVE"
            } else {
                "off"
            }
        );
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
    let value: Value = resp
        .json()
        .await
        .map_err(|e| CliError::Http(format!("failed to parse safety status: {e}")))?;
    let status = SafetyStatus::from_api_value(value)
        .map_err(|e| CliError::Http(format!("failed to normalize safety status: {e}")))?;
    print_output(&status, output);
    Ok(())
}

/// Kill all agents. Requires double confirmation:
/// 1. `--yes` flag (or interactive y/n)
/// 2. Type "KILL" to confirm (unless `--force`)
pub async fn run_kill_all(
    backend: &CliBackend,
    yes: bool,
    force: bool,
    output: OutputFormat,
) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;

    // First confirmation
    if !confirm(
        "Kill ALL agents? This will immediately stop all agent activity. [y/N]",
        yes,
    ) {
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

    backend
        .http()
        .post("/api/safety/kill-all", &serde_json::json!({}))
        .await?;

    if matches!(output, OutputFormat::Table) {
        println!("Kill switch activated. All agents stopped.");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"kill_switch": "activated"}))
                .unwrap_or_default()
        );
    }
    Ok(())
}

pub async fn run_clear(
    backend: &CliBackend,
    yes: bool,
    output: OutputFormat,
) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    if !confirm("Clear kill switch and resume normal operation? [y/N]", yes) {
        return Err(CliError::Cancelled);
    }

    backend
        .http()
        .post("/api/safety/clear", &serde_json::json!({}))
        .await?;

    if matches!(output, OutputFormat::Table) {
        println!("Kill switch cleared. Normal operation resumed.");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"kill_switch": "cleared"}))
                .unwrap_or_default()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_status_normalizes_gateway_response_shape() {
        let status = SafetyStatus::from_api_value(serde_json::json!({
            "platform_level": "Normal",
            "platform_killed": false,
            "per_agent": {
                "agent-paused": { "level": "Pause" },
                "agent-quarantined": { "level": "Quarantine" },
                "agent-normal": { "level": "Normal" }
            }
        }))
        .unwrap();

        assert!(status.kill_switch_active);
        assert_eq!(status.paused_agents, vec!["agent-paused"]);
        assert_eq!(status.quarantined_agents, vec!["agent-quarantined"]);
    }

    #[test]
    fn safety_status_normalizes_legacy_summary_shape() {
        let status = SafetyStatus::from_api_value(serde_json::json!({
            "kill_switch_active": true,
            "paused_agents": ["agent-a"],
            "quarantined_agents": ["agent-b"]
        }))
        .unwrap();

        assert!(status.kill_switch_active);
        assert_eq!(status.paused_agents, vec!["agent-a"]);
        assert_eq!(status.quarantined_agents, vec!["agent-b"]);
    }

    #[test]
    fn safety_status_normalizes_viewer_shape() {
        let status = SafetyStatus::from_api_value(serde_json::json!({
            "platform_killed": false,
            "state": "Normal"
        }))
        .unwrap();

        assert!(!status.kill_switch_active);
        assert!(status.paused_agents.is_empty());
        assert!(status.quarantined_agents.is_empty());
    }
}
