//! HTTP API transport (Req 9 AC8).
//!
//! axum server on configurable port (default 18790).
//! Endpoints: GET /health, /status, /scores, /scores/:agent_id, /sessions, /interventions
//!            POST /events, /events/batch, /recalculate, /gateway-shutdown

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::IngestEvent;

/// Shared state for the HTTP API.
pub struct HttpApiState {
    pub ingest_tx: mpsc::Sender<IngestEvent>,
    pub healthy: bool,
}

/// Health response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

/// Score response.
#[derive(Serialize)]
pub struct ScoreResponse {
    pub agent_id: Uuid,
    pub score: f64,
    pub level: u8,
}

/// Batch event request.
#[derive(Deserialize)]
pub struct BatchEventRequest {
    pub events: Vec<IngestEvent>,
}

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
        .with_state(state)
}

async fn health_handler(
    State(state): State<Arc<tokio::sync::RwLock<HttpApiState>>>,
) -> impl IntoResponse {
    let state = state.read().await;
    let status = if state.healthy { "healthy" } else { "degraded" };
    Json(HealthResponse {
        status: status.to_string(),
        uptime_seconds: 0, // TODO: track actual uptime
    })
}

async fn status_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status": "running"}))
}

async fn scores_handler() -> impl IntoResponse {
    Json(serde_json::json!({"scores": []}))
}

async fn scores_by_agent_handler(Path(_agent_id): Path<String>) -> impl IntoResponse {
    Json(serde_json::json!({"score": 0.0, "level": 0}))
}

async fn sessions_handler() -> impl IntoResponse {
    Json(serde_json::json!({"sessions": []}))
}

async fn interventions_handler() -> impl IntoResponse {
    Json(serde_json::json!({"interventions": []}))
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

async fn recalculate_handler() -> impl IntoResponse {
    StatusCode::ACCEPTED
}

async fn gateway_shutdown_handler() -> impl IntoResponse {
    tracing::info!("gateway-shutdown notification received");
    StatusCode::OK
}
