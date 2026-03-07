//! OAuth API endpoints wired to `ghost_oauth::OAuthBroker`.
//!
//! - `GET  /api/oauth/providers`          — list configured providers
//! - `POST /api/oauth/connect`            — initiate OAuth flow
//! - `GET  /api/oauth/callback`           — OAuth redirect handler
//! - `GET  /api/oauth/connections`        — list active connections
//! - `DELETE /api/oauth/connections/:ref_id` — disconnect (revoke + delete)
//! - `POST /api/oauth/execute`            — execute API call through OAuth connection

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
    let port = crate::state::get_api_key("GHOST_GATEWAY_PORT").unwrap_or_else(|| "39780".into());
    format!("http://localhost:{port}/api/oauth/callback")
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

/// Request body for POST /api/oauth/execute.
#[derive(Deserialize)]
pub struct ApiCallRequest {
    /// OAuth connection reference (UUID string from a prior `/connect` flow).
    pub ref_id: String,
    /// The upstream API request to execute through this OAuth connection.
    pub api_request: ghost_oauth::ApiRequest,
}

/// POST /api/oauth/execute — execute an API call through an OAuth connection.
///
/// The broker injects the stored Bearer token into the request, executes it
/// against the upstream provider, and returns the raw response. The agent
/// never sees the access token.
pub async fn execute_api_call(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApiCallRequest>,
) -> impl IntoResponse {
    let ref_id = match uuid::Uuid::parse_str(&req.ref_id) {
        Ok(id) => ghost_oauth::OAuthRefId::from_uuid(id),
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid ref_id format"})),
            );
        }
    };

    match state.oauth_broker.execute(&ref_id, &req.api_request) {
        Ok(response) => {
            tracing::info!(
                ref_id = %req.ref_id,
                method = %req.api_request.method,
                url = %req.api_request.url,
                upstream_status = response.status,
                "OAuth API call executed"
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": response.status,
                    "headers": response.headers,
                    "body": response.body,
                })),
            )
        }
        Err(e) => {
            let status = match &e {
                ghost_oauth::OAuthError::TokenExpired(_) => StatusCode::UNAUTHORIZED,
                ghost_oauth::OAuthError::TokenRevoked(_) => StatusCode::UNAUTHORIZED,
                ghost_oauth::OAuthError::NotConnected(_) => StatusCode::NOT_FOUND,
                ghost_oauth::OAuthError::ProviderError(_) => StatusCode::BAD_GATEWAY,
                ghost_oauth::OAuthError::RefreshFailed(_) => StatusCode::BAD_GATEWAY,
                ghost_oauth::OAuthError::StorageError(_)
                | ghost_oauth::OAuthError::EncryptionError(_) => {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
                _ => StatusCode::BAD_REQUEST,
            };
            (status, Json(serde_json::json!({"error": e.to_string()})))
        }
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
