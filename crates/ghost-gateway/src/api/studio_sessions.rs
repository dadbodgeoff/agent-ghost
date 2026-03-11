//! Studio chat session persistence endpoints.
//!
//! DB-backed chat sessions with OutputInspector safety scanning
//! on both user input and LLM responses.
//!
//! Routes:
//!   GET    /api/studio/sessions                  — list sessions
//!   POST   /api/studio/sessions                  — create session
//!   GET    /api/studio/sessions/:id              — get session with messages
//!   DELETE /api/studio/sessions/:id              — delete session
//!   POST   /api/studio/sessions/:id/messages     — send message (blocking)
//!   POST   /api/studio/sessions/:id/messages/stream — send message (SSE streaming)
//!   GET    /api/studio/sessions/:id/stream/recover — recover missed stream events

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::response::Response;
use axum::Extension;
use axum::Json;
use base64::Engine;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use ghost_agent_loop::output_inspector::InspectionResult;
use ghost_agent_loop::runner::{AgentStreamErrorType, AgentStreamEvent};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult, ErrorResponse};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation,
    execute_idempotent_json_mutation, prepare_json_operation, start_operation_lease_heartbeat,
    PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, response_with_idempotency,
    write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::runtime_execution::{
    execute_blocking_turn, inspect_text_safety, inspection_safety_status, map_runner_error,
    map_runtime_safety_error, pre_loop_blocking_turn, prepare_stored_runtime_execution,
    PreparedRuntimeExecution,
};
use crate::api::stream_runtime::execute_streaming_turn;
use crate::api::websocket::WsEvent;
use crate::runtime_safety::{
    parse_or_stable_uuid, RunnerBuildOptions, RuntimeSafetyBuilder, STUDIO_SYNTHETIC_AGENT_NAME,
};
use crate::state::AppState;

const CREATE_SESSION_ROUTE_TEMPLATE: &str = "/api/studio/sessions";
const DELETE_SESSION_ROUTE_TEMPLATE: &str = "/api/studio/sessions/:id";
const SEND_MESSAGE_ROUTE_TEMPLATE: &str = "/api/studio/sessions/:id/messages";
const SEND_MESSAGE_STREAM_ROUTE_TEMPLATE: &str = "/api/studio/sessions/:id/messages/stream";
const STUDIO_MESSAGE_ROUTE_KIND: &str = "studio_send_message";
const STUDIO_MESSAGE_STREAM_ROUTE_KIND: &str = "studio_send_message_stream";
const STUDIO_MESSAGE_EXECUTION_STATE_VERSION: u32 = 1;
const STUDIO_STREAM_EXECUTION_STATE_VERSION: u32 = 1;
const EXECUTION_ACTIVE_STATUSES: &[&str] = &[
    "accepted",
    "preparing",
    "running",
    "recovery_required",
    "cancel_requested",
];

// ── Request / Response types ───────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateSessionRequest {
    pub agent_id: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub model: String,
    pub system_prompt: String,
    pub temperature: f64,
    pub max_tokens: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SessionWithMessagesResponse {
    #[serde(flatten)]
    pub session: SessionResponse,
    pub messages: Vec<MessageResponse>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub role: String,
    pub content: String,
    pub token_count: i64,
    pub safety_status: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    /// Optional model override for this message.
    pub model: Option<String>,
    /// Optional temperature override.
    pub temperature: Option<f64>,
    /// Optional max_tokens override.
    pub max_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub user_message: MessageResponse,
    pub assistant_message: MessageResponse,
    pub safety_status: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    /// Optional lower bound on `last_activity_at`.
    pub active_since: Option<String>,
    /// Opaque cursor returned from a previous session-list page.
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionResponse>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

// ── Stream recovery types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RecoverStreamQuery {
    pub message_id: String,
    pub after_seq: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RecoverStreamResponse {
    pub events: Vec<StreamEventApiResponse>,
}

#[derive(Debug, Serialize)]
pub struct StreamEventApiResponse {
    pub seq: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconstructed: Option<bool>,
}

// ── Handlers ───────────────────────────────────────────────────────

fn studio_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

/// POST /api/studio/sessions — create a new chat session.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<CreateSessionRequest>,
) -> Response {
    let actor = studio_actor(claims.as_ref().map(|claims| &claims.0));
    let agent = RuntimeSafetyBuilder::new(&state)
        .resolve_agent(req.agent_id.as_deref(), STUDIO_SYNTHETIC_AGENT_NAME)
        .map_err(map_runtime_safety_error);
    let Ok(agent) = agent else {
        return error_response_with_idempotency(agent.err().unwrap());
    };
    let request_body = serde_json::to_value(&req).unwrap_or(serde_json::Value::Null);
    let id = operation_context
        .operation_id
        .clone()
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    let title = req.title.unwrap_or_else(|| "New Chat".into());
    let model = req.model.unwrap_or_else(|| "qwen3.5:9b".into());
    let system_prompt = req.system_prompt.unwrap_or_default();
    let temperature = req.temperature.unwrap_or(0.5);
    let max_tokens = req.max_tokens.unwrap_or(4096);
    let now = Utc::now().to_rfc3339();
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_SESSION_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            cortex_storage::queries::studio_chat_queries::create_session(
                conn,
                &id,
                &agent.id.to_string(),
                &title,
                &model,
                &system_prompt,
                temperature,
                max_tokens,
            )
            .map_err(|e| ApiError::db_error("create_session", e))?;

            Ok((
                StatusCode::CREATED,
                serde_json::to_value(SessionResponse {
                    id: id.clone(),
                    agent_id: agent.id.to_string(),
                    title: title.clone(),
                    model: model.clone(),
                    system_prompt: system_prompt.clone(),
                    temperature,
                    max_tokens,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &id,
                "create_studio_session",
                "info",
                actor,
                "created",
                serde_json::json!({
                    "session_id": id,
                    "agent_id": agent.id.to_string(),
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/studio/sessions — list sessions.
///
/// WP9-D: Supports `active_since` query parameter to filter by last activity.
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListSessionsQuery>,
) -> ApiResult<SessionListResponse> {
    let limit = params.limit.unwrap_or(50).min(200);
    let active_since = params.active_since.as_deref();
    if active_since.is_some() && params.cursor.is_some() {
        return Err(ApiError::bad_request(
            "active_since cannot be combined with cursor pagination",
        ));
    }
    let cursor = params
        .cursor
        .as_deref()
        .map(parse_session_list_cursor)
        .transpose()?;

    let mut sessions = {
        let db = state
            .db
            .read()
            .map_err(|e| ApiError::db_error("list_sessions", e))?;
        if let Some(active_since) = active_since {
            cortex_storage::queries::studio_chat_queries::list_sessions_active_since(
                &db,
                active_since,
                limit.saturating_add(1),
                0,
            )
            .map_err(|e| ApiError::db_error("list_sessions", e))?
        } else {
            cortex_storage::queries::studio_chat_queries::list_sessions_cursor(
                &db,
                limit.saturating_add(1),
                cursor.as_ref().map(|value| value.updated_at.as_str()),
                cursor.as_ref().map(|value| value.id.as_str()),
            )
            .map_err(|e| ApiError::db_error("list_sessions", e))?
        }
    };

    let has_more = sessions.len() > limit as usize;
    if has_more {
        sessions.truncate(limit as usize);
    }
    let next_cursor = if has_more && active_since.is_none() {
        sessions.last().map(encode_session_list_cursor)
    } else {
        None
    };

    Ok(Json(SessionListResponse {
        sessions: sessions.into_iter().map(session_row_to_response).collect(),
        next_cursor,
        has_more,
    }))
}

/// GET /api/studio/sessions/:id — get session with messages.
pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<SessionWithMessagesResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_session", e))?;

    let session = cortex_storage::queries::studio_chat_queries::get_session(&db, &id)
        .map_err(|e| ApiError::db_error("get_session", e))?
        .ok_or_else(|| ApiError::not_found(format!("session {id} not found")))?;

    let messages = cortex_storage::queries::studio_chat_queries::list_messages(&db, &id)
        .map_err(|e| ApiError::db_error("list_messages", e))?;

    Ok(Json(SessionWithMessagesResponse {
        session: session_row_to_response(session),
        messages: messages.into_iter().map(message_row_to_response).collect(),
    }))
}

/// DELETE /api/studio/sessions/:id — delete a session (CASCADE deletes messages).
pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(id): Path<String>,
) -> Response {
    let actor = studio_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "session_id": id.clone() });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "DELETE",
        DELETE_SESSION_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let deleted = cortex_storage::queries::studio_chat_queries::delete_session(conn, &id)
                .map_err(|e| ApiError::db_error("delete_session", e))?;

            if !deleted {
                return Err(ApiError::not_found(format!("session {id} not found")));
            }

            Ok((StatusCode::OK, serde_json::json!({ "deleted": true })))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &id,
                "delete_studio_session",
                "high",
                actor,
                "deleted",
                serde_json::json!({ "session_id": id }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/studio/sessions/:id/stream/recover — recover missed stream events.
///
/// Returns persisted stream events after the given sequence number.
/// Used by the frontend to recover after SSE disconnect.
pub async fn recover_stream(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<RecoverStreamQuery>,
) -> ApiResult<RecoverStreamResponse> {
    let after_seq = params.after_seq.unwrap_or(0);
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("recover_stream", e))?;

    let session = cortex_storage::queries::studio_chat_queries::get_session(&db, &session_id)
        .map_err(|e| ApiError::db_error("recover_stream", e))?;
    if session.is_none() {
        return Err(ApiError::not_found(format!(
            "session {session_id} not found"
        )));
    }

    let all_events = cortex_storage::queries::stream_event_queries::recover_events_after(
        &db,
        &session_id,
        &params.message_id,
        0,
    )
    .map_err(|e| ApiError::db_error("recover_stream", e))?;

    let assistant_message =
        cortex_storage::queries::studio_chat_queries::get_message(&db, &params.message_id)
            .map_err(|e| ApiError::db_error("recover_stream", e))?
            .filter(|message| message.session_id == session_id && message.role == "assistant");

    let mut api_events: Vec<_> = all_events
        .into_iter()
        .filter(|row| row.id > after_seq)
        .filter_map(|row| {
            let event_type = public_stream_event_type(&row.event_type)?;
            Some(StreamEventApiResponse {
                seq: row.id,
                event_type: event_type.to_string(),
                payload: serde_json::from_str(&row.payload).unwrap_or(serde_json::json!({})),
                created_at: normalize_public_timestamp(&row.created_at),
                reconstructed: None,
            })
        })
        .collect();

    append_reconstructed_recover_events(
        &mut api_events,
        &cortex_storage::queries::stream_event_queries::recover_events_after(
            &db,
            &session_id,
            &params.message_id,
            0,
        )
        .map_err(|e| ApiError::db_error("recover_stream", e))?,
        after_seq,
        assistant_message.as_ref(),
    );

    Ok(Json(RecoverStreamResponse { events: api_events }))
}

/// POST /api/studio/sessions/:id/messages — send a message and get LLM response.
///
/// Uses the full AgentRunner with:
/// - SOUL.md identity + L4 environment context
/// - All registered skills (safety, git, code_analysis, bundled, delegation)
/// - Builtin tools (read_file, write_file, list_dir, shell, etc.)
/// - Recursive tool call execution with gate checks
/// - OutputInspector credential scanning on input/output
/// - DB persistence of messages and safety audit
/// - WebSocket broadcast
pub async fn send_message(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(session_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Response {
    if req.content.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request(
            "message content must not be empty",
        ));
    }

    let actor = studio_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = studio_stream_request_body(&session_id, &req);

    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            SEND_MESSAGE_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                &session_id,
                "send_studio_message",
                "medium",
                actor,
                "replayed",
                serde_json::json!({
                    "session_id": session_id,
                    "user_message_id": stored.body.get("user_message").and_then(|value| value.get("id")).cloned().unwrap_or(serde_json::Value::Null),
                    "assistant_message_id": stored.body.get("assistant_message").and_then(|value| value.get("id")).cloned().unwrap_or(serde_json::Value::Null),
                    "status": stored.body.get("status").cloned().unwrap_or(serde_json::Value::Null),
                }),
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
                "route_template": SEND_MESSAGE_ROUTE_TEMPLATE,
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

            let mut execution_record = {
                let db = state.db.write().await;
                match cortex_storage::queries::live_execution_queries::get_by_journal_id(
                    &db,
                    &lease.journal_id,
                ) {
                    Ok(record) => record,
                    Err(error) => {
                        return error_response_with_idempotency(ApiError::db_error(
                            "load_live_execution_record",
                            error,
                        ));
                    }
                }
            };

            if execution_record.is_none() {
                let session = {
                    let db = match state.db.read() {
                        Ok(db) => db,
                        Err(error) => {
                            let db = state.db.write().await;
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(ApiError::db_error(
                                "get_session",
                                error,
                            ));
                        }
                    };
                    match cortex_storage::queries::studio_chat_queries::get_session(
                        &db,
                        &session_id,
                    ) {
                        Ok(Some(session)) => session,
                        Ok(None) => {
                            let db = state.db.write().await;
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(ApiError::not_found(format!(
                                "session {session_id} not found"
                            )));
                        }
                        Err(error) => {
                            let db = state.db.write().await;
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(ApiError::db_error(
                                "get_session",
                                error,
                            ));
                        }
                    }
                };

                let history = {
                    let db = match state.db.read() {
                        Ok(db) => db,
                        Err(error) => {
                            let db = state.db.write().await;
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(ApiError::db_error(
                                "list_messages",
                                error,
                            ));
                        }
                    };
                    match cortex_storage::queries::studio_chat_queries::list_messages(
                        &db,
                        &session_id,
                    ) {
                        Ok(history) => history,
                        Err(error) => {
                            let db = state.db.write().await;
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(ApiError::db_error(
                                "list_messages",
                                error,
                            ));
                        }
                    }
                };

                let prepared_runtime = match prepare_stored_runtime_execution(
                    &state,
                    &session.agent_id,
                    STUDIO_SYNTHETIC_AGENT_NAME,
                    parse_or_stable_uuid(&session_id, "studio-session"),
                    RunnerBuildOptions {
                        system_prompt: Some(session.system_prompt.clone()),
                        conversation_history: build_conversation_history(&history),
                        skill_allowlist: None,
                    },
                ) {
                    Ok(prepared_runtime) => prepared_runtime,
                    Err(error) => {
                        let db = state.db.write().await;
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(error);
                    }
                };

                let PreparedRuntimeExecution {
                    runtime_ctx,
                    mut runner,
                    providers,
                    ..
                } = prepared_runtime;

                let input_inspection = inspect_text_safety(&req.content, runtime_ctx.agent.id);
                let user_safety_status = inspection_safety_status(&input_inspection);

                let pre_loop_ctx = runner
                    .pre_loop(
                        runtime_ctx.agent.id,
                        runtime_ctx.session_id,
                        "studio",
                        &req.content,
                    )
                    .await;
                if let Err(error) = pre_loop_ctx {
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(map_runner_error(error));
                }

                let user_msg_id = Uuid::now_v7().to_string();
                let assistant_msg_id = Uuid::now_v7().to_string();
                let execution_id = Uuid::now_v7().to_string();
                let accepted_response = studio_message_accepted_body(
                    &session_id,
                    &user_msg_id,
                    &assistant_msg_id,
                    &execution_id,
                );
                let execution_state = StudioMessageExecutionState {
                    version: STUDIO_MESSAGE_EXECUTION_STATE_VERSION,
                    session_id: session_id.clone(),
                    user_message_id: user_msg_id.clone(),
                    assistant_message_id: assistant_msg_id.clone(),
                    accepted_response: accepted_response.clone(),
                    final_status_code: None,
                    final_response: None,
                };

                {
                    let db = state.db.write().await;
                    let audit_id = Uuid::now_v7().to_string();
                    let detail = match &input_inspection {
                        InspectionResult::Warning { pattern_name, .. } => {
                            Some(pattern_name.as_str())
                        }
                        InspectionResult::KillAll { pattern_name, .. } => {
                            Some(pattern_name.as_str())
                        }
                        InspectionResult::Clean => None,
                    };

                    if let Err(error) = cortex_storage::queries::studio_chat_queries::insert_message(
                        &db,
                        &user_msg_id,
                        &session_id,
                        "user",
                        &req.content,
                        0,
                        user_safety_status,
                    ) {
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(ApiError::db_error(
                            "insert_user_message",
                            error,
                        ));
                    }
                    if let Err(error) =
                        cortex_storage::queries::studio_chat_queries::insert_safety_audit(
                            &db,
                            &audit_id,
                            &session_id,
                            &user_msg_id,
                            "input_scan",
                            user_safety_status,
                            detail,
                        )
                    {
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(ApiError::db_error(
                            "insert_safety_audit",
                            error,
                        ));
                    }
                    if let Err(error) = persist_live_execution_record(
                        &db,
                        &execution_id,
                        &lease.journal_id,
                        &operation_id,
                        actor,
                        STUDIO_MESSAGE_ROUTE_KIND,
                        "accepted",
                        &execution_state,
                    ) {
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(error);
                    }
                }

                if matches!(input_inspection, InspectionResult::KillAll { .. }) {
                    let response_body = validation_error_body(
                        "Message blocked: credential pattern detected in input",
                    );
                    return finalize_studio_message_terminal_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_id,
                        0,
                        execution_state,
                        StatusCode::UNPROCESSABLE_ENTITY,
                        response_body,
                    )
                    .await;
                }

                if providers.is_empty() {
                    let response_body = validation_error_body(
                        "No model providers configured. Add provider config to ghost.yml.",
                    );
                    return finalize_studio_message_terminal_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_id,
                        0,
                        execution_state,
                        StatusCode::UNPROCESSABLE_ENTITY,
                        response_body,
                    )
                    .await;
                }

                execution_record = Some(
                    cortex_storage::queries::live_execution_queries::LiveExecutionRecord {
                        id: execution_id,
                        journal_id: lease.journal_id.clone(),
                        operation_id,
                        route_kind: STUDIO_MESSAGE_ROUTE_KIND.to_string(),
                        actor_key: actor.to_string(),
                        state_version: STUDIO_MESSAGE_EXECUTION_STATE_VERSION as i64,
                        attempt: 0,
                        status: "accepted".to_string(),
                        state_json: serde_json::to_string(&execution_state)
                            .unwrap_or_else(|_| "{}".to_string()),
                        created_at: String::new(),
                        updated_at: String::new(),
                    },
                );
            }

            let execution_record = execution_record.expect("execution record must exist");
            let execution_state = match parse_studio_message_execution_state(&execution_record) {
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
                        stored_studio_message_terminal_response(&state.db, &execution_state)
                    {
                        return finalize_studio_message_terminal_response(
                            &state,
                            &lease,
                            &operation_context,
                            &session_id,
                            actor,
                            &execution_record.id,
                            execution_record.attempt,
                            execution_state,
                            status,
                            body,
                        )
                        .await;
                    }
                }
                "recovery_required" => {
                    let response_body = studio_message_recovery_body(&execution_state);
                    return finalize_studio_message_terminal_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
                        execution_record.attempt,
                        execution_state,
                        StatusCode::ACCEPTED,
                        response_body,
                    )
                    .await;
                }
                "cancelled" | "cancel_requested" => {
                    let cancelled_body = studio_message_cancelled_body(&execution_state);
                    return finalize_studio_message_cancelled_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
                        execution_record.attempt,
                        execution_state,
                        cancelled_body,
                    )
                    .await;
                }
                "running" => {
                    if let Some((status, body)) =
                        stored_studio_message_terminal_response(&state.db, &execution_state)
                    {
                        return finalize_studio_message_terminal_response(
                            &state,
                            &lease,
                            &operation_context,
                            &session_id,
                            actor,
                            &execution_record.id,
                            execution_record.attempt,
                            execution_state,
                            status,
                            body,
                        )
                        .await;
                    }

                    let response_body = studio_message_recovery_body(&execution_state);
                    return finalize_studio_message_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
                        execution_record.attempt,
                        execution_state,
                        response_body,
                    )
                    .await;
                }
                "accepted" => {}
                other => {
                    return error_response_with_idempotency(ApiError::internal(format!(
                        "unsupported live execution status: {other}"
                    )));
                }
            }

            let session = {
                let db = match state.db.read() {
                    Ok(db) => db,
                    Err(error) => {
                        return error_response_with_idempotency(ApiError::db_error(
                            "get_session",
                            error,
                        ));
                    }
                };
                match cortex_storage::queries::studio_chat_queries::get_session(
                    &db,
                    &execution_state.session_id,
                ) {
                    Ok(Some(session)) => session,
                    Ok(None) => {
                        let response_body = studio_message_recovery_body(&execution_state);
                        return finalize_studio_message_recovery_response(
                            &state,
                            &lease,
                            &operation_context,
                            &session_id,
                            actor,
                            &execution_record.id,
                            execution_record.attempt,
                            execution_state,
                            response_body,
                        )
                        .await;
                    }
                    Err(error) => {
                        return error_response_with_idempotency(ApiError::db_error(
                            "get_session",
                            error,
                        ));
                    }
                }
            };

            let history = {
                let db = match state.db.read() {
                    Ok(db) => db,
                    Err(error) => {
                        return error_response_with_idempotency(ApiError::db_error(
                            "list_messages",
                            error,
                        ));
                    }
                };
                match cortex_storage::queries::studio_chat_queries::list_messages(
                    &db,
                    &execution_state.session_id,
                ) {
                    Ok(history) => history,
                    Err(error) => {
                        return error_response_with_idempotency(ApiError::db_error(
                            "list_messages",
                            error,
                        ));
                    }
                }
            };

            let history_cutoff: Vec<_> = if !history.is_empty()
                && history.last().map(|message| message.id.as_str())
                    == Some(execution_state.user_message_id.as_str())
            {
                history[..history.len() - 1].to_vec()
            } else {
                history
            };

            let prepared_runtime = match prepare_stored_runtime_execution(
                &state,
                &session.agent_id,
                STUDIO_SYNTHETIC_AGENT_NAME,
                parse_or_stable_uuid(&execution_state.session_id, "studio-session"),
                RunnerBuildOptions {
                    system_prompt: Some(session.system_prompt.clone()),
                    conversation_history: build_conversation_history(&history_cutoff),
                    skill_allowlist: None,
                },
            ) {
                Ok(prepared_runtime) => prepared_runtime,
                Err(error) => return error_response_with_idempotency(error),
            };

            let PreparedRuntimeExecution {
                runtime_ctx,
                mut runner,
                providers,
                ..
            } = prepared_runtime;

            let (cancel_token, _execution_control_guard) =
                state.acquire_live_execution_control(execution_record.id.clone());
            let execution_attempt = {
                let state_json = match serde_json::to_string(&execution_state) {
                    Ok(json) => json,
                    Err(error) => {
                        return error_response_with_idempotency(ApiError::internal(
                            error.to_string(),
                        ))
                    }
                };
                let db = state.db.write().await;
                match begin_execution_attempt(
                    &db,
                    &execution_record.id,
                    STUDIO_MESSAGE_EXECUTION_STATE_VERSION as i64,
                    &state_json,
                    Some(lease.owner_token.as_str()),
                    Some(lease.lease_epoch),
                ) {
                    Ok(attempt) => attempt,
                    Err(error) => return error_response_with_idempotency(error),
                }
            };
            runner.set_execution_context(execution_record.id.clone(), execution_attempt);
            runner.set_cancel_token(Arc::clone(&cancel_token));

            if providers.is_empty() {
                let response_body = validation_error_body(
                    "No model providers configured. Add provider config to ghost.yml.",
                );
                return finalize_studio_message_terminal_response(
                    &state,
                    &lease,
                    &operation_context,
                    &session_id,
                    actor,
                    &execution_record.id,
                    execution_attempt,
                    execution_state,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    response_body,
                )
                .await;
            }

            let heartbeat = start_operation_lease_heartbeat(Arc::clone(&state.db), lease.clone());
            let mut ctx =
                match pre_loop_blocking_turn(&mut runner, &runtime_ctx, "studio", &req.content)
                    .await
                {
                    Ok(ctx) => ctx,
                    Err(error) => {
                        let heartbeat_result = heartbeat.stop().await;
                        if let Err(error) = heartbeat_result {
                            return error_response_with_idempotency(error);
                        }
                        return error_response_with_idempotency(map_runner_error(error));
                    }
                };

            if cancel_token.is_cancelled() {
                let heartbeat_result = heartbeat.stop().await;
                if let Err(error) = heartbeat_result {
                    return error_response_with_idempotency(error);
                }
                let cancelled_body = studio_message_cancelled_body(&execution_state);
                return finalize_studio_message_cancelled_response(
                    &state,
                    &lease,
                    &operation_context,
                    &session_id,
                    actor,
                    &execution_record.id,
                    execution_attempt,
                    execution_state,
                    cancelled_body,
                )
                .await;
            }

            let result = match execute_blocking_turn(
                &mut runner,
                &mut ctx,
                &req.content,
                &providers,
            )
            .await
            {
                Ok(result) => {
                    if let Err(error) = heartbeat.stop().await {
                        return error_response_with_idempotency(error);
                    }
                    result
                }
                Err(ghost_agent_loop::runner::RunError::Cancelled) => {
                    let heartbeat_result = heartbeat.stop().await;
                    if let Err(error) = heartbeat_result {
                        return error_response_with_idempotency(error);
                    }
                    let cancelled_body = studio_message_cancelled_body(&execution_state);
                    return finalize_studio_message_cancelled_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
                        execution_attempt,
                        execution_state,
                        cancelled_body,
                    )
                    .await;
                }
                Err(error) => {
                    let heartbeat_result = heartbeat.stop().await;
                    if let Err(error) = heartbeat_result {
                        return error_response_with_idempotency(error);
                    }
                    let response_body = studio_message_recovery_body(&execution_state);
                    let response = finalize_studio_message_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
                        execution_attempt,
                        execution_state,
                        response_body,
                    )
                    .await;
                    tracing::warn!(error = %error, "studio send_message execution requires recovery");
                    return response;
                }
            };

            let response_content = result.output.unwrap_or_default();
            let token_count = result.total_tokens as i64;
            let output_inspection = inspect_text_safety(&response_content, runtime_ctx.agent.id);
            let output_safety_status = inspection_safety_status(&output_inspection);

            {
                let db = state.db.write().await;
                if let Err(error) = cortex_storage::queries::studio_chat_queries::insert_message(
                    &db,
                    &execution_state.assistant_message_id,
                    &execution_state.session_id,
                    "assistant",
                    &response_content,
                    token_count,
                    output_safety_status,
                ) {
                    tracing::warn!(error = %error, "failed to persist assistant message");
                    let response_body = studio_message_recovery_body(&execution_state);
                    return finalize_studio_message_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
                        execution_attempt,
                        execution_state,
                        response_body,
                    )
                    .await;
                }

                let audit_id = Uuid::now_v7().to_string();
                let detail = match &output_inspection {
                    InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
                    InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
                    InspectionResult::Clean => None,
                };
                if let Err(error) =
                    cortex_storage::queries::studio_chat_queries::insert_safety_audit(
                        &db,
                        &audit_id,
                        &execution_state.session_id,
                        &execution_state.assistant_message_id,
                        "output_scan",
                        output_safety_status,
                        detail,
                    )
                {
                    tracing::warn!(error = %error, "failed to persist output safety audit");
                }

                if session.title == "New Chat" {
                    let title = truncate_for_title(&req.content);
                    let _ = cortex_storage::queries::studio_chat_queries::update_session_title(
                        &db,
                        &execution_state.session_id,
                        &title,
                    );
                }
            }
            if let Err(error) = crate::speculative_context::record_completed_turn(
                &state,
                crate::speculative_context::CompletedTurnInput {
                    agent_id: runtime_ctx.agent.id,
                    session_id: runtime_ctx.session_id,
                    turn_id: execution_state.assistant_message_id.clone(),
                    route_kind: "studio",
                    user_message: req.content.clone(),
                    assistant_message: response_content.clone(),
                },
            )
            .await
            {
                tracing::warn!(
                    error = %error,
                    session_id = %execution_state.session_id,
                    assistant_message_id = %execution_state.assistant_message_id,
                    "failed to record speculative context for studio turn"
                );
            }

            let final_body =
                match reconstruct_studio_message_completed_body(&state.db, &execution_state) {
                    Ok(Some(body)) => body,
                    Ok(None) => studio_message_recovery_body(&execution_state),
                    Err(error) => return error_response_with_idempotency(error),
                };

            finalize_studio_message_terminal_response(
                &state,
                &lease,
                &operation_context,
                &session_id,
                actor,
                &execution_record.id,
                execution_attempt,
                execution_state,
                StatusCode::OK,
                final_body,
            )
            .await
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StudioMessageExecutionState {
    #[serde(default = "studio_message_execution_state_version")]
    version: u32,
    session_id: String,
    user_message_id: String,
    assistant_message_id: String,
    accepted_response: serde_json::Value,
    final_status_code: Option<u16>,
    final_response: Option<serde_json::Value>,
}

fn studio_message_execution_state_version() -> u32 {
    STUDIO_MESSAGE_EXECUTION_STATE_VERSION
}

fn parse_studio_message_execution_state(
    record: &cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
) -> Result<StudioMessageExecutionState, ApiError> {
    if record.state_version != STUDIO_MESSAGE_EXECUTION_STATE_VERSION as i64 {
        return Err(ApiError::internal(format!(
            "unsupported studio execution state version: {}",
            record.state_version
        )));
    }

    let state = serde_json::from_str::<StudioMessageExecutionState>(&record.state_json).map_err(
        |error| ApiError::internal(format!("failed to parse studio execution state: {error}")),
    )?;
    if state.version != STUDIO_MESSAGE_EXECUTION_STATE_VERSION {
        return Err(ApiError::internal(format!(
            "unsupported studio execution state version: {}",
            state.version
        )));
    }
    Ok(state)
}

fn studio_message_accepted_body(
    session_id: &str,
    user_message_id: &str,
    assistant_message_id: &str,
    execution_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "status": "accepted",
        "session_id": session_id,
        "user_message_id": user_message_id,
        "assistant_message_id": assistant_message_id,
        "execution_id": execution_id,
    })
}

fn studio_message_recovery_body(state: &StudioMessageExecutionState) -> serde_json::Value {
    let mut body = state.accepted_response.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert("recovery_required".into(), serde_json::Value::Bool(true));
    }
    body
}

fn studio_message_cancelled_body(state: &StudioMessageExecutionState) -> serde_json::Value {
    state.final_response.clone().unwrap_or_else(|| {
        serde_json::to_value(ErrorResponse::new(
            "EXECUTION_CANCELLED",
            "Execution cancelled by user",
        ))
        .unwrap_or(serde_json::Value::Null)
    })
}

fn ensure_execution_attempt_started(
    conn: &rusqlite::Connection,
    execution_id: &str,
    attempt: i64,
    owner_token: Option<&str>,
    lease_epoch: Option<i64>,
    status: &str,
) -> Result<(), ApiError> {
    cortex_storage::queries::execution_attempt_queries::insert_or_ignore(
        conn,
        &cortex_storage::queries::execution_attempt_queries::NewExecutionAttempt {
            execution_id,
            attempt,
            owner_token,
            lease_epoch,
            status,
            started_at: &chrono::Utc::now().to_rfc3339(),
        },
    )
    .map_err(|error| ApiError::db_error("insert_execution_attempt", error))
}

fn finish_execution_attempt(
    conn: &rusqlite::Connection,
    execution_id: &str,
    attempt: i64,
    status: &str,
    failure_class: Option<&str>,
    failure_detail: Option<&str>,
) -> Result<(), ApiError> {
    let updated = cortex_storage::queries::execution_attempt_queries::update_status(
        conn,
        execution_id,
        attempt,
        status,
        Some(&chrono::Utc::now().to_rfc3339()),
        failure_class,
        failure_detail,
        &chrono::Utc::now().to_rfc3339(),
    )
    .map_err(|error| ApiError::db_error("update_execution_attempt", error))?;
    if !updated {
        tracing::debug!(
            execution_id,
            attempt,
            status,
            "execution attempt missing during finish"
        );
    }
    Ok(())
}

fn begin_execution_attempt(
    conn: &rusqlite::Connection,
    execution_id: &str,
    state_version: i64,
    state_json: &str,
    owner_token: Option<&str>,
    lease_epoch: Option<i64>,
) -> Result<i64, ApiError> {
    let attempt = if let Some(attempt) = cortex_storage::queries::live_execution_queries::
        advance_attempt_and_update_status_if_in_statuses(
            conn,
            execution_id,
            state_version,
            &["accepted", "preparing"],
            "running",
            state_json,
        )
        .map_err(|error| ApiError::db_error("advance_live_execution_attempt", error))?
    {
        attempt
    } else {
        let record = cortex_storage::queries::live_execution_queries::get_by_id(conn, execution_id)
            .map_err(|error| ApiError::db_error("get_live_execution_record", error))?
            .ok_or_else(|| ApiError::not_found(format!("live execution {execution_id} not found")))?;
        if record.status != "running" {
            return Err(ApiError::custom(
                StatusCode::CONFLICT,
                "LIVE_EXECUTION_STATE_CONFLICT",
                format!("live execution {execution_id} could not transition to running"),
            ));
        }
        record.attempt
    };
    ensure_execution_attempt_started(
        conn,
        execution_id,
        attempt,
        owner_token,
        lease_epoch,
        "running",
    )?;
    Ok(attempt)
}

fn persist_live_execution_record(
    conn: &rusqlite::Connection,
    execution_id: &str,
    journal_id: &str,
    operation_id: &str,
    actor: &str,
    route_kind: &str,
    status: &str,
    state: &StudioMessageExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::insert(
        conn,
        &cortex_storage::queries::live_execution_queries::NewLiveExecutionRecord {
            id: execution_id,
            journal_id,
            operation_id,
            route_kind,
            actor_key: actor,
            state_version: STUDIO_MESSAGE_EXECUTION_STATE_VERSION as i64,
            attempt: 0,
            status,
            state_json: &state_json,
        },
    )
    .map_err(|error| ApiError::db_error("insert_live_execution_record", error))
}

fn transition_live_execution_state(
    conn: &rusqlite::Connection,
    execution_id: &str,
    expected_statuses: &[&str],
    status: &str,
    state: &StudioMessageExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    let updated =
        cortex_storage::queries::live_execution_queries::update_status_and_state_if_in_statuses(
            conn,
            execution_id,
            STUDIO_MESSAGE_EXECUTION_STATE_VERSION as i64,
            expected_statuses,
            status,
            &state_json,
        )
        .map_err(|error| ApiError::db_error("update_live_execution_record", error))?;
    if !updated {
        return Err(ApiError::custom(
            StatusCode::CONFLICT,
            "LIVE_EXECUTION_STATE_CONFLICT",
            format!("live execution {execution_id} could not transition to {status}"),
        ));
    }
    Ok(())
}

fn stored_studio_message_terminal_response(
    db: &crate::db_pool::DbPool,
    state: &StudioMessageExecutionState,
) -> Option<(StatusCode, serde_json::Value)> {
    if let (Some(status_code), Some(body)) = (state.final_status_code, state.final_response.clone())
    {
        return StatusCode::from_u16(status_code)
            .ok()
            .map(|status| (status, body));
    }

    reconstruct_studio_message_completed_body(db, state)
        .ok()
        .flatten()
        .map(|body| (StatusCode::OK, body))
}

fn reconstruct_studio_message_completed_body(
    db: &crate::db_pool::DbPool,
    state: &StudioMessageExecutionState,
) -> Result<Option<serde_json::Value>, ApiError> {
    let conn = db
        .read()
        .map_err(|error| ApiError::db_error("reconstruct_studio_message", error))?;
    let user_message =
        cortex_storage::queries::studio_chat_queries::get_message(&conn, &state.user_message_id)
            .map_err(|error| ApiError::db_error("reconstruct_studio_message", error))?;
    let assistant_message = cortex_storage::queries::studio_chat_queries::get_message(
        &conn,
        &state.assistant_message_id,
    )
    .map_err(|error| ApiError::db_error("reconstruct_studio_message", error))?;

    let (Some(user_message), Some(assistant_message)) = (user_message, assistant_message) else {
        return Ok(None);
    };
    if user_message.session_id != state.session_id
        || assistant_message.session_id != state.session_id
    {
        return Ok(None);
    }

    let safety_status = match (
        user_message.safety_status.as_str(),
        assistant_message.safety_status.as_str(),
    ) {
        ("blocked", _) | (_, "blocked") => "blocked",
        ("warning", _) | (_, "warning") => "warning",
        _ => "clean",
    };

    Ok(Some(
        serde_json::to_value(SendMessageResponse {
            user_message: message_row_to_response(user_message),
            assistant_message: message_row_to_response(assistant_message),
            safety_status: safety_status.into(),
        })
        .unwrap_or(serde_json::Value::Null),
    ))
}

async fn finalize_studio_message_terminal_response(
    state: &Arc<AppState>,
    lease: &crate::api::idempotency::PreparedOperationLease,
    operation_context: &OperationContext,
    session_id: &str,
    actor: &str,
    execution_id: &str,
    execution_attempt: i64,
    mut execution_state: StudioMessageExecutionState,
    status: StatusCode,
    body: serde_json::Value,
) -> Response {
    execution_state.final_status_code = Some(status.as_u16());
    execution_state.final_response = Some(body.clone());

    let db = state.db.write().await;
    if let Err(error) = transition_live_execution_state(
        &db,
        execution_id,
        &[
            "accepted",
            "preparing",
            "running",
            "recovery_required",
            "cancel_requested",
        ],
        "completed",
        &execution_state,
    ) {
        return error_response_with_idempotency(error);
    }
    let _ = finish_execution_attempt(
        &db,
        execution_id,
        execution_attempt,
        "completed",
        None,
        None,
    );

    match commit_prepared_json_operation(&db, operation_context, lease, status, &body) {
        Ok(outcome) => {
            if status == StatusCode::OK {
                let assistant_message = outcome.body.get("assistant_message");
                let safety_status = outcome
                    .body
                    .get("safety_status")
                    .and_then(|value| value.as_str())
                    .unwrap_or("clean");
                if let Some(message) = assistant_message {
                    if let (Some(message_id), Some(content)) = (
                        message.get("id").and_then(|value| value.as_str()),
                        message.get("content").and_then(|value| value.as_str()),
                    ) {
                        crate::api::websocket::broadcast_event(
                            state,
                            WsEvent::ChatMessage {
                                session_id: session_id.to_string(),
                                message_id: message_id.to_string(),
                                role: "assistant".into(),
                                content: truncate_preview(content, 200),
                                safety_status: safety_status.to_string(),
                            },
                        );
                    }
                }
            }
            let audit_outcome = if status == StatusCode::OK {
                "completed"
            } else {
                "rejected"
            };
            write_mutation_audit_entry(
                &db,
                session_id,
                "send_studio_message",
                "medium",
                actor,
                audit_outcome,
                serde_json::json!({
                    "session_id": session_id,
                    "user_message_id": outcome.body.get("user_message").and_then(|value| value.get("id")).cloned().unwrap_or(serde_json::Value::Null),
                    "assistant_message_id": outcome.body.get("assistant_message").and_then(|value| value.get("id")).cloned().unwrap_or(serde_json::Value::Null),
                    "status": outcome.body.get("status").cloned().unwrap_or(serde_json::Value::Null),
                    "error": outcome.body.get("error").cloned().unwrap_or(serde_json::Value::Null),
                }),
                operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

async fn finalize_studio_message_cancelled_response(
    state: &Arc<AppState>,
    lease: &crate::api::idempotency::PreparedOperationLease,
    operation_context: &OperationContext,
    session_id: &str,
    actor: &str,
    execution_id: &str,
    execution_attempt: i64,
    mut execution_state: StudioMessageExecutionState,
    body: serde_json::Value,
) -> Response {
    execution_state.final_status_code = Some(StatusCode::CONFLICT.as_u16());
    execution_state.final_response = Some(body.clone());

    let db = state.db.write().await;
    if let Err(error) = transition_live_execution_state(
        &db,
        execution_id,
        &[
            "accepted",
            "preparing",
            "running",
            "recovery_required",
            "cancel_requested",
        ],
        "cancelled",
        &execution_state,
    ) {
        return error_response_with_idempotency(error);
    }
    let _ = finish_execution_attempt(
        &db,
        execution_id,
        execution_attempt,
        "cancelled",
        None,
        None,
    );

    match commit_prepared_json_operation(&db, operation_context, lease, StatusCode::CONFLICT, &body)
    {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                session_id,
                "send_studio_message",
                "medium",
                actor,
                "cancelled",
                serde_json::json!({
                    "session_id": session_id,
                    "user_message_id": outcome.body.get("user_message").and_then(|value| value.get("id")).cloned().unwrap_or(serde_json::Value::Null),
                    "assistant_message_id": outcome.body.get("assistant_message").and_then(|value| value.get("id")).cloned().unwrap_or(serde_json::Value::Null),
                    "status": outcome.body.get("status").cloned().unwrap_or(serde_json::Value::Null),
                    "error": outcome.body.get("error").cloned().unwrap_or(serde_json::Value::Null),
                }),
                operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

async fn finalize_studio_message_recovery_response(
    state: &Arc<AppState>,
    lease: &crate::api::idempotency::PreparedOperationLease,
    operation_context: &OperationContext,
    session_id: &str,
    actor: &str,
    execution_id: &str,
    execution_attempt: i64,
    execution_state: StudioMessageExecutionState,
    body: serde_json::Value,
) -> Response {
    let db = state.db.write().await;
    if let Err(error) = transition_live_execution_state(
        &db,
        execution_id,
        &[
            "accepted",
            "preparing",
            "running",
            "recovery_required",
            "cancel_requested",
        ],
        "recovery_required",
        &execution_state,
    ) {
        return error_response_with_idempotency(error);
    }
    let _ = finish_execution_attempt(
        &db,
        execution_id,
        execution_attempt,
        "recovery_required",
        Some("route_recovery"),
        None,
    );

    match commit_prepared_json_operation(&db, operation_context, lease, StatusCode::ACCEPTED, &body)
    {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                session_id,
                "send_studio_message",
                "medium",
                actor,
                "accepted",
                serde_json::json!({
                    "session_id": session_id,
                    "user_message_id": outcome.body.get("user_message_id").cloned().unwrap_or(serde_json::Value::Null),
                    "assistant_message_id": outcome.body.get("assistant_message_id").cloned().unwrap_or(serde_json::Value::Null),
                    "status": outcome.body.get("status").cloned().unwrap_or(serde_json::Value::Null),
                    "recovery_required": outcome.body.get("recovery_required").cloned().unwrap_or(serde_json::Value::Bool(false)),
                }),
                operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/studio/sessions/:id/messages/stream — send a message with SSE streaming.
///
/// Same pipeline as `send_message` but returns an SSE stream that yields
/// `text_delta`, `tool_use`, `tool_result`, and `stream_end` events as
/// the agent generates its response.
pub async fn send_message_stream(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(session_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Response {
    if req.content.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request(
            "message content must not be empty",
        ));
    }

    let actor = studio_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = studio_stream_request_body(&session_id, &req);
    let user_content = req.content.clone();

    let outcome = {
        let db = state.db.write().await;
        let outcome = execute_idempotent_json_mutation(
            &db,
            &operation_context,
            actor,
            "POST",
            SEND_MESSAGE_STREAM_ROUTE_TEMPLATE,
            &request_body,
            |conn| {
                let session =
                    cortex_storage::queries::studio_chat_queries::get_session(conn, &session_id)
                        .map_err(|e| ApiError::db_error("get_session", e))?
                        .ok_or_else(|| {
                            ApiError::not_found(format!("session {session_id} not found"))
                        })?;

                let prepared_runtime = prepare_stored_runtime_execution(
                    &state,
                    &session.agent_id,
                    STUDIO_SYNTHETIC_AGENT_NAME,
                    parse_or_stable_uuid(&session_id, "studio-session"),
                    RunnerBuildOptions::default(),
                )?;

                let input_inspection =
                    inspect_text_safety(&user_content, prepared_runtime.runtime_ctx.agent.id);
                let user_safety_status = inspection_safety_status(&input_inspection);

                let user_msg_id = Uuid::now_v7().to_string();
                let audit_id = Uuid::now_v7().to_string();
                let detail = match &input_inspection {
                    InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
                    InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
                    InspectionResult::Clean => None,
                };

                cortex_storage::queries::studio_chat_queries::insert_message(
                    conn,
                    &user_msg_id,
                    &session_id,
                    "user",
                    &user_content,
                    0,
                    user_safety_status,
                )
                .map_err(|e| ApiError::db_error("insert_user_message", e))?;
                cortex_storage::queries::studio_chat_queries::insert_safety_audit(
                    conn,
                    &audit_id,
                    &session_id,
                    &user_msg_id,
                    "input_scan",
                    user_safety_status,
                    detail,
                )
                .map_err(|e| ApiError::db_error("insert_safety_audit", e))?;

                if matches!(input_inspection, InspectionResult::KillAll { .. }) {
                    return Ok((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        validation_error_body(
                            "Message blocked: credential pattern detected in input",
                        ),
                    ));
                }

                if prepared_runtime.providers.is_empty() {
                    return Ok((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        validation_error_body(
                            "No model providers configured. Add provider config to ghost.yml.",
                        ),
                    ));
                }

                let assistant_msg_id = Uuid::now_v7().to_string();
                let execution_id = Uuid::now_v7().to_string();
                let start_payload = serde_json::json!({
                    "execution_id": execution_id,
                    "session_id": session_id,
                    "message_id": assistant_msg_id,
                });
                let start_seq = cortex_storage::queries::stream_event_queries::insert_stream_event(
                    conn,
                    &session_id,
                    &assistant_msg_id,
                    "stream_start",
                    &start_payload.to_string(),
                )
                .map_err(|e| ApiError::db_error("insert_stream_start", e))?;

                Ok((
                    StatusCode::OK,
                    studio_stream_accepted_body(
                        &execution_id,
                        &session_id,
                        &user_msg_id,
                        &assistant_msg_id,
                        start_seq,
                    ),
                ))
            },
        );

        if let Ok(ref outcome) = outcome {
            let audit_outcome = match (&outcome.idempotency_status, outcome.status) {
                (IdempotencyStatus::Replayed, _) => "replayed",
                (_, status) if status.is_success() => "accepted",
                _ => "rejected",
            };
            write_mutation_audit_entry(
                &db,
                &session_id,
                "send_studio_message_stream",
                "medium",
                actor,
                audit_outcome,
                serde_json::json!({
                    "session_id": session_id,
                    "user_message_id": outcome.body.get("user_message_id").cloned().unwrap_or(serde_json::Value::Null),
                    "assistant_message_id": outcome.body.get("assistant_message_id").cloned().unwrap_or(serde_json::Value::Null),
                    "error": outcome.body.get("error").cloned().unwrap_or(serde_json::Value::Null),
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
        }

        outcome
    };

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => return error_response_with_idempotency(error),
    };

    if outcome.status != StatusCode::OK {
        return json_response_with_idempotency(
            outcome.status,
            outcome.body,
            outcome.idempotency_status,
        );
    }

    let idempotency_status = outcome.idempotency_status.clone();
    let acceptance = match parse_studio_stream_acceptance(&outcome.body) {
        Ok(acceptance) => acceptance,
        Err(error) => {
            return response_with_idempotency(error.into_response(), idempotency_status);
        }
    };
    let operation_id = operation_context
        .operation_id
        .clone()
        .unwrap_or_else(|| acceptance.execution_id.clone());

    match idempotency_status {
        IdempotencyStatus::Executed => {
            let execution_record = match ensure_studio_stream_execution_record(
                &state,
                &operation_id,
                actor,
                &acceptance,
            )
            .await
            {
                Ok(record) => record,
                Err(error) => {
                    return response_with_idempotency(
                        error.into_response(),
                        IdempotencyStatus::Executed,
                    );
                }
            };
            let execution_state = match parse_studio_stream_execution_state(&execution_record) {
                Ok(state) => state,
                Err(error) => {
                    return response_with_idempotency(
                        error.into_response(),
                        IdempotencyStatus::Executed,
                    );
                }
            };
            let execution_attempt = {
                let state_json = match serde_json::to_string(&execution_state) {
                    Ok(json) => json,
                    Err(error) => {
                        return response_with_idempotency(
                            ApiError::internal(error.to_string()).into_response(),
                            IdempotencyStatus::Executed,
                        );
                    }
                };
                let db = state.db.write().await;
                match begin_execution_attempt(
                    &db,
                    &execution_record.id,
                    STUDIO_STREAM_EXECUTION_STATE_VERSION as i64,
                    &state_json,
                    None,
                    None,
                ) {
                    Ok(attempt) => attempt,
                    Err(error) => {
                        return response_with_idempotency(
                            error.into_response(),
                            IdempotencyStatus::Executed,
                        );
                    }
                }
            };
            let (stream_rx, task_handle) = spawn_studio_stream_execution(
                Arc::clone(&state),
                execution_record.id.clone(),
                execution_attempt,
                acceptance.session_id.clone(),
                acceptance.user_message_id.clone(),
                acceptance.assistant_message_id.clone(),
                user_content,
            );
            state.background_tasks.lock().await.push(task_handle);
            studio_live_stream_response(
                Arc::clone(&state),
                acceptance,
                execution_record.id,
                execution_attempt,
                execution_state,
                stream_rx,
                IdempotencyStatus::Executed,
            )
        }
        IdempotencyStatus::Replayed => {
            let execution_state =
                match find_studio_stream_execution_record(&state, &operation_id, &acceptance).await
                {
                    Ok(Some(record)) => match parse_studio_stream_execution_state(&record) {
                        Ok(state) => Some(state),
                        Err(error) => {
                            return response_with_idempotency(
                                error.into_response(),
                                IdempotencyStatus::Replayed,
                            );
                        }
                    },
                    Ok(None) => None,
                    Err(error) => {
                        return response_with_idempotency(
                            error.into_response(),
                            IdempotencyStatus::Replayed,
                        );
                    }
                };
            let (persisted_events, assistant_message) = {
                let db = match state.db.read() {
                    Ok(db) => db,
                    Err(error) => {
                        return response_with_idempotency(
                            ApiError::db_error("replay_stream_read", error).into_response(),
                            IdempotencyStatus::Replayed,
                        );
                    }
                };
                let events =
                    match cortex_storage::queries::stream_event_queries::recover_events_after(
                        &db,
                        &acceptance.session_id,
                        &acceptance.assistant_message_id,
                        0,
                    ) {
                        Ok(events) => events,
                        Err(error) => {
                            return response_with_idempotency(
                                ApiError::db_error("replay_stream_read", error).into_response(),
                                IdempotencyStatus::Replayed,
                            );
                        }
                    };
                let assistant_message =
                    match cortex_storage::queries::studio_chat_queries::get_message(
                        &db,
                        &acceptance.assistant_message_id,
                    ) {
                        Ok(message) => message,
                        Err(error) => {
                            return response_with_idempotency(
                                ApiError::db_error("replay_stream_read", error).into_response(),
                                IdempotencyStatus::Replayed,
                            );
                        }
                    }
                    .filter(|message| message.session_id == acceptance.session_id);
                (events, assistant_message)
            };
            studio_replay_stream_response(
                acceptance,
                persisted_events,
                assistant_message,
                execution_state,
                IdempotencyStatus::Replayed,
            )
        }
        IdempotencyStatus::InProgress | IdempotencyStatus::Mismatch => unreachable!(),
    }
}

#[derive(Debug, Clone)]
struct StudioStreamAcceptance {
    execution_id: String,
    session_id: String,
    user_message_id: String,
    assistant_message_id: String,
    stream_start_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StudioStreamExecutionState {
    #[serde(default = "studio_stream_execution_state_version")]
    version: u32,
    session_id: String,
    user_message_id: String,
    assistant_message_id: String,
    stream_start_seq: i64,
    #[serde(default = "default_accepted_response")]
    accepted_response: serde_json::Value,
    recovery_required: bool,
    terminal_event_type: Option<String>,
    terminal_payload: Option<serde_json::Value>,
}

fn studio_stream_execution_state_version() -> u32 {
    STUDIO_STREAM_EXECUTION_STATE_VERSION
}

fn default_accepted_response() -> serde_json::Value {
    serde_json::Value::Null
}

fn validation_error_body(message: &str) -> serde_json::Value {
    serde_json::to_value(ErrorResponse::new("VALIDATION_ERROR", message)).unwrap_or_else(
        |_| serde_json::json!({ "error": { "code": "VALIDATION_ERROR", "message": message } }),
    )
}

fn studio_stream_request_body(session_id: &str, req: &SendMessageRequest) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "content": req.content,
        "model": req.model,
        "temperature": req.temperature,
        "max_tokens": req.max_tokens,
    })
}

fn studio_stream_accepted_body(
    execution_id: &str,
    session_id: &str,
    user_message_id: &str,
    assistant_message_id: &str,
    stream_start_seq: i64,
) -> serde_json::Value {
    serde_json::json!({
        "status": "accepted",
        "execution_id": execution_id,
        "session_id": session_id,
        "user_message_id": user_message_id,
        "assistant_message_id": assistant_message_id,
        "stream_start_seq": stream_start_seq,
    })
}

fn parse_studio_stream_acceptance(
    body: &serde_json::Value,
) -> Result<StudioStreamAcceptance, ApiError> {
    let assistant_message_id = body
        .get("assistant_message_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ApiError::internal("missing accepted assistant_message_id"))?
        .to_string();

    Ok(StudioStreamAcceptance {
        execution_id: body
            .get("execution_id")
            .and_then(|value| value.as_str())
            .unwrap_or(assistant_message_id.as_str())
            .to_string(),
        session_id: body
            .get("session_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("missing accepted session_id"))?
            .to_string(),
        user_message_id: body
            .get("user_message_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("missing accepted user_message_id"))?
            .to_string(),
        assistant_message_id,
        stream_start_seq: body
            .get("stream_start_seq")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| ApiError::internal("missing accepted stream_start_seq"))?,
    })
}

fn studio_stream_execution_state(
    acceptance: &StudioStreamAcceptance,
) -> StudioStreamExecutionState {
    StudioStreamExecutionState {
        version: STUDIO_STREAM_EXECUTION_STATE_VERSION,
        session_id: acceptance.session_id.clone(),
        user_message_id: acceptance.user_message_id.clone(),
        assistant_message_id: acceptance.assistant_message_id.clone(),
        stream_start_seq: acceptance.stream_start_seq,
        accepted_response: studio_stream_accepted_body(
            &acceptance.execution_id,
            &acceptance.session_id,
            &acceptance.user_message_id,
            &acceptance.assistant_message_id,
            acceptance.stream_start_seq,
        ),
        recovery_required: false,
        terminal_event_type: None,
        terminal_payload: None,
    }
}

fn parse_studio_stream_execution_state(
    record: &cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
) -> Result<StudioStreamExecutionState, ApiError> {
    if record.state_version != STUDIO_STREAM_EXECUTION_STATE_VERSION as i64 {
        return Err(ApiError::internal(format!(
            "unsupported studio stream execution state version: {}",
            record.state_version
        )));
    }

    let state = serde_json::from_str::<StudioStreamExecutionState>(&record.state_json).map_err(
        |error| {
            ApiError::internal(format!(
                "failed to parse studio stream execution state: {error}"
            ))
        },
    )?;
    if state.version != STUDIO_STREAM_EXECUTION_STATE_VERSION {
        return Err(ApiError::internal(format!(
            "unsupported studio stream execution state version: {}",
            state.version
        )));
    }

    Ok(state)
}

fn persist_studio_stream_execution_record(
    conn: &rusqlite::Connection,
    execution_id: &str,
    journal_id: &str,
    operation_id: &str,
    actor: &str,
    state: &StudioStreamExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::insert(
        conn,
        &cortex_storage::queries::live_execution_queries::NewLiveExecutionRecord {
            id: execution_id,
            journal_id,
            operation_id,
            route_kind: STUDIO_MESSAGE_STREAM_ROUTE_KIND,
            actor_key: actor,
            state_version: STUDIO_STREAM_EXECUTION_STATE_VERSION as i64,
            attempt: 0,
            status: "accepted",
            state_json: &state_json,
        },
    )
    .map_err(|error| ApiError::db_error("insert_studio_stream_execution_record", error))
}

fn update_studio_stream_execution_state(
    conn: &rusqlite::Connection,
    execution_id: &str,
    status: &str,
    state: &StudioStreamExecutionState,
) -> Result<(), ApiError> {
    transition_studio_stream_execution_state(
        conn,
        execution_id,
        EXECUTION_ACTIVE_STATUSES,
        status,
        state,
    )
}

fn transition_studio_stream_execution_state(
    conn: &rusqlite::Connection,
    execution_id: &str,
    expected_statuses: &[&str],
    status: &str,
    state: &StudioStreamExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    let updated =
        cortex_storage::queries::live_execution_queries::update_status_and_state_if_in_statuses(
            conn,
            execution_id,
            STUDIO_STREAM_EXECUTION_STATE_VERSION as i64,
            expected_statuses,
            status,
            &state_json,
        )
        .map_err(|error| ApiError::db_error("update_studio_stream_execution_record", error))?;
    if !updated {
        return Err(ApiError::custom(
            StatusCode::CONFLICT,
            "LIVE_EXECUTION_STATE_CONFLICT",
            format!("live execution {execution_id} could not transition to {status}"),
        ));
    }
    Ok(())
}

fn studio_stream_keep_alive() -> axum::response::sse::KeepAlive {
    axum::response::sse::KeepAlive::new()
        .interval(std::time::Duration::from_secs(15))
        .text("ping")
}

fn studio_stream_start_event(acceptance: &StudioStreamAcceptance) -> Event {
    Event::default()
        .event("stream_start")
        .id(acceptance.stream_start_seq.to_string())
        .data(
            serde_json::json!({
                "execution_id": acceptance.execution_id.clone(),
                "session_id": acceptance.session_id.clone(),
                "message_id": acceptance.assistant_message_id.clone(),
            })
            .to_string(),
        )
}

fn replay_stream_event(
    row: cortex_storage::queries::stream_event_queries::StreamEventRow,
) -> Option<Event> {
    let event_name = match row.event_type.as_str() {
        "stream_start" => "stream_start",
        "text_chunk" => "text_delta",
        "tool_use" => "tool_use",
        "tool_result" => "tool_result",
        "turn_complete" => "stream_end",
        "error" => "error",
        _ => return None,
    };

    Some(
        Event::default()
            .event(event_name)
            .id(row.id.to_string())
            .data(row.payload),
    )
}

fn replay_has_terminal_event(
    persisted_events: &[cortex_storage::queries::stream_event_queries::StreamEventRow],
) -> bool {
    persisted_events
        .iter()
        .any(|event| matches!(event.event_type.as_str(), "turn_complete" | "error"))
}

fn replay_has_text_output(
    persisted_events: &[cortex_storage::queries::stream_event_queries::StreamEventRow],
) -> bool {
    persisted_events
        .iter()
        .any(|event| event.event_type.as_str() == "text_chunk")
}

fn replay_fallback_text_event(
    message: &cortex_storage::queries::studio_chat_queries::StudioMessageRow,
) -> Option<Event> {
    replay_fallback_text_event_with_content(&message.content)
}

fn replay_fallback_text_event_with_content(content: &str) -> Option<Event> {
    if content.is_empty() {
        return None;
    }

    Some(
        Event::default().event("text_delta").data(
            mark_reconstructed_payload(serde_json::json!({
                "content": content,
            }))
            .to_string(),
        ),
    )
}

fn replay_fallback_stream_end_event(
    message: &cortex_storage::queries::studio_chat_queries::StudioMessageRow,
) -> Event {
    Event::default().event("stream_end").data(
        mark_reconstructed_payload(serde_json::json!({
            "message_id": message.id,
            "token_count": message.token_count,
            "safety_status": message.safety_status,
        }))
        .to_string(),
    )
}

fn studio_replay_stream_response(
    acceptance: StudioStreamAcceptance,
    persisted_events: Vec<cortex_storage::queries::stream_event_queries::StreamEventRow>,
    assistant_message: Option<cortex_storage::queries::studio_chat_queries::StudioMessageRow>,
    execution_state: Option<StudioStreamExecutionState>,
    idempotency_status: IdempotencyStatus,
) -> Response {
    let replay_missing_start = persisted_events
        .first()
        .map(|row| row.event_type.as_str() != "stream_start")
        .unwrap_or(true);
    let replay_missing_text = !replay_has_text_output(&persisted_events);
    let replay_missing_terminal = !replay_has_terminal_event(&persisted_events);
    let sse_stream = async_stream::stream! {
        if replay_missing_start {
            yield Ok::<Event, std::convert::Infallible>(studio_stream_start_event(&acceptance));
        }
        for event in persisted_events {
            if let Some(event) = replay_stream_event(event) {
                yield Ok(event);
            }
        }
        if let Some(assistant_message) = assistant_message {
            if replay_missing_text {
                if let Some(event) = replay_fallback_text_event(&assistant_message) {
                    yield Ok(event);
                }
            }
            if replay_missing_terminal {
                yield Ok(replay_fallback_stream_end_event(&assistant_message));
            }
        } else if replay_missing_terminal {
            if let Some(execution_state) = execution_state {
                if let (Some(event_type), Some(payload)) = (
                    execution_state.terminal_event_type.as_deref(),
                    execution_state.terminal_payload,
                ) {
                    yield Ok(Event::default()
                        .event(event_type)
                        .data(mark_reconstructed_payload(payload).to_string()));
                }
            }
        }
    };

    response_with_idempotency(
        Sse::new(sse_stream)
            .keep_alive(studio_stream_keep_alive())
            .into_response(),
        idempotency_status,
    )
}

fn studio_stream_recovery_payload(message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "message": message.into(),
        "recovery_required": true,
    })
}

async fn ensure_studio_stream_execution_record(
    state: &Arc<AppState>,
    operation_id: &str,
    actor: &str,
    acceptance: &StudioStreamAcceptance,
) -> Result<cortex_storage::queries::live_execution_queries::LiveExecutionRecord, ApiError> {
    let db = state.db.write().await;

    if let Some(record) =
        cortex_storage::queries::live_execution_queries::get_by_id(&db, &acceptance.execution_id)
            .map_err(|error| ApiError::db_error("get_studio_stream_execution_by_id", error))?
    {
        return Ok(record);
    }

    if let Some(record) =
        cortex_storage::queries::live_execution_queries::get_by_operation_id(&db, operation_id)
            .map_err(|error| {
                ApiError::db_error("get_studio_stream_execution_by_operation_id", error)
            })?
    {
        return Ok(record);
    }

    let journal =
        cortex_storage::queries::operation_journal_queries::get_by_operation_id(&db, operation_id)
            .map_err(|error| ApiError::db_error("get_operation_journal", error))?
            .ok_or_else(|| {
                ApiError::internal(format!(
                    "missing operation journal for studio stream execution {operation_id}"
                ))
            })?;

    let execution_state = studio_stream_execution_state(acceptance);
    persist_studio_stream_execution_record(
        &db,
        &acceptance.execution_id,
        &journal.id,
        operation_id,
        actor,
        &execution_state,
    )?;

    Ok(
        cortex_storage::queries::live_execution_queries::LiveExecutionRecord {
            id: acceptance.execution_id.clone(),
            journal_id: journal.id,
            operation_id: operation_id.to_string(),
            route_kind: STUDIO_MESSAGE_STREAM_ROUTE_KIND.to_string(),
            actor_key: actor.to_string(),
            state_version: STUDIO_STREAM_EXECUTION_STATE_VERSION as i64,
            attempt: 0,
            status: "accepted".to_string(),
            state_json: serde_json::to_string(&execution_state)
                .unwrap_or_else(|_| "{}".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
        },
    )
}

async fn find_studio_stream_execution_record(
    state: &Arc<AppState>,
    operation_id: &str,
    acceptance: &StudioStreamAcceptance,
) -> Result<Option<cortex_storage::queries::live_execution_queries::LiveExecutionRecord>, ApiError>
{
    let db = state
        .db
        .read()
        .map_err(|error| ApiError::db_error("get_studio_stream_execution", error))?;

    if let Some(record) =
        cortex_storage::queries::live_execution_queries::get_by_id(&db, &acceptance.execution_id)
            .map_err(|error| ApiError::db_error("get_studio_stream_execution_by_id", error))?
    {
        return Ok(Some(record));
    }

    cortex_storage::queries::live_execution_queries::get_by_operation_id(&db, operation_id)
        .map_err(|error| ApiError::db_error("get_studio_stream_execution_by_operation_id", error))
}

async fn persist_studio_stream_terminal_state(
    db: &Arc<crate::db_pool::DbPool>,
    execution_id: &str,
    attempt: i64,
    status: &str,
    execution_state: &StudioStreamExecutionState,
) {
    let conn = db.write().await;
    let _ = update_studio_stream_execution_state(&conn, execution_id, status, execution_state);
    let _ = finish_execution_attempt(
        &conn,
        execution_id,
        attempt,
        status,
        (status == "recovery_required").then_some("stream_recovery"),
        None,
    );
}

fn studio_live_stream_response(
    state: Arc<AppState>,
    acceptance: StudioStreamAcceptance,
    execution_id: String,
    execution_attempt: i64,
    mut execution_state: StudioStreamExecutionState,
    mut rx: tokio::sync::mpsc::Receiver<AgentStreamEvent>,
    idempotency_status: IdempotencyStatus,
) -> Response {
    let session_id_sse = acceptance.session_id.clone();
    let assistant_msg_id_sse = acceptance.assistant_message_id.clone();
    let db_for_stream = Arc::clone(&state.db);
    let state_for_stream = Arc::clone(&state);
    let start_event = studio_stream_start_event(&acceptance);

    let sse_stream = async_stream::stream! {
        {
            let conn = db_for_stream.write().await;
            if let Err(error) = transition_studio_stream_execution_state(
                &conn,
                &execution_id,
                &["accepted", "preparing", "running"],
                "running",
                &execution_state,
            ) {
                let payload = studio_stream_recovery_payload(format!(
                    "Failed to mark stream execution as running: {error}"
                ));
                execution_state.recovery_required = true;
                execution_state.terminal_event_type = Some("error".to_string());
                execution_state.terminal_payload = Some(payload.clone());
                drop(conn);
                persist_studio_stream_terminal_state(
                    &db_for_stream,
                    &execution_id,
                    execution_attempt,
                    "recovery_required",
                    &execution_state,
                )
                .await;
                yield Ok::<Event, std::convert::Infallible>(
                    Event::default().event("error").data(payload.to_string())
                );
                return;
            }
        }

        let mut text_buffer = String::new();
        const TEXT_FLUSH_THRESHOLD: usize = 2048;

        yield Ok::<Event, std::convert::Infallible>(start_event);

        state_for_stream.client_heartbeats.insert(session_id_sse.clone(), std::time::Instant::now());
        const BACKPRESSURE_STALE_SECS: u64 = 90;
        let mut stream_ended = false;

        while let Some(event) = rx.recv().await {
            let is_stale = state_for_stream.client_heartbeats
                .get(&session_id_sse)
                .map(|hb| hb.elapsed().as_secs() > BACKPRESSURE_STALE_SECS)
                .unwrap_or(false);
            if is_stale {
                tracing::debug!(session_id = %session_id_sse, "client heartbeat stale — applying backpressure (2s pause)");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }

            match event {
                AgentStreamEvent::StreamStart { message_id } => {
                    yield Ok(Event::default()
                        .event("stream_start")
                        .data(serde_json::json!({
                            "execution_id": execution_id.clone(),
                            "session_id": session_id_sse.clone(),
                            "message_id": message_id,
                        }).to_string()));
                }
                AgentStreamEvent::TextDelta { content } => {
                    text_buffer.push_str(&content);
                    let mut seq = None;
                    if text_buffer.len() >= TEXT_FLUSH_THRESHOLD {
                        seq = match flush_studio_stream_text_durable(
                            &db_for_stream,
                            &session_id_sse,
                            &assistant_msg_id_sse,
                            &mut text_buffer,
                        )
                        .await
                        {
                            Ok(seq) => seq,
                            Err(error) => {
                                let payload = studio_stream_recovery_payload(format!(
                                    "Failed to persist text chunk: {error}"
                                ));
                                execution_state.recovery_required = true;
                                execution_state.terminal_event_type = Some("error".to_string());
                                execution_state.terminal_payload = Some(payload.clone());
                                persist_studio_stream_terminal_state(
                                    &db_for_stream,
                                    &execution_id,
                                    execution_attempt,
                                    "recovery_required",
                                    &execution_state,
                                )
                                .await;
                                yield Ok(Event::default().event("error").data(payload.to_string()));
                                stream_ended = true;
                                break;
                            }
                        };
                    }

                    let mut ev = Event::default()
                        .event("text_delta")
                        .data(serde_json::json!({ "content": content }).to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    yield Ok(ev);
                }
                AgentStreamEvent::ToolUse { tool, tool_id, status } => {
                    match flush_studio_stream_text_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &assistant_msg_id_sse,
                        &mut text_buffer,
                    )
                    .await
                    {
                        Ok(Some(_)) => {}
                        Ok(None) => {}
                        Err(error) => {
                            let payload = studio_stream_recovery_payload(format!(
                                "Failed to persist text chunk: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(payload.clone());
                            persist_studio_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                execution_attempt,
                                "recovery_required",
                                &execution_state,
                            )
                            .await;
                            yield Ok(Event::default().event("error").data(payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    }

                    let payload = serde_json::json!({
                        "tool": tool,
                        "tool_id": tool_id,
                        "status": status,
                    });
                    let seq = match persist_studio_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &assistant_msg_id_sse,
                        "tool_use",
                        &payload,
                    )
                    .await
                    {
                        Ok(seq) => Some(seq),
                        Err(error) => {
                            let payload = studio_stream_recovery_payload(format!(
                                "Failed to persist tool_use event: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(payload.clone());
                            persist_studio_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                execution_attempt,
                                "recovery_required",
                                &execution_state,
                            )
                            .await;
                            yield Ok(Event::default().event("error").data(payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };

                    crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                        session_id: session_id_sse.clone(),
                        event_id: tool_id.clone(),
                        event_type: format!("tool_use:{}", tool),
                        sender: Some(tool.clone()),
                        sequence_number: seq.unwrap_or(0),
                    });

                    let mut ev = Event::default()
                        .event("tool_use")
                        .data(payload.to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    yield Ok(ev);
                }
                AgentStreamEvent::ToolResult { tool, tool_id, status, preview } => {
                    let payload = serde_json::json!({
                        "tool": tool,
                        "tool_id": tool_id,
                        "status": status,
                        "preview": preview,
                    });
                    let seq = match persist_studio_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &assistant_msg_id_sse,
                        "tool_result",
                        &payload,
                    )
                    .await
                    {
                        Ok(seq) => Some(seq),
                        Err(error) => {
                            let payload = studio_stream_recovery_payload(format!(
                                "Failed to persist tool_result event: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(payload.clone());
                            persist_studio_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                execution_attempt,
                                "recovery_required",
                                &execution_state,
                            )
                            .await;
                            yield Ok(Event::default().event("error").data(payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };

                    crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                        session_id: session_id_sse.clone(),
                        event_id: tool_id.clone(),
                        event_type: format!("tool_result:{}", tool),
                        sender: Some(tool.clone()),
                        sequence_number: seq.unwrap_or(0),
                    });

                    let mut ev = Event::default()
                        .event("tool_result")
                        .data(payload.to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    yield Ok(ev);
                }
                AgentStreamEvent::Heartbeat { phase } => {
                    yield Ok(Event::default()
                        .event("heartbeat")
                        .data(serde_json::json!({ "phase": phase }).to_string()));
                }
                AgentStreamEvent::TurnComplete { token_count, safety_status } => {
                    match flush_studio_stream_text_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &assistant_msg_id_sse,
                        &mut text_buffer,
                    )
                    .await
                    {
                        Ok(Some(_)) => {}
                        Ok(None) => {}
                        Err(error) => {
                            let payload = studio_stream_recovery_payload(format!(
                                "Failed to persist text chunk: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(payload.clone());
                            persist_studio_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                execution_attempt,
                                "recovery_required",
                                &execution_state,
                            )
                            .await;
                            yield Ok(Event::default().event("error").data(payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    }

                    let payload = serde_json::json!({
                        "message_id": assistant_msg_id_sse,
                        "token_count": token_count,
                        "safety_status": safety_status,
                    });
                    let seq = match persist_studio_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &assistant_msg_id_sse,
                        "turn_complete",
                        &payload,
                    )
                    .await
                    {
                        Ok(seq) => Some(seq),
                        Err(error) => {
                            let payload = studio_stream_recovery_payload(format!(
                                "Failed to persist terminal stream event: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(payload.clone());
                            persist_studio_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                execution_attempt,
                                "recovery_required",
                                &execution_state,
                            )
                            .await;
                            yield Ok(Event::default().event("error").data(payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };

                    if let Some(s) = seq {
                        crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                            session_id: session_id_sse.clone(),
                            event_id: assistant_msg_id_sse.clone(),
                            event_type: "turn_complete".to_string(),
                            sender: None,
                            sequence_number: s,
                        });
                    }
                    let mut ev = Event::default()
                        .event("stream_end")
                        .data(payload.to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    execution_state.recovery_required = false;
                    execution_state.terminal_event_type = Some("stream_end".to_string());
                    execution_state.terminal_payload = Some(payload.clone());
                    persist_studio_stream_terminal_state(
                        &db_for_stream,
                        &execution_id,
                        execution_attempt,
                        "completed",
                        &execution_state,
                    )
                    .await;
                    yield Ok(ev);
                    stream_ended = true;
                    break;
                }
                AgentStreamEvent::Error {
                    message,
                    error_type,
                    provider,
                    fallback,
                    terminal,
                    cancelled,
                } => {
                    let payload = studio_stream_error_payload(
                        &message,
                        error_type,
                        provider.as_deref(),
                        fallback,
                        terminal,
                        cancelled,
                    );

                    if !terminal {
                        yield Ok(Event::default()
                            .event("error")
                            .data(payload.to_string()));
                        continue;
                    }

                    match flush_studio_stream_text_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &assistant_msg_id_sse,
                        &mut text_buffer,
                    )
                    .await
                    {
                        Ok(Some(_)) => {}
                        Ok(None) => {}
                        Err(error) => {
                            let recovery_payload = studio_stream_recovery_payload(format!(
                                "Failed to persist text chunk: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(recovery_payload.clone());
                            persist_studio_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                execution_attempt,
                                "recovery_required",
                                &execution_state,
                            )
                            .await;
                            yield Ok(Event::default().event("error").data(recovery_payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    }

                    let seq = match persist_studio_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &assistant_msg_id_sse,
                        "error",
                        &payload,
                    )
                    .await
                    {
                        Ok(seq) => Some(seq),
                        Err(error) => {
                            let recovery_payload = studio_stream_recovery_payload(format!(
                                "Failed to persist terminal error event: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(recovery_payload.clone());
                            persist_studio_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                execution_attempt,
                                "recovery_required",
                                &execution_state,
                            )
                            .await;
                            yield Ok(Event::default().event("error").data(recovery_payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };

                    if let Some(s) = seq {
                        crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                            session_id: session_id_sse.clone(),
                            event_id: assistant_msg_id_sse.clone(),
                            event_type: "error".to_string(),
                            sender: None,
                            sequence_number: s,
                        });
                    }
                    let mut ev = Event::default()
                        .event("error")
                        .data(payload.to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    execution_state.recovery_required = !cancelled;
                    execution_state.terminal_event_type = Some("error".to_string());
                    execution_state.terminal_payload = Some(payload.clone());
                    persist_studio_stream_terminal_state(
                        &db_for_stream,
                        &execution_id,
                        execution_attempt,
                        if cancelled { "cancelled" } else { "recovery_required" },
                        &execution_state,
                    )
                    .await;
                    yield Ok(ev);
                    stream_ended = true;
                    break;
                }
            }
        }

        if !stream_ended {
            match flush_studio_stream_text_durable(
                &db_for_stream,
                &session_id_sse,
                &assistant_msg_id_sse,
                &mut text_buffer,
            )
            .await
            {
                Ok(Some(_)) => {}
                Ok(None) => {}
                Err(error) => {
                    let payload = studio_stream_recovery_payload(format!(
                        "Failed to persist trailing text chunk: {error}"
                    ));
                    execution_state.recovery_required = true;
                    execution_state.terminal_event_type = Some("error".to_string());
                    execution_state.terminal_payload = Some(payload.clone());
                    persist_studio_stream_terminal_state(
                        &db_for_stream,
                        &execution_id,
                        execution_attempt,
                        "recovery_required",
                        &execution_state,
                    )
                    .await;
                    yield Ok(Event::default().event("error").data(payload.to_string()));
                    state_for_stream.client_heartbeats.remove(&session_id_sse);
                    return;
                }
            }

            let payload = studio_stream_recovery_payload(
                "Stream ended before a durable terminal event was persisted",
            );
            execution_state.recovery_required = true;
            execution_state.terminal_event_type = Some("error".to_string());
            execution_state.terminal_payload = Some(payload.clone());
            persist_studio_stream_terminal_state(
                &db_for_stream,
                &execution_id,
                execution_attempt,
                "recovery_required",
                &execution_state,
            )
            .await;
            yield Ok(Event::default().event("error").data(payload.to_string()));
        }

        state_for_stream.client_heartbeats.remove(&session_id_sse);
    };

    response_with_idempotency(
        Sse::new(sse_stream)
            .keep_alive(studio_stream_keep_alive())
            .into_response(),
        idempotency_status,
    )
}

async fn persist_studio_stream_event_durable(
    db: &Arc<crate::db_pool::DbPool>,
    session_id: &str,
    message_id: &str,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<i64, ApiError> {
    let conn = db.write().await;
    cortex_storage::queries::stream_event_queries::insert_stream_event(
        &conn,
        session_id,
        message_id,
        event_type,
        &payload.to_string(),
    )
    .map_err(|error| ApiError::db_error("persist_stream_event", error))
}

async fn flush_studio_stream_text_durable(
    db: &Arc<crate::db_pool::DbPool>,
    session_id: &str,
    message_id: &str,
    buffer: &mut String,
) -> Result<Option<i64>, ApiError> {
    if buffer.is_empty() {
        return Ok(None);
    }

    let payload = serde_json::json!({ "content": buffer.as_str() });
    let result =
        persist_studio_stream_event_durable(db, session_id, message_id, "text_chunk", &payload)
            .await?;
    buffer.clear();
    Ok(Some(result))
}

fn spawn_studio_stream_execution(
    state: Arc<AppState>,
    execution_id: String,
    execution_attempt: i64,
    session_id: String,
    user_msg_id: String,
    assistant_msg_id: String,
    user_content: String,
) -> (
    tokio::sync::mpsc::Receiver<AgentStreamEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel::<AgentStreamEvent>(256);
    let state_for_task = Arc::clone(&state);
    let handle = tokio::spawn(async move {
        let (cancel_token, _execution_control_guard) =
            state_for_task.acquire_live_execution_control(execution_id.clone());
        let session = {
            let db = match state_for_task.db.read() {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::terminal_error(format!(
                            "failed to load session: {}",
                            ApiError::db_error("get_session", error)
                        )))
                        .await;
                    return;
                }
            };
            match cortex_storage::queries::studio_chat_queries::get_session(&db, &session_id) {
                Ok(Some(session)) => session,
                Ok(None) => {
                    let _ = tx
                        .send(AgentStreamEvent::terminal_error(format!(
                            "session {session_id} not found"
                        )))
                        .await;
                    return;
                }
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::terminal_error(format!(
                            "failed to load session: {}",
                            ApiError::db_error("get_session", error)
                        )))
                        .await;
                    return;
                }
            }
        };

        let history = {
            let db = match state_for_task.db.read() {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::terminal_error(format!(
                            "failed to load history: {}",
                            ApiError::db_error("list_messages", error)
                        )))
                        .await;
                    return;
                }
            };
            match cortex_storage::queries::studio_chat_queries::list_messages(&db, &session_id) {
                Ok(history) => history,
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::terminal_error(format!(
                            "failed to load history: {}",
                            ApiError::db_error("list_messages", error)
                        )))
                        .await;
                    return;
                }
            }
        };

        let history_cutoff: Vec<_> = if !history.is_empty()
            && history.last().map(|message| message.id.as_str()) == Some(user_msg_id.as_str())
        {
            history[..history.len() - 1].to_vec()
        } else {
            history
        };

        let prepared_runtime = match prepare_stored_runtime_execution(
            &state_for_task,
            &session.agent_id,
            STUDIO_SYNTHETIC_AGENT_NAME,
            parse_or_stable_uuid(&session_id, "studio-session"),
            RunnerBuildOptions {
                system_prompt: Some(session.system_prompt.clone()),
                conversation_history: build_conversation_history(&history_cutoff),
                skill_allowlist: None,
            },
        ) {
            Ok(prepared_runtime) => prepared_runtime,
            Err(error) => {
                let _ = tx
                    .send(AgentStreamEvent::terminal_error(error.to_string()))
                    .await;
                return;
            }
        };

        let PreparedRuntimeExecution {
            runtime_ctx,
            mut runner,
            providers: all_providers,
            ..
        } = prepared_runtime;
        runner.set_execution_context(execution_id.clone(), execution_attempt);
        runner.set_cancel_token(Arc::clone(&cancel_token));

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<AgentStreamEvent>(256);
        let tx_forward = tx.clone();
        let forward_handle = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if tx_forward.send(event).await.is_err() {
                    break;
                }
            }
        });

        let session_id_clone = session_id.clone();
        let assistant_msg_id_clone = assistant_msg_id.clone();
        let session_title = session.title.clone();
        let db_clone = Arc::clone(&state_for_task.db);
        let state_clone = Arc::clone(&state_for_task);
        let user_content_for_title = user_content.clone();
        if let Some(run_result) = execute_streaming_turn(
            &event_tx,
            &mut runner,
            &runtime_ctx,
            "studio",
            &user_content,
            &all_providers,
            "No model providers configured. Add provider config to ghost.yml.",
            "Agent turn timed out after 5 minutes",
        )
        .await
        {
            let response_content = run_result.output.unwrap_or_default();
            let token_count = run_result.total_tokens as i64;
            let output_inspection = inspect_text_safety(&response_content, runtime_ctx.agent.id);
            let output_safety_status = inspection_safety_status(&output_inspection);

            {
                let db = db_clone.write().await;
                if let Err(error) = cortex_storage::queries::studio_chat_queries::insert_message(
                    &db,
                    &assistant_msg_id_clone,
                    &session_id_clone,
                    "assistant",
                    &response_content,
                    token_count,
                    output_safety_status,
                ) {
                    let _ = event_tx
                        .send(AgentStreamEvent::terminal_error(format!(
                            "failed to persist assistant message: {}",
                            ApiError::db_error("insert_assistant_message", error)
                        )))
                        .await;
                    drop(event_tx);
                    let _ = forward_handle.await;
                    return;
                }

                let audit_id = Uuid::now_v7().to_string();
                let detail = match &output_inspection {
                    InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
                    InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
                    InspectionResult::Clean => None,
                };
                let _ = cortex_storage::queries::studio_chat_queries::insert_safety_audit(
                    &db,
                    &audit_id,
                    &session_id_clone,
                    &assistant_msg_id_clone,
                    "output_scan",
                    output_safety_status,
                    detail,
                );

                if session_title == "New Chat" {
                    let title = truncate_for_title(&user_content_for_title);
                    let _ = cortex_storage::queries::studio_chat_queries::update_session_title(
                        &db,
                        &session_id_clone,
                        &title,
                    );
                }
            }

            crate::api::websocket::broadcast_event(
                &state_clone,
                WsEvent::ChatMessage {
                    session_id: session_id_clone.clone(),
                    message_id: assistant_msg_id_clone.clone(),
                    role: "assistant".into(),
                    content: truncate_preview(&response_content, 200),
                    safety_status: output_safety_status.into(),
                },
            );

            if let Err(error) = crate::speculative_context::record_completed_turn(
                &state_clone,
                crate::speculative_context::CompletedTurnInput {
                    agent_id: runtime_ctx.agent.id,
                    session_id: runtime_ctx.session_id,
                    turn_id: assistant_msg_id_clone.clone(),
                    route_kind: "studio_stream",
                    user_message: user_content.clone(),
                    assistant_message: response_content.clone(),
                },
            )
            .await
            {
                tracing::warn!(
                    error = %error,
                    session_id = %session_id_clone,
                    assistant_message_id = %assistant_msg_id_clone,
                    "failed to record speculative context for studio stream"
                );
            }

            let _ = event_tx
                .send(AgentStreamEvent::TurnComplete {
                    token_count: run_result.total_tokens,
                    safety_status: output_safety_status.to_string(),
                })
                .await;
        }

        drop(event_tx);
        let _ = forward_handle.await;
    });

    (rx, handle)
}

// ── Helpers ────────────────────────────────────────────────────────

fn session_row_to_response(
    row: cortex_storage::queries::studio_chat_queries::StudioSessionRow,
) -> SessionResponse {
    SessionResponse {
        id: row.id,
        agent_id: row.agent_id,
        title: row.title,
        model: row.model,
        system_prompt: row.system_prompt,
        temperature: row.temperature,
        max_tokens: row.max_tokens,
        created_at: normalize_public_timestamp(&row.created_at),
        updated_at: normalize_public_timestamp(&row.updated_at),
    }
}

fn message_row_to_response(
    row: cortex_storage::queries::studio_chat_queries::StudioMessageRow,
) -> MessageResponse {
    MessageResponse {
        id: row.id,
        role: row.role,
        content: row.content,
        token_count: row.token_count,
        safety_status: row.safety_status,
        created_at: normalize_public_timestamp(&row.created_at),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionListCursor {
    updated_at: String,
    id: String,
}

fn parse_session_list_cursor(value: &str) -> Result<SessionListCursor, ApiError> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ApiError::bad_request("invalid studio session cursor"))?;
    serde_json::from_slice::<SessionListCursor>(&decoded)
        .map_err(|_| ApiError::bad_request("invalid studio session cursor"))
}

fn encode_session_list_cursor(
    row: &cortex_storage::queries::studio_chat_queries::StudioSessionRow,
) -> String {
    let cursor = SessionListCursor {
        updated_at: row.updated_at.clone(),
        id: row.id.clone(),
    };
    let encoded = serde_json::to_vec(&cursor).unwrap_or_default();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(encoded)
}

fn public_stream_event_type(event_type: &str) -> Option<&'static str> {
    match event_type {
        "stream_start" => Some("stream_start"),
        "text_chunk" => Some("text_delta"),
        "tool_use" => Some("tool_use"),
        "tool_result" => Some("tool_result"),
        "turn_complete" => Some("stream_end"),
        "error" => Some("error"),
        _ => None,
    }
}

fn mark_reconstructed_payload(mut payload: serde_json::Value) -> serde_json::Value {
    if let Some(object) = payload.as_object_mut() {
        object.insert("reconstructed".into(), serde_json::Value::Bool(true));
    }
    payload
}

fn durable_text_prefix(
    events: &[cortex_storage::queries::stream_event_queries::StreamEventRow],
    up_to_seq: i64,
) -> String {
    let mut prefix = String::new();
    for event in events
        .iter()
        .filter(|event| event.id <= up_to_seq && event.event_type == "text_chunk")
    {
        let payload: serde_json::Value =
            serde_json::from_str(&event.payload).unwrap_or(serde_json::json!({}));
        if let Some(content) = payload.get("content").and_then(|value| value.as_str()) {
            prefix.push_str(content);
        }
    }
    prefix
}

fn reconstructed_text_suffix(full_content: &str, delivered_prefix: &str) -> Option<String> {
    if full_content.is_empty() {
        return None;
    }
    if delivered_prefix.is_empty() {
        return Some(full_content.to_string());
    }
    if let Some(suffix) = full_content.strip_prefix(delivered_prefix) {
        if suffix.is_empty() {
            return None;
        }
        return Some(suffix.to_string());
    }
    Some(full_content.to_string())
}

fn append_reconstructed_recover_events(
    api_events: &mut Vec<StreamEventApiResponse>,
    all_events: &[cortex_storage::queries::stream_event_queries::StreamEventRow],
    after_seq: i64,
    assistant_message: Option<&cortex_storage::queries::studio_chat_queries::StudioMessageRow>,
) {
    let Some(assistant_message) = assistant_message else {
        return;
    };

    let remaining_has_text = all_events
        .iter()
        .any(|event| event.id > after_seq && event.event_type == "text_chunk");
    let remaining_has_terminal = all_events.iter().any(|event| {
        event.id > after_seq && matches!(event.event_type.as_str(), "turn_complete" | "error")
    });
    let delivered_prefix = durable_text_prefix(all_events, after_seq);
    let mut next_seq = all_events.last().map(|event| event.id).unwrap_or(0);

    if !remaining_has_text {
        if let Some(content) =
            reconstructed_text_suffix(&assistant_message.content, &delivered_prefix)
        {
            next_seq += 1;
            if next_seq > after_seq {
                api_events.push(StreamEventApiResponse {
                    seq: next_seq,
                    event_type: "text_delta".to_string(),
                    payload: mark_reconstructed_payload(serde_json::json!({
                        "content": content,
                    })),
                    created_at: normalize_public_timestamp(&assistant_message.created_at),
                    reconstructed: Some(true),
                });
            }
        }
    }

    if !remaining_has_terminal {
        next_seq += 1;
        if next_seq > after_seq {
            api_events.push(StreamEventApiResponse {
                seq: next_seq,
                event_type: "stream_end".to_string(),
                payload: mark_reconstructed_payload(serde_json::json!({
                    "message_id": assistant_message.id,
                    "token_count": assistant_message.token_count,
                    "safety_status": assistant_message.safety_status,
                })),
                created_at: normalize_public_timestamp(&assistant_message.created_at),
                reconstructed: Some(true),
            });
        }
    }
}

fn studio_stream_error_payload(
    message: &str,
    error_type: Option<AgentStreamErrorType>,
    provider: Option<&str>,
    fallback: bool,
    terminal: bool,
    cancelled: bool,
) -> serde_json::Value {
    let mut payload = serde_json::json!({ "message": message });
    if let Some(error_type) = error_type {
        payload["error_type"] = serde_json::json!(error_type);
    }
    if let Some(provider) = provider {
        payload["provider"] = serde_json::json!(provider);
    }
    if fallback {
        payload["fallback"] = serde_json::json!(true);
    }
    if !terminal {
        payload["terminal"] = serde_json::json!(false);
    }
    if cancelled {
        payload["cancelled"] = serde_json::json!(true);
    }
    payload
}

fn normalize_public_timestamp(value: &str) -> String {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return parsed.with_timezone(&Utc).to_rfc3339();
    }
    if let Ok(parsed) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return parsed.and_utc().to_rfc3339();
    }
    value.to_string()
}

fn build_conversation_history(
    history: &[cortex_storage::queries::studio_chat_queries::StudioMessageRow],
) -> Vec<ghost_llm::provider::ChatMessage> {
    history
        .iter()
        .map(|msg| ghost_llm::provider::ChatMessage {
            role: match msg.role.as_str() {
                "user" => ghost_llm::provider::MessageRole::User,
                "assistant" => ghost_llm::provider::MessageRole::Assistant,
                "system" => ghost_llm::provider::MessageRole::System,
                _ => ghost_llm::provider::MessageRole::User,
            },
            content: msg.content.clone(),
            tool_calls: None,
            tool_call_id: None,
        })
        .collect()
}

/// Truncate user message to create a session title (max 60 chars).
fn truncate_for_title(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() <= 60 {
        trimmed.to_string()
    } else {
        let mut end = 57;
        while end > 0 && !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &trimmed[..end])
    }
}

/// Build a streaming LLM connection from a provider config (WP2-A).
///
/// Extracted from inline closure to support provider fallback iteration.
/// Truncate for WS preview (max n bytes, UTF-8 safe).
fn truncate_preview(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max.saturating_sub(1);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_for(
        state_version: i64,
        state_json: serde_json::Value,
    ) -> cortex_storage::queries::live_execution_queries::LiveExecutionRecord {
        cortex_storage::queries::live_execution_queries::LiveExecutionRecord {
            id: "exec-1".to_string(),
            journal_id: "journal-1".to_string(),
            operation_id: "op-1".to_string(),
            route_kind: STUDIO_MESSAGE_ROUTE_KIND.to_string(),
            actor_key: "actor-1".to_string(),
            state_version,
            attempt: 0,
            status: "accepted".to_string(),
            state_json: state_json.to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn studio_execution_state_parser_requires_versioned_record() {
        let error = parse_studio_message_execution_state(&record_for(
            0,
            serde_json::json!({
                "session_id": "session-1",
                "user_message_id": "user-1",
                "assistant_message_id": "assistant-1",
                "accepted_response": {
                    "status": "accepted",
                    "session_id": "session-1",
                    "user_message_id": "user-1",
                    "assistant_message_id": "assistant-1",
                },
                "final_status_code": serde_json::Value::Null,
                "final_response": serde_json::Value::Null,
            }),
        ))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unsupported studio execution state version"),
            "{error}"
        );
    }

    #[test]
    fn studio_execution_state_parser_rejects_unknown_version() {
        let error = parse_studio_message_execution_state(&record_for(
            (STUDIO_MESSAGE_EXECUTION_STATE_VERSION + 1) as i64,
            serde_json::json!({
                "version": STUDIO_MESSAGE_EXECUTION_STATE_VERSION + 1,
                "session_id": "session-1",
                "user_message_id": "user-1",
                "assistant_message_id": "assistant-1",
                "accepted_response": {
                    "status": "accepted",
                    "session_id": "session-1",
                    "user_message_id": "user-1",
                    "assistant_message_id": "assistant-1",
                },
                "final_status_code": serde_json::Value::Null,
                "final_response": serde_json::Value::Null,
            }),
        ))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unsupported studio execution state version"),
            "{error}"
        );
    }

    #[test]
    fn session_cursor_round_trips_storage_sort_keys() {
        let row = cortex_storage::queries::studio_chat_queries::StudioSessionRow {
            id: "session-1".to_string(),
            agent_id: "agent-1".to_string(),
            title: "Session".to_string(),
            model: "qwen3.5:9b".to_string(),
            system_prompt: String::new(),
            temperature: 0.5,
            max_tokens: 4096,
            created_at: "2026-03-08 12:00:00".to_string(),
            updated_at: "2026-03-08 12:05:00".to_string(),
        };

        let encoded = encode_session_list_cursor(&row);
        let decoded = parse_session_list_cursor(&encoded).expect("cursor should decode");

        assert_eq!(decoded.id, row.id);
        assert_eq!(decoded.updated_at, row.updated_at);
    }

    #[test]
    fn normalize_public_timestamp_converts_sqlite_datetime_to_rfc3339() {
        assert_eq!(
            normalize_public_timestamp("2026-03-08 12:05:00"),
            "2026-03-08T12:05:00+00:00"
        );
        assert_eq!(
            normalize_public_timestamp("2026-03-08T12:05:00Z"),
            "2026-03-08T12:05:00+00:00"
        );
    }
}
