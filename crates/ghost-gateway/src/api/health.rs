//! Health and readiness endpoints.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

/// GET /api/health — liveness probe.
///
/// Always returns 200 if the server is running. In production, this
/// would check GatewaySharedState via axum State extractor.
pub async fn health_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "alive",
            "state": "Healthy"
        })),
    )
}

/// GET /api/ready — readiness probe.
pub async fn ready_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ready",
            "state": "Healthy"
        })),
    )
}
