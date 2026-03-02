//! ghost heartbeat — agent heartbeat monitoring (T-4.5.1, §4.1).

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

pub struct HeartbeatStatusArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
struct HeartbeatState {
    engine_state: String,
    frequency_seconds: u64,
    tier: String,
    last_beat: Option<String>,
    agents_monitored: u32,
}

impl TableDisplay for HeartbeatState {
    fn print_table(&self) {
        println!("Heartbeat Engine");
        println!("  State:             {}", self.engine_state);
        println!("  Frequency:         {}s", self.frequency_seconds);
        println!("  Convergence tier:  {}", self.tier);
        println!(
            "  Last beat:         {}",
            self.last_beat.as_deref().unwrap_or("never")
        );
        println!("  Agents monitored:  {}", self.agents_monitored);
    }
}

/// Run `ghost heartbeat status`.
pub async fn run_status(args: HeartbeatStatusArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    // Try the health endpoint for heartbeat info.
    let resp = client.get("/api/health").await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse health: {e}")))?;

    // Extract heartbeat-relevant fields from the health response.
    let state = HeartbeatState {
        engine_state: body["status"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        frequency_seconds: body["heartbeat_frequency"]
            .as_u64()
            .unwrap_or(60),
        tier: body["convergence_tier"]
            .as_str()
            .unwrap_or("default")
            .to_string(),
        last_beat: body["last_heartbeat"]
            .as_str()
            .map(String::from),
        agents_monitored: body["agents_count"]
            .as_u64()
            .unwrap_or(0) as u32,
    };

    print_output(&state, args.output);
    Ok(())
}
