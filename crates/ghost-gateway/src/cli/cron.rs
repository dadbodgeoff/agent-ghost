//! ghost cron — control-plane schedule ledger views.

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

pub struct CronListArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Deserialize)]
struct AutonomyJobList {
    jobs: Vec<AutonomyJob>,
}

#[derive(Debug, Deserialize)]
struct AutonomyJob {
    id: String,
    job_type: String,
    agent_id: String,
    state: String,
    next_run_at: String,
    schedule_json: String,
    retry_after: Option<String>,
    last_success_at: Option<String>,
    last_failure_at: Option<String>,
    manual_review_required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CronJob {
    id: String,
    schedule: String,
    agent_id: String,
    kind: String,
    last_run: Option<String>,
    next_run: Option<String>,
    status: String,
    manual_review_required: bool,
}

#[derive(Serialize)]
struct CronJobList {
    jobs: Vec<CronJob>,
}

impl TableDisplay for CronJobList {
    fn print_table(&self) {
        if self.jobs.is_empty() {
            println!("No control-plane jobs registered.");
            return;
        }
        println!(
            "{:<12}  {:<18}  {:<14}  {:<18}  {:<26}  {:<26}  STATUS",
            "ID", "KIND", "AGENT", "SCHEDULE", "LAST RUN", "NEXT RUN"
        );
        println!("{}", "─".repeat(128));
        for job in &self.jobs {
            let id = &job.id[..job.id.len().min(12)];
            let agent = &job.agent_id[..job.agent_id.len().min(14)];
            let last_run = job.last_run.as_deref().unwrap_or("never");
            let next_run = job.next_run.as_deref().unwrap_or("-");
            let status = if job.manual_review_required {
                format!("{}*", job.status)
            } else {
                job.status.clone()
            };
            println!(
                "{:<12}  {:<18}  {:<14}  {:<18}  {:<26}  {:<26}  {}",
                id, job.kind, agent, job.schedule, last_run, next_run, status
            );
        }
        println!("\n{} job(s) registered.", self.jobs.len());
    }
}

pub async fn run_list(args: CronListArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    let jobs = client
        .get("/api/autonomy/jobs?limit=200")
        .await?
        .json::<AutonomyJobList>()
        .await
        .map_err(|e| CliError::Internal(format!("parse autonomy jobs: {e}")))?;

    let jobs = jobs
        .jobs
        .into_iter()
        .map(|job| CronJob {
            id: job.id,
            schedule: schedule_label(&job.schedule_json),
            agent_id: job.agent_id,
            kind: job.job_type,
            last_run: job.last_success_at.or(job.last_failure_at),
            next_run: job.retry_after.or(Some(job.next_run_at)),
            status: job.state,
            manual_review_required: job.manual_review_required,
        })
        .collect();

    print_output(&CronJobList { jobs }, args.output);
    Ok(())
}

pub struct CronHistoryArgs {
    pub limit: u32,
    pub output: OutputFormat,
}

#[derive(Debug, Deserialize)]
struct AutonomyRunList {
    runs: Vec<AutonomyRun>,
}

#[derive(Debug, Deserialize)]
struct AutonomyRun {
    job_id: String,
    due_at: String,
    attempt: i64,
    state: String,
    side_effect_status: String,
    terminal_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CronExecution {
    job_id: String,
    due_at: String,
    attempt: i64,
    status: String,
    side_effect_status: String,
    terminal_reason: Option<String>,
}

#[derive(Serialize)]
struct CronHistoryList {
    executions: Vec<CronExecution>,
}

impl TableDisplay for CronHistoryList {
    fn print_table(&self) {
        if self.executions.is_empty() {
            println!("No control-plane execution history.");
            return;
        }
        println!(
            "{:<12}  {:<26}  {:>7}  {:<12}  {:<16}  REASON",
            "JOB", "DUE AT", "ATTEMPT", "STATE", "SIDE EFFECT"
        );
        println!("{}", "─".repeat(104));
        for execution in &self.executions {
            let id = &execution.job_id[..execution.job_id.len().min(12)];
            println!(
                "{:<12}  {:<26}  {:>7}  {:<12}  {:<16}  {}",
                id,
                execution.due_at,
                execution.attempt,
                execution.status,
                execution.side_effect_status,
                execution.terminal_reason.as_deref().unwrap_or("-"),
            );
        }
        println!("\n{} execution(s) shown.", self.executions.len());
    }
}

pub async fn run_history(args: CronHistoryArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();
    let path = format!("/api/autonomy/runs?limit={}", args.limit.clamp(1, 200));
    let runs = client
        .get(&path)
        .await?
        .json::<AutonomyRunList>()
        .await
        .map_err(|e| CliError::Internal(format!("parse autonomy runs: {e}")))?;

    let executions = runs
        .runs
        .into_iter()
        .map(|run| CronExecution {
            job_id: run.job_id,
            due_at: run.due_at,
            attempt: run.attempt,
            status: run.state,
            side_effect_status: run.side_effect_status,
            terminal_reason: run.terminal_reason,
        })
        .collect();

    print_output(&CronHistoryList { executions }, args.output);
    Ok(())
}

fn schedule_label(schedule_json: &str) -> String {
    let value = serde_json::from_str::<serde_json::Value>(schedule_json).unwrap_or_default();
    match value["kind"].as_str() {
        Some("interval") => value["every_seconds"]
            .as_u64()
            .map(|seconds| format!("every {seconds}s"))
            .unwrap_or_else(|| "interval".to_string()),
        Some(kind) => kind.to_string(),
        None => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_cli_lists_control_plane_jobs_not_inferred_workflows() {
        let parsed = serde_json::from_value::<AutonomyJobList>(serde_json::json!({
            "jobs": [{
                "id": "job-1",
                "job_type": "workflow_trigger",
                "agent_id": "agent-1",
                "state": "queued",
                "next_run_at": "2026-03-10T12:00:00Z",
                "schedule_json": "{\"kind\":\"interval\",\"every_seconds\":60}",
                "retry_after": null,
                "last_success_at": null,
                "last_failure_at": null,
                "manual_review_required": false
            }]
        }))
        .expect("autonomy job payload");

        let list = CronJobList {
            jobs: parsed
                .jobs
                .into_iter()
                .map(|job| CronJob {
                    id: job.id,
                    schedule: schedule_label(&job.schedule_json),
                    agent_id: job.agent_id,
                    kind: job.job_type,
                    last_run: job.last_success_at.or(job.last_failure_at),
                    next_run: job.retry_after.or(Some(job.next_run_at)),
                    status: job.state,
                    manual_review_required: job.manual_review_required,
                })
                .collect(),
        };

        assert_eq!(list.jobs.len(), 1);
        assert_eq!(list.jobs[0].kind, "workflow_trigger");
        assert_eq!(list.jobs[0].schedule, "every 60s");
    }
}
