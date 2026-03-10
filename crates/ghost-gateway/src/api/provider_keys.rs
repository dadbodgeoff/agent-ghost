//! Provider API key management endpoints.
//!
//! Admin-only endpoints to set, list, and delete LLM provider API keys
//! through the dashboard UI. Keys are stored via the configured
//! `SecretProvider` (keychain/vault) and injected as env vars for
//! immediate pickup by the provider builder.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::OperationContext;
use crate::state::AppState;

const SET_PROVIDER_KEY_ROUTE_TEMPLATE: &str = "/api/admin/provider-keys";
const DELETE_PROVIDER_KEY_ROUTE_TEMPLATE: &str = "/api/admin/provider-keys/:env_name";

fn actor_id(ext: &axum::http::Extensions) -> &str {
    ext.get::<Claims>()
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

// ── Types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
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

#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderKeysResponse {
    pub providers: Vec<ProviderKeyInfo>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SetKeyRequest {
    /// The environment variable name (must match a configured provider).
    pub env_name: String,
    /// The API key value.
    pub value: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetKeyResponse {
    pub env_name: String,
    pub preview: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteKeyResponse {
    pub env_name: String,
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

/// Default env var name for a provider.
fn default_key_env(provider_name: &str) -> &str {
    match provider_name {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" | "openai_compat" => "OPENAI_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        _ => "OPENAI_API_KEY",
    }
}

fn provider_key_env(provider: &crate::config::ProviderConfig) -> Option<String> {
    match provider.name.as_str() {
        "ollama" => None,
        "codex" => provider.api_key_env.clone(),
        _ => Some(
            provider
                .api_key_env
                .clone()
                .unwrap_or_else(|| default_key_env(&provider.name).to_string()),
        ),
    }
}

// ── Handlers ────────────────────────────────────────────────────────

/// GET /api/admin/provider-keys — list all configured providers and key status.
pub async fn list_provider_keys(
    State(state): State<Arc<AppState>>,
    _request: axum::http::Request<axum::body::Body>,
) -> ApiResult<ProviderKeysResponse> {
    let mut providers = Vec::new();

    for p in &state.model_providers {
        if let Some(env_name) = provider_key_env(p) {
            let current_value = crate::state::get_api_key(&env_name);

            providers.push(ProviderKeyInfo {
                provider_name: p.name.clone(),
                model: p.model.clone().unwrap_or_default(),
                env_name,
                is_set: current_value.is_some(),
                preview: current_value.as_deref().map(mask_key),
            });
        } else {
            providers.push(ProviderKeyInfo {
                provider_name: p.name.clone(),
                model: p.model.clone().unwrap_or_default(),
                env_name: String::new(),
                is_set: true,
                preview: None,
            });
        }
    }

    Ok(Json(ProviderKeysResponse { providers }))
}

/// PUT /api/admin/provider-keys — set a provider API key.
pub async fn set_provider_key(
    State(state): State<Arc<AppState>>,
    axum::Extension(operation_context): axum::Extension<OperationContext>,
    request: axum::http::Request<axum::body::Body>,
) -> Response {
    let actor = actor_id(request.extensions()).to_string();

    let body_bytes = match axum::body::to_bytes(request.into_body(), 4096).await {
        Ok(body_bytes) => body_bytes,
        Err(_) => {
            return error_response_with_idempotency(ApiError::bad_request("invalid request body"))
        }
    };
    let body: SetKeyRequest = match axum::Json::from_bytes(&body_bytes) {
        Ok(body) => body.0,
        Err(_) => {
            return error_response_with_idempotency(ApiError::bad_request(
                "invalid JSON: expected { env_name, value }",
            ))
        }
    };

    if body.env_name.is_empty() || body.value.is_empty() {
        return error_response_with_idempotency(ApiError::bad_request(
            "env_name and value must not be empty",
        ));
    }

    // Validate that env_name matches a configured provider.
    let valid = state
        .model_providers
        .iter()
        .filter_map(provider_key_env)
        .any(|expected| expected == body.env_name);

    if !valid {
        return error_response_with_idempotency(ApiError::bad_request(
            "env_name does not match any configured provider's api_key_env",
        ));
    }

    let preview = mask_key(&body.value);
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        &actor,
        "PUT",
        SET_PROVIDER_KEY_ROUTE_TEMPLATE,
        &serde_json::to_value(&body).unwrap_or(serde_json::Value::Null),
        |_| {
            if let Err(error) = state
                .secret_provider
                .set_secret(&body.env_name, &body.value)
            {
                tracing::warn!(
                    env_name = %body.env_name,
                    error = %error,
                    "Could not persist key to secret provider (will still set env var)"
                );
            }

            crate::state::set_api_key(&body.env_name, &body.value);
            tracing::info!(
                env_name = %body.env_name,
                preview = %preview,
                "Provider API key updated via dashboard"
            );

            Ok((
                axum::http::StatusCode::OK,
                serde_json::to_value(SetKeyResponse {
                    env_name: body.env_name.clone(),
                    preview: preview.clone(),
                    message: "API key saved successfully".to_string(),
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "provider_key_set",
                "high",
                &actor,
                "saved",
                serde_json::json!({
                    "env_name": body.env_name,
                    "preview": preview,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// DELETE /api/admin/provider-keys/:env_name — remove a provider API key.
pub async fn delete_provider_key(
    State(state): State<Arc<AppState>>,
    axum::Extension(operation_context): axum::Extension<OperationContext>,
    Path(env_name): Path<String>,
    request: axum::http::Request<axum::body::Body>,
) -> Response {
    let actor = actor_id(request.extensions()).to_string();

    // Validate env_name matches a configured provider.
    let valid = state
        .model_providers
        .iter()
        .filter_map(provider_key_env)
        .any(|expected| expected == env_name);

    if !valid {
        return error_response_with_idempotency(ApiError::bad_request(
            "env_name does not match any configured provider",
        ));
    }

    let db = state.db.write().await;
    let request_body = serde_json::json!({ "env_name": env_name });

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        &actor,
        "DELETE",
        DELETE_PROVIDER_KEY_ROUTE_TEMPLATE,
        &request_body,
        |_| {
            if let Err(error) = state.secret_provider.delete_secret(&env_name) {
                tracing::warn!(
                    env_name = %env_name,
                    error = %error,
                    "Could not remove key from secret provider"
                );
            }

            crate::state::remove_api_key(&env_name);
            tracing::info!(env_name = %env_name, "Provider API key removed via dashboard");

            Ok((
                axum::http::StatusCode::OK,
                serde_json::to_value(DeleteKeyResponse {
                    env_name: env_name.clone(),
                    message: "API key removed".to_string(),
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "provider_key_delete",
                "high",
                &actor,
                "removed",
                serde_json::json!({ "env_name": env_name }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ProviderConfig;

    use super::provider_key_env;

    #[test]
    fn codex_without_api_key_env_is_managed_outside_provider_keys() {
        let provider = ProviderConfig {
            name: "codex".into(),
            api_key_env: None,
            model: None,
            base_url: None,
        };

        assert_eq!(provider_key_env(&provider), None);
    }

    #[test]
    fn codex_with_api_key_env_is_exposed_in_provider_keys() {
        let provider = ProviderConfig {
            name: "codex".into(),
            api_key_env: Some("OPENAI_API_KEY".into()),
            model: None,
            base_url: None,
        };

        assert_eq!(
            provider_key_env(&provider).as_deref(),
            Some("OPENAI_API_KEY")
        );
    }
}
