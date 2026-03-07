//! HTTP API transport (Req 9 AC8).
//!
//! axum server on configurable port (default 18790).
//! Endpoints: GET /health, /status, /scores, /scores/:agent_id, /sessions, /interventions
//!            POST /events, /events/batch, /recalculate, /gateway-shutdown,
//!                 /interventions/:agent_id/acknowledge,
//!                 /config/threshold, /config/threshold/confirm

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::IngestEvent;

// ── Monitor request/response types (T-6.3.2, T-6.4.2) ─────────────

/// Requests sent from HTTP handlers to the monitor event loop.
pub enum MonitorRequest {
    /// Acknowledge a Level 2 intervention (T-6.3.2, Req 9 AC4).
    Acknowledge {
        agent_id: Uuid,
        reply: oneshot::Sender<AckResult>,
    },
    /// Propose a threshold change (T-6.4.2, CS§ dual-key).
    ThresholdChange {
        current: f64,
        proposed: f64,
        reply: oneshot::Sender<ThresholdChangeResult>,
    },
    /// Confirm a dual-key threshold change (T-6.4.2).
    ThresholdConfirm {
        token: String,
        reply: oneshot::Sender<bool>,
    },
}

/// Result of an acknowledge request.
#[derive(Debug)]
pub enum AckResult {
    /// Acknowledgment accepted.
    Ok,
    /// Agent is not at Level 2 or does not require acknowledgment.
    NotLevel2,
    /// Agent not found.
    NotFound,
}

/// Result of a threshold change request.
#[derive(Debug)]
pub enum ThresholdChangeResult {
    /// Change applied immediately (non-critical or allowed).
    Applied,
    /// Change rejected (config locked or below floor).
    Rejected { reason: String },
    /// Critical change — dual-key confirmation required.
    DualKeyRequired { token: String },
}

// ── Snapshot types (populated by monitor, read by handlers) ──────────

/// Score snapshot for a single agent.
#[derive(Debug, Clone, Serialize)]
pub struct ScoreSnapshot {
    pub agent_id: Uuid,
    pub composite_score: f64,
    pub level: u8,
    pub signals: [f64; 8],
    pub computed_at: DateTime<Utc>,
}

/// Intervention snapshot for a single agent.
#[derive(Debug, Clone, Serialize)]
pub struct InterventionSnapshot {
    pub agent_id: Uuid,
    pub level: u8,
    pub cooldown_remaining_secs: Option<i64>,
    pub ack_required: bool,
    pub consecutive_normal: u32,
}

/// Session snapshot (mirrors SessionState from registry).
#[derive(Debug, Clone, Serialize)]
pub struct SessionSnapshot {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
    pub event_count: u64,
    pub is_active: bool,
}

// ── Shared state ─────────────────────────────────────────────────────

/// Shared state for the HTTP API.
///
/// The monitor writes snapshots into this struct after each event.
/// HTTP handlers read from it via `Arc<RwLock<HttpApiState>>`.
pub struct HttpApiState {
    pub ingest_tx: mpsc::Sender<IngestEvent>,
    pub healthy: bool,
    pub start_time: Instant,
    /// Live score snapshots per agent (T-6.2.1, T-6.2.2).
    pub scores: BTreeMap<Uuid, ScoreSnapshot>,
    /// Active session snapshots (T-6.2.3).
    pub sessions: Vec<SessionSnapshot>,
    /// Intervention state snapshots per agent (T-6.2.4).
    pub interventions: BTreeMap<Uuid, InterventionSnapshot>,
    /// Tracked agent count (T-6.2.5).
    pub agent_count: usize,
    /// Total events processed (T-6.2.5).
    pub event_count: u64,
    /// Last score computation time (T-6.2.5).
    pub last_computation: Option<DateTime<Utc>>,
    /// Channel to request a full recalculation (T-6.2.7).
    pub recalculate_tx: mpsc::Sender<()>,
    /// Timestamp of last recalculation (rate limiting: 1 per 10s).
    pub last_recalculate: Option<Instant>,
    /// Channel to request graceful shutdown (T-6.2.8).
    pub shutdown_tx: mpsc::Sender<()>,
    /// Channel for request/response communication with the monitor (T-6.3.2, T-6.4.2).
    pub monitor_tx: mpsc::Sender<MonitorRequest>,
}

// ── Request/response types ───────────────────────────────────────────

/// Health response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

/// Batch event request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEventRequest {
    pub events: Vec<IngestEvent>,
}

// ── Router ───────────────────────────────────────────────────────────

/// Build the HTTP API router.
pub fn build_router(state: Arc<tokio::sync::RwLock<HttpApiState>>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/status", get(status_handler))
        .route("/scores", get(scores_handler))
        .route("/scores/{agent_id}", get(scores_by_agent_handler))
        .route("/sessions", get(sessions_handler))
        .route("/interventions", get(interventions_handler))
        .route("/events", post(events_handler))
        .route("/events/batch", post(events_batch_handler))
        .route("/recalculate", post(recalculate_handler))
        .route("/gateway-shutdown", post(gateway_shutdown_handler))
        .route(
            "/interventions/{agent_id}/acknowledge",
            post(acknowledge_handler),
        )
        .route("/config/threshold", post(threshold_change_handler))
        .route("/config/threshold/confirm", post(threshold_confirm_handler))
        .with_state(state)
}

// ── Handlers ─────────────────────────────────────────────────────────

async fn health_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let state = state.read().await;
    let status = if state.healthy { "healthy" } else { "degraded" };
    Json(HealthResponse {
        status: status.to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
    })
}

/// T-6.2.5: Real monitor status.
async fn status_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let state = state.read().await;
    Json(serde_json::json!({
        "status": if state.healthy { "running" } else { "degraded" },
        "agent_count": state.agent_count,
        "event_count": state.event_count,
        "last_computation": state.last_computation,
        "uptime_seconds": state.start_time.elapsed().as_secs(),
    }))
}

/// T-6.2.1: Live score data.
async fn scores_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let state = state.read().await;
    let scores: Vec<&ScoreSnapshot> = state.scores.values().collect();
    Json(serde_json::json!({ "scores": scores }))
}

/// T-6.2.2: Score for a specific agent.
async fn scores_by_agent_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let state = state.read().await;
    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid agent_id"})),
            );
        }
    };
    match state.scores.get(&uuid) {
        Some(snapshot) => (StatusCode::OK, Json(serde_json::json!(snapshot))),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "no score for agent"})),
        ),
    }
}

/// T-6.2.3: Active sessions from SessionRegistry.
async fn sessions_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let state = state.read().await;
    Json(serde_json::json!({ "sessions": state.sessions }))
}

/// T-6.2.4: Intervention state per agent.
async fn interventions_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let state = state.read().await;
    let interventions: Vec<&InterventionSnapshot> = state.interventions.values().collect();
    Json(serde_json::json!({ "interventions": interventions }))
}

async fn events_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
    Json(event): Json<IngestEvent>,
) -> impl IntoResponse {
    let state = state.read().await;
    match state.ingest_tx.try_send(event) {
        Ok(()) => StatusCode::ACCEPTED,
        Err(_) => StatusCode::TOO_MANY_REQUESTS,
    }
}

async fn events_batch_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
    Json(batch): Json<BatchEventRequest>,
) -> impl IntoResponse {
    if batch.events.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "batch size exceeds 100"})),
        );
    }

    let state = state.read().await;
    let mut accepted = 0u32;
    for event in batch.events {
        if state.ingest_tx.try_send(event).is_ok() {
            accepted += 1;
        }
    }

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"accepted": accepted})),
    )
}

/// T-6.2.7: Trigger score recomputation (rate-limited: 1 per 10s).
async fn recalculate_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let mut state = state.write().await;
    if let Some(last) = state.last_recalculate {
        if last.elapsed().as_secs() < 10 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(
                    serde_json::json!({"error": "rate limited — at most 1 recalculation per 10 seconds"}),
                ),
            );
        }
    }
    match state.recalculate_tx.try_send(()) {
        Ok(()) => {
            state.last_recalculate = Some(Instant::now());
            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({"status": "recalculation requested"})),
            )
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "recalculation channel full"})),
        ),
    }
}

/// T-6.2.8: Initiate graceful shutdown.
async fn gateway_shutdown_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let state = state.read().await;
    tracing::info!("gateway-shutdown notification received");
    match state.shutdown_tx.try_send(()) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "shutdown initiated"})),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "shutdown already in progress"})),
        ),
    }
}

/// T-6.3.2: Acknowledge a Level 2 intervention (Req 9 AC4).
///
/// Only Level 2 interventions require acknowledgment. Acknowledging a
/// non-Level-2 state returns 409 Conflict.
async fn acknowledge_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid agent_id"})),
            );
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    let monitor_tx = {
        let state = state.read().await;
        state.monitor_tx.clone()
    };

    if monitor_tx
        .send(MonitorRequest::Acknowledge {
            agent_id: uuid,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "monitor unavailable"})),
        );
    }

    match reply_rx.await {
        Ok(AckResult::Ok) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "acknowledged"})),
        ),
        Ok(AckResult::NotLevel2) => (
            StatusCode::CONFLICT,
            Json(
                serde_json::json!({"error": "agent is not at Level 2 or does not require acknowledgment"}),
            ),
        ),
        Ok(AckResult::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found"})),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "monitor did not respond"})),
        ),
    }
}

/// Threshold change request body.
#[derive(Debug, Clone, Deserialize)]
pub struct ThresholdChangeRequest {
    pub current: f64,
    pub proposed: f64,
}

/// Threshold confirm request body.
#[derive(Debug, Clone, Deserialize)]
pub struct ThresholdConfirmRequest {
    pub token: String,
}

/// T-6.4.2: Propose a threshold change (CS§ dual-key).
///
/// Non-critical changes (raising thresholds) are applied immediately.
/// Critical changes (below floor) require dual-key confirmation — the
/// endpoint returns a token that must be confirmed via POST /config/threshold/confirm.
async fn threshold_change_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
    Json(req): Json<ThresholdChangeRequest>,
) -> impl IntoResponse {
    let (reply_tx, reply_rx) = oneshot::channel();
    let monitor_tx = {
        let state = state.read().await;
        state.monitor_tx.clone()
    };

    if monitor_tx
        .send(MonitorRequest::ThresholdChange {
            current: req.current,
            proposed: req.proposed,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "monitor unavailable"})),
        );
    }

    match reply_rx.await {
        Ok(ThresholdChangeResult::Applied) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "applied"})),
        ),
        Ok(ThresholdChangeResult::Rejected { reason }) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": reason})),
        ),
        Ok(ThresholdChangeResult::DualKeyRequired { token }) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "status": "dual_key_required",
                "token": token,
                "expires_in_secs": 300,
            })),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "monitor did not respond"})),
        ),
    }
}

/// T-6.4.2: Confirm a dual-key threshold change.
///
/// Token must match the one returned by POST /config/threshold.
/// Token expires after 5 minutes.
async fn threshold_confirm_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
    Json(req): Json<ThresholdConfirmRequest>,
) -> impl IntoResponse {
    let (reply_tx, reply_rx) = oneshot::channel();
    let monitor_tx = {
        let state = state.read().await;
        state.monitor_tx.clone()
    };

    if monitor_tx
        .send(MonitorRequest::ThresholdConfirm {
            token: req.token,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "monitor unavailable"})),
        );
    }

    match reply_rx.await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "confirmed and applied"})),
        ),
        Ok(false) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "invalid or expired token"})),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "monitor did not respond"})),
        ),
    }
}
