//! ghost heartbeat — truthful control-plane status.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

pub struct HeartbeatStatusArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Deserialize)]
struct AutonomyStatus {
    deployment_mode: String,
    runtime_state: String,
    scheduler_running: bool,
    worker_count: usize,
    paused_jobs: usize,
    quarantined_jobs: usize,
    manual_review_jobs: usize,
    last_successful_dispatch_at: Option<String>,
    saturation: AutonomySaturation,
}

#[derive(Debug, Deserialize)]
struct AutonomySaturation {
    saturated: bool,
    blocked_due_jobs: usize,
    reserved_slots: usize,
    global_concurrency: usize,
    per_agent_concurrency: usize,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AutonomyJobsResponse {
    jobs: Vec<AutonomyJob>,
}

#[derive(Debug, Deserialize)]
struct AutonomyJob {
    job_type: String,
    agent_id: String,
    last_heartbeat_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct HeartbeatState {
    runtime_state: String,
    deployment_mode: String,
    scheduler_running: bool,
    worker_count: usize,
    monitored_agents: usize,
    heartbeat_jobs: usize,
    paused_jobs: usize,
    quarantined_jobs: usize,
    manual_review_jobs: usize,
    last_heartbeat_at: Option<String>,
    last_successful_dispatch_at: Option<String>,
    saturated: bool,
    blocked_due_jobs: usize,
    reserved_slots: usize,
    global_concurrency: usize,
    per_agent_concurrency: usize,
    saturation_reason: Option<String>,
}

impl TableDisplay for HeartbeatState {
    fn print_table(&self) {
        println!("Autonomy Heartbeat");
        println!("  Runtime:           {}", self.runtime_state);
        println!("  Deployment:        {}", self.deployment_mode);
        println!(
            "  Scheduler:         {}",
            if self.scheduler_running {
                "running"
            } else {
                "stopped"
            }
        );
        println!("  Workers:           {}", self.worker_count);
        println!("  Agents monitored:  {}", self.monitored_agents);
        println!("  Heartbeat jobs:    {}", self.heartbeat_jobs);
        println!(
            "  Last heartbeat:    {}",
            self.last_heartbeat_at.as_deref().unwrap_or("never")
        );
        println!(
            "  Last dispatch:     {}",
            self.last_successful_dispatch_at
                .as_deref()
                .unwrap_or("never")
        );
        println!("  Paused jobs:       {}", self.paused_jobs);
        println!("  Quarantined jobs:  {}", self.quarantined_jobs);
        println!("  Manual review:     {}", self.manual_review_jobs);
        println!(
            "  Saturation:        {}",
            if self.saturated { "yes" } else { "no" }
        );
        println!("  Blocked due jobs:  {}", self.blocked_due_jobs);
        println!(
            "  Slots:             {}/{} (per agent {})",
            self.reserved_slots, self.global_concurrency, self.per_agent_concurrency
        );
        if let Some(reason) = self.saturation_reason.as_deref() {
            println!("  Saturation reason: {}", reason);
        }
    }
}

fn build_state(status: AutonomyStatus, jobs: AutonomyJobsResponse) -> HeartbeatState {
    let heartbeat_jobs = jobs
        .jobs
        .iter()
        .filter(|job| job.job_type == "heartbeat_observe")
        .collect::<Vec<_>>();
    let monitored_agents = heartbeat_jobs
        .iter()
        .map(|job| job.agent_id.clone())
        .collect::<BTreeSet<_>>()
        .len();
    let last_heartbeat_at = heartbeat_jobs
        .iter()
        .filter_map(|job| job.last_heartbeat_at.clone())
        .max();

    HeartbeatState {
        runtime_state: status.runtime_state,
        deployment_mode: status.deployment_mode,
        scheduler_running: status.scheduler_running,
        worker_count: status.worker_count,
        monitored_agents,
        heartbeat_jobs: heartbeat_jobs.len(),
        paused_jobs: status.paused_jobs,
        quarantined_jobs: status.quarantined_jobs,
        manual_review_jobs: status.manual_review_jobs,
        last_heartbeat_at,
        last_successful_dispatch_at: status.last_successful_dispatch_at,
        saturated: status.saturation.saturated,
        blocked_due_jobs: status.saturation.blocked_due_jobs,
        reserved_slots: status.saturation.reserved_slots,
        global_concurrency: status.saturation.global_concurrency,
        per_agent_concurrency: status.saturation.per_agent_concurrency,
        saturation_reason: status.saturation.reason,
    }
}

/// Run `ghost heartbeat status`.
pub async fn run_status(args: HeartbeatStatusArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    let status = client
        .get("/api/autonomy/status")
        .await?
        .json::<AutonomyStatus>()
        .await
        .map_err(|e| CliError::Internal(format!("parse autonomy status: {e}")))?;
    let jobs = client
        .get("/api/autonomy/jobs?limit=200")
        .await?
        .json::<AutonomyJobsResponse>()
        .await
        .map_err(|e| CliError::Internal(format!("parse autonomy jobs: {e}")))?;

    print_output(&build_state(status, jobs), args.output);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_cli_fails_if_server_omits_required_status_fields() {
        let parsed = serde_json::from_value::<AutonomyStatus>(serde_json::json!({
            "runtime_state": "running"
        }));
        assert!(parsed.is_err());
    }
}
