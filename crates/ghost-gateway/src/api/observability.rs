use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::agents::registry::AgentLifecycleState;
use crate::gateway::GatewayState;
use crate::runtime_status::{convergence_protection_summary_value, distributed_kill_status_value};
use crate::state::{AppState, RuntimeSubsystemStatus};

const MONITOR_STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const MONITOR_STATUS_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeObservabilitySnapshot {
    pub sampled_at: String,
    pub stale: bool,
    pub status: String,
    pub gateway: AdeGatewaySnapshot,
    pub monitor: AdeMonitorSnapshot,
    pub agents: AdeAgentSnapshot,
    pub websocket: AdeWebSocketSnapshot,
    pub database: AdeDatabaseSnapshot,
    pub backup_scheduler: AdeBackupSchedulerSnapshot,
    pub config_watcher: AdeConfigWatcherSnapshot,
    pub autonomy: crate::autonomy::AutonomyStatusResponse,
    pub convergence_protection: AdeConvergenceProtectionSnapshot,
    pub distributed_kill: AdeDistributedKillSnapshot,
    pub speculative_context: crate::speculative_context::SpeculativeContextStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeGatewaySnapshot {
    pub liveness: String,
    pub readiness: String,
    pub state: String,
    pub uptime_seconds: u64,
    pub platform_killed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeMonitorSnapshot {
    pub enabled: bool,
    pub connected: bool,
    pub status: String,
    pub uptime_seconds: Option<u64>,
    pub agent_count: Option<usize>,
    pub event_count: Option<u64>,
    pub last_computation: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeAgentSnapshot {
    pub active_count: usize,
    pub registered_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeWebSocketSnapshot {
    pub active_connections: u32,
    pub per_ip_limit: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeDatabaseSnapshot {
    pub path: Option<String>,
    pub size_bytes: Option<u64>,
    pub wal_mode: Option<bool>,
    pub status: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeBackupSchedulerSnapshot {
    pub enabled: bool,
    pub status: String,
    pub retention_days: u64,
    pub schedule: String,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeConfigWatcherSnapshot {
    pub enabled: bool,
    pub status: String,
    pub watched_path: Option<String>,
    pub mode: Option<String>,
    pub last_reload_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeConvergenceProtectionAgents {
    pub healthy: usize,
    pub missing: usize,
    pub stale: usize,
    pub corrupted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeConvergenceProtectionSnapshot {
    pub execution_mode: String,
    pub stale_after_secs: u64,
    pub agents: AdeConvergenceProtectionAgents,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdeDistributedKillSnapshot {
    pub enabled: bool,
    pub status: String,
    pub authoritative: bool,
    pub resume_permitted: Option<bool>,
    pub node_id: Option<String>,
    pub closed_at: Option<String>,
    pub close_reason: Option<String>,
    pub acked_nodes: Option<Vec<String>>,
    pub chain_length: Option<u64>,
    pub reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MonitorStatusResponse {
    status: String,
    agent_count: usize,
    event_count: u64,
    last_computation: Option<String>,
    uptime_seconds: u64,
}

pub async fn ade_handler(State(state): State<Arc<AppState>>) -> Json<AdeObservabilitySnapshot> {
    let sampled_at = Utc::now();
    let gw_state = state.gateway.current_state();
    let liveness = match gw_state {
        GatewayState::Healthy | GatewayState::Degraded | GatewayState::Recovering => "alive",
        GatewayState::Initializing | GatewayState::ShuttingDown | GatewayState::FatalError => {
            "unavailable"
        }
    };
    let readiness = if gw_state == GatewayState::Healthy {
        "ready"
    } else {
        "not_ready"
    };
    let status = match gw_state {
        GatewayState::Healthy => "healthy",
        GatewayState::Degraded => "degraded",
        GatewayState::Recovering => "recovering",
        GatewayState::Initializing | GatewayState::ShuttingDown | GatewayState::FatalError => {
            "unavailable"
        }
    };

    let (registered_count, active_count, agent_ids) = match state.agents.read() {
        Ok(agents) => {
            let all = agents.all_agents();
            let registered_count = all.len();
            let active_count = all
                .iter()
                .filter(|agent| {
                    matches!(
                        agent.state,
                        AgentLifecycleState::Starting | AgentLifecycleState::Ready
                    )
                })
                .count();
            let agent_ids = all.iter().map(|agent| agent.id).collect::<Vec<_>>();
            (registered_count, active_count, agent_ids)
        }
        Err(_) => (0, 0, Vec::new()),
    };

    let monitor_runtime = state
        .monitor_runtime_status
        .read()
        .map(|status| status.clone())
        .unwrap_or_default();
    let monitor_connected = state
        .monitor_healthy
        .load(std::sync::atomic::Ordering::Relaxed);
    let monitor_sample_age = monitor_runtime
        .sampled_at
        .as_deref()
        .and_then(parse_rfc3339_utc)
        .map(|ts| sampled_at.signed_duration_since(ts).num_seconds().max(0) as u64);
    let stale = state.monitor_enabled
        && monitor_sample_age
            .map(|age| age > (MONITOR_STATUS_REFRESH_INTERVAL.as_secs() * 2))
            .unwrap_or(true);

    let monitor_status = if !state.monitor_enabled {
        "disabled"
    } else if stale {
        "unreachable"
    } else if monitor_connected {
        "running"
    } else {
        "degraded"
    };

    let ws_tracker = Arc::clone(&state.ws_connection_tracker);
    let websocket = AdeWebSocketSnapshot {
        active_connections: ws_tracker.total_connections(),
        per_ip_limit: ws_tracker.per_ip_limit(),
        status: "healthy".into(),
    };

    let database = load_database_snapshot(&state);

    let backup_runtime = state
        .backup_scheduler_status
        .read()
        .map(|status| status.clone())
        .unwrap_or_default();
    let config_runtime = state
        .config_watcher_status
        .read()
        .map(|status| status.clone())
        .unwrap_or_default();

    let autonomy = state.autonomy.status(&state).await;
    let speculative_context = crate::speculative_context::status(&state).await;

    Json(AdeObservabilitySnapshot {
        sampled_at: sampled_at.to_rfc3339(),
        stale,
        status: status.into(),
        gateway: AdeGatewaySnapshot {
            liveness: liveness.into(),
            readiness: readiness.into(),
            state: format!("{gw_state:?}"),
            uptime_seconds: state.started_at.elapsed().as_secs(),
            platform_killed: crate::safety::kill_switch::PLATFORM_KILLED
                .load(std::sync::atomic::Ordering::SeqCst),
        },
        monitor: AdeMonitorSnapshot {
            enabled: state.monitor_enabled,
            connected: monitor_connected,
            status: monitor_status.into(),
            uptime_seconds: monitor_runtime.uptime_seconds,
            agent_count: monitor_runtime.agent_count,
            event_count: monitor_runtime.event_count,
            last_computation: monitor_runtime.last_computation,
            last_error: monitor_runtime.last_error,
        },
        agents: AdeAgentSnapshot {
            active_count,
            registered_count,
        },
        websocket,
        database,
        backup_scheduler: AdeBackupSchedulerSnapshot {
            enabled: backup_runtime.enabled,
            status: runtime_subsystem_status_label(backup_runtime.status).into(),
            retention_days: backup_retention_days(),
            schedule: "daily at 03:00 UTC".into(),
            last_success_at: backup_runtime.last_success_at,
            last_failure_at: backup_runtime.last_failure_at,
            last_error: backup_runtime.last_error,
        },
        config_watcher: AdeConfigWatcherSnapshot {
            enabled: config_runtime.enabled,
            status: runtime_subsystem_status_label(config_runtime.status).into(),
            watched_path: config_runtime.watched_path,
            mode: config_runtime.mode,
            last_reload_at: config_runtime.last_reload_at,
            last_error: config_runtime.last_error,
        },
        autonomy,
        convergence_protection: convergence_protection_snapshot(
            agent_ids,
            state.monitor_enabled,
            state.monitor_block_on_degraded,
            state.convergence_state_stale_after,
        ),
        distributed_kill: distributed_kill_snapshot(
            state.distributed_kill_enabled,
            state.kill_gate.as_ref(),
        ),
        speculative_context,
    })
}

pub async fn monitor_status_snapshot_task(state: Arc<AppState>) {
    let client = reqwest::Client::new();
    let url = format!("http://{}/status", state.monitor_address);
    let mut interval = tokio::time::interval(MONITOR_STATUS_REFRESH_INTERVAL);

    loop {
        interval.tick().await;
        let sampled_at = Utc::now().to_rfc3339();
        let result = tokio::time::timeout(MONITOR_STATUS_TIMEOUT, client.get(&url).send()).await;

        match result {
            Ok(Ok(resp)) if resp.status().is_success() => {
                match resp.json::<MonitorStatusResponse>().await {
                    Ok(payload) => {
                        if let Ok(mut snapshot) = state.monitor_runtime_status.write() {
                            snapshot.sampled_at = Some(sampled_at);
                            snapshot.connected = payload.status == "running";
                            snapshot.uptime_seconds = Some(payload.uptime_seconds);
                            snapshot.agent_count = Some(payload.agent_count);
                            snapshot.event_count = Some(payload.event_count);
                            snapshot.last_computation = payload.last_computation;
                            snapshot.last_error = None;
                        }
                    }
                    Err(error) => {
                        record_monitor_snapshot_error(
                            &state,
                            sampled_at,
                            format!("Invalid monitor /status payload: {error}"),
                        );
                    }
                }
            }
            Ok(Ok(resp)) => {
                record_monitor_snapshot_error(
                    &state,
                    sampled_at,
                    format!("Monitor /status returned HTTP {}", resp.status()),
                );
            }
            Ok(Err(error)) => {
                record_monitor_snapshot_error(
                    &state,
                    sampled_at,
                    format!("Monitor /status request failed: {error}"),
                );
            }
            Err(_) => {
                record_monitor_snapshot_error(
                    &state,
                    sampled_at,
                    format!(
                        "Monitor /status timed out after {}s",
                        MONITOR_STATUS_TIMEOUT.as_secs()
                    ),
                );
            }
        }
    }
}

fn record_monitor_snapshot_error(state: &Arc<AppState>, sampled_at: String, error: String) {
    if let Ok(mut snapshot) = state.monitor_runtime_status.write() {
        snapshot.sampled_at = Some(sampled_at);
        snapshot.connected = false;
        snapshot.last_error = Some(error);
    }
}

fn load_database_snapshot(state: &Arc<AppState>) -> AdeDatabaseSnapshot {
    let db_path = state.db.db_path().to_path_buf();
    let path = Some(db_path.display().to_string());
    let size_bytes = std::fs::metadata(&db_path).ok().map(|meta| meta.len());

    let read = match state.db.read() {
        Ok(read) => read,
        Err(error) => {
            return AdeDatabaseSnapshot {
                path,
                size_bytes,
                wal_mode: None,
                status: "unavailable".into(),
                last_error: Some(format!("DB read acquisition failed: {error}")),
            };
        }
    };

    match read.query_row("PRAGMA journal_mode;", [], |row| row.get::<_, String>(0)) {
        Ok(mode) => {
            let wal_mode = mode.eq_ignore_ascii_case("wal");
            AdeDatabaseSnapshot {
                path,
                size_bytes,
                wal_mode: Some(wal_mode),
                status: if wal_mode {
                    "healthy".into()
                } else {
                    "degraded".into()
                },
                last_error: if wal_mode {
                    None
                } else {
                    Some(format!("Unexpected journal_mode: {mode}"))
                },
            }
        }
        Err(error) => AdeDatabaseSnapshot {
            path,
            size_bytes,
            wal_mode: None,
            status: "degraded".into(),
            last_error: Some(format!("Failed to query journal_mode: {error}")),
        },
    }
}

fn backup_retention_days() -> u64 {
    std::env::var("GHOST_BACKUP_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30)
}

pub fn convergence_protection_snapshot(
    agent_ids: impl IntoIterator<Item = uuid::Uuid>,
    monitor_enabled: bool,
    block_on_degraded: bool,
    stale_after: std::time::Duration,
) -> AdeConvergenceProtectionSnapshot {
    let value = convergence_protection_summary_value(
        agent_ids,
        monitor_enabled,
        block_on_degraded,
        stale_after,
    );
    serde_json::from_value(value).unwrap_or_else(|_| AdeConvergenceProtectionSnapshot {
        execution_mode: if block_on_degraded {
            "block".into()
        } else {
            "allow".into()
        },
        stale_after_secs: stale_after.as_secs(),
        agents: AdeConvergenceProtectionAgents {
            healthy: 0,
            missing: 0,
            stale: 0,
            corrupted: 0,
        },
    })
}

pub fn distributed_kill_snapshot(
    distributed_kill_enabled: bool,
    kill_gate: Option<&Arc<std::sync::RwLock<crate::safety::kill_gate_bridge::KillGateBridge>>>,
) -> AdeDistributedKillSnapshot {
    let value = distributed_kill_status_value(distributed_kill_enabled, kill_gate);
    serde_json::from_value(value).unwrap_or_else(|_| AdeDistributedKillSnapshot {
        enabled: distributed_kill_enabled,
        status: "unavailable".into(),
        authoritative: false,
        resume_permitted: None,
        node_id: None,
        closed_at: None,
        close_reason: None,
        acked_nodes: None,
        chain_length: None,
        reason: Some("failed to decode distributed kill status".into()),
        error: None,
    })
}

fn runtime_subsystem_status_label(status: RuntimeSubsystemStatus) -> &'static str {
    match status {
        RuntimeSubsystemStatus::Healthy => "healthy",
        RuntimeSubsystemStatus::Degraded => "degraded",
        RuntimeSubsystemStatus::Disabled => "disabled",
        RuntimeSubsystemStatus::Unavailable => "unavailable",
    }
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}
