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
    /// Optional DB pool for delegation state persistence.
    pub db: Option<Arc<crate::db_pool::DbPool>>,
}

/// Build the mesh router with A2A endpoints.
///
/// Returns `None` if mesh is disabled (no routes registered).
pub fn mesh_router(state: Arc<Mutex<A2AServerState>>, known_keys: Vec<Vec<u8>>) -> axum::Router {
    mesh_router_with_db(state, known_keys, None)
}

/// Build the mesh router with A2A endpoints and optional DB for delegation persistence.
pub fn mesh_router_with_db(
    state: Arc<Mutex<A2AServerState>>,
    known_keys: Vec<Vec<u8>>,
    db: Option<Arc<crate::db_pool::DbPool>>,
) -> axum::Router {
    let route_state = MeshRouteState {
        dispatcher: Arc::new(A2ADispatcher::new(state)),
        known_keys: Arc::new(known_keys),
        db,
    };

    axum::Router::new()
        .route("/.well-known/agent.json", get(handle_agent_card))
        .route("/a2a", post(handle_a2a))
        .with_state(route_state)
}

/// GET /.well-known/agent.json — serve this agent's signed card.
async fn handle_agent_card(State(state): State<MeshRouteState>) -> impl IntoResponse {
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

    // Persist delegation state transitions for delegation-related methods.
    if let Some(ref db) = state.db {
        persist_delegation_from_message(&msg, db).await;
    }

    (StatusCode::OK, Json(response))
}

/// Persist delegation state transitions based on the incoming A2A message.
async fn persist_delegation_from_message(msg: &MeshMessage, db: &Arc<crate::db_pool::DbPool>) {
    let conn = db.write().await;
    let params = match &msg.params {
        Some(p) => p,
        None => return,
    };
    let method = msg.method.as_str();
    match method {
        "tasks/send" | "tasks/sendSubscribe" => {
            // New task = new delegation offer.
            let id = uuid::Uuid::now_v7().to_string();
            let delegation_id = params
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&id);
            let sender = params
                .get("sender_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let recipient = params
                .get("recipient_id")
                .and_then(|v| v.as_str())
                .unwrap_or("self");
            let task = params
                .get("task")
                .or_else(|| params.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("a2a_task");
            let event_hash = blake3::hash(id.as_bytes());
            if let Err(e) = cortex_storage::queries::delegation_state_queries::insert_delegation(
                &conn,
                &id,
                delegation_id,
                sender,
                recipient,
                task,
                &msg.id.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                event_hash.as_bytes(),
                &[0u8; 32],
            ) {
                tracing::warn!(error = %e, "failed to persist delegation offer from A2A");
            }
        }
        "tasks/cancel" => {
            // Cancel = transition to Rejected (lookup by delegation_id since task_id
            // from A2A params corresponds to delegation_id, not the internal row id).
            if let Some(task_id) = params.get("task_id").and_then(|v| v.as_str()) {
                if let Err(e) =
                    cortex_storage::queries::delegation_state_queries::transition_by_delegation_id(
                        &conn,
                        task_id,
                        "Rejected",
                        None,
                        None,
                        None,
                        Some("canceled via A2A"),
                    )
                {
                    tracing::warn!(error = %e, task_id = %task_id, "failed to persist delegation cancel");
                }
            }
        }
        _ => {}
    }
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
