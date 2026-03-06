//! ghost agent — agent lifecycle management (CLI§11).
//!
//! All subcommands delegate to existing `/api/agents/*` and `/api/safety/*`
//! endpoints via `CliBackend`.

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::confirm::confirm;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

/// Agent summary returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub spending_cap: f64,
}

#[derive(Debug, Serialize)]
struct AgentListResponse {
    agents: Vec<AgentSummary>,
}

impl TableDisplay for AgentListResponse {
    fn print_table(&self) {
        if self.agents.is_empty() {
            println!("No agents found.");
            return;
        }
        println!("{:<38} {:<20} {:<12} {:>10}", "ID", "NAME", "STATUS", "CAP");
        println!("{}", "─".repeat(82));
        for a in &self.agents {
            println!("{:<38} {:<20} {:<12} {:>10.2}", a.id, a.name, a.status, a.spending_cap);
        }
    }
}

impl TableDisplay for AgentSummary {
    fn print_table(&self) {
        println!("Agent: {}", self.name);
        println!("  ID:     {}", self.id);
        println!("  Status: {}", self.status);
        println!("  Cap:    {:.2}", self.spending_cap);
    }
}

pub async fn run_list(backend: &CliBackend, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let resp = backend.http().get("/api/agents").await?;
    let agents: Vec<AgentSummary> = resp.json().await.map_err(|e| {
        CliError::Http(format!("failed to parse agent list: {e}"))
    })?;
    print_output(&AgentListResponse { agents }, output);
    Ok(())
}

pub async fn run_inspect(backend: &CliBackend, id: &str, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let resp = backend.http().get(&format!("/api/agents/{id}")).await?;
    let agent: AgentSummary = resp.json().await.map_err(|e| {
        CliError::Http(format!("failed to parse agent: {e}"))
    })?;
    print_output(&agent, output);
    Ok(())
}

pub async fn run_create(backend: &CliBackend, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let resp = backend.http().post("/api/agents", &serde_json::json!({})).await?;
    let agent: AgentSummary = resp.json().await.map_err(|e| {
        CliError::Http(format!("failed to parse created agent: {e}"))
    })?;
    print_output(&agent, output);
    Ok(())
}

pub async fn run_delete(backend: &CliBackend, id: &str, yes: bool, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    if !confirm(&format!("Delete agent {id}? [y/N]"), yes) {
        return Err(CliError::Cancelled);
    }
    backend.http().delete(&format!("/api/agents/{id}")).await?;
    let resp = serde_json::json!({"deleted": id});
    if matches!(output, OutputFormat::Table) {
        println!("Agent {id} deleted.");
    } else {
        println!("{}", serde_json::to_string_pretty(&resp).unwrap_or_default());
    }
    Ok(())
}

pub async fn run_pause(backend: &CliBackend, id: &str, yes: bool, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    if !confirm(&format!("Pause agent {id}? [y/N]"), yes) {
        return Err(CliError::Cancelled);
    }
    backend.http().post(&format!("/api/safety/agents/{id}/pause"), &serde_json::json!({})).await?;
    if matches!(output, OutputFormat::Table) {
        println!("Agent {id} paused.");
    } else {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"paused": id})).unwrap_or_default());
    }
    Ok(())
}

pub async fn run_resume(backend: &CliBackend, id: &str, yes: bool, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    if !confirm(&format!("Resume agent {id}? [y/N]"), yes) {
        return Err(CliError::Cancelled);
    }
    backend.http().post(&format!("/api/safety/agents/{id}/resume"), &serde_json::json!({})).await?;
    if matches!(output, OutputFormat::Table) {
        println!("Agent {id} resumed.");
    } else {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"resumed": id})).unwrap_or_default());
    }
    Ok(())
}

pub async fn run_quarantine(backend: &CliBackend, id: &str, yes: bool, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    if !confirm(&format!("Quarantine agent {id}? [y/N]"), yes) {
        return Err(CliError::Cancelled);
    }
    backend.http().post(&format!("/api/safety/agents/{id}/quarantine"), &serde_json::json!({})).await?;
    if matches!(output, OutputFormat::Table) {
        println!("Agent {id} quarantined.");
    } else {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"quarantined": id})).unwrap_or_default());
    }
    Ok(())
}

pub async fn run_update(backend: &CliBackend, id: &str, output: OutputFormat) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let resp = backend.http().patch(&format!("/api/agents/{id}"), &serde_json::json!({})).await?;
    let agent: AgentSummary = resp.json().await.map_err(|e| {
        CliError::Http(format!("failed to parse updated agent: {e}"))
    })?;
    print_output(&agent, output);
    Ok(())
}
