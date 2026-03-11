//! Agent API endpoints.
//!
//! The ADE agent surface is driven by backend-owned read models rather than
//! client-side stitching of unrelated list endpoints.

use std::sync::Arc;

use axum::extract::Query;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use ghost_audit::query_engine::{AuditEntry, AuditFilter, AuditQueryEngine};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::agents::registry::{AgentLifecycleState, RegisteredAgent};
use crate::api::convergence::ConvergenceScoreResponse;
use crate::api::integrity::{
    IntegrityChains, IntegrityEventId, ItpEventsIntegrity, MemoryEventsIntegrity,
    VerifyChainResponse,
};
use crate::api::sessions::RuntimeSessionSummary;
use crate::api::state::{CrdtDelta, CrdtStateResponse};
use crate::api::websocket::WsEvent;
use crate::safety::kill_switch::{KillLevel, KillSwitchState};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycleStateValue {
    Starting,
    Ready,
    Stopping,
    Stopped,
}

impl From<AgentLifecycleState> for AgentLifecycleStateValue {
    fn from(value: AgentLifecycleState) -> Self {
        match value {
            AgentLifecycleState::Starting => Self::Starting,
            AgentLifecycleState::Ready => Self::Ready,
            AgentLifecycleState::Stopping => Self::Stopping,
            AgentLifecycleState::Stopped => Self::Stopped,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSafetyStateValue {
    Normal,
    Paused,
    Quarantined,
    KillAllBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentEffectiveStateValue {
    Starting,
    Ready,
    Paused,
    Quarantined,
    KillAllBlocked,
    Stopping,
    Stopped,
}

impl AgentEffectiveStateValue {
    pub fn as_status_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Paused => "paused",
            Self::Quarantined => "quarantined",
            Self::KillAllBlocked => "kill_all_blocked",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentResumeKind {
    Pause,
    Quarantine,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgentActionPolicy {
    pub can_pause: bool,
    pub can_quarantine: bool,
    pub can_resume: bool,
    pub can_delete: bool,
    pub resume_kind: Option<AgentResumeKind>,
    pub requires_forensic_review: bool,
    pub requires_second_confirmation: bool,
    pub monitoring_duration_hours: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    /// Compatibility field for older consumers. Mirrors `effective_state`.
    pub status: String,
    pub lifecycle_state: AgentLifecycleStateValue,
    pub safety_state: AgentSafetyStateValue,
    pub effective_state: AgentEffectiveStateValue,
    pub spending_cap: f64,
    pub isolation: crate::config::IsolationMode,
    pub capabilities: Vec<String>,
    pub sandbox: crate::config::AgentSandboxConfig,
    pub sandbox_metrics: crate::sandbox_reviews::AgentSandboxMetrics,
    pub action_policy: AgentActionPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_keypair: Option<bool>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentAuditEntrySummary {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub severity: String,
    pub details: String,
    pub agent_id: String,
    pub actor_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentCostSummary {
    pub agent_id: String,
    pub agent_name: String,
    pub daily_total: f64,
    pub compaction_cost: f64,
    pub spending_cap: f64,
    pub cap_remaining: f64,
    pub cap_utilization_pct: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverviewPanelState {
    Ready,
    Empty,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OverviewPanelStatus {
    pub state: OverviewPanelState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentOverviewPanelHealth {
    pub convergence: OverviewPanelStatus,
    pub cost: OverviewPanelStatus,
    pub recent_sessions: OverviewPanelStatus,
    pub recent_audit_entries: OverviewPanelStatus,
    pub crdt_summary: OverviewPanelStatus,
    pub integrity_summary: OverviewPanelStatus,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentOverviewResponse {
    pub agent: AgentInfo,
    pub convergence: Option<ConvergenceScoreResponse>,
    pub cost: Option<AgentCostSummary>,
    pub recent_sessions: Vec<RuntimeSessionSummary>,
    pub recent_audit_entries: Vec<AgentAuditEntrySummary>,
    pub crdt_summary: Option<CrdtStateResponse>,
    pub integrity_summary: Option<VerifyChainResponse>,
    pub panel_health: AgentOverviewPanelHealth,
}

#[derive(Debug, Clone)]
struct AgentRow {
    agent: RegisteredAgent,
    sandbox: crate::config::AgentSandboxConfig,
    sandbox_metrics: crate::sandbox_reviews::AgentSandboxMetrics,
}

fn lookup_agent_id(state: &AppState, id_or_name: &str) -> Result<Uuid, StatusCode> {
    match Uuid::parse_str(id_or_name) {
        Ok(id) => Ok(id),
        Err(_) => state
            .agents
            .read()
            .map_err(|error| {
                tracing::error!(error = %error, "Agent registry lock poisoned while resolving agent");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .lookup_by_name(id_or_name)
            .map(|agent| agent.id)
            .ok_or(StatusCode::NOT_FOUND),
    }
}

fn load_agent_row(state: &AppState, agent_id: Uuid) -> Result<Option<AgentRow>, StatusCode> {
    let guard = state.agents.read().map_err(|error| {
        tracing::error!(error = %error, "Agent registry lock poisoned while loading agent");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(guard.lookup_by_id(agent_id).map(|agent| AgentRow {
        agent: agent.clone(),
        sandbox: guard.sandbox_for(agent_id),
        sandbox_metrics: crate::sandbox_reviews::AgentSandboxMetrics::default(),
    }))
}

fn derive_safety_state(
    lifecycle_state: AgentLifecycleStateValue,
    kill_state: &KillSwitchState,
    agent_id: Uuid,
) -> AgentSafetyStateValue {
    if matches!(
        lifecycle_state,
        AgentLifecycleStateValue::Stopping | AgentLifecycleStateValue::Stopped
    ) {
        return AgentSafetyStateValue::Normal;
    }

    if kill_state.platform_level == KillLevel::KillAll {
        return AgentSafetyStateValue::KillAllBlocked;
    }

    match kill_state.per_agent.get(&agent_id).map(|entry| entry.level) {
        Some(KillLevel::Pause) => AgentSafetyStateValue::Paused,
        Some(KillLevel::Quarantine) => AgentSafetyStateValue::Quarantined,
        _ => AgentSafetyStateValue::Normal,
    }
}

fn derive_effective_state(
    lifecycle_state: &AgentLifecycleStateValue,
    safety_state: &AgentSafetyStateValue,
) -> AgentEffectiveStateValue {
    match lifecycle_state {
        AgentLifecycleStateValue::Stopped => AgentEffectiveStateValue::Stopped,
        AgentLifecycleStateValue::Stopping => AgentEffectiveStateValue::Stopping,
        AgentLifecycleStateValue::Starting => match safety_state {
            AgentSafetyStateValue::Paused => AgentEffectiveStateValue::Paused,
            AgentSafetyStateValue::Quarantined => AgentEffectiveStateValue::Quarantined,
            AgentSafetyStateValue::KillAllBlocked => AgentEffectiveStateValue::KillAllBlocked,
            AgentSafetyStateValue::Normal => AgentEffectiveStateValue::Starting,
        },
        AgentLifecycleStateValue::Ready => match safety_state {
            AgentSafetyStateValue::Paused => AgentEffectiveStateValue::Paused,
            AgentSafetyStateValue::Quarantined => AgentEffectiveStateValue::Quarantined,
            AgentSafetyStateValue::KillAllBlocked => AgentEffectiveStateValue::KillAllBlocked,
            AgentSafetyStateValue::Normal => AgentEffectiveStateValue::Ready,
        },
    }
}

fn derive_action_policy(
    lifecycle_state: &AgentLifecycleStateValue,
    safety_state: &AgentSafetyStateValue,
) -> AgentActionPolicy {
    let is_terminal = matches!(
        lifecycle_state,
        AgentLifecycleStateValue::Stopping | AgentLifecycleStateValue::Stopped
    );
    let kill_all_blocked = *safety_state == AgentSafetyStateValue::KillAllBlocked;
    let paused = *safety_state == AgentSafetyStateValue::Paused;
    let quarantined = *safety_state == AgentSafetyStateValue::Quarantined;

    AgentActionPolicy {
        can_pause: !is_terminal && !kill_all_blocked && !paused && !quarantined,
        can_quarantine: !is_terminal && !kill_all_blocked && !quarantined,
        can_resume: paused || quarantined,
        can_delete: !quarantined,
        resume_kind: if quarantined {
            Some(AgentResumeKind::Quarantine)
        } else if paused {
            Some(AgentResumeKind::Pause)
        } else {
            None
        },
        requires_forensic_review: quarantined,
        requires_second_confirmation: quarantined,
        monitoring_duration_hours: quarantined.then_some(24),
    }
}

fn agent_info(row: &AgentRow, kill_state: &KillSwitchState) -> AgentInfo {
    let lifecycle_state = AgentLifecycleStateValue::from(row.agent.state);
    let safety_state = derive_safety_state(lifecycle_state.clone(), kill_state, row.agent.id);
    let effective_state = derive_effective_state(&lifecycle_state, &safety_state);
    AgentInfo {
        id: row.agent.id.to_string(),
        name: row.agent.name.clone(),
        status: effective_state.as_status_str().into(),
        lifecycle_state: lifecycle_state.clone(),
        safety_state: safety_state.clone(),
        effective_state,
        spending_cap: row.agent.spending_cap,
        isolation: row.agent.isolation,
        capabilities: row.agent.capabilities.clone(),
        sandbox: row.sandbox.clone(),
        sandbox_metrics: row.sandbox_metrics.clone(),
        action_policy: derive_action_policy(&lifecycle_state, &safety_state),
        has_keypair: None,
    }
}

pub fn agent_operational_event_from_info(agent: &AgentInfo, reason: &str) -> WsEvent {
    WsEvent::AgentOperationalStatusChanged {
        agent_id: agent.id.clone(),
        lifecycle_state: serde_json::to_string(&agent.lifecycle_state)
            .unwrap_or_else(|_| "\"ready\"".into())
            .trim_matches('"')
            .to_string(),
        safety_state: serde_json::to_string(&agent.safety_state)
            .unwrap_or_else(|_| "\"normal\"".into())
            .trim_matches('"')
            .to_string(),
        effective_state: serde_json::to_string(&agent.effective_state)
            .unwrap_or_else(|_| "\"ready\"".into())
            .trim_matches('"')
            .to_string(),
        reason: reason.into(),
        changed_at: chrono::Utc::now().to_rfc3339(),
    }
}

pub fn broadcast_agent_operational_status(
    state: &AppState,
    agent_id: Uuid,
    reason: &str,
) -> Result<(), StatusCode> {
    let kill_state = state.kill_switch.current_state();
    let row = load_agent_row(state, agent_id)?.ok_or(StatusCode::NOT_FOUND)?;
    let info = agent_info(&row, &kill_state);
    crate::api::websocket::broadcast_event(state, agent_operational_event_from_info(&info, reason));
    crate::api::websocket::broadcast_event(
        state,
        WsEvent::AgentStateChange {
            agent_id: info.id,
            new_state: info.status,
        },
    );
    Ok(())
}

pub fn broadcast_deleted_agent_operational_status(state: &AppState, agent_id: Uuid, reason: &str) {
    crate::api::websocket::broadcast_event(
        state,
        WsEvent::AgentOperationalStatusChanged {
            agent_id: agent_id.to_string(),
            lifecycle_state: "stopped".into(),
            safety_state: "normal".into(),
            effective_state: "stopped".into(),
            reason: reason.into(),
            changed_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    crate::api::websocket::broadcast_event(
        state,
        WsEvent::AgentStateChange {
            agent_id: agent_id.to_string(),
            new_state: "stopped".into(),
        },
    );
}

/// GET /api/agents — returns the live agent registry summary.
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AgentInfo>>, StatusCode> {
    let sandbox_metrics = state
        .sandbox_reviews
        .agent_metrics()
        .await
        .unwrap_or_default();
    let agent_rows: Vec<(RegisteredAgent, crate::config::AgentSandboxConfig)> =
        match state.agents.read() {
            Ok(guard) => guard
                .all_agents()
                .iter()
                .map(|agent| ((*agent).clone(), guard.sandbox_for(agent.id)))
                .collect(),
            Err(error) => {
                tracing::error!(error = %error, "Agent registry RwLock poisoned in list_agents");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };
    let kill_state = state.kill_switch.current_state();
    let list = agent_rows
        .iter()
        .map(|(agent, sandbox)| {
            agent_info(
                &AgentRow {
                    agent: agent.clone(),
                    sandbox: sandbox.clone(),
                    sandbox_metrics: sandbox_metrics
                        .get(&agent.id.to_string())
                        .cloned()
                        .unwrap_or_default(),
                },
                &kill_state,
            )
        })
        .collect();
    Ok(Json(list))
}

/// GET /api/agents/:id — returns the canonical agent detail model.
pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
) -> impl IntoResponse {
    let agent_id = match lookup_agent_id(&state, &agent_id_str) {
        Ok(agent_id) => agent_id,
        Err(StatusCode::NOT_FOUND) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "agent not found", "id": agent_id_str})),
            )
                .into_response()
        }
        Err(status) => {
            return (
                status,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response()
        }
    };

    let sandbox_metrics = state
        .sandbox_reviews
        .agent_metrics()
        .await
        .unwrap_or_default();
    let row = match state.agents.read() {
        Ok(guard) => guard.lookup_by_id(agent_id).map(|agent| AgentRow {
            agent: agent.clone(),
            sandbox: guard.sandbox_for(agent_id),
            sandbox_metrics: sandbox_metrics
                .get(&agent_id.to_string())
                .cloned()
                .unwrap_or_default(),
        }),
        Err(error) => {
            tracing::error!(error = %error, "Agent registry RwLock poisoned in get_agent");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response();
        }
    };

    match row {
        Some(row) => {
            let kill_state = state.kill_switch.current_state();
            (StatusCode::OK, Json(agent_info(&row, &kill_state))).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found", "id": agent_id_str})),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AgentOverviewQuery {
    pub sessions_limit: Option<u32>,
    pub audit_limit: Option<u32>,
    pub crdt_limit: Option<u32>,
}

/// GET /api/agents/:id/overview — cohesive read model for the detail page.
pub async fn get_agent_overview(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
    Query(query): Query<AgentOverviewQuery>,
) -> impl IntoResponse {
    let agent_id = match lookup_agent_id(&state, &agent_id_str) {
        Ok(agent_id) => agent_id,
        Err(StatusCode::NOT_FOUND) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "agent not found", "id": agent_id_str})),
            )
                .into_response()
        }
        Err(status) => {
            return (
                status,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response()
        }
    };

    let sandbox_metrics = state
        .sandbox_reviews
        .agent_metrics()
        .await
        .unwrap_or_default();
    let row = match state.agents.read() {
        Ok(guard) => guard.lookup_by_id(agent_id).map(|agent| AgentRow {
            agent: agent.clone(),
            sandbox: guard.sandbox_for(agent_id),
            sandbox_metrics: sandbox_metrics
                .get(&agent_id.to_string())
                .cloned()
                .unwrap_or_default(),
        }),
        Err(error) => {
            tracing::error!(error = %error, "Agent registry RwLock poisoned in get_agent_overview");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response();
        }
    };
    let Some(row) = row else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found", "id": agent_id_str})),
        )
            .into_response();
    };

    let kill_state = state.kill_switch.current_state();
    let agent = agent_info(&row, &kill_state);
    let db = match state.db.read() {
        Ok(db) => db,
        Err(error) => {
            tracing::error!(error = %error, "Database read lock poisoned in get_agent_overview");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database connection error"})),
            )
                .into_response();
        }
    };

    let convergence = load_convergence(&db, &row);
    let cost = Some(load_cost(&state, &row));
    let recent_sessions = load_recent_sessions(
        &state,
        &db,
        agent_id,
        query.sessions_limit.unwrap_or(10).min(50),
    );
    let recent_audit_entries =
        load_recent_audit_entries(&db, agent_id, query.audit_limit.unwrap_or(20).min(100));
    let crdt_summary = load_crdt_summary(&db, agent_id, query.crdt_limit.unwrap_or(50).min(200));
    let integrity_summary = load_integrity_summary(&db, agent_id);

    let response = AgentOverviewResponse {
        agent,
        convergence: convergence.value,
        cost,
        recent_sessions: recent_sessions.value,
        recent_audit_entries: recent_audit_entries.value,
        crdt_summary: crdt_summary.value,
        integrity_summary: integrity_summary.value,
        panel_health: AgentOverviewPanelHealth {
            convergence: convergence.status,
            cost: OverviewPanelStatus {
                state: OverviewPanelState::Ready,
                message: None,
            },
            recent_sessions: recent_sessions.status,
            recent_audit_entries: recent_audit_entries.status,
            crdt_summary: crdt_summary.status,
            integrity_summary: integrity_summary.status,
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}

struct PanelLoad<T> {
    value: T,
    status: OverviewPanelStatus,
}

fn load_convergence(
    db: &crate::db_pool::ReadConn<'_>,
    row: &AgentRow,
) -> PanelLoad<Option<ConvergenceScoreResponse>> {
    match cortex_storage::queries::convergence_score_queries::latest_by_agent(
        db,
        &row.agent.id.to_string(),
    ) {
        Ok(Some(score)) => PanelLoad {
            value: Some(ConvergenceScoreResponse {
                agent_id: row.agent.id.to_string(),
                agent_name: row.agent.name.clone(),
                score: score.composite_score,
                level: score.level,
                profile: score.profile,
                signal_scores: serde_json::from_str(&score.signal_scores)
                    .unwrap_or_else(|_| serde_json::json!({})),
                computed_at: Some(score.computed_at),
            }),
            status: OverviewPanelStatus {
                state: OverviewPanelState::Ready,
                message: None,
            },
        },
        Ok(None) => PanelLoad {
            value: None,
            status: OverviewPanelStatus {
                state: OverviewPanelState::Empty,
                message: Some("No convergence data available".into()),
            },
        },
        Err(error) => PanelLoad {
            value: None,
            status: OverviewPanelStatus {
                state: OverviewPanelState::Error,
                message: Some(format!("convergence query failed: {error}")),
            },
        },
    }
}

fn load_cost(state: &AppState, row: &AgentRow) -> AgentCostSummary {
    let daily = state.cost_tracker.get_daily_total(row.agent.id);
    let compaction = state.cost_tracker.get_compaction_cost(row.agent.id);
    let remaining = (row.agent.spending_cap - daily).max(0.0);
    let utilization = if row.agent.spending_cap > 0.0 {
        (daily / row.agent.spending_cap * 100.0).min(100.0)
    } else {
        0.0
    };
    AgentCostSummary {
        agent_id: row.agent.id.to_string(),
        agent_name: row.agent.name.clone(),
        daily_total: daily,
        compaction_cost: compaction,
        spending_cap: row.agent.spending_cap,
        cap_remaining: remaining,
        cap_utilization_pct: utilization,
    }
}

fn load_recent_sessions(
    state: &AppState,
    db: &crate::db_pool::ReadConn<'_>,
    agent_id: Uuid,
    limit: u32,
) -> PanelLoad<Vec<RuntimeSessionSummary>> {
    let mut stmt = match db.prepare(
        "SELECT session_id,
                MIN(timestamp) as started_at,
                MAX(timestamp) as last_event_at,
                COUNT(*) as event_count,
                GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown')) as agents,
                MAX(sb.source_session_id) as branched_from
         FROM itp_events
         LEFT JOIN session_branches AS sb
           ON sb.session_id = itp_events.session_id
         WHERE session_id IN (
             SELECT DISTINCT session_id FROM itp_events WHERE sender = ?1
         )
         GROUP BY session_id
         ORDER BY last_event_at DESC
         LIMIT ?2",
    ) {
        Ok(stmt) => stmt,
        Err(error) => {
            return PanelLoad {
                value: Vec::new(),
                status: OverviewPanelStatus {
                    state: OverviewPanelState::Error,
                    message: Some(format!("session query prepare failed: {error}")),
                },
            }
        }
    };

    let rows = stmt.query_map(rusqlite::params![agent_id.to_string(), limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            row.get::<_, Option<String>>(5)?,
        ))
    });

    match rows {
        Ok(rows) => {
            let mut sessions = Vec::new();
            for row in rows {
                let (session_id, started_at, last_event_at, event_count, agents_csv, branched_from) =
                    match row {
                        Ok(row) => row,
                        Err(_) => continue,
                    };
                let agent_ids = agents_csv
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                let chain_valid = session_chain_valid(db, &session_id).unwrap_or(true);
                let cumulative_cost = Uuid::parse_str(&session_id)
                    .ok()
                    .map(|session_uuid| state.cost_tracker.get_session_total(session_uuid))
                    .unwrap_or(0.0);
                sessions.push(RuntimeSessionSummary {
                    session_id,
                    agent_ids,
                    started_at,
                    last_event_at,
                    event_count,
                    chain_valid,
                    cumulative_cost,
                    branched_from,
                });
            }
            let state = if sessions.is_empty() {
                OverviewPanelState::Empty
            } else {
                OverviewPanelState::Ready
            };
            PanelLoad {
                value: sessions,
                status: OverviewPanelStatus {
                    state,
                    message: None,
                },
            }
        }
        Err(error) => PanelLoad {
            value: Vec::new(),
            status: OverviewPanelStatus {
                state: OverviewPanelState::Error,
                message: Some(format!("session query failed: {error}")),
            },
        },
    }
}

fn session_chain_valid(
    db: &crate::db_pool::ReadConn<'_>,
    session_id: &str,
) -> Result<bool, rusqlite::Error> {
    let mut previous_hash: Option<String> = None;
    let mut stmt = db.prepare(
        "SELECT hex(event_hash) AS event_hash_hex, hex(previous_hash) AS previous_hash_hex
         FROM itp_events
         WHERE session_id = ?1
         ORDER BY sequence_number ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![session_id], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?.unwrap_or_default(),
            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        ))
    })?;

    for row in rows {
        let (event_hash, previous_event_hash) = row?;
        if let Some(expected_previous_hash) = &previous_hash {
            if expected_previous_hash != &previous_event_hash {
                return Ok(false);
            }
        }
        previous_hash = Some(event_hash);
    }

    Ok(true)
}

fn load_recent_audit_entries(
    db: &crate::db_pool::ReadConn<'_>,
    agent_id: Uuid,
    limit: u32,
) -> PanelLoad<Vec<AgentAuditEntrySummary>> {
    let engine = AuditQueryEngine::new(db);
    match engine.query(&AuditFilter {
        agent_id: Some(agent_id.to_string()),
        page: 1,
        page_size: limit,
        ..AuditFilter::default()
    }) {
        Ok(result) => {
            let entries = result
                .items
                .into_iter()
                .map(|entry: AuditEntry| AgentAuditEntrySummary {
                    id: entry.id,
                    timestamp: entry.timestamp,
                    event_type: entry.event_type,
                    severity: entry.severity,
                    details: entry.details,
                    agent_id: entry.agent_id,
                    actor_id: entry.actor_id,
                })
                .collect::<Vec<_>>();
            let state = if entries.is_empty() {
                OverviewPanelState::Empty
            } else {
                OverviewPanelState::Ready
            };
            PanelLoad {
                value: entries,
                status: OverviewPanelStatus {
                    state,
                    message: None,
                },
            }
        }
        Err(error) => PanelLoad {
            value: Vec::new(),
            status: OverviewPanelStatus {
                state: OverviewPanelState::Error,
                message: Some(format!("audit query failed: {error}")),
            },
        },
    }
}

fn load_crdt_summary(
    db: &crate::db_pool::ReadConn<'_>,
    agent_id: Uuid,
    limit: u32,
) -> PanelLoad<Option<CrdtStateResponse>> {
    let query = "SELECT event_id, memory_id, event_type, delta, actor_id, recorded_at,
                        hex(event_hash) as event_hash_hex, hex(previous_hash) as prev_hash_hex
                 FROM memory_events
                 WHERE actor_id = ?1
                 ORDER BY recorded_at ASC
                 LIMIT ?2 OFFSET 0";
    let count_query = "SELECT COUNT(*) FROM memory_events WHERE actor_id = ?1";
    let total = match db.query_row(
        count_query,
        rusqlite::params![agent_id.to_string()],
        |row| row.get::<_, u32>(0),
    ) {
        Ok(total) => total,
        Err(error) => {
            return PanelLoad {
                value: None,
                status: OverviewPanelStatus {
                    state: OverviewPanelState::Error,
                    message: Some(format!("crdt count failed: {error}")),
                },
            }
        }
    };

    let mut stmt = match db.prepare(query) {
        Ok(stmt) => stmt,
        Err(error) => {
            return PanelLoad {
                value: None,
                status: OverviewPanelStatus {
                    state: OverviewPanelState::Error,
                    message: Some(format!("crdt prepare failed: {error}")),
                },
            }
        }
    };

    let rows = stmt.query_map(rusqlite::params![agent_id.to_string(), limit], |row| {
        Ok(CrdtDelta {
            event_id: row.get::<_, i64>(0)?,
            memory_id: row.get::<_, String>(1)?,
            event_type: row.get::<_, String>(2)?,
            delta: row.get::<_, String>(3)?,
            actor_id: row.get::<_, String>(4)?,
            recorded_at: row.get::<_, String>(5)?,
            event_hash: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            previous_hash: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
        })
    });

    match rows {
        Ok(rows) => {
            let deltas = rows.filter_map(Result::ok).collect::<Vec<_>>();
            let chain_valid = deltas
                .windows(2)
                .all(|window| window[0].event_hash == window[1].previous_hash);
            let response = CrdtStateResponse {
                agent_id: agent_id.to_string(),
                deltas,
                total,
                limit,
                offset: 0,
                chain_valid,
            };
            let state = if response.total == 0 {
                OverviewPanelState::Empty
            } else {
                OverviewPanelState::Ready
            };
            PanelLoad {
                value: Some(response),
                status: OverviewPanelStatus {
                    state,
                    message: None,
                },
            }
        }
        Err(error) => PanelLoad {
            value: None,
            status: OverviewPanelStatus {
                state: OverviewPanelState::Error,
                message: Some(format!("crdt query failed: {error}")),
            },
        },
    }
}

fn load_integrity_summary(
    db: &crate::db_pool::ReadConn<'_>,
    agent_id: Uuid,
) -> PanelLoad<Option<VerifyChainResponse>> {
    let agent_id_str = agent_id.to_string();
    let itp_sessions = match db.prepare(
        "SELECT DISTINCT session_id FROM itp_events WHERE sender = ?1 ORDER BY timestamp ASC",
    ) {
        Ok(mut stmt) => stmt
            .query_map([agent_id_str.clone()], |row| row.get::<_, String>(0))
            .map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>()),
        Err(error) => Err(error),
    };
    let itp_sessions = match itp_sessions {
        Ok(sessions) => sessions,
        Err(error) => {
            return PanelLoad {
                value: None,
                status: OverviewPanelStatus {
                    state: OverviewPanelState::Error,
                    message: Some(format!("integrity session query failed: {error}")),
                },
            }
        }
    };

    let mut itp_breaks = Vec::new();
    let mut itp_total = 0usize;
    let mut itp_verified = 0usize;
    for session_id in &itp_sessions {
        let mut stmt = match db.prepare(
            "SELECT id, hex(event_hash), hex(previous_hash)
             FROM itp_events
             WHERE session_id = ?1
             ORDER BY sequence_number ASC",
        ) {
            Ok(stmt) => stmt,
            Err(error) => {
                return PanelLoad {
                    value: None,
                    status: OverviewPanelStatus {
                        state: OverviewPanelState::Error,
                        message: Some(format!("integrity prepare failed: {error}")),
                    },
                }
            }
        };
        let events = match stmt.query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        }) {
            Ok(rows) => rows.filter_map(Result::ok).collect::<Vec<_>>(),
            Err(error) => {
                return PanelLoad {
                    value: None,
                    status: OverviewPanelStatus {
                        state: OverviewPanelState::Error,
                        message: Some(format!("integrity query failed: {error}")),
                    },
                }
            }
        };
        itp_total += events.len();
        for (idx, event) in events.iter().enumerate() {
            if idx == 0 {
                itp_verified += 1;
                continue;
            }
            let expected_prev = events[idx - 1].1.clone();
            let actual_prev = event.2.clone();
            if expected_prev == actual_prev {
                itp_verified += 1;
            } else {
                itp_breaks.push(crate::api::integrity::IntegrityBreak {
                    session_id: Some(session_id.clone()),
                    memory_id: None,
                    event_id: IntegrityEventId::Text(event.0.clone()),
                    position: idx,
                    expected_prev,
                    actual_prev,
                });
            }
        }
    }

    let memory_ids = match db
        .prepare("SELECT DISTINCT memory_id FROM memory_events WHERE actor_id = ?1")
    {
        Ok(mut stmt) => match stmt.query_map([agent_id_str.clone()], |row| row.get::<_, String>(0))
        {
            Ok(rows) => rows.filter_map(Result::ok).collect::<Vec<_>>(),
            Err(error) => {
                return PanelLoad {
                    value: None,
                    status: OverviewPanelStatus {
                        state: OverviewPanelState::Error,
                        message: Some(format!("memory integrity query failed: {error}")),
                    },
                }
            }
        },
        Err(error) => {
            return PanelLoad {
                value: None,
                status: OverviewPanelStatus {
                    state: OverviewPanelState::Error,
                    message: Some(format!("memory integrity prepare failed: {error}")),
                },
            }
        }
    };

    let mut memory_breaks = Vec::new();
    let mut memory_total = 0usize;
    let mut memory_verified = 0usize;
    for memory_id in &memory_ids {
        let mut stmt = match db.prepare(
            "SELECT event_id, hex(event_hash), hex(previous_hash)
             FROM memory_events
             WHERE memory_id = ?1 AND actor_id = ?2
             ORDER BY recorded_at ASC, event_id ASC",
        ) {
            Ok(stmt) => stmt,
            Err(error) => {
                return PanelLoad {
                    value: None,
                    status: OverviewPanelStatus {
                        state: OverviewPanelState::Error,
                        message: Some(format!("memory integrity prepare failed: {error}")),
                    },
                }
            }
        };
        let events =
            match stmt.query_map(rusqlite::params![memory_id, agent_id_str.clone()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ))
            }) {
                Ok(rows) => rows.filter_map(Result::ok).collect::<Vec<_>>(),
                Err(error) => {
                    return PanelLoad {
                        value: None,
                        status: OverviewPanelStatus {
                            state: OverviewPanelState::Error,
                            message: Some(format!("memory integrity query failed: {error}")),
                        },
                    }
                }
            };
        memory_total += events.len();
        for (idx, event) in events.iter().enumerate() {
            if idx == 0 {
                memory_verified += 1;
                continue;
            }
            let expected_prev = events[idx - 1].1.clone();
            let actual_prev = event.2.clone();
            if expected_prev == actual_prev {
                memory_verified += 1;
            } else {
                memory_breaks.push(crate::api::integrity::IntegrityBreak {
                    session_id: None,
                    memory_id: Some(memory_id.clone()),
                    event_id: IntegrityEventId::Numeric(event.0),
                    position: idx,
                    expected_prev,
                    actual_prev,
                });
            }
        }
    }

    let response = VerifyChainResponse {
        agent_id: agent_id_str,
        chain_type: "both".into(),
        chains: IntegrityChains {
            itp_events: Some(ItpEventsIntegrity {
                sessions_checked: itp_sessions.len(),
                total_events: itp_total,
                verified_events: itp_verified,
                is_valid: itp_breaks.is_empty(),
                breaks: itp_breaks,
            }),
            memory_events: Some(MemoryEventsIntegrity {
                memory_chains_checked: memory_ids.len(),
                total_events: memory_total,
                verified_events: memory_verified,
                is_valid: memory_breaks.is_empty(),
                breaks: memory_breaks,
            }),
        },
    };

    let state = if response
        .chains
        .itp_events
        .as_ref()
        .map(|chain| chain.total_events)
        .unwrap_or(0)
        == 0
        && response
            .chains
            .memory_events
            .as_ref()
            .map(|chain| chain.total_events)
            .unwrap_or(0)
            == 0
    {
        OverviewPanelState::Empty
    } else {
        OverviewPanelState::Ready
    };

    PanelLoad {
        value: Some(response),
        status: OverviewPanelStatus {
            state,
            message: None,
        },
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAgentRequest {
    pub name: String,
    pub spending_cap: Option<f64>,
    pub capabilities: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub sandbox: Option<crate::config::AgentSandboxConfig>,
    pub generate_keypair: Option<bool>,
}

/// POST /api/agents — create a new agent with optional keypair generation.
pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    {
        let agents = match state.agents.read() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::error!(error = %error, "Agent registry RwLock poisoned in create_agent");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                );
            }
        };
        if agents.lookup_by_name(&body.name).is_some() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "agent with this name already exists",
                    "name": body.name,
                })),
            );
        }
    }

    let agent_id = crate::agents::registry::durable_agent_id(&body.name);
    let spending_cap = body.spending_cap.unwrap_or(5.0);
    let capabilities = body.capabilities.unwrap_or_default();
    let skills = body.skills;
    let sandbox = body.sandbox.unwrap_or_default();

    let mut has_keypair = false;
    if body.generate_keypair.unwrap_or(true) {
        let keys_dir_str =
            crate::bootstrap::shellexpand_tilde(&format!("~/.ghost/agents/{}/keys", body.name));
        let keys_dir = std::path::PathBuf::from(&keys_dir_str);
        let mut kpm = ghost_identity::keypair_manager::AgentKeypairManager::new(keys_dir);
        match kpm.generate() {
            Ok(_vk) => {
                has_keypair = true;
                tracing::info!(agent = %body.name, "Ed25519 keypair generated");
            }
            Err(error) => {
                tracing::warn!(agent = %body.name, error = %error, "Keypair generation failed");
            }
        }
    }

    let registered = RegisteredAgent {
        id: agent_id,
        name: body.name.clone(),
        state: AgentLifecycleState::Starting,
        channel_bindings: Vec::new(),
        isolation: crate::config::IsolationMode::InProcess,
        full_access: false,
        capabilities: capabilities.clone(),
        skills: skills.clone(),
        baseline_capabilities: capabilities,
        baseline_skills: skills,
        access_pullback_active: false,
        spending_cap,
        template: None,
    };

    {
        let mut agents = match state.agents.write() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::error!(error = %error, "Agent registry RwLock poisoned in create_agent (write)");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                );
            }
        };
        agents.register_with_sandbox(registered, sandbox);
    }

    let sandbox_metrics = state
        .sandbox_reviews
        .agent_metrics()
        .await
        .unwrap_or_default();
    let row = match state.agents.read() {
        Ok(guard) => match guard.lookup_by_id(agent_id) {
            Some(agent) => AgentRow {
                agent: agent.clone(),
                sandbox: guard.sandbox_for(agent_id),
                sandbox_metrics: sandbox_metrics
                    .get(&agent_id.to_string())
                    .cloned()
                    .unwrap_or_default(),
            },
            None => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "created agent missing from registry"})),
                )
            }
        },
        Err(error) => {
            tracing::error!(error = %error, "Agent registry RwLock poisoned after create");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            );
        }
    };

    let kill_state = state.kill_switch.current_state();
    let mut response = agent_info(&row, &kill_state);
    response.has_keypair = Some(has_keypair);

    crate::api::websocket::broadcast_event(
        &state,
        agent_operational_event_from_info(&response, "created"),
    );
    crate::api::websocket::broadcast_event(
        &state,
        WsEvent::AgentStateChange {
            agent_id: agent_id.to_string(),
            new_state: response.status.clone(),
        },
    );

    tracing::info!(
        agent_id = %agent_id,
        name = %body.name,
        spending_cap = spending_cap,
        "Agent created via API"
    );

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({}))),
    )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAgentRequest {
    pub spending_cap: Option<f64>,
    pub capabilities: Option<Vec<String>>,
    pub sandbox: Option<crate::config::AgentSandboxConfig>,
}

/// PATCH /api/agents/:id — update a live agent's runtime settings.
pub async fn update_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<UpdateAgentRequest>,
) -> impl IntoResponse {
    let agent_id = match lookup_agent_id(&state, &agent_id_str) {
        Ok(agent_id) => agent_id,
        Err(StatusCode::NOT_FOUND) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "agent not found", "id": agent_id_str})),
            )
                .into_response()
        }
        Err(status) => {
            return (
                status,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response()
        }
    };

    {
        let mut agents = match state.agents.write() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::error!(error = %error, "Agent registry RwLock poisoned in update_agent (write)");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                )
                    .into_response();
            }
        };

        let Some(agent) = agents.lookup_by_id_mut(agent_id) else {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "agent not found", "id": agent_id.to_string()})),
            )
                .into_response();
        };

        if let Some(spending_cap) = body.spending_cap {
            agent.spending_cap = spending_cap;
        }

        if let Some(capabilities) = body.capabilities.clone() {
            agent.capabilities = capabilities.clone();
            agent.baseline_capabilities = capabilities;
        }

        if let Some(sandbox) = body.sandbox {
            if let Err(error) = agents.update_sandbox(agent_id, sandbox) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": error})),
                )
                    .into_response();
            }
        }
    }

    let sandbox_metrics = state
        .sandbox_reviews
        .agent_metrics()
        .await
        .unwrap_or_default();
    let row =
        match state.agents.read() {
            Ok(guard) => match guard.lookup_by_id(agent_id) {
                Some(agent) => AgentRow {
                    agent: agent.clone(),
                    sandbox: guard.sandbox_for(agent_id),
                    sandbox_metrics: sandbox_metrics
                        .get(&agent_id.to_string())
                        .cloned()
                        .unwrap_or_default(),
                },
                None => return (
                    StatusCode::NOT_FOUND,
                    Json(
                        serde_json::json!({"error": "agent not found", "id": agent_id.to_string()}),
                    ),
                )
                    .into_response(),
            },
            Err(error) => {
                tracing::error!(error = %error, "Agent registry RwLock poisoned in update_agent");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                )
                    .into_response();
            }
        };
    let kill_state = state.kill_switch.current_state();
    let response = agent_info(&row, &kill_state);
    crate::api::websocket::broadcast_event(
        &state,
        agent_operational_event_from_info(&response, "updated"),
    );
    crate::api::websocket::broadcast_event(
        &state,
        WsEvent::AgentStateChange {
            agent_id: agent_id.to_string(),
            new_state: response.status.clone(),
        },
    );
    (StatusCode::OK, Json(response)).into_response()
}

/// DELETE /api/agents/:id — remove an agent from the registry.
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
) -> impl IntoResponse {
    let agent_id = match lookup_agent_id(&state, &agent_id_str) {
        Ok(agent_id) => agent_id,
        Err(StatusCode::NOT_FOUND) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "agent not found", "id": agent_id_str})),
            )
                .into_response()
        }
        Err(status) => {
            return (
                status,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response()
        }
    };

    let ks_state = state.kill_switch.current_state();
    if let Some(agent_ks) = ks_state.per_agent.get(&agent_id) {
        if agent_ks.level == KillLevel::Quarantine {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "cannot delete quarantined agent — resume first",
                    "id": agent_id.to_string(),
                })),
            )
                .into_response();
        }
    }

    let agent_channels = match state.channel_manager.load_channels() {
        Ok(channels) => channels
            .into_iter()
            .filter(|channel| channel.agent_id == agent_id.to_string())
            .collect::<Vec<_>>(),
        Err(error) => {
            tracing::error!(
                agent_id = %agent_id,
                error = %error,
                "Failed to load channels during agent deletion"
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "failed to load agent channels"})),
            )
                .into_response();
        }
    };

    if !agent_channels.is_empty() {
        let db = state.db.write().await;
        if let Err(error) = db.execute(
            "DELETE FROM channels WHERE agent_id = ?1",
            [agent_id.to_string()],
        ) {
            tracing::error!(
                agent_id = %agent_id,
                error = %error,
                "Failed to delete channels during agent deletion"
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "failed to delete agent channels"})),
            )
                .into_response();
        }
        drop(db);

        for channel in &agent_channels {
            if let Err(error) = state.channel_manager.remove_channel_runtime(channel).await {
                tracing::error!(
                    agent_id = %agent_id,
                    channel_id = %channel.id,
                    error = %error,
                    "Failed to remove channel runtime during agent deletion"
                );
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "failed to clean up agent channels"})),
                )
                    .into_response();
            }
        }
    }

    let mut agents = match state.agents.write() {
        Ok(guard) => guard,
        Err(error) => {
            tracing::error!(error = %error, "Agent registry RwLock poisoned in delete_agent (write)");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response();
        }
    };
    match agents.unregister(agent_id) {
        Some(agent) => {
            broadcast_deleted_agent_operational_status(&state, agent_id, "deleted");

            tracing::info!(
                agent_id = %agent_id,
                name = %agent.name,
                deleted_channels = agent_channels.len(),
                "Agent deleted via API"
            );

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "deleted",
                    "id": agent_id.to_string(),
                    "name": agent.name,
                })),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found", "id": agent_id.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_all_overrides_ready_for_effective_state() {
        let lifecycle = AgentLifecycleStateValue::Ready;
        let mut kill_state = KillSwitchState::default();
        kill_state.platform_level = KillLevel::KillAll;
        let safety = derive_safety_state(lifecycle.clone(), &kill_state, Uuid::nil());
        let effective = derive_effective_state(&lifecycle, &safety);
        assert_eq!(safety, AgentSafetyStateValue::KillAllBlocked);
        assert_eq!(effective, AgentEffectiveStateValue::KillAllBlocked);
    }

    #[test]
    fn stopping_remains_terminal_even_if_platform_killed() {
        let lifecycle = AgentLifecycleStateValue::Stopping;
        let mut kill_state = KillSwitchState::default();
        kill_state.platform_level = KillLevel::KillAll;
        let safety = derive_safety_state(lifecycle.clone(), &kill_state, Uuid::nil());
        let effective = derive_effective_state(&lifecycle, &safety);
        assert_eq!(safety, AgentSafetyStateValue::Normal);
        assert_eq!(effective, AgentEffectiveStateValue::Stopping);
    }

    #[test]
    fn quarantine_requires_review_and_confirmation() {
        let policy = derive_action_policy(
            &AgentLifecycleStateValue::Ready,
            &AgentSafetyStateValue::Quarantined,
        );
        assert!(policy.can_resume);
        assert_eq!(policy.resume_kind, Some(AgentResumeKind::Quarantine));
        assert!(policy.requires_forensic_review);
        assert!(policy.requires_second_confirmation);
        assert_eq!(policy.monitoring_duration_hours, Some(24));
    }
}
