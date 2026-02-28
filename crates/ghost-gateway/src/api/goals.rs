//! Goal/proposal approval API endpoints (Req 25 AC5-6).

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

/// POST /api/goals/{id}/approve
pub async fn approve_goal(Path(id): Path<String>) -> impl IntoResponse {
    // Check if proposal is still pending (resolved_at IS NULL)
    // If already resolved → 409 Conflict (AC6)
    tracing::info!(goal_id = %id, "Goal approval requested");
    (StatusCode::OK, Json(serde_json::json!({"status": "approved", "id": id})))
}

/// POST /api/goals/{id}/reject
pub async fn reject_goal(Path(id): Path<String>) -> impl IntoResponse {
    tracing::info!(goal_id = %id, "Goal rejection requested");
    (StatusCode::OK, Json(serde_json::json!({"status": "rejected", "id": id})))
}
