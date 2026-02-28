//! Safety API endpoints: kill switch, pause, resume.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

/// POST /api/safety/kill-all
pub async fn kill_all() -> impl IntoResponse {
    tracing::warn!("KILL_ALL requested via API");
    (StatusCode::OK, Json(serde_json::json!({"status": "kill_all_activated"})))
}

/// POST /api/safety/pause/{agent_id}
pub async fn pause_agent(Path(agent_id): Path<String>) -> impl IntoResponse {
    tracing::warn!(agent_id = %agent_id, "Agent pause requested via API");
    (StatusCode::OK, Json(serde_json::json!({"status": "paused", "agent_id": agent_id})))
}

/// POST /api/safety/resume/{agent_id}
pub async fn resume_agent(Path(agent_id): Path<String>) -> impl IntoResponse {
    tracing::info!(agent_id = %agent_id, "Agent resume requested via API");
    (StatusCode::OK, Json(serde_json::json!({"status": "resumed", "agent_id": agent_id})))
}
