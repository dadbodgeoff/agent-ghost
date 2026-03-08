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
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::error::ApiError;
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation,
    execute_idempotent_json_mutation, prepare_json_operation, start_operation_lease_heartbeat,
    PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::state::AppState;
use axum::extract::State;
use std::sync::Arc;

const CONNECT_ROUTE_TEMPLATE: &str = "/api/oauth/connect";
const EXECUTE_ROUTE_TEMPLATE: &str = "/api/oauth/execute";
const DISCONNECT_ROUTE_TEMPLATE: &str = "/api/oauth/connections/:ref_id";
const OAUTH_EXECUTE_ROUTE_KIND: &str = "oauth_execute_api_call";
const OAUTH_EXECUTE_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OAuthExecuteExecutionState {
    version: u32,
    ref_id: String,
    accepted_response: serde_json::Value,
    final_status_code: Option<u16>,
    final_response: Option<serde_json::Value>,
}

fn oauth_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

fn oauth_connect_error(error: ghost_oauth::OAuthError) -> ApiError {
    match error {
        ghost_oauth::OAuthError::ProviderError(message)
        | ghost_oauth::OAuthError::FlowFailed(message)
        | ghost_oauth::OAuthError::InvalidState(message)
        | ghost_oauth::OAuthError::RefreshFailed(message)
        | ghost_oauth::OAuthError::NotConnected(message)
        | ghost_oauth::OAuthError::TokenExpired(message)
        | ghost_oauth::OAuthError::TokenRevoked(message) => {
            ApiError::custom(StatusCode::BAD_REQUEST, "OAUTH_CONNECT_FAILED", message)
        }
        ghost_oauth::OAuthError::StorageError(message)
        | ghost_oauth::OAuthError::EncryptionError(message) => ApiError::custom(
            StatusCode::INTERNAL_SERVER_ERROR,
            "OAUTH_CONNECT_STORAGE_ERROR",
            message,
        ),
    }
}

fn oauth_disconnect_error(error: ghost_oauth::OAuthError) -> ApiError {
    match error {
        ghost_oauth::OAuthError::NotConnected(message)
        | ghost_oauth::OAuthError::ProviderError(message)
        | ghost_oauth::OAuthError::FlowFailed(message)
        | ghost_oauth::OAuthError::InvalidState(message)
        | ghost_oauth::OAuthError::RefreshFailed(message)
        | ghost_oauth::OAuthError::TokenExpired(message)
        | ghost_oauth::OAuthError::TokenRevoked(message) => {
            ApiError::custom(StatusCode::BAD_REQUEST, "OAUTH_DISCONNECT_FAILED", message)
        }
        ghost_oauth::OAuthError::StorageError(message)
        | ghost_oauth::OAuthError::EncryptionError(message) => ApiError::custom(
            StatusCode::INTERNAL_SERVER_ERROR,
            "OAUTH_DISCONNECT_STORAGE_ERROR",
            message,
        ),
    }
}

fn connect_ref_id(operation_context: &OperationContext) -> ghost_oauth::OAuthRefId {
    let seed = operation_context
        .operation_id
        .as_deref()
        .unwrap_or(operation_context.request_id.as_str());
    ghost_oauth::OAuthRefId::from_uuid(uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        seed.as_bytes(),
    ))
}

fn connect_state_nonce(operation_context: &OperationContext) -> String {
    let seed = operation_context
        .operation_id
        .as_deref()
        .unwrap_or(operation_context.request_id.as_str());
    blake3::hash(seed.as_bytes()).to_hex().to_string()
}

fn oauth_execute_error_body(message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({ "error": message.into() })
}

fn validate_execute_request(req: &ApiCallRequest) -> Result<(), ApiError> {
    let method = req.api_request.method.trim().to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH") {
        return Err(ApiError::custom(
            StatusCode::BAD_REQUEST,
            "OAUTH_EXECUTE_INVALID_METHOD",
            "api_request.method must be one of GET, POST, PUT, DELETE, or PATCH",
        ));
    }

    reqwest::Url::parse(req.api_request.url.trim()).map_err(|_| {
        ApiError::custom(
            StatusCode::BAD_REQUEST,
            "OAUTH_EXECUTE_INVALID_URL",
            "api_request.url must be a valid absolute URL",
        )
    })?;

    Ok(())
}

fn oauth_execute_accepted_body(ref_id: &str, execution_id: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "accepted",
        "ref_id": ref_id,
        "execution_id": execution_id,
    })
}

fn oauth_execute_recovery_body(state: &OAuthExecuteExecutionState) -> serde_json::Value {
    let mut body = state.accepted_response.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert("recovery_required".into(), serde_json::Value::Bool(true));
    }
    body
}

fn parse_oauth_execute_state(
    record: &cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
) -> Result<OAuthExecuteExecutionState, ApiError> {
    if record.state_version != OAUTH_EXECUTE_STATE_VERSION as i64 {
        return Err(ApiError::internal(format!(
            "unsupported oauth execute state version: {}",
            record.state_version
        )));
    }

    let state = serde_json::from_str::<OAuthExecuteExecutionState>(&record.state_json).map_err(
        |error| ApiError::internal(format!("failed to parse oauth execute state: {error}")),
    )?;
    if state.version != OAUTH_EXECUTE_STATE_VERSION {
        return Err(ApiError::internal(format!(
            "unsupported oauth execute state version: {}",
            state.version
        )));
    }
    Ok(state)
}

fn persist_oauth_execute_record(
    conn: &rusqlite::Connection,
    execution_id: &str,
    journal_id: &str,
    operation_id: &str,
    actor: &str,
    state: &OAuthExecuteExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::insert(
        conn,
        &cortex_storage::queries::live_execution_queries::NewLiveExecutionRecord {
            id: execution_id,
            journal_id,
            operation_id,
            route_kind: OAUTH_EXECUTE_ROUTE_KIND,
            actor_key: actor,
            state_version: OAUTH_EXECUTE_STATE_VERSION as i64,
            status: "accepted",
            state_json: &state_json,
        },
    )
    .map_err(|error| ApiError::db_error("insert_live_execution_record", error))
}

fn update_oauth_execute_state(
    conn: &rusqlite::Connection,
    execution_id: &str,
    status: &str,
    state: &OAuthExecuteExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::update_status_and_state(
        conn,
        execution_id,
        OAUTH_EXECUTE_STATE_VERSION as i64,
        status,
        &state_json,
    )
    .map_err(|error| ApiError::db_error("update_live_execution_record", error))
}

fn stored_oauth_execute_terminal_response(
    state: &OAuthExecuteExecutionState,
) -> Option<(StatusCode, serde_json::Value)> {
    if let (Some(status_code), Some(body)) = (state.final_status_code, state.final_response.clone())
    {
        return StatusCode::from_u16(status_code)
            .ok()
            .map(|status| (status, body));
    }

    None
}

fn oauth_execute_terminal_error(
    error: &ghost_oauth::OAuthError,
) -> Option<(StatusCode, serde_json::Value)> {
    let message = error.to_string();
    match error {
        ghost_oauth::OAuthError::NotConnected(_)
        | ghost_oauth::OAuthError::FlowFailed(_)
        | ghost_oauth::OAuthError::InvalidState(_) => {
            Some((StatusCode::NOT_FOUND, oauth_execute_error_body(message)))
        }
        ghost_oauth::OAuthError::TokenExpired(_) | ghost_oauth::OAuthError::TokenRevoked(_) => {
            Some((StatusCode::UNAUTHORIZED, oauth_execute_error_body(message)))
        }
        ghost_oauth::OAuthError::RefreshFailed(_) => {
            Some((StatusCode::BAD_GATEWAY, oauth_execute_error_body(message)))
        }
        ghost_oauth::OAuthError::StorageError(_) | ghost_oauth::OAuthError::EncryptionError(_) => {
            Some((
                StatusCode::INTERNAL_SERVER_ERROR,
                oauth_execute_error_body(message),
            ))
        }
        ghost_oauth::OAuthError::ProviderError(message)
            if message.starts_with("provider gone:")
                || message.starts_with("unknown provider:") =>
        {
            Some((StatusCode::BAD_GATEWAY, oauth_execute_error_body(message)))
        }
        ghost_oauth::OAuthError::ProviderError(_) => None,
    }
}

fn oauth_execute_audit_details(
    req: &ApiCallRequest,
    body: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ref_id": req.ref_id,
        "method": req.api_request.method,
        "url": req.api_request.url,
        "execution_id": body.get("execution_id").cloned().unwrap_or(serde_json::Value::Null),
        "upstream_status": body.get("status").cloned().unwrap_or(serde_json::Value::Null),
        "recovery_required": body.get("recovery_required").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "error": body.get("error").cloned().unwrap_or(serde_json::Value::Null),
    })
}

async fn finalize_oauth_execute_terminal_response(
    state: &Arc<AppState>,
    lease: &crate::api::idempotency::PreparedOperationLease,
    operation_context: &OperationContext,
    actor: &str,
    execution_id: &str,
    mut execution_state: OAuthExecuteExecutionState,
    status: StatusCode,
    body: serde_json::Value,
    req: &ApiCallRequest,
) -> Response {
    execution_state.final_status_code = Some(status.as_u16());
    execution_state.final_response = Some(body.clone());

    let db = state.db.write().await;
    if let Err(error) = update_oauth_execute_state(&db, execution_id, "completed", &execution_state)
    {
        return error_response_with_idempotency(error);
    }

    match commit_prepared_json_operation(&db, operation_context, lease, status, &body) {
        Ok(outcome) => {
            let audit_outcome = if status == StatusCode::OK {
                "completed"
            } else {
                "rejected"
            };
            write_mutation_audit_entry(
                &db,
                "platform",
                "oauth_execute_api_call",
                "high",
                actor,
                audit_outcome,
                oauth_execute_audit_details(req, &outcome.body),
                operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

async fn finalize_oauth_execute_recovery_response(
    state: &Arc<AppState>,
    lease: &crate::api::idempotency::PreparedOperationLease,
    operation_context: &OperationContext,
    actor: &str,
    execution_id: &str,
    execution_state: OAuthExecuteExecutionState,
    body: serde_json::Value,
    req: &ApiCallRequest,
) -> Response {
    let db = state.db.write().await;
    if let Err(error) =
        update_oauth_execute_state(&db, execution_id, "recovery_required", &execution_state)
    {
        return error_response_with_idempotency(error);
    }

    match commit_prepared_json_operation(&db, operation_context, lease, StatusCode::ACCEPTED, &body)
    {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "oauth_execute_api_call",
                "high",
                actor,
                "accepted",
                oauth_execute_audit_details(req, &outcome.body),
                operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/oauth/providers — list configured providers.
pub async fn list_providers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let names = state.oauth_broker.provider_names();
    let providers: Vec<serde_json::Value> = names
        .iter()
        .map(|name| serde_json::json!({"name": name}))
        .collect();
    (StatusCode::OK, Json(serde_json::json!(providers)))
}

/// Request body for POST /api/oauth/connect.
#[derive(Deserialize, Serialize)]
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
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<ConnectRequest>,
) -> Response {
    if req.provider.trim().is_empty() {
        return error_response_with_idempotency(ApiError::custom(
            StatusCode::BAD_REQUEST,
            "OAUTH_PROVIDER_REQUIRED",
            "provider must not be empty",
        ));
    }

    let actor = oauth_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::to_value(&req).unwrap_or(serde_json::Value::Null);
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CONNECT_ROUTE_TEMPLATE,
        &request_body,
        |_| {
            let ref_id = connect_ref_id(&operation_context);
            let state_nonce = connect_state_nonce(&operation_context);
            let (auth_url, ref_id) = state
                .oauth_broker
                .connect_with_ref_id(
                    &req.provider,
                    &req.scopes,
                    &req.redirect_uri,
                    ref_id,
                    &state_nonce,
                )
                .map_err(oauth_connect_error)?;
            Ok((
                StatusCode::OK,
                serde_json::json!({
                    "authorization_url": auth_url,
                    "ref_id": ref_id.to_string(),
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "oauth_connect",
                "medium",
                actor,
                match &outcome.idempotency_status {
                    IdempotencyStatus::Executed => "initiated",
                    IdempotencyStatus::Replayed => "replayed",
                    IdempotencyStatus::InProgress => "in_progress",
                    IdempotencyStatus::Mismatch => "mismatch",
                },
                serde_json::json!({
                    "provider": req.provider,
                    "scopes": req.scopes,
                    "redirect_uri": req.redirect_uri,
                    "ref_id": outcome.body.get("ref_id").cloned().unwrap_or(serde_json::Value::Null),
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
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
pub async fn list_connections(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.oauth_broker.list_connections() {
        Ok(connections) => {
            let json: Vec<serde_json::Value> = connections
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "ref_id": c.ref_id.to_string(),
                        "provider": c.provider,
                        "scopes": c.scopes,
                        "connected_at": c.connected_at.to_rfc3339(),
                        "status": c.status,
                    })
                })
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
#[derive(Deserialize, Serialize)]
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
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<ApiCallRequest>,
) -> Response {
    let ref_id = match uuid::Uuid::parse_str(&req.ref_id) {
        Ok(id) => ghost_oauth::OAuthRefId::from_uuid(id),
        Err(_) => {
            return error_response_with_idempotency(ApiError::custom(
                StatusCode::BAD_REQUEST,
                "OAUTH_INVALID_REF_ID",
                "invalid ref_id format",
            ));
        }
    };

    if let Err(error) = validate_execute_request(&req) {
        return error_response_with_idempotency(error);
    }

    let actor = oauth_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::to_value(&req).unwrap_or(serde_json::Value::Null);
    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            EXECUTE_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                "platform",
                "oauth_execute_api_call",
                "high",
                actor,
                "replayed",
                oauth_execute_audit_details(&req, &stored.body),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": EXECUTE_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            let operation_id = operation_context
                .operation_id
                .clone()
                .expect("prepared operations require operation_id");

            let execution_record = {
                let db = state.db.write().await;
                match cortex_storage::queries::live_execution_queries::get_by_journal_id(
                    &db,
                    &lease.journal_id,
                ) {
                    Ok(Some(record)) => Some(record),
                    Ok(None) => {
                        let execution_id = uuid::Uuid::now_v7().to_string();
                        let execution_state = OAuthExecuteExecutionState {
                            version: OAUTH_EXECUTE_STATE_VERSION,
                            ref_id: req.ref_id.clone(),
                            accepted_response: oauth_execute_accepted_body(
                                &req.ref_id,
                                &execution_id,
                            ),
                            final_status_code: None,
                            final_response: None,
                        };
                        if let Err(error) = persist_oauth_execute_record(
                            &db,
                            &execution_id,
                            &lease.journal_id,
                            &operation_id,
                            actor,
                            &execution_state,
                        ) {
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(error);
                        }

                        Some(
                            cortex_storage::queries::live_execution_queries::LiveExecutionRecord {
                                id: execution_id,
                                journal_id: lease.journal_id.clone(),
                                operation_id: operation_id.clone(),
                                route_kind: OAUTH_EXECUTE_ROUTE_KIND.to_string(),
                                actor_key: actor.to_string(),
                                state_version: OAUTH_EXECUTE_STATE_VERSION as i64,
                                status: "accepted".to_string(),
                                state_json: serde_json::to_string(&execution_state)
                                    .unwrap_or_else(|_| "{}".to_string()),
                                created_at: String::new(),
                                updated_at: String::new(),
                            },
                        )
                    }
                    Err(error) => {
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(ApiError::db_error(
                            "load_live_execution_record",
                            error,
                        ));
                    }
                }
            };

            let execution_record =
                execution_record.expect("oauth execute execution record must exist");
            let execution_state = match parse_oauth_execute_state(&execution_record) {
                Ok(state) => state,
                Err(error) => {
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(error);
                }
            };

            match execution_record.status.as_str() {
                "completed" => {
                    if let Some((status, body)) =
                        stored_oauth_execute_terminal_response(&execution_state)
                    {
                        return finalize_oauth_execute_terminal_response(
                            &state,
                            &lease,
                            &operation_context,
                            actor,
                            &execution_record.id,
                            execution_state,
                            status,
                            body,
                            &req,
                        )
                        .await;
                    }

                    let recovery_body = oauth_execute_recovery_body(&execution_state);
                    return finalize_oauth_execute_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        actor,
                        &execution_record.id,
                        execution_state,
                        recovery_body,
                        &req,
                    )
                    .await;
                }
                "running" | "recovery_required" => {
                    let recovery_body = oauth_execute_recovery_body(&execution_state);
                    return finalize_oauth_execute_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        actor,
                        &execution_record.id,
                        execution_state,
                        recovery_body,
                        &req,
                    )
                    .await;
                }
                "accepted" => {}
                other => {
                    return error_response_with_idempotency(ApiError::internal(format!(
                        "unsupported oauth execute status: {other}"
                    )));
                }
            }

            {
                let db = state.db.write().await;
                if let Err(error) = update_oauth_execute_state(
                    &db,
                    &execution_record.id,
                    "running",
                    &execution_state,
                ) {
                    return error_response_with_idempotency(error);
                }
            }

            let heartbeat = start_operation_lease_heartbeat(Arc::clone(&state.db), lease.clone());
            match state.oauth_broker.execute(&ref_id, &req.api_request) {
                Ok(response) => {
                    if let Err(error) = heartbeat.stop().await {
                        return error_response_with_idempotency(error);
                    }
                    tracing::info!(
                        ref_id = %req.ref_id,
                        method = %req.api_request.method,
                        url = %req.api_request.url,
                        upstream_status = response.status,
                        "OAuth API call executed"
                    );
                    finalize_oauth_execute_terminal_response(
                        &state,
                        &lease,
                        &operation_context,
                        actor,
                        &execution_record.id,
                        execution_state,
                        StatusCode::OK,
                        serde_json::json!({
                            "status": response.status,
                            "headers": response.headers,
                            "body": response.body,
                        }),
                        &req,
                    )
                    .await
                }
                Err(error) => {
                    if let Err(heartbeat_error) = heartbeat.stop().await {
                        return error_response_with_idempotency(heartbeat_error);
                    }
                    if let Some((status, body)) = oauth_execute_terminal_error(&error) {
                        return finalize_oauth_execute_terminal_response(
                            &state,
                            &lease,
                            &operation_context,
                            actor,
                            &execution_record.id,
                            execution_state,
                            status,
                            body,
                            &req,
                        )
                        .await;
                    }

                    let recovery_body = oauth_execute_recovery_body(&execution_state);
                    tracing::warn!(
                        operation_id = %operation_id,
                        ref_id = %req.ref_id,
                        error = %error,
                        "oauth execute entered recovery-required state"
                    );
                    finalize_oauth_execute_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        actor,
                        &execution_record.id,
                        execution_state,
                        recovery_body,
                        &req,
                    )
                    .await
                }
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// DELETE /api/oauth/connections/:ref_id — disconnect.
pub async fn disconnect(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(ref_id_str): Path<String>,
) -> Response {
    let ref_id = match uuid::Uuid::parse_str(&ref_id_str) {
        Ok(id) => ghost_oauth::OAuthRefId::from_uuid(id),
        Err(_) => {
            return error_response_with_idempotency(ApiError::custom(
                StatusCode::BAD_REQUEST,
                "OAUTH_INVALID_REF_ID",
                "invalid ref_id format",
            ));
        }
    };

    let actor = oauth_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "ref_id": ref_id_str.clone() });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "DELETE",
        DISCONNECT_ROUTE_TEMPLATE,
        &request_body,
        |_| {
            state
                .oauth_broker
                .disconnect(&ref_id)
                .map_err(oauth_disconnect_error)?;
            tracing::info!(ref_id = %ref_id_str, "OAuth connection disconnected");
            Ok((
                StatusCode::OK,
                serde_json::json!({
                    "status": "disconnected",
                    "ref_id": ref_id_str,
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "oauth_disconnect",
                "high",
                actor,
                match &outcome.idempotency_status {
                    IdempotencyStatus::Executed => "disconnected",
                    IdempotencyStatus::Replayed => "replayed",
                    IdempotencyStatus::InProgress => "in_progress",
                    IdempotencyStatus::Mismatch => "mismatch",
                },
                serde_json::json!({
                    "ref_id": ref_id_str,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}
