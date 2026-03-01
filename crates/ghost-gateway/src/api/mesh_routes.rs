//! Mesh A2A routes for ghost-gateway (Task 22.1).
//!
//! Wires `ghost_mesh::transport::a2a_server::A2ADispatcher` into the
//! gateway's axum router:
//! - `GET /.well-known/agent.json` → serve agent card
//! - `POST /a2a` → JSON-RPC 2.0 dispatch
//!
//! Auth: Ed25519 signature verification via `X-Ghost-Signature` header.

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;

use ghost_mesh::transport::a2a_server::{A2ADispatcher, A2AServerState};
use ghost_mesh::types::MeshMessage;

/// Maximum request body size (1 MB).
const MAX_BODY_SIZE: usize = 1_048_576;

/// Shared state for mesh routes.
#[derive(Clone)]
pub struct MeshRouteState {
    pub dispatcher: Arc<A2ADispatcher>,
    /// Known agent public keys for signature verification (agent_name → pubkey bytes).
    pub known_keys: Arc<Vec<Vec<u8>>>,
}

/// Build the mesh router with A2A endpoints.
///
/// Returns `None` if mesh is disabled (no routes registered).
pub fn mesh_router(state: Arc<Mutex<A2AServerState>>, known_keys: Vec<Vec<u8>>) -> axum::Router {
    let route_state = MeshRouteState {
        dispatcher: Arc::new(A2ADispatcher::new(state)),
        known_keys: Arc::new(known_keys),
    };

    axum::Router::new()
        .route("/.well-known/agent.json", get(handle_agent_card))
        .route("/a2a", post(handle_a2a))
        .with_state(route_state)
}

/// GET /.well-known/agent.json — serve this agent's signed card.
async fn handle_agent_card(
    State(state): State<MeshRouteState>,
) -> impl IntoResponse {
    match state.dispatcher.agent_card() {
        Some(card) => (StatusCode::OK, Json(card)).into_response(),
        None => {
            tracing::error!("Failed to retrieve agent card — state unavailable");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// POST /a2a — JSON-RPC 2.0 dispatch with Ed25519 signature verification.
async fn handle_a2a(
    State(state): State<MeshRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Reject oversized bodies
    if body.len() > MAX_BODY_SIZE {
        return (
            StatusCode::BAD_REQUEST,
            Json(MeshMessage::error_response(
                serde_json::json!(null),
                -32600,
                "request body too large (max 1MB)",
            )),
        );
    }

    // Verify Ed25519 signature from X-Ghost-Signature header
    let signature_header = headers
        .get("X-Ghost-Signature")
        .and_then(|v| v.to_str().ok());

    match signature_header {
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(MeshMessage::error_response(
                    serde_json::json!(null),
                    -32000,
                    "missing X-Ghost-Signature header",
                )),
            );
        }
        Some(sig_b64) => {
            if !verify_request_signature(sig_b64, &body, &state.known_keys) {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(MeshMessage::error_response(
                        serde_json::json!(null),
                        -32000,
                        "invalid signature",
                    )),
                );
            }
        }
    }

    // Parse JSON-RPC message
    let msg: MeshMessage = match serde_json::from_slice(&body) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %e, "Malformed JSON-RPC body on /a2a");
            return (
                StatusCode::BAD_REQUEST,
                Json(MeshMessage::error_response(
                    serde_json::json!(null),
                    -32700,
                    &format!("parse error: {e}"),
                )),
            );
        }
    };

    // Dispatch
    let response = state.dispatcher.dispatch(&msg);
    (StatusCode::OK, Json(response))
}

/// Verify an Ed25519 signature over the request body against known agent keys.
fn verify_request_signature(sig_b64: &str, body: &[u8], known_keys: &[Vec<u8>]) -> bool {
    // Decode base64 signature
    let sig_bytes = match base64_decode(sig_b64) {
        Some(b) => b,
        None => return false,
    };

    let Some(sig) = ghost_signing::Signature::from_bytes(&sig_bytes) else {
        return false;
    };

    // Try each known public key
    for pubkey_bytes in known_keys {
        if pubkey_bytes.len() != 32 {
            continue;
        }
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(pubkey_bytes);
        if let Some(vk) = ghost_signing::VerifyingKey::from_bytes(&key_arr) {
            if ghost_signing::verify(body, &sig, &vk) {
                return true;
            }
        }
    }
    false
}

/// Simple base64 decode (standard alphabet).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    // Use a minimal base64 decoder — ghost-gateway already has base64 via deps
    // For now, use the standard library approach via manual decode
    // In production this would use the `base64` crate
    let engine = base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    engine.decode(input).ok()
}
