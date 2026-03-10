use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::http::StatusCode;
use chrono::{DateTime, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::agents::registry::RegisteredAgent;
use crate::api::error::ApiError;
use crate::api::idempotency::PreparedOperationLease;
use crate::runtime::GatewayRuntime;
use crate::runtime_safety::{RuntimeSafetyBuilder, RuntimeSafetyContext};
use crate::state::AppState;

const DEFAULT_HEARTBEAT_INTERVAL_SECONDS: u64 = 30 * 60;
const HEARTBEAT_TIER0_INTERVAL_SECONDS: u64 = 120;
const HEARTBEAT_TIER1_INTERVAL_SECONDS: u64 = 30;
const HEARTBEAT_TIER2_INTERVAL_SECONDS: u64 = 15;
const HEARTBEAT_TIER3_INTERVAL_SECONDS: u64 = 5;
const HEARTBEAT_MESSAGE: &str = "[HEARTBEAT] Autonomy control plane requested a tier-3 review.";

#[derive(Debug, Clone)]
pub struct AutonomyRuntimeConfig {
    pub poll_interval: Duration,
    pub lease_ttl: Duration,
    pub global_concurrency: usize,
    pub per_agent_concurrency: usize,
    pub max_select_per_tick: usize,
}

impl Default for AutonomyRuntimeConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            lease_ttl: Duration::from_secs(45),
            global_concurrency: 2,
            per_agent_concurrency: 1,
            max_select_per_tick: 16,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyRetryPolicy {
    pub attempts: u32,
    pub max_retry_duration_seconds: u64,
    pub backoff: String,
    pub min_backoff_seconds: u64,
    pub max_backoff_seconds: u64,
    #[serde(default)]
    pub retryable_classes: Vec<String>,
}

impl Default for AutonomyRetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 3,
            max_retry_duration_seconds: 15 * 60,
            backoff: "exponential".into(),
            min_backoff_seconds: 15,
            max_backoff_seconds: 5 * 60,
            retryable_classes: vec!["transient".into(), "provider".into(), "timeout".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyScheduleSpec {
    pub version: u32,
    pub kind: String,
    pub every_seconds: Option<u64>,
    pub timezone: Option<String>,
    pub cron: Option<String>,
    pub jitter_seconds: Option<u64>,
    pub max_runtime_seconds: Option<u64>,
}

impl AutonomyScheduleSpec {
    pub fn interval(seconds: u64) -> Self {
        Self {
            version: 1,
            kind: "interval".into(),
            every_seconds: Some(seconds),
            timezone: None,
            cron: None,
            jitter_seconds: Some(0),
            max_runtime_seconds: Some(300),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HeartbeatJobPayload {
    pub version: u32,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub template_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowJobPayload {
    pub version: u32,
    pub workflow_id: String,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotificationJobPayload {
    pub version: u32,
    pub title: String,
    pub body: String,
    pub channel: String,
    pub correlation_scope: String,
    pub draft_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct QuietHoursPolicy {
    pub timezone: String,
    pub start_hour: u8,
    pub end_hour: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InitiativeBudgetPolicy {
    pub max_daily_cost: f64,
    pub max_risk_score: f64,
    pub max_interruptions_per_day: u32,
    pub max_novelty_score: f64,
    pub min_trust_score: f64,
}

impl Default for InitiativeBudgetPolicy {
    fn default() -> Self {
        Self {
            max_daily_cost: 2.0,
            max_risk_score: 1.0,
            max_interruptions_per_day: 5,
            max_novelty_score: 1.0,
            min_trust_score: 0.25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyPolicyDocument {
    pub version: u32,
    pub pause: bool,
    pub draft_only: bool,
    pub approval_required: bool,
    pub quiet_hours: Option<QuietHoursPolicy>,
    pub initiative_budget: InitiativeBudgetPolicy,
    pub retention_days: u32,
}

impl Default for AutonomyPolicyDocument {
    fn default() -> Self {
        Self {
            version: 1,
            pause: false,
            draft_only: false,
            approval_required: false,
            quiet_hours: None,
            initiative_budget: InitiativeBudgetPolicy::default(),
            retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyWhyNow {
    pub trigger_source: String,
    pub due_at: String,
    pub schedule_kind: String,
    pub selected_mode: String,
    pub selected_tier: Option<String>,
    pub convergence_level: Option<i64>,
    pub convergence_score: Option<f64>,
    pub score_delta: Option<f64>,
    pub changed_since_previous_run: Vec<String>,
    pub interruption_justified: bool,
    pub policy_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct AutonomySaturationStatus {
    pub saturated: bool,
    pub reserved_slots: usize,
    pub global_concurrency: usize,
    pub per_agent_concurrency: usize,
    pub blocked_due_jobs: usize,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyStatusResponse {
    pub deployment_mode: String,
    pub runtime_state: String,
    pub scheduler_running: bool,
    pub worker_count: usize,
    pub due_jobs: usize,
    pub leased_jobs: usize,
    pub running_jobs: usize,
    pub waiting_jobs: usize,
    pub paused_jobs: usize,
    pub quarantined_jobs: usize,
    pub manual_review_jobs: usize,
    pub oldest_overdue_at: Option<String>,
    pub last_successful_dispatch_at: Option<String>,
    pub owner_identity: String,
    pub saturation: AutonomySaturationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyJobSummary {
    pub id: String,
    pub job_type: String,
    pub agent_id: String,
    pub workflow_id: Option<String>,
    pub policy_scope: String,
    pub state: String,
    pub next_run_at: String,
    pub schedule_json: String,
    pub current_run_id: Option<String>,
    pub overlap_policy: String,
    pub missed_run_policy: String,
    pub initiative_mode: String,
    pub approval_policy: String,
    pub manual_review_required: bool,
    pub retry_count: i64,
    pub retry_after: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyJobListResponse {
    pub jobs: Vec<AutonomyJobSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyRunSummary {
    pub id: String,
    pub job_id: String,
    pub attempt: i64,
    pub state: String,
    pub trigger_source: String,
    pub due_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub approval_state: String,
    pub side_effect_status: String,
    pub why_now_json: serde_json::Value,
    pub terminal_reason: Option<String>,
    pub manual_review_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomyRunListResponse {
    pub runs: Vec<AutonomyRunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomySuppressionSummary {
    pub id: String,
    pub scope_kind: String,
    pub scope_key: String,
    pub fingerprint: String,
    pub reason: String,
    pub created_by: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub active: bool,
    pub metadata_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomySuppressionsResponse {
    pub suppressions: Vec<AutonomySuppressionSummary>,
}

#[derive(Debug, Clone)]
struct AutonomyRuntimeHealth {
    runtime_state: String,
    scheduler_running: bool,
    worker_count: usize,
    last_successful_dispatch_at: Option<String>,
    saturation: AutonomySaturationStatus,
}

impl Default for AutonomyRuntimeHealth {
    fn default() -> Self {
        Self {
            runtime_state: "stopped".into(),
            scheduler_running: false,
            worker_count: 0,
            last_successful_dispatch_at: None,
            saturation: AutonomySaturationStatus::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct DispatchItem {
    job_id: String,
    run_id: String,
    due_at: String,
    owner_token: String,
    lease_epoch: i64,
    agent_id: String,
}

#[derive(Debug, Clone)]
struct DispatchPolicyState {
    policy: AutonomyPolicyDocument,
    cost_over_budget: bool,
}

pub struct AutonomyService {
    config: AutonomyRuntimeConfig,
    owner_identity: String,
    started: AtomicBool,
    queued_slots: AtomicUsize,
    health: Arc<RwLock<AutonomyRuntimeHealth>>,
    reserved_by_agent: Arc<Mutex<BTreeMap<String, usize>>>,
}

impl AutonomyService {
    pub fn new(config: AutonomyRuntimeConfig) -> Self {
        Self {
            config,
            owner_identity: format!("gateway:{}:autonomy", std::process::id()),
            started: AtomicBool::new(false),
            queued_slots: AtomicUsize::new(0),
            health: Arc::new(RwLock::new(AutonomyRuntimeHealth::default())),
            reserved_by_agent: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub async fn reconcile_bootstrap_jobs(&self, state: &AppState) -> Result<(), String> {
        let agents: Vec<RegisteredAgent> = state
            .agents
            .read()
            .map_err(|_| "agent registry lock poisoned".to_string())?
            .all_agents()
            .into_iter()
            .cloned()
            .collect();
        let mut conn = state.db.write().await;
        for agent in &agents {
            self.ensure_heartbeat_job(&mut conn, agent)?;
        }
        Ok(())
    }

    fn ensure_heartbeat_job(
        &self,
        conn: &mut rusqlite::Connection,
        agent: &RegisteredAgent,
    ) -> Result<(), String> {
        let job_id = heartbeat_job_id(agent.id);
        if cortex_storage::queries::autonomy_queries::get_job(conn, &job_id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(());
        }

        let interval_seconds = if let Some(template_name) = agent.template.as_ref() {
            if template_name.eq_ignore_ascii_case("researcher") {
                120 * 60
            } else if template_name.eq_ignore_ascii_case("developer") {
                60 * 60
            } else {
                DEFAULT_HEARTBEAT_INTERVAL_SECONDS
            }
        } else {
            DEFAULT_HEARTBEAT_INTERVAL_SECONDS
        };
        let payload = serde_json::to_string(&HeartbeatJobPayload {
            version: 1,
            agent_id: agent.id.to_string(),
            agent_name: Some(agent.name.clone()),
            template_name: agent.template.clone(),
        })
        .map_err(|error| error.to_string())?;
        let schedule = serde_json::to_string(&AutonomyScheduleSpec::interval(interval_seconds))
            .map_err(|error| error.to_string())?;
        let retry_policy = serde_json::to_string(&AutonomyRetryPolicy::default())
            .map_err(|error| error.to_string())?;
        let now = Utc::now().to_rfc3339();

        cortex_storage::queries::autonomy_queries::insert_job(
            conn,
            &cortex_storage::queries::autonomy_queries::NewAutonomyJob {
                id: &job_id,
                job_type: "heartbeat_observe",
                agent_id: &agent.id.to_string(),
                tenant_key: "local",
                workflow_id: None,
                policy_scope: &format!("agent:{}", agent.id),
                payload_version: 1,
                payload_json: &payload,
                schedule_version: 1,
                schedule_json: &schedule,
                overlap_policy: "forbid",
                missed_run_policy: "reschedule_from_now",
                retry_policy_json: &retry_policy,
                initiative_mode: "observe",
                approval_policy: "none",
                state: "queued",
                next_run_at: &now,
                created_at: &now,
                updated_at: &now,
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn start(self: Arc<Self>, runtime: &GatewayRuntime, state: Arc<AppState>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::channel::<DispatchItem>(self.config.global_concurrency);
        let shared_rx = Arc::new(tokio::sync::Mutex::new(rx));

        {
            if let Ok(mut health) = self.health.write() {
                health.runtime_state = "starting".into();
                health.worker_count = self.config.global_concurrency;
                health.saturation.global_concurrency = self.config.global_concurrency;
                health.saturation.per_agent_concurrency = self.config.per_agent_concurrency;
            }
        }

        for worker_idx in 0..self.config.global_concurrency {
            let service = Arc::clone(&self);
            let state = Arc::clone(&state);
            let rx = Arc::clone(&shared_rx);
            runtime.spawn_tracked("autonomy_worker", async move {
                service.worker_loop(worker_idx, state, rx).await;
            });
        }

        let service = Arc::clone(&self);
        runtime.spawn_tracked("autonomy_scheduler", async move {
            service.scheduler_loop(state, tx).await;
        });
    }

    pub async fn status(&self, state: &AppState) -> AutonomyStatusResponse {
        let counts = self.compute_db_counts(state).await;
        let health = self
            .health
            .read()
            .map(|health| health.clone())
            .unwrap_or_default();
        AutonomyStatusResponse {
            deployment_mode: "single_gateway_leased".into(),
            runtime_state: health.runtime_state,
            scheduler_running: health.scheduler_running,
            worker_count: health.worker_count,
            due_jobs: counts.due_jobs,
            leased_jobs: counts.leased_jobs,
            running_jobs: counts.running_jobs,
            waiting_jobs: counts.waiting_jobs,
            paused_jobs: counts.paused_jobs,
            quarantined_jobs: counts.quarantined_jobs,
            manual_review_jobs: counts.manual_review_jobs,
            oldest_overdue_at: counts.oldest_overdue_at,
            last_successful_dispatch_at: health.last_successful_dispatch_at,
            owner_identity: self.owner_identity.clone(),
            saturation: health.saturation,
        }
    }

    pub async fn list_jobs(
        &self,
        state: &AppState,
        limit: usize,
    ) -> Result<AutonomyJobListResponse, ApiError> {
        let conn = state
            .db
            .read()
            .map_err(|error| ApiError::db_error("autonomy_list_jobs", error))?;
        let jobs = cortex_storage::queries::autonomy_queries::list_jobs(&conn, limit)
            .map_err(|error| ApiError::db_error("autonomy_list_jobs", error))?
            .into_iter()
            .map(|job| AutonomyJobSummary {
                id: job.id,
                job_type: job.job_type,
                agent_id: job.agent_id,
                workflow_id: job.workflow_id,
                policy_scope: job.policy_scope,
                state: job.state,
                next_run_at: job.next_run_at,
                schedule_json: job.schedule_json,
                current_run_id: job.current_run_id,
                overlap_policy: job.overlap_policy,
                missed_run_policy: job.missed_run_policy,
                initiative_mode: job.initiative_mode,
                approval_policy: job.approval_policy,
                manual_review_required: job.manual_review_required,
                retry_count: job.retry_count,
                retry_after: job.retry_after,
                last_heartbeat_at: job.last_heartbeat_at,
                last_success_at: job.last_success_at,
                last_failure_at: job.last_failure_at,
                terminal_reason: job.terminal_reason,
            })
            .collect();
        Ok(AutonomyJobListResponse { jobs })
    }

    pub async fn list_runs(
        &self,
        state: &AppState,
        limit: usize,
    ) -> Result<AutonomyRunListResponse, ApiError> {
        let conn = state
            .db
            .read()
            .map_err(|error| ApiError::db_error("autonomy_list_runs", error))?;
        let jobs = cortex_storage::queries::autonomy_queries::list_jobs(&conn, limit)
            .map_err(|error| ApiError::db_error("autonomy_list_runs_jobs", error))?;
        let mut runs = Vec::new();
        for job in jobs {
            let recent =
                cortex_storage::queries::autonomy_queries::list_runs_for_job(&conn, &job.id, 1)
                    .map_err(|error| ApiError::db_error("autonomy_list_runs", error))?;
            for run in recent {
                runs.push(AutonomyRunSummary {
                    id: run.id,
                    job_id: run.job_id,
                    attempt: run.attempt,
                    state: run.state,
                    trigger_source: run.trigger_source,
                    due_at: run.due_at,
                    started_at: run.started_at,
                    completed_at: run.completed_at,
                    approval_state: run.approval_state,
                    side_effect_status: run.side_effect_status,
                    why_now_json: serde_json::from_str(&run.why_now_json).unwrap_or_default(),
                    terminal_reason: run.terminal_reason,
                    manual_review_required: run.manual_review_required,
                });
            }
        }
        runs.sort_by(|left, right| right.due_at.cmp(&left.due_at));
        runs.truncate(limit);
        Ok(AutonomyRunListResponse { runs })
    }

    pub async fn get_policy_document(
        &self,
        state: &AppState,
        scope_kind: &str,
        scope_key: &str,
    ) -> Result<AutonomyPolicyDocument, ApiError> {
        self.load_effective_policy(state, scope_kind, scope_key)
            .await
    }

    pub async fn put_policy_document(
        &self,
        state: &AppState,
        scope_kind: &str,
        scope_key: &str,
        policy: &AutonomyPolicyDocument,
        _actor: &str,
    ) -> Result<(), ApiError> {
        let now = Utc::now().to_rfc3339();
        let policy_json = serde_json::to_string(policy)
            .map_err(|error| ApiError::internal(format!("serialize autonomy policy: {error}")))?;
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::upsert_policy(
            &conn,
            &cortex_storage::queries::autonomy_queries::UpsertAutonomyPolicy {
                id: &policy_id(scope_kind, scope_key),
                scope_kind,
                scope_key,
                policy_version: policy.version as i64,
                policy_json: &policy_json,
                created_at: &now,
                updated_at: &now,
            },
        )
        .map_err(|error| ApiError::db_error("autonomy_put_policy", error))?;
        Ok(())
    }

    pub async fn list_suppressions(
        &self,
        state: &AppState,
        scope_kind: &str,
        scope_key: &str,
    ) -> Result<AutonomySuppressionsResponse, ApiError> {
        let now = Utc::now().to_rfc3339();
        let conn = state
            .db
            .read()
            .map_err(|error| ApiError::db_error("autonomy_list_suppressions", error))?;
        let suppressions = cortex_storage::queries::autonomy_queries::list_active_suppressions(
            &conn, scope_kind, scope_key, &now,
        )
        .map_err(|error| ApiError::db_error("autonomy_list_suppressions", error))?
        .into_iter()
        .map(|row| AutonomySuppressionSummary {
            id: row.id,
            scope_kind: row.scope_kind,
            scope_key: row.scope_key,
            fingerprint: row.fingerprint,
            reason: row.reason,
            created_by: row.created_by,
            created_at: row.created_at,
            expires_at: row.expires_at,
            active: row.active,
            metadata_json: serde_json::from_str(&row.metadata_json).unwrap_or_default(),
        })
        .collect();
        Ok(AutonomySuppressionsResponse { suppressions })
    }

    pub async fn create_suppression(
        &self,
        state: &AppState,
        scope_kind: &str,
        scope_key: &str,
        fingerprint: &str,
        reason: &str,
        expires_at: Option<&str>,
        actor: &str,
        metadata: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let now = Utc::now().to_rfc3339();
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::insert_suppression(
            &conn,
            &cortex_storage::queries::autonomy_queries::NewAutonomySuppression {
                id: &Uuid::now_v7().to_string(),
                scope_kind,
                scope_key,
                fingerprint,
                reason,
                created_by: actor,
                created_at: &now,
                expires_at,
                active: true,
                policy_version: 1,
                metadata_json: &serde_json::to_string(metadata).unwrap_or_else(|_| "{}".into()),
            },
        )
        .map_err(|error| ApiError::db_error("autonomy_create_suppression", error))?;
        Ok(())
    }

    pub async fn approve_run(
        &self,
        state: &AppState,
        run_id: &str,
        ttl_seconds: u64,
        actor: &str,
    ) -> Result<(String, String), ApiError> {
        let approved_at = Utc::now();
        let approval_expires_at =
            (approved_at + chrono::Duration::seconds(ttl_seconds as i64)).to_rfc3339();
        let updated_at = approved_at.to_rfc3339();
        let conn = state.db.write().await;
        let changed = cortex_storage::queries::autonomy_queries::approve_run(
            &conn,
            run_id,
            actor,
            &approval_expires_at,
            &updated_at,
        )
        .map_err(|error| ApiError::db_error("autonomy_approve_run", error))?;
        if !changed {
            return Err(ApiError::conflict(format!(
                "autonomy run {run_id} is not pending approval"
            )));
        }
        Ok(("approved".into(), approval_expires_at))
    }

    async fn scheduler_loop(
        self: Arc<Self>,
        state: Arc<AppState>,
        dispatch_tx: tokio::sync::mpsc::Sender<DispatchItem>,
    ) {
        if let Ok(mut health) = self.health.write() {
            health.runtime_state = "running".into();
            health.scheduler_running = true;
        }

        let mut interval = tokio::time::interval(self.config.poll_interval);
        interval.tick().await;

        loop {
            if crate::safety::kill_switch::PLATFORM_KILLED.load(Ordering::SeqCst) {
                if let Ok(mut health) = self.health.write() {
                    health.runtime_state = "killed".into();
                    health.scheduler_running = false;
                }
                break;
            }

            interval.tick().await;
            self.refresh_health(&state).await;
            let now = Utc::now().to_rfc3339();
            let due_jobs = match state.db.read() {
                Ok(conn) => match cortex_storage::queries::autonomy_queries::select_due_jobs(
                    &conn,
                    &now,
                    self.config.max_select_per_tick,
                ) {
                    Ok(jobs) => jobs,
                    Err(error) => {
                        tracing::warn!(error = %error, "autonomy due-job selection failed");
                        continue;
                    }
                },
                Err(error) => {
                    tracing::warn!(error = %error, "autonomy due-job selection could not get read connection");
                    continue;
                }
            };

            let mut blocked_due_jobs = 0usize;
            for job in due_jobs {
                if !self.reserve_slot(&job.agent_id) {
                    blocked_due_jobs += 1;
                    continue;
                }

                match self.prepare_dispatch(&state, &job, &now).await {
                    Ok(Some(item)) => {
                        if dispatch_tx.try_send(item).is_err() {
                            self.release_slot(&job.agent_id);
                            blocked_due_jobs += 1;
                        }
                    }
                    Ok(None) => {
                        self.release_slot(&job.agent_id);
                    }
                    Err(error) => {
                        self.release_slot(&job.agent_id);
                        tracing::warn!(job_id = %job.id, error = %error, "autonomy dispatch prepare failed");
                    }
                }
            }

            if let Ok(mut health) = self.health.write() {
                health.saturation.blocked_due_jobs = blocked_due_jobs;
                health.saturation.reserved_slots = self.queued_slots.load(Ordering::SeqCst);
                health.saturation.saturated = blocked_due_jobs > 0;
                health.saturation.reason = if blocked_due_jobs > 0 {
                    Some("dispatcher saturated or fairness-limited".into())
                } else {
                    None
                };
            }
        }
    }

    async fn worker_loop(
        self: Arc<Self>,
        _worker_idx: usize,
        state: Arc<AppState>,
        rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<DispatchItem>>>,
    ) {
        loop {
            let maybe_item = {
                let mut guard = rx.lock().await;
                guard.recv().await
            };
            let Some(item) = maybe_item else {
                break;
            };

            let result = self.dispatch_item(&state, &item).await;
            self.release_slot(&item.agent_id);
            if let Err(error) = result {
                tracing::warn!(job_id = %item.job_id, run_id = %item.run_id, error = %error, "autonomy dispatch failed");
            } else if let Ok(mut health) = self.health.write() {
                health.last_successful_dispatch_at = Some(Utc::now().to_rfc3339());
            }
            self.refresh_health(&state).await;
        }
    }

    async fn prepare_dispatch(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        now: &str,
    ) -> Result<Option<DispatchItem>, String> {
        let due_at = job
            .retry_after
            .clone()
            .unwrap_or_else(|| job.next_run_at.clone());
        let correlation_key = format!("{}:{}:{}", job.job_type, job.id, due_at);
        let run_id = Uuid::now_v7().to_string();
        let owner_token = Uuid::now_v7().to_string();

        let conn = state.db.write().await;
        let lease = cortex_storage::queries::autonomy_queries::acquire_lease(
            &conn,
            &job.id,
            &run_id,
            &self.owner_identity,
            &owner_token,
            now,
            &(Utc::now() + chrono::Duration::seconds(self.config.lease_ttl.as_secs() as i64))
                .to_rfc3339(),
        )
        .map_err(|error| error.to_string())?;
        let Some(lease) = lease else {
            return Ok(None);
        };

        let existing = cortex_storage::queries::autonomy_queries::get_run_by_side_effect_key(
            &conn,
            &correlation_key,
        )
        .map_err(|error| error.to_string())?;

        let effective_run_id = if let Some(existing) = existing {
            cortex_storage::queries::autonomy_queries::rebind_run_owner(
                &conn,
                &existing.id,
                &job.id,
                &self.owner_identity,
                &owner_token,
                lease.lease_epoch,
                now,
            )
            .map_err(|error| error.to_string())?;
            existing.id
        } else {
            let previous =
                cortex_storage::queries::autonomy_queries::latest_run_for_job(&conn, &job.id)
                    .map_err(|error| error.to_string())?;
            let attempt = previous.map(|run| run.attempt + 1).unwrap_or(0);
            let why_now = serde_json::to_string(&AutonomyWhyNow {
                trigger_source: "schedule".into(),
                due_at: due_at.clone(),
                schedule_kind: schedule_kind(&job.schedule_json),
                selected_mode: job.initiative_mode.clone(),
                selected_tier: None,
                convergence_level: None,
                convergence_score: None,
                score_delta: None,
                changed_since_previous_run: Vec::new(),
                interruption_justified: false,
                policy_reasons: vec!["due_by_schedule".into()],
            })
            .map_err(|error| error.to_string())?;

            cortex_storage::queries::autonomy_queries::insert_run(
                &conn,
                &cortex_storage::queries::autonomy_queries::NewAutonomyRun {
                    id: &run_id,
                    job_id: &job.id,
                    attempt,
                    trigger_source: "schedule",
                    triggered_at: now,
                    due_at: &due_at,
                    state: "leased",
                    why_now_json: &why_now,
                    payload_version: job.payload_version,
                    payload_json: &job.payload_json,
                    initiative_mode: &job.initiative_mode,
                    approval_state: if job.approval_policy == "none" {
                        "not_required"
                    } else {
                        "pending"
                    },
                    approval_proposal_id: None,
                    approval_expires_at: None,
                    owner_identity: Some(&self.owner_identity),
                    owner_token: Some(&owner_token),
                    lease_epoch: lease.lease_epoch,
                    side_effect_correlation_key: Some(&correlation_key),
                    side_effect_status: "not_started",
                    result_json: "{}",
                    created_at: now,
                    updated_at: now,
                },
            )
            .map_err(|error| error.to_string())?;
            run_id
        };

        Ok(Some(DispatchItem {
            job_id: job.id.clone(),
            run_id: effective_run_id,
            due_at,
            owner_token,
            lease_epoch: lease.lease_epoch,
            agent_id: job.agent_id.clone(),
        }))
    }

    async fn dispatch_item(&self, state: &AppState, item: &DispatchItem) -> Result<(), String> {
        let job = {
            let conn = state
                .db
                .read()
                .map_err(|error| format!("autonomy read job: {error}"))?;
            cortex_storage::queries::autonomy_queries::get_job(&conn, &item.job_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("autonomy job {} missing", item.job_id))?
        };

        let run = {
            let conn = state
                .db
                .read()
                .map_err(|error| format!("autonomy read run: {error}"))?;
            cortex_storage::queries::autonomy_queries::get_run(&conn, &item.run_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("autonomy run {} missing", item.run_id))?
        };

        let policy_state = match self
            .enforce_pre_dispatch_controls(state, &job, &run, item)
            .await?
        {
            Some(policy_state) => policy_state,
            None => return Ok(()),
        };

        {
            let conn = state.db.write().await;
            cortex_storage::queries::autonomy_queries::mark_run_running(
                &conn,
                &item.job_id,
                &item.run_id,
                &item.owner_token,
                item.lease_epoch,
                &Utc::now().to_rfc3339(),
            )
            .map_err(|error| error.to_string())?;
        }

        match job.job_type.as_str() {
            "heartbeat_observe" => {
                self.dispatch_heartbeat(state, &job, &run, item, &policy_state)
                    .await
            }
            "workflow_trigger" => {
                self.dispatch_workflow(state, &job, &run, item, &policy_state)
                    .await
            }
            "notification_delivery" => {
                self.dispatch_notification(state, &job, &run, item, &policy_state)
                    .await
            }
            other => {
                self.fail_to_manual_review(
                    state,
                    &item.job_id,
                    &item.run_id,
                    &item.owner_token,
                    item.lease_epoch,
                    &format!("unsupported_job_type:{other}"),
                    "{\"error\":\"unsupported job type\"}",
                )
                .await
            }
        }
    }

    async fn dispatch_workflow(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        run: &cortex_storage::queries::autonomy_queries::AutonomyRunRow,
        item: &DispatchItem,
        policy_state: &DispatchPolicyState,
    ) -> Result<(), String> {
        if policy_state.policy.draft_only {
            return self
                .fail_to_manual_review(
                    state,
                    &item.job_id,
                    &item.run_id,
                    &item.owner_token,
                    item.lease_epoch,
                    "draft_only_policy_blocks_workflow_execution",
                    "{\"status\":\"draft_only\"}",
                )
                .await;
        }
        if policy_state.cost_over_budget {
            return self
                .fail_to_manual_review(
                    state,
                    &item.job_id,
                    &item.run_id,
                    &item.owner_token,
                    item.lease_epoch,
                    "initiative_budget_daily_cost_exceeded",
                    "{\"status\":\"budget_blocked\"}",
                )
                .await;
        }
        let payload: WorkflowJobPayload =
            serde_json::from_str(&job.payload_json).map_err(|error| error.to_string())?;
        let prepared_lease = PreparedOperationLease {
            journal_id: format!("autonomy:{}", run.id),
            owner_token: item.owner_token.clone(),
            lease_epoch: item.lease_epoch,
        };
        let response = crate::api::workflows::execute_workflow_inner(
            Arc::new(state.clone_for_background()),
            payload.workflow_id.clone(),
            crate::api::workflows::ExecuteWorkflowRequest {
                input: payload.input.clone(),
            },
            &prepared_lease,
            &run.id,
        )
        .await;

        match response {
            Ok((status, body)) if status == StatusCode::OK => {
                let result_json =
                    serde_json::to_string(&body).unwrap_or_else(|_| "{\"status\":\"ok\"}".into());
                self.complete_success(state, job, item, &result_json, None)
                    .await
            }
            Ok((_status, body)) => {
                let result_json = serde_json::to_string(&body)
                    .unwrap_or_else(|_| "{\"status\":\"workflow_rejected\"}".into());
                self.retry_or_manual_review(state, job, run, item, "workflow_status", &result_json)
                    .await
            }
            Err(error) => {
                self.retry_or_manual_review(
                    state,
                    job,
                    run,
                    item,
                    "workflow_error",
                    &serde_json::json!({ "error": error.to_string() }).to_string(),
                )
                .await
            }
        }
    }

    async fn dispatch_notification(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        run: &cortex_storage::queries::autonomy_queries::AutonomyRunRow,
        item: &DispatchItem,
        policy_state: &DispatchPolicyState,
    ) -> Result<(), String> {
        let mut payload: NotificationJobPayload =
            serde_json::from_str(&job.payload_json).map_err(|error| error.to_string())?;
        if policy_state.policy.draft_only || policy_state.cost_over_budget {
            payload.draft_only = true;
        }
        let correlation_key = format!("notification:{}:{}", job.id, run.id);
        {
            let conn = state.db.write().await;
            let delivery_state = if payload.draft_only {
                "draft"
            } else {
                "manual_review"
            };
            cortex_storage::queries::autonomy_queries::insert_notification(
                &conn,
                &cortex_storage::queries::autonomy_queries::NewAutonomyNotification {
                    id: &Uuid::now_v7().to_string(),
                    run_id: &run.id,
                    job_id: &job.id,
                    delivery_state,
                    channel: &payload.channel,
                    correlation_key: &correlation_key,
                    payload_json: &job.payload_json,
                    approval_proposal_id: None,
                    last_error: if payload.draft_only {
                        None
                    } else {
                        Some("external delivery is operator-gated")
                    },
                    created_at: &Utc::now().to_rfc3339(),
                    updated_at: &Utc::now().to_rfc3339(),
                },
            )
            .map_err(|error| error.to_string())?;
        }
        self.complete_success(
            state,
            job,
            item,
            &serde_json::json!({
                "title": payload.title,
                "channel": payload.channel,
                "draft_only": payload.draft_only,
            })
            .to_string(),
            None,
        )
        .await
    }

    async fn dispatch_heartbeat(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        run: &cortex_storage::queries::autonomy_queries::AutonomyRunRow,
        item: &DispatchItem,
        policy_state: &DispatchPolicyState,
    ) -> Result<(), String> {
        let payload: HeartbeatJobPayload =
            serde_json::from_str(&job.payload_json).map_err(|error| error.to_string())?;
        let agent_id = Uuid::parse_str(&payload.agent_id).map_err(|error| error.to_string())?;
        let convergence = read_convergence_state(state, agent_id);
        let prior_runs = {
            let conn = state
                .db
                .read()
                .map_err(|error| format!("heartbeat prior runs: {error}"))?;
            cortex_storage::queries::autonomy_queries::list_runs_for_job(&conn, &job.id, 8)
                .map_err(|error| error.to_string())?
        };
        let previous_result = prior_runs
            .iter()
            .find(|candidate| candidate.id != run.id && candidate.state == "succeeded")
            .and_then(|candidate| {
                serde_json::from_str::<serde_json::Value>(&candidate.result_json).ok()
            });
        let previous_score = previous_result
            .as_ref()
            .and_then(|value| value["convergence_score"].as_f64());
        let score_delta = previous_score.map(|score| (convergence.score - score).abs());
        let selected_tier = select_heartbeat_tier(score_delta.unwrap_or(0.0), convergence.level);

        if selected_tier == "h2" && convergence.level >= 2 {
            let notification_payload = serde_json::to_string(&NotificationJobPayload {
                version: 1,
                title: "Heartbeat escalation snapshot".into(),
                body: format!(
                    "Agent {} reached convergence level {} (score {:.4})",
                    payload
                        .agent_name
                        .clone()
                        .unwrap_or_else(|| payload.agent_id.clone()),
                    convergence.level,
                    convergence.score
                ),
                channel: "dashboard".into(),
                correlation_scope: format!("heartbeat:{}", job.id),
                draft_only: true,
            })
            .map_err(|error| error.to_string())?;
            self.enqueue_notification_job(state, job, &notification_payload)
                .await?;
        }

        if selected_tier == "h3" && !policy_state.cost_over_budget {
            let _ = execute_tier3_turn(state, agent_id).await;
        }

        let interval = match selected_tier.as_str() {
            "h0" => HEARTBEAT_TIER0_INTERVAL_SECONDS,
            "h1" => HEARTBEAT_TIER1_INTERVAL_SECONDS,
            "h2" => HEARTBEAT_TIER2_INTERVAL_SECONDS,
            _ => HEARTBEAT_TIER3_INTERVAL_SECONDS,
        };
        let result_json = serde_json::json!({
            "selected_tier": selected_tier,
            "convergence_score": convergence.score,
            "convergence_level": convergence.level,
            "score_delta": score_delta,
            "observed_at": Utc::now().to_rfc3339(),
        })
        .to_string();
        self.complete_success(
            state,
            job,
            item,
            &result_json,
            Some(Utc::now() + chrono::Duration::seconds(interval as i64)),
        )
        .await
    }

    async fn enqueue_notification_job(
        &self,
        state: &AppState,
        source_job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        payload_json: &str,
    ) -> Result<(), String> {
        let id = format!("notification:{}:{}", source_job.id, Uuid::now_v7());
        let schedule = serde_json::to_string(&AutonomyScheduleSpec::interval(24 * 60 * 60))
            .map_err(|error| error.to_string())?;
        let retry_policy = serde_json::to_string(&AutonomyRetryPolicy::default())
            .map_err(|error| error.to_string())?;
        let now = Utc::now().to_rfc3339();
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::insert_job(
            &conn,
            &cortex_storage::queries::autonomy_queries::NewAutonomyJob {
                id: &id,
                job_type: "notification_delivery",
                agent_id: &source_job.agent_id,
                tenant_key: "local",
                workflow_id: None,
                policy_scope: &source_job.policy_scope,
                payload_version: 1,
                payload_json,
                schedule_version: 1,
                schedule_json: &schedule,
                overlap_policy: "queue_one",
                missed_run_policy: "skip",
                retry_policy_json: &retry_policy,
                initiative_mode: "draft",
                approval_policy: "none",
                state: "queued",
                next_run_at: &now,
                created_at: &now,
                updated_at: &now,
            },
        )
        .map_err(|error| error.to_string())
    }

    async fn complete_success(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        item: &DispatchItem,
        result_json: &str,
        explicit_next_run_at: Option<DateTime<Utc>>,
    ) -> Result<(), String> {
        let next_run_at = explicit_next_run_at
            .map(|ts| ts.to_rfc3339())
            .or_else(|| {
                compute_next_run_at(
                    &job.schedule_json,
                    DateTime::parse_from_rfc3339(&item.due_at)
                        .ok()
                        .map(|ts| ts.with_timezone(&Utc))
                        .unwrap_or_else(Utc::now),
                )
            })
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        let heartbeat_at = if job.job_type == "heartbeat_observe" {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        let conn = state.db.write().await;
        if job.job_type == "notification_delivery" {
            cortex_storage::queries::autonomy_queries::finish_run(
                &conn,
                &item.job_id,
                &item.run_id,
                &item.owner_token,
                item.lease_epoch,
                &cortex_storage::queries::autonomy_queries::AutonomyRunFinish {
                    next_state: "succeeded",
                    side_effect_status: "applied",
                    result_json,
                    error_class: None,
                    error_message: None,
                    terminal_reason: None,
                    manual_review_required: false,
                    completed_at: &Utc::now().to_rfc3339(),
                    updated_at: &Utc::now().to_rfc3339(),
                },
            )
            .map_err(|error| error.to_string())?;
            return Ok(());
        }

        cortex_storage::queries::autonomy_queries::complete_run_and_requeue(
            &conn,
            &item.job_id,
            &item.run_id,
            &item.owner_token,
            item.lease_epoch,
            &cortex_storage::queries::autonomy_queries::AutonomyRunCompletionFollowup {
                finish: cortex_storage::queries::autonomy_queries::AutonomyRunFinish {
                    next_state: "succeeded",
                    side_effect_status: "applied",
                    result_json,
                    error_class: None,
                    error_message: None,
                    terminal_reason: None,
                    manual_review_required: false,
                    completed_at: &Utc::now().to_rfc3339(),
                    updated_at: &Utc::now().to_rfc3339(),
                },
                next_job_state: "queued",
                next_run_at: &next_run_at,
                retry_after: None,
                last_heartbeat_at: heartbeat_at.as_deref(),
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn retry_or_manual_review(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        run: &cortex_storage::queries::autonomy_queries::AutonomyRunRow,
        item: &DispatchItem,
        error_class: &str,
        result_json: &str,
    ) -> Result<(), String> {
        let retry_policy = parse_retry_policy(&job.retry_policy_json);
        if (run.attempt as u32) < retry_policy.attempts {
            let next = Utc::now()
                + chrono::Duration::seconds(compute_backoff_seconds(
                    run.attempt as u32,
                    &retry_policy,
                ) as i64);
            let conn = state.db.write().await;
            cortex_storage::queries::autonomy_queries::reschedule_job(
                &conn,
                &item.job_id,
                &item.run_id,
                &item.owner_token,
                item.lease_epoch,
                &cortex_storage::queries::autonomy_queries::AutonomyJobReschedule {
                    run_state: "waiting",
                    job_state: "waiting",
                    next_run_at: &next.to_rfc3339(),
                    waiting_until: Some(&next.to_rfc3339()),
                    side_effect_status: "failed",
                    result_json,
                    error_class: Some(error_class),
                    error_message: Some(error_class),
                    updated_at: &Utc::now().to_rfc3339(),
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        } else {
            self.fail_to_manual_review(
                state,
                &item.job_id,
                &item.run_id,
                &item.owner_token,
                item.lease_epoch,
                error_class,
                result_json,
            )
            .await
        }
    }

    async fn fail_to_manual_review(
        &self,
        state: &AppState,
        job_id: &str,
        run_id: &str,
        owner_token: &str,
        lease_epoch: i64,
        reason: &str,
        result_json: &str,
    ) -> Result<(), String> {
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::finish_run(
            &conn,
            job_id,
            run_id,
            owner_token,
            lease_epoch,
            &cortex_storage::queries::autonomy_queries::AutonomyRunFinish {
                next_state: "failed",
                side_effect_status: "manual_review",
                result_json,
                error_class: Some(reason),
                error_message: Some(reason),
                terminal_reason: Some(reason),
                manual_review_required: true,
                completed_at: &Utc::now().to_rfc3339(),
                updated_at: &Utc::now().to_rfc3339(),
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn refresh_health(&self, state: &AppState) {
        let counts = self.compute_db_counts(state).await;
        if let Ok(mut health) = self.health.write() {
            health.saturation.reserved_slots = self.queued_slots.load(Ordering::SeqCst);
            health.saturation.global_concurrency = self.config.global_concurrency;
            health.saturation.per_agent_concurrency = self.config.per_agent_concurrency;
            if health.runtime_state != "killed" {
                health.runtime_state = "running".into();
                health.scheduler_running = true;
            }
            if counts.manual_review_jobs > 0 && health.saturation.reason.is_none() {
                health.saturation.reason = Some("manual review required".into());
            }
        }
    }

    async fn compute_db_counts(&self, state: &AppState) -> AutonomyDbCounts {
        let now = Utc::now().to_rfc3339();
        let Ok(conn) = state.db.read() else {
            return AutonomyDbCounts::default();
        };
        let due_jobs = count_query(
            &conn,
            "SELECT COUNT(*) FROM autonomy_jobs WHERE manual_review_required = 0 AND state IN ('queued', 'waiting', 'failed') AND COALESCE(retry_after, next_run_at) <= ?1",
            &[&now],
        );
        let leased_jobs = count_query(
            &conn,
            "SELECT COUNT(*) FROM autonomy_leases WHERE lease_expires_at > ?1",
            &[&now],
        );
        let running_jobs = count_query(
            &conn,
            "SELECT COUNT(*) FROM autonomy_jobs WHERE state = 'running'",
            &[],
        );
        let waiting_jobs = count_query(
            &conn,
            "SELECT COUNT(*) FROM autonomy_jobs WHERE state = 'waiting'",
            &[],
        );
        let paused_jobs = count_query(
            &conn,
            "SELECT COUNT(*) FROM autonomy_jobs WHERE state = 'paused'",
            &[],
        );
        let quarantined_jobs = count_query(
            &conn,
            "SELECT COUNT(*) FROM autonomy_jobs WHERE state = 'quarantined'",
            &[],
        );
        let manual_review_jobs = count_query(
            &conn,
            "SELECT COUNT(*) FROM autonomy_jobs WHERE manual_review_required = 1",
            &[],
        );
        let oldest_overdue_at = conn
            .query_row(
                "SELECT MIN(COALESCE(retry_after, next_run_at))
                 FROM autonomy_jobs
                 WHERE manual_review_required = 0
                   AND state IN ('queued', 'waiting', 'failed')
                   AND COALESCE(retry_after, next_run_at) <= ?1",
                [&now],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        AutonomyDbCounts {
            due_jobs,
            leased_jobs,
            running_jobs,
            waiting_jobs,
            paused_jobs,
            quarantined_jobs,
            manual_review_jobs,
            oldest_overdue_at,
        }
    }

    fn reserve_slot(&self, agent_id: &str) -> bool {
        let current_total = self.queued_slots.load(Ordering::SeqCst);
        if current_total >= self.config.global_concurrency {
            return false;
        }
        let mut reserved = self.reserved_by_agent.lock().expect("reserved_by_agent");
        let per_agent = reserved.get(agent_id).copied().unwrap_or(0);
        if per_agent >= self.config.per_agent_concurrency {
            return false;
        }
        reserved.insert(agent_id.to_string(), per_agent + 1);
        self.queued_slots.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn release_slot(&self, agent_id: &str) {
        self.queued_slots.fetch_sub(1, Ordering::SeqCst);
        if let Ok(mut reserved) = self.reserved_by_agent.lock() {
            if let Some(count) = reserved.get_mut(agent_id) {
                if *count > 1 {
                    *count -= 1;
                } else {
                    reserved.remove(agent_id);
                }
            }
        }
    }

    async fn enforce_pre_dispatch_controls(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        run: &cortex_storage::queries::autonomy_queries::AutonomyRunRow,
        item: &DispatchItem,
    ) -> Result<Option<DispatchPolicyState>, String> {
        let policy = self
            .load_effective_policy(state, "agent", &job.agent_id)
            .await
            .map_err(|error| error.to_string())?;
        let now = Utc::now();
        let agent_id = Uuid::parse_str(&job.agent_id).map_err(|error| error.to_string())?;

        if policy.pause {
            self.pause_run(
                state,
                &item.job_id,
                &item.run_id,
                &item.owner_token,
                item.lease_epoch,
                "policy_pause",
            )
            .await?;
            return Ok(None);
        }

        if state
            .quarantine
            .read()
            .map(|manager| manager.get_forensic_state(agent_id).is_some())
            .unwrap_or(false)
        {
            self.quarantine_run(
                state,
                &item.job_id,
                &item.run_id,
                &item.owner_token,
                item.lease_epoch,
                "agent_quarantined",
            )
            .await?;
            return Ok(None);
        }

        let pullback_active = state
            .agents
            .read()
            .ok()
            .and_then(|agents| {
                agents
                    .lookup_by_id(agent_id)
                    .map(|agent| agent.access_pullback_active)
            })
            .unwrap_or(false);
        if pullback_active {
            self.pause_run(
                state,
                &item.job_id,
                &item.run_id,
                &item.owner_token,
                item.lease_epoch,
                "capability_pullback_active",
            )
            .await?;
            return Ok(None);
        }

        if let Some(quiet_hours) = policy.quiet_hours.as_ref() {
            if let Some(release_at) = quiet_hours_release_at(quiet_hours, now) {
                let release_at_rfc3339 = release_at.to_rfc3339();
                self.defer_run(
                    state,
                    item,
                    "quiet_hours",
                    &serde_json::json!({
                        "status": "quiet_hours",
                        "resume_at": release_at_rfc3339,
                    })
                    .to_string(),
                    "waiting",
                    &release_at_rfc3339,
                    None,
                    None,
                )
                .await?;
                return Ok(None);
            }
        }

        let fingerprint = suppression_fingerprint(job);
        let agent_suppressions = self
            .list_suppressions(state, "agent", &job.agent_id)
            .await
            .map_err(|error| error.to_string())?;
        let platform_suppressions = self
            .list_suppressions(state, "platform", "global")
            .await
            .map_err(|error| error.to_string())?;
        let is_suppressed = agent_suppressions
            .suppressions
            .iter()
            .chain(platform_suppressions.suppressions.iter())
            .any(|suppression| {
                suppression.fingerprint == fingerprint || suppression.fingerprint == "*"
            });
        if is_suppressed {
            self.complete_suppressed(
                state,
                job,
                item,
                &serde_json::json!({
                    "status": "suppressed",
                    "fingerprint": fingerprint,
                })
                .to_string(),
            )
            .await?;
            return Ok(None);
        }

        if approval_required(job, &policy) && !approval_is_valid(run, now) {
            let approval_state = if run.approval_state == "approved" {
                Some("expired")
            } else {
                Some("pending")
            };
            let next_attempt = (now + chrono::Duration::seconds(30)).to_rfc3339();
            self.defer_run(
                state,
                item,
                "approval_required",
                &serde_json::json!({
                    "status": "approval_required",
                    "approval_state": approval_state.unwrap_or("pending"),
                })
                .to_string(),
                "waiting",
                &next_attempt,
                approval_state,
                run.approval_expires_at.as_deref(),
            )
            .await?;
            return Ok(None);
        }

        let cost_over_budget =
            state.cost_tracker.get_daily_total(agent_id) > policy.initiative_budget.max_daily_cost;
        Ok(Some(DispatchPolicyState {
            policy,
            cost_over_budget,
        }))
    }

    async fn pause_run(
        &self,
        state: &AppState,
        job_id: &str,
        run_id: &str,
        owner_token: &str,
        lease_epoch: i64,
        reason: &str,
    ) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let result_json = serde_json::json!({ "status": "paused", "reason": reason }).to_string();
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::finish_run(
            &conn,
            job_id,
            run_id,
            owner_token,
            lease_epoch,
            &cortex_storage::queries::autonomy_queries::AutonomyRunFinish {
                next_state: "paused",
                side_effect_status: "aborted",
                result_json: &result_json,
                error_class: Some(reason),
                error_message: Some(reason),
                terminal_reason: Some(reason),
                manual_review_required: false,
                completed_at: &now,
                updated_at: &now,
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn quarantine_run(
        &self,
        state: &AppState,
        job_id: &str,
        run_id: &str,
        owner_token: &str,
        lease_epoch: i64,
        reason: &str,
    ) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let result_json =
            serde_json::json!({ "status": "quarantined", "reason": reason }).to_string();
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::finish_run(
            &conn,
            job_id,
            run_id,
            owner_token,
            lease_epoch,
            &cortex_storage::queries::autonomy_queries::AutonomyRunFinish {
                next_state: "quarantined",
                side_effect_status: "aborted",
                result_json: &result_json,
                error_class: Some(reason),
                error_message: Some(reason),
                terminal_reason: Some(reason),
                manual_review_required: false,
                completed_at: &now,
                updated_at: &now,
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn defer_run(
        &self,
        state: &AppState,
        item: &DispatchItem,
        reason: &str,
        result_json: &str,
        job_state: &str,
        next_run_at: &str,
        approval_state: Option<&str>,
        approval_expires_at: Option<&str>,
    ) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::defer_run(
            &conn,
            &item.job_id,
            &item.run_id,
            &item.owner_token,
            item.lease_epoch,
            &cortex_storage::queries::autonomy_queries::AutonomyRunDeferral {
                run_state: "waiting",
                job_state,
                next_run_at,
                waiting_until: Some(next_run_at),
                side_effect_status: "not_started",
                result_json,
                error_class: Some(reason),
                error_message: Some(reason),
                approval_state,
                approval_expires_at,
                updated_at: &now,
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn complete_suppressed(
        &self,
        state: &AppState,
        job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
        item: &DispatchItem,
        result_json: &str,
    ) -> Result<(), String> {
        let next_run_at = compute_next_run_at(
            &job.schedule_json,
            DateTime::parse_from_rfc3339(&item.due_at)
                .ok()
                .map(|ts| ts.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        )
        .unwrap_or_else(|| Utc::now().to_rfc3339());
        let now = Utc::now().to_rfc3339();
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::complete_run_and_requeue(
            &conn,
            &item.job_id,
            &item.run_id,
            &item.owner_token,
            item.lease_epoch,
            &cortex_storage::queries::autonomy_queries::AutonomyRunCompletionFollowup {
                finish: cortex_storage::queries::autonomy_queries::AutonomyRunFinish {
                    next_state: "succeeded",
                    side_effect_status: "suppressed",
                    result_json,
                    error_class: None,
                    error_message: None,
                    terminal_reason: None,
                    manual_review_required: false,
                    completed_at: &now,
                    updated_at: &now,
                },
                next_job_state: "queued",
                next_run_at: &next_run_at,
                retry_after: None,
                last_heartbeat_at: None,
            },
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn load_effective_policy(
        &self,
        state: &AppState,
        scope_kind: &str,
        scope_key: &str,
    ) -> Result<AutonomyPolicyDocument, ApiError> {
        let conn = state
            .db
            .read()
            .map_err(|error| ApiError::db_error("autonomy_get_policy", error))?;
        if let Some(row) =
            cortex_storage::queries::autonomy_queries::get_policy(&conn, scope_kind, scope_key)
                .map_err(|error| ApiError::db_error("autonomy_get_policy", error))?
        {
            return parse_policy_document(&row.policy_json);
        }
        if scope_kind == "agent" {
            if let Some(row) =
                cortex_storage::queries::autonomy_queries::get_policy(&conn, "platform", "global")
                    .map_err(|error| ApiError::db_error("autonomy_get_policy", error))?
            {
                return parse_policy_document(&row.policy_json);
            }
        }
        Ok(AutonomyPolicyDocument::default())
    }
}

impl Default for AutonomyService {
    fn default() -> Self {
        Self::new(AutonomyRuntimeConfig::default())
    }
}

#[derive(Default)]
struct AutonomyDbCounts {
    due_jobs: usize,
    leased_jobs: usize,
    running_jobs: usize,
    waiting_jobs: usize,
    paused_jobs: usize,
    quarantined_jobs: usize,
    manual_review_jobs: usize,
    oldest_overdue_at: Option<String>,
}

#[derive(Debug, Clone)]
struct ConvergenceSnapshot {
    score: f64,
    level: i64,
}

fn heartbeat_job_id(agent_id: Uuid) -> String {
    format!("heartbeat:{agent_id}")
}

fn count_query(conn: &rusqlite::Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> usize {
    conn.query_row(sql, params, |row| row.get::<_, i64>(0))
        .map(|value| value as usize)
        .unwrap_or(0)
}

fn schedule_kind(schedule_json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(schedule_json)
        .ok()
        .and_then(|value| value["kind"].as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".into())
}

fn compute_next_run_at(schedule_json: &str, from: DateTime<Utc>) -> Option<String> {
    let spec = serde_json::from_str::<AutonomyScheduleSpec>(schedule_json).ok()?;
    if spec.kind == "interval" {
        let every = spec.every_seconds?;
        return Some((from + chrono::Duration::seconds(every as i64)).to_rfc3339());
    }
    None
}

fn parse_retry_policy(raw: &str) -> AutonomyRetryPolicy {
    serde_json::from_str(raw).unwrap_or_default()
}

fn compute_backoff_seconds(attempt: u32, policy: &AutonomyRetryPolicy) -> u64 {
    let capped_attempt = attempt.min(10);
    let base = match policy.backoff.as_str() {
        "linear" => policy
            .min_backoff_seconds
            .saturating_mul(capped_attempt as u64 + 1),
        _ => policy
            .min_backoff_seconds
            .saturating_mul(2_u64.saturating_pow(capped_attempt)),
    };
    base.clamp(policy.min_backoff_seconds, policy.max_backoff_seconds)
}

fn parse_policy_document(raw: &str) -> Result<AutonomyPolicyDocument, ApiError> {
    serde_json::from_str(raw)
        .map_err(|error| ApiError::internal(format!("invalid autonomy policy document: {error}")))
}

fn policy_id(scope_kind: &str, scope_key: &str) -> String {
    format!("autonomy-policy:{scope_kind}:{scope_key}")
}

fn approval_required(
    job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
    policy: &AutonomyPolicyDocument,
) -> bool {
    policy.approval_required || job.approval_policy != "none"
}

fn approval_is_valid(
    run: &cortex_storage::queries::autonomy_queries::AutonomyRunRow,
    now: DateTime<Utc>,
) -> bool {
    if run.approval_state != "approved" {
        return false;
    }
    run.approval_expires_at
        .as_deref()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|ts| ts.with_timezone(&Utc) > now)
        .unwrap_or(false)
}

fn suppression_fingerprint(
    job: &cortex_storage::queries::autonomy_queries::AutonomyJobRow,
) -> String {
    if job.job_type == "notification_delivery" {
        if let Ok(payload) = serde_json::from_str::<NotificationJobPayload>(&job.payload_json) {
            return format!("notification:{}", payload.correlation_scope);
        }
    }
    format!("job_type:{}", job.job_type)
}

fn quiet_hours_release_at(
    quiet_hours: &QuietHoursPolicy,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let offset = parse_fixed_timezone(&quiet_hours.timezone)?;
    let local_now = now.with_timezone(&offset);
    let hour = local_now.hour() as u8;
    let in_window = if quiet_hours.start_hour < quiet_hours.end_hour {
        hour >= quiet_hours.start_hour && hour < quiet_hours.end_hour
    } else {
        hour >= quiet_hours.start_hour || hour < quiet_hours.end_hour
    };
    if !in_window {
        return None;
    }

    let mut target_date = local_now.date_naive();
    if quiet_hours.start_hour >= quiet_hours.end_hour && hour >= quiet_hours.start_hour {
        target_date = target_date.succ_opt()?;
    }

    let resume_local = target_date.and_hms_opt(quiet_hours.end_hour as u32, 0, 0)?;
    offset
        .from_local_datetime(&resume_local)
        .single()
        .map(|ts| ts.with_timezone(&Utc))
}

fn parse_fixed_timezone(raw: &str) -> Option<chrono::FixedOffset> {
    if raw.eq_ignore_ascii_case("utc") || raw == "Z" {
        return chrono::FixedOffset::east_opt(0);
    }
    if raw.len() != 6 || &raw[3..4] != ":" {
        return None;
    }
    let sign = match &raw[..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let hours: i32 = raw[1..3].parse().ok()?;
    let minutes: i32 = raw[4..6].parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    chrono::FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
}

fn read_convergence_state(state: &AppState, agent_id: Uuid) -> ConvergenceSnapshot {
    let state_path = state
        .db
        .db_path()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("convergence_state")
        .join(format!("{agent_id}.json"));
    let Ok(raw) = std::fs::read_to_string(state_path) else {
        return ConvergenceSnapshot {
            score: 0.0,
            level: 0,
        };
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return ConvergenceSnapshot {
            score: 0.0,
            level: 0,
        };
    };
    ConvergenceSnapshot {
        score: value["score"].as_f64().unwrap_or(0.0),
        level: value["level"].as_i64().unwrap_or(0),
    }
}

fn select_heartbeat_tier(score_delta: f64, convergence_level: i64) -> String {
    if convergence_level >= 3 && score_delta >= 0.1 {
        "h3".into()
    } else if convergence_level >= 2 || score_delta >= 0.05 {
        "h2".into()
    } else if score_delta < 0.01 {
        "h0".into()
    } else {
        "h1".into()
    }
}

async fn execute_tier3_turn(state: &AppState, agent_id: Uuid) -> Result<(), String> {
    let resolved = RuntimeSafetyBuilder::new(state)
        .resolve_agent_by_id_or_synthetic(agent_id, "__ghost_autonomy_heartbeat__")
        .map_err(|error| error.to_string())?;
    let session_key = heartbeat_session_key(agent_id);
    let ctx = RuntimeSafetyContext::from_state(state, resolved, session_key, None);
    ctx.ensure_execution_permitted()
        .map_err(|error| error.to_string())?;
    let mut runner = RuntimeSafetyBuilder::new(state)
        .build_live_runner(&ctx, crate::runtime_safety::RunnerBuildOptions::default())
        .map_err(|error| error.to_string())?;
    let providers = crate::provider_runtime::ordered_provider_configs(state);
    if providers.is_empty() {
        return Err("no model providers configured".into());
    }
    let mut fallback_chain = crate::provider_runtime::build_fallback_chain(&providers);
    let mut run_ctx = runner
        .pre_loop(agent_id, session_key, "heartbeat", HEARTBEAT_MESSAGE)
        .await
        .map_err(|error| error.to_string())?;
    runner
        .run_turn(&mut run_ctx, &mut fallback_chain, HEARTBEAT_MESSAGE)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn heartbeat_session_key(agent_id: Uuid) -> Uuid {
    let input = format!("{agent_id}:autonomy-heartbeat:{agent_id}");
    let hash = blake3::hash(input.as_bytes());
    let bytes: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap_or([0; 16]);
    Uuid::from_bytes(bytes)
}

trait CloneForBackground {
    fn clone_for_background(&self) -> Self;
}

impl CloneForBackground for AppState {
    fn clone_for_background(&self) -> Self {
        Self {
            gateway: Arc::clone(&self.gateway),
            config_path: self.config_path.clone(),
            agents: Arc::clone(&self.agents),
            kill_switch: Arc::clone(&self.kill_switch),
            quarantine: Arc::clone(&self.quarantine),
            db: Arc::clone(&self.db),
            event_tx: self.event_tx.clone(),
            trigger_sender: self.trigger_sender.clone(),
            replay_buffer: Arc::clone(&self.replay_buffer),
            cost_tracker: Arc::clone(&self.cost_tracker),
            kill_gate: self.kill_gate.clone(),
            secret_provider: Arc::clone(&self.secret_provider),
            oauth_broker: Arc::clone(&self.oauth_broker),
            mesh_signing_key: self.mesh_signing_key.clone(),
            soul_drift_threshold: self.soul_drift_threshold,
            convergence_profile: self.convergence_profile.clone(),
            model_providers: self.model_providers.clone(),
            default_model_provider: self.default_model_provider.clone(),
            pc_control_circuit_breaker: Arc::clone(&self.pc_control_circuit_breaker),
            websocket_auth_tickets: Arc::clone(&self.websocket_auth_tickets),
            ws_ticket_auth_only: self.ws_ticket_auth_only,
            tools_config: self.tools_config.clone(),
            custom_safety_checks: Arc::clone(&self.custom_safety_checks),
            shutdown_token: self.shutdown_token.clone(),
            background_tasks: Arc::clone(&self.background_tasks),
            live_execution_controls: Arc::clone(&self.live_execution_controls),
            safety_cooldown: Arc::clone(&self.safety_cooldown),
            monitor_address: self.monitor_address.clone(),
            monitor_enabled: self.monitor_enabled,
            monitor_block_on_degraded: self.monitor_block_on_degraded,
            convergence_state_stale_after: self.convergence_state_stale_after,
            monitor_healthy: Arc::clone(&self.monitor_healthy),
            distributed_kill_enabled: self.distributed_kill_enabled,
            embedding_engine: Arc::clone(&self.embedding_engine),
            skill_catalog: Arc::clone(&self.skill_catalog),
            client_heartbeats: Arc::clone(&self.client_heartbeats),
            session_ttl_days: self.session_ttl_days,
            autonomy: Arc::clone(&self.autonomy),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::to_bytes;
    use axum::extract::State;
    use axum::response::IntoResponse;

    async fn test_state(agent_id: Uuid) -> Arc<AppState> {
        let db_path = std::env::temp_dir().join(format!("autonomy-tests-{}.db", Uuid::now_v7()));
        let db = crate::db_pool::create_pool(db_path).unwrap();
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }

        let mut registry = crate::agents::registry::AgentRegistry::new();
        registry.register(crate::agents::registry::RegisteredAgent {
            id: agent_id,
            name: "autonomy-test-agent".into(),
            state: crate::agents::registry::AgentLifecycleState::Ready,
            channel_bindings: Vec::new(),
            full_access: false,
            capabilities: vec!["skill:echo".into()],
            skills: None,
            baseline_capabilities: vec!["skill:echo".into()],
            baseline_skills: None,
            access_pullback_active: false,
            spending_cap: 10.0,
            template: Some("developer".into()),
        });

        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let token_store =
            ghost_oauth::TokenStore::with_default_dir(Box::new(ghost_secrets::EnvProvider));
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(
            std::collections::BTreeMap::new(),
            token_store,
        ));
        let embedding_engine =
            cortex_embeddings::EmbeddingEngine::new(cortex_embeddings::EmbeddingConfig::default());

        Arc::new(AppState {
            gateway: Arc::new(crate::gateway::GatewaySharedState::new()),
            config_path: std::path::PathBuf::from("ghost.yml"),
            agents: Arc::new(RwLock::new(registry)),
            kill_switch: Arc::new(crate::safety::kill_switch::KillSwitch::new()),
            quarantine: Arc::new(RwLock::new(
                crate::safety::quarantine::QuarantineManager::new(),
            )),
            db: Arc::clone(&db),
            event_tx,
            trigger_sender:
                tokio::sync::mpsc::channel::<cortex_core::safety::trigger::TriggerEvent>(16).0,
            replay_buffer: Arc::new(crate::api::websocket::EventReplayBuffer::new(16)),
            cost_tracker: Arc::new(crate::cost::tracker::CostTracker::new()),
            kill_gate: None,
            secret_provider: Arc::new(ghost_secrets::EnvProvider),
            oauth_broker,
            mesh_signing_key: None,
            soul_drift_threshold: 0.15,
            convergence_profile: "standard".into(),
            model_providers: Vec::new(),
            default_model_provider: None,
            pc_control_circuit_breaker: ghost_pc_control::safety::PcControlConfig::default()
                .circuit_breaker(),
            websocket_auth_tickets: Arc::new(dashmap::DashMap::new()),
            ws_ticket_auth_only: false,
            tools_config: crate::config::ToolsConfig::default(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            live_execution_controls: Arc::new(dashmap::DashMap::new()),
            safety_cooldown: Arc::new(crate::api::rate_limit::SafetyCooldown::new()),
            monitor_address: "127.0.0.1:0".into(),
            monitor_enabled: false,
            monitor_block_on_degraded: false,
            convergence_state_stale_after: std::time::Duration::from_secs(300),
            monitor_healthy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            distributed_kill_enabled: false,
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            skill_catalog: Arc::new(crate::skill_catalog::SkillCatalogService::empty_for_tests(
                Arc::clone(&db),
            )),
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: 90,
            autonomy: Arc::new(AutonomyService::default()),
        })
    }

    async fn insert_job(
        state: &AppState,
        job_id: &str,
        agent_id: Uuid,
        job_type: &str,
        approval_policy: &str,
    ) -> cortex_storage::queries::autonomy_queries::AutonomyJobRow {
        let payload = if job_type == "workflow_trigger" {
            serde_json::to_string(&WorkflowJobPayload {
                version: 1,
                workflow_id: "wf-test".into(),
                input: None,
            })
            .unwrap()
        } else {
            "{\"version\":1}".to_string()
        };
        let schedule = serde_json::to_string(&AutonomyScheduleSpec::interval(60)).unwrap();
        let retry = serde_json::to_string(&AutonomyRetryPolicy::default()).unwrap();
        let now = Utc::now().to_rfc3339();
        let conn = state.db.write().await;
        cortex_storage::queries::autonomy_queries::insert_job(
            &conn,
            &cortex_storage::queries::autonomy_queries::NewAutonomyJob {
                id: job_id,
                job_type,
                agent_id: &agent_id.to_string(),
                tenant_key: "local",
                workflow_id: None,
                policy_scope: &format!("agent:{agent_id}"),
                payload_version: 1,
                payload_json: &payload,
                schedule_version: 1,
                schedule_json: &schedule,
                overlap_policy: "forbid",
                missed_run_policy: "reschedule_from_now",
                retry_policy_json: &retry,
                initiative_mode: "act",
                approval_policy,
                state: "queued",
                next_run_at: &now,
                created_at: &now,
                updated_at: &now,
            },
        )
        .unwrap();
        cortex_storage::queries::autonomy_queries::get_job(&conn, job_id)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn dispatcher_respects_per_agent_concurrency_limit() {
        let service = AutonomyService::new(AutonomyRuntimeConfig {
            global_concurrency: 2,
            per_agent_concurrency: 1,
            ..AutonomyRuntimeConfig::default()
        });

        assert!(service.reserve_slot("agent-a"));
        assert!(!service.reserve_slot("agent-a"));
        assert!(service.reserve_slot("agent-b"));
        assert!(!service.reserve_slot("agent-c"));
        service.release_slot("agent-a");
        assert!(service.reserve_slot("agent-c"));
    }

    #[tokio::test]
    async fn due_jobs_dispatch_under_single_owner() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let service = AutonomyService::default();
        let job = insert_job(
            &state,
            "job-single-owner",
            agent_id,
            "heartbeat_observe",
            "none",
        )
        .await;
        let now = Utc::now().to_rfc3339();

        let first = service.prepare_dispatch(&state, &job, &now).await.unwrap();
        let second = service.prepare_dispatch(&state, &job, &now).await.unwrap();

        assert!(first.is_some());
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn approval_required_job_blocks_side_effects() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let service = AutonomyService::default();
        let job = insert_job(
            &state,
            "job-approval",
            agent_id,
            "workflow_trigger",
            "always",
        )
        .await;
        let now = Utc::now().to_rfc3339();
        let item = service
            .prepare_dispatch(&state, &job, &now)
            .await
            .unwrap()
            .unwrap();
        let conn = state.db.read().unwrap();
        let run = cortex_storage::queries::autonomy_queries::get_run(&conn, &item.run_id)
            .unwrap()
            .unwrap();
        drop(conn);

        let decision = service
            .enforce_pre_dispatch_controls(&state, &job, &run, &item)
            .await
            .unwrap();
        assert!(decision.is_none());

        let conn = state.db.read().unwrap();
        let job_after = cortex_storage::queries::autonomy_queries::get_job(&conn, &job.id)
            .unwrap()
            .unwrap();
        let run_after = cortex_storage::queries::autonomy_queries::get_run(&conn, &item.run_id)
            .unwrap()
            .unwrap();
        assert_eq!(job_after.state, "waiting");
        assert_eq!(run_after.approval_state, "pending");
        assert_eq!(run_after.state, "waiting");
    }

    #[tokio::test]
    async fn quarantine_prevents_due_job_dispatch() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let service = AutonomyService::default();
        {
            state.quarantine.write().unwrap().quarantine(
                agent_id,
                "test quarantine".into(),
                Vec::new(),
                serde_json::json!({}),
                Vec::new(),
            );
        }
        let job = insert_job(
            &state,
            "job-quarantine",
            agent_id,
            "heartbeat_observe",
            "none",
        )
        .await;
        let now = Utc::now().to_rfc3339();
        let item = service
            .prepare_dispatch(&state, &job, &now)
            .await
            .unwrap()
            .unwrap();
        let conn = state.db.read().unwrap();
        let run = cortex_storage::queries::autonomy_queries::get_run(&conn, &item.run_id)
            .unwrap()
            .unwrap();
        drop(conn);

        let decision = service
            .enforce_pre_dispatch_controls(&state, &job, &run, &item)
            .await
            .unwrap();
        assert!(decision.is_none());

        let conn = state.db.read().unwrap();
        let job_after = cortex_storage::queries::autonomy_queries::get_job(&conn, &job.id)
            .unwrap()
            .unwrap();
        assert_eq!(job_after.state, "quarantined");
    }

    #[tokio::test]
    async fn delayed_approval_revalidates_policy_and_budget() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let service = AutonomyService::default();
        let job = insert_job(
            &state,
            "job-revalidate",
            agent_id,
            "workflow_trigger",
            "always",
        )
        .await;
        let now = Utc::now().to_rfc3339();
        let item = service
            .prepare_dispatch(&state, &job, &now)
            .await
            .unwrap()
            .unwrap();

        service
            .approve_run(&state, &item.run_id, 600, "tester")
            .await
            .unwrap();
        service
            .put_policy_document(
                &state,
                "platform",
                "global",
                &AutonomyPolicyDocument {
                    pause: true,
                    ..AutonomyPolicyDocument::default()
                },
                "tester",
            )
            .await
            .unwrap();

        let conn = state.db.read().unwrap();
        let run = cortex_storage::queries::autonomy_queries::get_run(&conn, &item.run_id)
            .unwrap()
            .unwrap();
        drop(conn);

        let decision = service
            .enforce_pre_dispatch_controls(&state, &job, &run, &item)
            .await
            .unwrap();
        assert!(decision.is_none());

        let conn = state.db.read().unwrap();
        let job_after = cortex_storage::queries::autonomy_queries::get_job(&conn, &job.id)
            .unwrap()
            .unwrap();
        assert_eq!(job_after.state, "paused");
    }

    #[tokio::test]
    async fn user_suppression_reduces_future_initiative() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let service = AutonomyService::default();
        service
            .create_suppression(
                &state,
                "agent",
                &agent_id.to_string(),
                "notification:test",
                "user rejected proactive notification",
                None,
                "tester",
                &serde_json::json!({ "source": "test" }),
            )
            .await
            .unwrap();
        let payload = serde_json::to_string(&NotificationJobPayload {
            version: 1,
            title: "hello".into(),
            body: "world".into(),
            channel: "dashboard".into(),
            correlation_scope: "test".into(),
            draft_only: false,
        })
        .unwrap();
        let schedule = serde_json::to_string(&AutonomyScheduleSpec::interval(60)).unwrap();
        let retry = serde_json::to_string(&AutonomyRetryPolicy::default()).unwrap();
        let now = Utc::now().to_rfc3339();
        {
            let conn = state.db.write().await;
            cortex_storage::queries::autonomy_queries::insert_job(
                &conn,
                &cortex_storage::queries::autonomy_queries::NewAutonomyJob {
                    id: "job-suppressed",
                    job_type: "notification_delivery",
                    agent_id: &agent_id.to_string(),
                    tenant_key: "local",
                    workflow_id: None,
                    policy_scope: &format!("agent:{agent_id}"),
                    payload_version: 1,
                    payload_json: &payload,
                    schedule_version: 1,
                    schedule_json: &schedule,
                    overlap_policy: "forbid",
                    missed_run_policy: "reschedule_from_now",
                    retry_policy_json: &retry,
                    initiative_mode: "act",
                    approval_policy: "none",
                    state: "queued",
                    next_run_at: &now,
                    created_at: &now,
                    updated_at: &now,
                },
            )
            .unwrap();
        }
        let conn = state.db.read().unwrap();
        let job = cortex_storage::queries::autonomy_queries::get_job(&conn, "job-suppressed")
            .unwrap()
            .unwrap();
        drop(conn);
        let item = service
            .prepare_dispatch(&state, &job, &now)
            .await
            .unwrap()
            .unwrap();
        let conn = state.db.read().unwrap();
        let run = cortex_storage::queries::autonomy_queries::get_run(&conn, &item.run_id)
            .unwrap()
            .unwrap();
        drop(conn);

        let decision = service
            .enforce_pre_dispatch_controls(&state, &job, &run, &item)
            .await
            .unwrap();
        assert!(decision.is_none());

        let conn = state.db.read().unwrap();
        let run_after = cortex_storage::queries::autonomy_queries::get_run(&conn, &item.run_id)
            .unwrap()
            .unwrap();
        assert_eq!(run_after.side_effect_status, "suppressed");
    }

    #[tokio::test]
    async fn poison_job_moves_to_manual_review_visible_in_health() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let service = AutonomyService::default();
        let job = insert_job(&state, "job-poison", agent_id, "unsupported_kind", "none").await;
        let now = Utc::now().to_rfc3339();
        let item = service
            .prepare_dispatch(&state, &job, &now)
            .await
            .unwrap()
            .unwrap();

        service.dispatch_item(&state, &item).await.unwrap();

        let response = crate::api::health::health_handler(State(Arc::clone(&state)))
            .await
            .into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["autonomy"]["manual_review_jobs"], 1);
    }

    #[tokio::test]
    async fn legacy_schedule_backfill_preserves_due_work() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let service = AutonomyService::default();

        service.reconcile_bootstrap_jobs(&state).await.unwrap();

        let conn = state.db.read().unwrap();
        let heartbeat_job =
            cortex_storage::queries::autonomy_queries::get_job(&conn, &heartbeat_job_id(agent_id))
                .unwrap()
                .unwrap();
        assert_eq!(heartbeat_job.job_type, "heartbeat_observe");
        assert_eq!(heartbeat_job.state, "queued");
    }

    #[tokio::test]
    async fn health_autonomy_section_matches_runtime_state() {
        let agent_id = Uuid::now_v7();
        let state = test_state(agent_id).await;
        let status = state.autonomy.status(&state).await;

        let response = crate::api::health::health_handler(State(Arc::clone(&state)))
            .await
            .into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["autonomy"]["runtime_state"], status.runtime_state);
        assert_eq!(value["autonomy"]["worker_count"], status.worker_count);
        assert_eq!(value["autonomy"]["deployment_mode"], status.deployment_mode);
    }
}
