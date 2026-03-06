//! Provider API key management endpoints.
//!
//! Admin-only endpoints to set, list, and delete LLM provider API keys
//! through the dashboard UI. Keys are stored via the configured
//! `SecretProvider` (keychain/vault) and injected as env vars for
//! immediate pickup by the provider builder.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

// ── Auth helper (mirrors admin.rs) ──────────────────────────────────

fn require_admin(ext: &axum::http::Extensions) -> Result<(), ApiError> {
    if let Some(claims) = ext.get::<Claims>() {
        if claims.role == "admin" {
            return Ok(());
        }
    }
    Err(ApiError {
        status: axum::http::StatusCode::FORBIDDEN,
        body: crate::api::error::ErrorResponse::new(
            "FORBIDDEN",
            "Admin role required for this operation",
        ),
    })
}

// ── Types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ProviderKeyInfo {
    /// Provider name from config (e.g. "openai_compat", "anthropic").
    pub provider_name: String,
    /// Model configured for this provider.
    pub model: String,
    /// Environment variable name for the API key.
    pub env_name: String,
    /// Whether the key is currently set.
    pub is_set: bool,
    /// Masked preview (e.g. "xai-...zkZKu"), null if not set.
    pub preview: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderKeysResponse {
    pub providers: Vec<ProviderKeyInfo>,
}

#[derive(Debug, Deserialize)]
pub struct SetKeyRequest {
    /// The environment variable name (must match a configured provider).
    pub env_name: String,
    /// The API key value.
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct SetKeyResponse {
    pub env_name: String,
    pub preview: String,
    pub message: String,
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Mask an API key for display: show first 4 + last 4 chars.
fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "*".repeat(key.len());
    }
    let prefix = &key[..4];
    let suffix = &key[key.len() - 4..];
    format!("{prefix}...{suffix}")
}

/// Providers that require an API key (ollama is local, no key needed).
fn needs_api_key(provider_name: &str) -> bool {
    !matches!(provider_name, "ollama")
}

/// Default env var name for a provider.
fn default_key_env(provider_name: &str) -> &str {
    match provider_name {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        _ => "OPENAI_API_KEY",
    }
}

// ── Handlers ────────────────────────────────────────────────────────

/// GET /api/admin/provider-keys — list all configured providers and key status.
pub async fn list_provider_keys(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> ApiResult<ProviderKeysResponse> {
    require_admin(request.extensions())?;

    let mut providers = Vec::new();

    for p in &state.model_providers {
        if !needs_api_key(&p.name) {
            providers.push(ProviderKeyInfo {
                provider_name: p.name.clone(),
                model: p.model.clone().unwrap_or_default(),
                env_name: String::new(),
                is_set: true, // local providers are always "set"
                preview: None,
            });
            continue;
        }

        let env_name = p
            .api_key_env
            .clone()
            .unwrap_or_else(|| default_key_env(&p.name).to_string());

        let current_value = std::env::var(&env_name).ok().filter(|v| !v.is_empty());

        providers.push(ProviderKeyInfo {
            provider_name: p.name.clone(),
            model: p.model.clone().unwrap_or_default(),
            env_name,
            is_set: current_value.is_some(),
            preview: current_value.as_deref().map(mask_key),
        });
    }

    Ok(Json(ProviderKeysResponse { providers }))
}

/// PUT /api/admin/provider-keys — set a provider API key.
pub async fn set_provider_key(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> ApiResult<SetKeyResponse> {
    require_admin(request.extensions())?;

    let body: SetKeyRequest = axum::Json::from_bytes(
        &axum::body::to_bytes(request.into_body(), 4096)
            .await
            .map_err(|_| ApiError::bad_request("invalid request body"))?,
    )
    .map_err(|_| ApiError::bad_request("invalid JSON: expected { env_name, value }"))?
    .0;

    if body.env_name.is_empty() || body.value.is_empty() {
        return Err(ApiError::bad_request("env_name and value must not be empty"));
    }

    // Validate that env_name matches a configured provider.
    let valid = state.model_providers.iter().any(|p| {
        if !needs_api_key(&p.name) {
            return false;
        }
        let expected = p
            .api_key_env
            .as_deref()
            .unwrap_or_else(|| default_key_env(&p.name));
        expected == body.env_name
    });

    if !valid {
        return Err(ApiError::bad_request(
            "env_name does not match any configured provider's api_key_env",
        ));
    }

    // 1. Persist via secret_provider (if it supports writes).
    if let Err(e) = state.secret_provider.set_secret(&body.env_name, &body.value) {
        tracing::warn!(
            env_name = %body.env_name,
            error = %e,
            "Could not persist key to secret provider (will still set env var)"
        );
    }

    // 2. Set env var for immediate pickup by provider builders.
    // SAFETY: This is single-threaded at the point of the HTTP handler;
    // the env var is read fresh by build_fallback_chain_from_providers()
    // on each subsequent request.
    unsafe {
        std::env::set_var(&body.env_name, &body.value);
    }

    let preview = mask_key(&body.value);
    tracing::info!(env_name = %body.env_name, preview = %preview, "Provider API key updated via dashboard");

    Ok(Json(SetKeyResponse {
        env_name: body.env_name,
        preview,
        message: "API key saved successfully".into(),
    }))
}

/// DELETE /api/admin/provider-keys/:env_name — remove a provider API key.
pub async fn delete_provider_key(
    State(state): State<Arc<AppState>>,
    Path(env_name): Path<String>,
    request: axum::http::Request<axum::body::Body>,
) -> ApiResult<serde_json::Value> {
    require_admin(request.extensions())?;

    // Validate env_name matches a configured provider.
    let valid = state.model_providers.iter().any(|p| {
        if !needs_api_key(&p.name) {
            return false;
        }
        let expected = p
            .api_key_env
            .as_deref()
            .unwrap_or_else(|| default_key_env(&p.name));
        expected == env_name
    });

    if !valid {
        return Err(ApiError::bad_request(
            "env_name does not match any configured provider",
        ));
    }

    // 1. Remove from secret_provider.
    if let Err(e) = state.secret_provider.delete_secret(&env_name) {
        tracing::warn!(env_name = %env_name, error = %e, "Could not remove key from secret provider");
    }

    // 2. Remove env var.
    unsafe {
        std::env::remove_var(&env_name);
    }

    tracing::info!(env_name = %env_name, "Provider API key removed via dashboard");

    Ok(Json(serde_json::json!({
        "env_name": env_name,
        "message": "API key removed"
    })))
}
