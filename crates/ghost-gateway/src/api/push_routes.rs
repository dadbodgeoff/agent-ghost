//! Push notification endpoints for PWA support (Task 22.3 / Phase 15.1).
//!
//! - `GET  /api/push/vapid-key` → return the VAPID public key
//! - `POST /api/push/subscribe` → register a push subscription
//! - `POST /api/push/unsubscribe` → remove a push subscription

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Shared state for push notification management.
#[derive(Clone)]
pub struct PushState {
    /// VAPID public key (base64url-encoded, raw P-256 point).
    pub vapid_public_key: String,
    /// Active subscriptions keyed by endpoint URL.
    pub subscriptions: Arc<Mutex<BTreeMap<String, PushSubscription>>>,
}

/// A Web Push subscription from a client.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PushSubscription {
    pub endpoint: String,
    #[serde(default)]
    pub keys: PushKeys,
}

/// Encryption keys for a push subscription.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PushKeys {
    #[serde(default)]
    pub p256dh: String,
    #[serde(default)]
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VapidKeyResponse {
    pub key: String,
}

/// Build the push notification router.
pub fn push_router(state: PushState) -> axum::Router {
    axum::Router::new()
        .route("/api/push/vapid-key", get(handle_vapid_key))
        .route("/api/push/subscribe", post(handle_subscribe))
        .route("/api/push/unsubscribe", post(handle_unsubscribe))
        .with_state(state)
}

/// GET /api/push/vapid-key — return the VAPID public key for client subscription.
async fn handle_vapid_key(State(state): State<PushState>) -> impl IntoResponse {
    Json(VapidKeyResponse {
        key: state.vapid_public_key,
    })
}

/// POST /api/push/subscribe — register a push subscription.
async fn handle_subscribe(
    State(state): State<PushState>,
    Json(sub): Json<PushSubscription>,
) -> impl IntoResponse {
    let endpoint = sub.endpoint.clone();
    match state.subscriptions.lock() {
        Ok(mut subs) => {
            subs.insert(endpoint.clone(), sub);
            tracing::info!(endpoint = %endpoint, "Push subscription registered");
            StatusCode::NO_CONTENT
        }
        Err(e) => {
            tracing::error!(error = %e, "Push subscription mutex poisoned");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// POST /api/push/unsubscribe — remove a push subscription.
async fn handle_unsubscribe(
    State(state): State<PushState>,
    Json(sub): Json<PushSubscription>,
) -> impl IntoResponse {
    match state.subscriptions.lock() {
        Ok(mut subs) => {
            let removed = subs.remove(&sub.endpoint).is_some();
            if removed {
                tracing::info!(endpoint = %sub.endpoint, "Push subscription removed");
            }
            StatusCode::NO_CONTENT
        }
        Err(e) => {
            tracing::error!(error = %e, "Push subscription mutex poisoned");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Generate a placeholder VAPID key pair.
///
/// In production, this would use the `ghost-secrets` provider to persist
/// the VAPID private key and derive the public key from it. For now,
/// we generate a deterministic placeholder from the ghost-signing keypair.
pub fn generate_vapid_public_key() -> String {
    // Use a deterministic seed so the key is stable across restarts.
    // In production, store via ghost-secrets and use the `web-push` crate.
    let seed = blake3::hash(b"ghost-vapid-key-seed");
    base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        &seed.as_bytes()[..32],
    )
}
