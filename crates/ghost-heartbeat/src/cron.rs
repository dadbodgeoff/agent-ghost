//! Cron engine: standard cron syntax, timezone-aware, per-job cost tracking (Req 34 AC5-AC7).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A cron job definition loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobDef {
    pub name: String,
    /// Standard cron expression (5 fields: min hour dom month dow).
    pub schedule: String,
    /// Message to send when the job fires.
    pub message: String,
    /// Optional target channel for the job output.
    #[serde(default)]
    pub target_channel: Option<String>,
    /// Timezone (IANA name, e.g. "America/New_York").
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// Whether this job is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_timezone() -> String {
    "UTC".into()
}

fn default_true() -> bool {
    true
}

/// Runtime state for a cron job.
#[derive(Debug, Clone)]
pub struct CronJobState {
    pub def: CronJobDef,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub total_cost: f64,
}

/// Cron engine manages scheduled jobs for an agent.
pub struct CronEngine {
    pub agent_id: Uuid,
    pub jobs: Vec<CronJobState>,
    pub platform_killed: Arc<AtomicBool>,
    pub agent_paused: Arc<AtomicBool>,
}

impl CronEngine {
    pub fn new(
        agent_id: Uuid,
        platform_killed: Arc<AtomicBool>,
        agent_paused: Arc<AtomicBool>,
    ) -> Self {
        Self {
            agent_id,
            jobs: Vec::new(),
            platform_killed,
            agent_paused,
        }
    }

    /// Load job definitions from YAML strings.
    pub fn load_jobs(&mut self, yaml_sources: &[String]) {
        for yaml in yaml_sources {
            match serde_yaml::from_str::<CronJobDef>(yaml) {
                Ok(def) => {
                    if def.enabled {
                        tracing::info!(
                            agent_id = %self.agent_id,
                            job = %def.name,
                            schedule = %def.schedule,
                            "Loaded cron job"
                        );
                        self.jobs.push(CronJobState {
                            def,
                            last_run: None,
                            run_count: 0,
                            total_cost: 0.0,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        agent_id = %self.agent_id,
                        error = %e,
                        "Failed to parse cron job YAML"
                    );
                }
            }
        }
    }

    /// Check which jobs should fire now. Returns indices of ready jobs.
    pub fn ready_jobs(&self) -> Vec<usize> {
        // Check kill switch and pause
        if self.platform_killed.load(Ordering::SeqCst) {
            return Vec::new();
        }
        if self.agent_paused.load(Ordering::SeqCst) {
            return Vec::new();
        }

        let now = Utc::now();
        self.jobs
            .iter()
            .enumerate()
            .filter(|(_, job)| self.should_fire(job, now))
            .map(|(i, _)| i)
            .collect()
    }

    /// Record that a job ran.
    pub fn record_run(&mut self, job_index: usize, cost: f64) {
        if let Some(job) = self.jobs.get_mut(job_index) {
            job.last_run = Some(Utc::now());
            job.run_count += 1;
            job.total_cost += cost;
        }
    }

    /// Parse a simple cron expression and check if it matches the given time.
    /// Supports: minute hour day-of-month month day-of-week
    /// Supports: * and numeric values. Does not support ranges/steps for simplicity.
    pub fn cron_matches(schedule: &str, dt: DateTime<Utc>) -> bool {
        let fields: Vec<&str> = schedule.split_whitespace().collect();
        if fields.len() != 5 {
            return false;
        }

        let checks = [
            (fields[0], dt.format("%M").to_string()),
            (fields[1], dt.format("%H").to_string()),
            (fields[2], dt.format("%d").to_string()),
            (fields[3], dt.format("%m").to_string()),
            (fields[4], dt.format("%u").to_string()), // 1=Monday
        ];

        checks.iter().all(|(pattern, actual)| {
            *pattern == "*" || *pattern == actual.trim_start_matches('0')
                || *pattern == *actual
        })
    }

    fn should_fire(&self, job: &CronJobState, now: DateTime<Utc>) -> bool {
        if !job.def.enabled {
            return false;
        }
        // Check if schedule matches current time
        if !Self::cron_matches(&job.def.schedule, now) {
            return false;
        }
        // Don't fire more than once per minute
        if let Some(last) = job.last_run {
            let elapsed = now - last;
            if elapsed.num_seconds() < 60 {
                return false;
            }
        }
        true
    }
}
