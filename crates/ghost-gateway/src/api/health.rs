//! Health and readiness endpoints.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::gateway::GatewayState;

/// GET /api/health — liveness probe.
pub async fn health_handler(state: GatewayState) -> impl IntoResponse {
    match state {
        GatewayState::FatalError => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "fatal_error"})),
        ),
        _ => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "alive",
                "state": format!("{state:?}")
            })),
        ),
    }
}

/// GET /api/ready — readiness probe.
pub async fn ready_handler(state: GatewayState) -> impl IntoResponse {
    match state {
        GatewayState::Healthy | GatewayState::Degraded => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ready",
                "state": format!("{state:?}")
            })),
        ),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "not_ready",
                "state": format!("{state:?}")
            })),
        ),
    }
}
