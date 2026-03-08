//! ghost cron — scheduled task management (T-4.5.2, §4.1).

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

// ─── ghost cron list ─────────────────────────────────────────────────────────

pub struct CronListArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
struct CronJob {
    id: String,
    schedule: String,
    agent_id: String,
    description: String,
    last_run: Option<String>,
    next_run: Option<String>,
    status: String,
}

#[derive(Serialize)]
struct CronJobList {
    jobs: Vec<CronJob>,
}

impl TableDisplay for CronJobList {
    fn print_table(&self) {
        if self.jobs.is_empty() {
            println!("No cron jobs registered.");
            return;
        }
        println!(
            "{:<12}  {:<16}  {:<12}  {:<26}  {:<26}  STATUS",
            "ID", "SCHEDULE", "AGENT", "LAST RUN", "NEXT RUN"
        );
        println!("{}", "─".repeat(100));
        for j in &self.jobs {
            let id = &j.id[..j.id.len().min(12)];
            let last = j.last_run.as_deref().unwrap_or("never");
            let next = j.next_run.as_deref().unwrap_or("-");
            let agent = &j.agent_id[..j.agent_id.len().min(12)];
            println!(
                "{:<12}  {:<16}  {:<12}  {:<26}  {:<26}  {}",
                id, j.schedule, agent, last, next, j.status
            );
        }
        println!("\n{} job(s) registered.", self.jobs.len());
    }
}

/// Run `ghost cron list`.
pub async fn run_list(args: CronListArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    // Query the workflows endpoint for scheduled workflows (cron-like).
    let resp = client.get("/api/workflows").await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse workflows: {e}")))?;

    let workflows: Vec<serde_json::Value> =
        serde_json::from_value(body["workflows"].clone()).unwrap_or_default();

    // Filter to scheduled workflows and map to CronJob.
    let jobs: Vec<CronJob> = workflows
        .iter()
        .filter(|w| w["schedule"].is_string())
        .map(|w| CronJob {
            id: w["id"].as_str().unwrap_or("").to_string(),
            schedule: w["schedule"].as_str().unwrap_or("").to_string(),
            agent_id: w["agent_id"].as_str().unwrap_or("-").to_string(),
            description: w["name"].as_str().unwrap_or("").to_string(),
            last_run: w["last_run"].as_str().map(String::from),
            next_run: w["next_run"].as_str().map(String::from),
            status: w["status"].as_str().unwrap_or("active").to_string(),
        })
        .collect();

    print_output(&CronJobList { jobs }, args.output);
    Ok(())
}

// ─── ghost cron history ──────────────────────────────────────────────────────

pub struct CronHistoryArgs {
    pub limit: u32,
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
struct CronExecution {
    job_id: String,
    timestamp: String,
    duration_ms: u64,
    status: String,
    cost: f64,
}

#[derive(Serialize)]
struct CronHistoryList {
    executions: Vec<CronExecution>,
}

impl TableDisplay for CronHistoryList {
    fn print_table(&self) {
        if self.executions.is_empty() {
            println!("No cron execution history.");
            return;
        }
        println!(
            "{:<12}  {:<26}  {:>10}  {:<10}  {:>10}",
            "JOB", "TIMESTAMP", "DURATION", "STATUS", "COST"
        );
        println!("{}", "─".repeat(75));
        for e in &self.executions {
            let id = &e.job_id[..e.job_id.len().min(12)];
            println!(
                "{:<12}  {:<26}  {:>8}ms  {:<10}  ${:>9.6}",
                id, e.timestamp, e.duration_ms, e.status, e.cost
            );
        }
        println!("\n{} execution(s) shown.", self.executions.len());
    }
}

/// Run `ghost cron history`.
pub async fn run_history(args: CronHistoryArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    let path = format!(
        "/api/audit?event_type=workflow_execution&limit={}",
        args.limit
    );
    let resp = client.get(&path).await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse audit: {e}")))?;

    let entries: Vec<serde_json::Value> =
        serde_json::from_value(body["entries"].clone()).unwrap_or_default();

    let executions: Vec<CronExecution> = entries
        .iter()
        .map(|e| CronExecution {
            job_id: e["details"]["workflow_id"]
                .as_str()
                .or_else(|| e["entity_id"].as_str())
                .unwrap_or("")
                .to_string(),
            timestamp: e["timestamp"].as_str().unwrap_or("").to_string(),
            duration_ms: e["details"]["duration_ms"].as_u64().unwrap_or(0),
            status: e["details"]["status"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            cost: e["details"]["cost"].as_f64().unwrap_or(0.0),
        })
        .collect();

    print_output(&CronHistoryList { executions }, args.output);
    Ok(())
}
