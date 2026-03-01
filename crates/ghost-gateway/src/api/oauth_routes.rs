//! OAuth API endpoints wired to `ghost_oauth::OAuthBroker`.
//!
//! - `GET  /api/oauth/providers`          — list configured providers
//! - `POST /api/oauth/connect`            — initiate OAuth flow
//! - `GET  /api/oauth/callback`           — OAuth redirect handler
//! - `GET  /api/oauth/connections`        — list active connections
//! - `DELETE /api/oauth/connections/:ref_id` — disconnect (revoke + delete)

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use std::sync::Arc;
use axum::extract::State;
use crate::state::AppState;

/// GET /api/oauth/providers — list configured providers.
pub async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let names = state.oauth_broker.provider_names();
    let providers: Vec<serde_json::Value> = names
        .iter()
        .map(|name| serde_json::json!({"name": name}))
        .collect();
    (StatusCode::OK, Json(serde_json::json!(providers)))
}

/// Request body for POST /api/oauth/connect.
#[derive(Deserialize)]
pub struct ConnectRequest {
    pub provider: String,
    pub scopes: Vec<String>,
    #[serde(default = "default_redirect_uri")]
    pub redirect_uri: String,
}

fn default_redirect_uri() -> String {
    "http://localhost:18789/api/oauth/callback".into()
}

/// POST /api/oauth/connect — initiate OAuth flow.
pub async fn connect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> impl IntoResponse {
    match state.oauth_broker.connect(&req.provider, &req.scopes, &req.redirect_uri) {
        Ok((auth_url, ref_id)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "authorization_url": auth_url,
                "ref_id": ref_id.to_string(),
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// Query parameters for GET /api/oauth/callback.
#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

/// GET /api/oauth/callback — OAuth redirect handler.
pub async fn callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackQuery>,
) -> impl IntoResponse {
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

    match state.oauth_broker.callback(&params.state, &params.code) {
        Ok(ref_id) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "connected",
                "ref_id": ref_id.to_string(),
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// GET /api/oauth/connections — list active connections.
pub async fn list_connections(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.oauth_broker.list_connections() {
        Ok(connections) => {
            let json: Vec<serde_json::Value> = connections
                .iter()
                .map(|c| serde_json::json!({
                    "ref_id": c.ref_id.to_string(),
                    "provider": c.provider,
                    "scopes": c.scopes,
                    "connected_at": c.connected_at.to_rfc3339(),
                    "status": c.status,
                }))
                .collect();
            (StatusCode::OK, Json(serde_json::json!(json)))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// DELETE /api/oauth/connections/:ref_id — disconnect.
pub async fn disconnect(
    State(state): State<Arc<AppState>>,
    Path(ref_id_str): Path<String>,
) -> impl IntoResponse {
    let ref_id = match uuid::Uuid::parse_str(&ref_id_str) {
        Ok(id) => ghost_oauth::OAuthRefId::from_uuid(id),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid ref_id format"})),
            );
        }
    };

    match state.oauth_broker.disconnect(&ref_id) {
        Ok(()) => {
            tracing::info!(ref_id = %ref_id_str, "OAuth connection disconnected");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "disconnected",
                    "ref_id": ref_id_str,
                })),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}
