use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Extension;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::codex::{
    get_account_status, login_with_chatgpt, logout_account, CodexAccount, CodexError,
    CodexLoginStart,
};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodexAccountView {
    ApiKey,
    Chatgpt { email: String, plan_type: String },
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CodexStatusResponse {
    pub requires_openai_auth: bool,
    pub account: Option<CodexAccountView>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CodexLoginStartResponse {
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CodexLogoutResponse {
    pub message: String,
}

fn admin_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown-admin")
}

fn map_account(account: CodexAccount) -> CodexAccountView {
    match account {
        CodexAccount::ApiKey => CodexAccountView::ApiKey,
        CodexAccount::Chatgpt { email, plan_type } => {
            CodexAccountView::Chatgpt { email, plan_type }
        }
    }
}

fn codex_api_error(error: CodexError) -> ApiError {
    match error {
        CodexError::Auth(message) => {
            ApiError::custom(StatusCode::CONFLICT, "CODEX_AUTH_REQUIRED", message)
        }
        CodexError::BinaryNotFound(message) => ApiError::custom(
            StatusCode::SERVICE_UNAVAILABLE,
            "CODEX_UNAVAILABLE",
            message,
        ),
        CodexError::Spawn(message) => {
            ApiError::custom(StatusCode::BAD_GATEWAY, "CODEX_SPAWN_FAILED", message)
        }
        CodexError::Io(message) => {
            ApiError::custom(StatusCode::BAD_GATEWAY, "CODEX_IO_FAILED", message)
        }
        CodexError::Protocol(message) => {
            ApiError::custom(StatusCode::BAD_GATEWAY, "CODEX_PROTOCOL_ERROR", message)
        }
        CodexError::Json(message) => {
            ApiError::custom(StatusCode::BAD_GATEWAY, "CODEX_JSON_ERROR", message)
        }
        CodexError::Server {
            code,
            message,
            data,
        } => {
            let details = serde_json::json!({
                "upstream_code": code,
                "upstream_data": data,
            });
            ApiError::with_details(
                StatusCode::BAD_GATEWAY,
                "CODEX_SERVER_ERROR",
                message,
                details,
            )
        }
    }
}

pub async fn get_status() -> ApiResult<CodexStatusResponse> {
    let status = get_account_status().await.map_err(codex_api_error)?;
    Ok(Json(CodexStatusResponse {
        requires_openai_auth: status.requires_openai_auth,
        account: status.account.map(map_account),
    }))
}

pub async fn start_login(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
) -> Response {
    let claims = claims.as_ref().map(|claims| &claims.0);
    let actor = admin_actor(claims);

    match login_with_chatgpt(false).await {
        Ok((login, _completion)) => {
            let response = match login {
                CodexLoginStart::Chatgpt { auth_url, login_id } => CodexLoginStartResponse {
                    auth_type: "chatgpt".into(),
                    auth_url: Some(auth_url),
                    login_id: Some(login_id),
                },
                CodexLoginStart::ApiKey => CodexLoginStartResponse {
                    auth_type: "api_key".into(),
                    auth_url: None,
                    login_id: None,
                },
                CodexLoginStart::ChatgptAuthTokens => CodexLoginStartResponse {
                    auth_type: "chatgpt_auth_tokens".into(),
                    auth_url: None,
                    login_id: None,
                },
            };

            let details = serde_json::to_value(&response).unwrap_or(serde_json::Value::Null);
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                "platform",
                "codex_login_start",
                "info",
                actor,
                "initiated",
                details.clone(),
                &operation_context,
                &IdempotencyStatus::Executed,
            );

            json_response_with_idempotency(StatusCode::OK, details, IdempotencyStatus::Executed)
        }
        Err(error) => error_response_with_idempotency(codex_api_error(error)),
    }
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
) -> Response {
    let claims = claims.as_ref().map(|claims| &claims.0);
    let actor = admin_actor(claims);

    match logout_account().await {
        Ok(()) => {
            let response = serde_json::to_value(CodexLogoutResponse {
                message: "Codex account logged out.".into(),
            })
            .unwrap_or(serde_json::Value::Null);
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                "platform",
                "codex_logout",
                "info",
                actor,
                "logged_out",
                response.clone(),
                &operation_context,
                &IdempotencyStatus::Executed,
            );

            json_response_with_idempotency(StatusCode::OK, response, IdempotencyStatus::Executed)
        }
        Err(error) => error_response_with_idempotency(codex_api_error(error)),
    }
}
