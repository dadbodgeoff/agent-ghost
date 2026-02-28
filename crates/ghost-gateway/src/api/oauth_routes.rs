//! OAuth API endpoints for the gateway.
//!
//! - `GET  /api/oauth/providers`          — list configured providers with scopes
//! - `POST /api/oauth/connect`            — initiate OAuth flow, returns authorization URL
//! - `GET  /api/oauth/callback`           — OAuth redirect handler
//! - `GET  /api/oauth/connections`        — list active connections (ref_ids, no tokens)
//! - `DELETE /api/oauth/connections/:ref_id` — disconnect (revoke + delete)
//!
//! All endpoints require `GHOST_TOKEN` Bearer auth (same as existing API).

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

/// GET /api/oauth/providers — list configured providers.
pub async fn list_providers() -> impl IntoResponse {
    // In production, this reads from the OAuthBroker's provider registry.
    // Stub: return the four supported providers.
    let providers = serde_json::json!([
        {
            "name": "google",
            "scopes": {
                "email": ["https://www.googleapis.com/auth/gmail.readonly"],
                "calendar": ["https://www.googleapis.com/auth/calendar"],
                "drive": ["https://www.googleapis.com/auth/drive.readonly"]
            }
        },
        {
            "name": "github",
            "scopes": {
                "repo": ["repo"],
                "user": ["read:user"],
                "org": ["read:org"]
            }
        },
        {
            "name": "slack",
            "scopes": {
                "chat": ["chat:write"],
                "channels": ["channels:read"],
                "users": ["users:read"]
            }
        },
        {
            "name": "microsoft",
            "scopes": {
                "mail": ["Mail.Read"],
                "calendar": ["Calendars.Read"],
                "user": ["User.Read"]
            }
        }
    ]);
    (StatusCode::OK, Json(providers))
}

/// Request body for POST /api/oauth/connect.
#[derive(Deserialize)]
pub struct ConnectRequest {
    pub provider: String,
    pub scopes: Vec<String>,
}

/// POST /api/oauth/connect — initiate OAuth flow.
pub async fn connect(Json(req): Json<ConnectRequest>) -> impl IntoResponse {
    // In production, this calls OAuthBroker::connect() and returns the auth URL.
    // Stub: return a placeholder authorization URL.
    let ref_id = uuid::Uuid::now_v7().to_string();
    let auth_url = format!(
        "https://{}.example.com/oauth/authorize?ref_id={}",
        req.provider, ref_id
    );
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "authorization_url": auth_url,
            "ref_id": ref_id
        })),
    )
}

/// Query parameters for GET /api/oauth/callback.
#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

/// GET /api/oauth/callback — OAuth redirect handler.
pub async fn callback(Query(params): Query<CallbackQuery>) -> impl IntoResponse {
    // In production, this calls OAuthBroker::callback(state, code).
    // Validates CSRF token in state, exchanges code for tokens.
    if params.state.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid state"})),
        );
    }
    if params.code.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "missing code"})),
        );
    }

    // Stub: return success
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "connected",
            "state": params.state
        })),
    )
}

/// GET /api/oauth/connections — list active connections.
pub async fn list_connections() -> impl IntoResponse {
    // In production, this calls OAuthBroker::list_connections().
    (StatusCode::OK, Json(serde_json::json!([])))
}

/// DELETE /api/oauth/connections/:ref_id — disconnect.
pub async fn disconnect(Path(ref_id): Path<String>) -> impl IntoResponse {
    // In production, this calls OAuthBroker::disconnect(ref_id).
    tracing::info!(ref_id = %ref_id, "OAuth disconnect requested");
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "disconnected",
            "ref_id": ref_id
        })),
    )
}
