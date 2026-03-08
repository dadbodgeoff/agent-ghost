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
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use ghost_agent_loop::output_inspector::{InspectionResult, OutputInspector};
use ghost_agent_loop::runner::AgentStreamEvent;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult, ErrorResponse};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation,
    execute_idempotent_json_mutation, prepare_json_operation, PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, response_with_idempotency,
    write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::websocket::WsEvent;
use crate::provider_runtime;
use crate::runtime_safety::{
    parse_or_stable_uuid, RunnerBuildOptions, RuntimeSafetyBuilder, RuntimeSafetyContext,
    RuntimeSafetyError, STUDIO_SYNTHETIC_AGENT_NAME,
};
use crate::state::AppState;

const CREATE_SESSION_ROUTE_TEMPLATE: &str = "/api/studio/sessions";
const DELETE_SESSION_ROUTE_TEMPLATE: &str = "/api/studio/sessions/:id";
const SEND_MESSAGE_ROUTE_TEMPLATE: &str = "/api/studio/sessions/:id/messages";
const SEND_MESSAGE_STREAM_ROUTE_TEMPLATE: &str = "/api/studio/sessions/:id/messages/stream";
const STUDIO_MESSAGE_ROUTE_KIND: &str = "studio_send_message";

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
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    /// WP9-D: Only return sessions active since this datetime (ISO 8601).
    pub active_since: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionResponse>,
}

// ── Stream recovery types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RecoverStreamQuery {
    pub message_id: String,
    pub after_seq: i64,
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
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
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
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let sessions = {
        let db = state
            .db
            .read()
            .map_err(|e| ApiError::db_error("list_sessions", e))?;
        if let Some(ref since) = params.active_since {
            cortex_storage::queries::studio_chat_queries::list_sessions_active_since(
                &db, since, limit, offset,
            )
            .map_err(|e| ApiError::db_error("list_sessions", e))?
        } else {
            cortex_storage::queries::studio_chat_queries::list_sessions(&db, limit, offset)
                .map_err(|e| ApiError::db_error("list_sessions", e))?
        }
    };

    Ok(Json(SessionListResponse {
        sessions: sessions.into_iter().map(session_row_to_response).collect(),
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
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("recover_stream", e))?;

    let events = cortex_storage::queries::stream_event_queries::recover_events_after(
        &db,
        &session_id,
        &params.message_id,
        params.after_seq,
    )
    .map_err(|e| ApiError::db_error("recover_stream", e))?;

    let api_events = events
        .into_iter()
        .map(|row| StreamEventApiResponse {
            seq: row.id,
            event_type: row.event_type,
            payload: serde_json::from_str(&row.payload).unwrap_or(serde_json::json!({})),
            created_at: row.created_at,
        })
        .collect();

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
        Ok(PreparedOperation::Acquired { journal_id }) => {
            let operation_id = operation_context
                .operation_id
                .clone()
                .expect("prepared operations require operation_id");

            let mut execution_record = {
                let db = state.db.write().await;
                match cortex_storage::queries::live_execution_queries::get_by_journal_id(
                    &db,
                    &journal_id,
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
                            let _ = abort_prepared_json_operation(&db, &journal_id);
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
                            let _ = abort_prepared_json_operation(&db, &journal_id);
                            return error_response_with_idempotency(ApiError::not_found(format!(
                                "session {session_id} not found"
                            )));
                        }
                        Err(error) => {
                            let db = state.db.write().await;
                            let _ = abort_prepared_json_operation(&db, &journal_id);
                            return error_response_with_idempotency(ApiError::db_error(
                                "get_session",
                                error,
                            ));
                        }
                    }
                };

                let agent = match RuntimeSafetyBuilder::new(&state)
                    .resolve_stored_agent(&session.agent_id, STUDIO_SYNTHETIC_AGENT_NAME)
                    .map_err(map_runtime_safety_error)
                {
                    Ok(agent) => agent,
                    Err(error) => {
                        let db = state.db.write().await;
                        let _ = abort_prepared_json_operation(&db, &journal_id);
                        return error_response_with_idempotency(error);
                    }
                };
                if let Err(error) = ensure_agent_available(&state, agent.id) {
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &journal_id);
                    return error_response_with_idempotency(error);
                }

                let inspector = OutputInspector::new();
                let input_inspection = inspector.scan(&req.content, agent.id);
                let user_safety_status = match &input_inspection {
                    InspectionResult::Clean => "clean",
                    InspectionResult::Warning { .. } => "warning",
                    InspectionResult::KillAll { .. } => "blocked",
                };

                let history = {
                    let db = match state.db.read() {
                        Ok(db) => db,
                        Err(error) => {
                            let db = state.db.write().await;
                            let _ = abort_prepared_json_operation(&db, &journal_id);
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
                            let _ = abort_prepared_json_operation(&db, &journal_id);
                            return error_response_with_idempotency(ApiError::db_error(
                                "list_messages",
                                error,
                            ));
                        }
                    }
                };

                let runtime_ctx = RuntimeSafetyContext::from_state(
                    &state,
                    agent.clone(),
                    parse_or_stable_uuid(&session_id, "studio-session"),
                    None,
                );
                if let Err(error) = runtime_ctx
                    .ensure_execution_permitted()
                    .map_err(map_runner_error)
                {
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &journal_id);
                    return error_response_with_idempotency(error);
                }

                let mut runner = match RuntimeSafetyBuilder::new(&state).build_live_runner(
                    &runtime_ctx,
                    RunnerBuildOptions {
                        system_prompt: Some(session.system_prompt.clone()),
                        conversation_history: build_conversation_history(&history),
                        skill_allowlist: None,
                    },
                ) {
                    Ok(runner) => runner,
                    Err(error) => {
                        let db = state.db.write().await;
                        let _ = abort_prepared_json_operation(&db, &journal_id);
                        return error_response_with_idempotency(map_runtime_safety_error(error));
                    }
                };

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
                    let _ = abort_prepared_json_operation(&db, &journal_id);
                    return error_response_with_idempotency(map_runner_error(error));
                }

                let user_msg_id = Uuid::now_v7().to_string();
                let assistant_msg_id = Uuid::now_v7().to_string();
                let accepted_response =
                    studio_message_accepted_body(&session_id, &user_msg_id, &assistant_msg_id);
                let execution_state = StudioMessageExecutionState {
                    session_id: session_id.clone(),
                    user_message_id: user_msg_id.clone(),
                    assistant_message_id: assistant_msg_id.clone(),
                    accepted_response: accepted_response.clone(),
                    final_status_code: None,
                    final_response: None,
                };
                let execution_id = Uuid::now_v7().to_string();

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
                        "clean",
                    ) {
                        let _ = abort_prepared_json_operation(&db, &journal_id);
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
                        let _ = abort_prepared_json_operation(&db, &journal_id);
                        return error_response_with_idempotency(ApiError::db_error(
                            "insert_safety_audit",
                            error,
                        ));
                    }
                    if let Err(error) = persist_live_execution_record(
                        &db,
                        &execution_id,
                        &journal_id,
                        &operation_id,
                        actor,
                        STUDIO_MESSAGE_ROUTE_KIND,
                        "accepted",
                        &execution_state,
                    ) {
                        let _ = abort_prepared_json_operation(&db, &journal_id);
                        return error_response_with_idempotency(error);
                    }
                }

                if matches!(input_inspection, InspectionResult::KillAll { .. }) {
                    let response_body = validation_error_body(
                        "Message blocked: credential pattern detected in input",
                    );
                    return finalize_studio_message_terminal_response(
                        &state,
                        &journal_id,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_id,
                        execution_state,
                        StatusCode::UNPROCESSABLE_ENTITY,
                        response_body,
                    )
                    .await;
                }

                let providers = provider_runtime::ordered_provider_configs(&state);
                if providers.is_empty() {
                    let response_body = validation_error_body(
                        "No model providers configured. Add provider config to ghost.yml.",
                    );
                    return finalize_studio_message_terminal_response(
                        &state,
                        &journal_id,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_id,
                        execution_state,
                        StatusCode::UNPROCESSABLE_ENTITY,
                        response_body,
                    )
                    .await;
                }

                execution_record = Some(
                    cortex_storage::queries::live_execution_queries::LiveExecutionRecord {
                        id: execution_id,
                        journal_id,
                        operation_id,
                        route_kind: STUDIO_MESSAGE_ROUTE_KIND.to_string(),
                        actor_key: actor.to_string(),
                        status: "accepted".to_string(),
                        state_json: serde_json::to_string(&execution_state)
                            .unwrap_or_else(|_| "{}".to_string()),
                        created_at: String::new(),
                        updated_at: String::new(),
                    },
                );
            }

            let execution_record = execution_record.expect("execution record must exist");
            let execution_state = match serde_json::from_str::<StudioMessageExecutionState>(
                &execution_record.state_json,
            ) {
                Ok(state) => state,
                Err(error) => {
                    return error_response_with_idempotency(ApiError::internal(format!(
                        "failed to parse studio execution state: {error}"
                    )));
                }
            };

            match execution_record.status.as_str() {
                "completed" => {
                    if let Some((status, body)) =
                        stored_studio_message_terminal_response(&state.db, &execution_state)
                    {
                        return finalize_studio_message_terminal_response(
                            &state,
                            &execution_record.journal_id,
                            &operation_context,
                            &session_id,
                            actor,
                            &execution_record.id,
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
                        &execution_record.journal_id,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
                        execution_state,
                        StatusCode::ACCEPTED,
                        response_body,
                    )
                    .await;
                }
                "running" => {
                    if let Some((status, body)) =
                        stored_studio_message_terminal_response(&state.db, &execution_state)
                    {
                        return finalize_studio_message_terminal_response(
                            &state,
                            &execution_record.journal_id,
                            &operation_context,
                            &session_id,
                            actor,
                            &execution_record.id,
                            execution_state,
                            status,
                            body,
                        )
                        .await;
                    }

                    let response_body = studio_message_recovery_body(&execution_state);
                    return finalize_studio_message_recovery_response(
                        &state,
                        &execution_record.journal_id,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
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
                            &execution_record.journal_id,
                            &operation_context,
                            &session_id,
                            actor,
                            &execution_record.id,
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

            let agent = match RuntimeSafetyBuilder::new(&state)
                .resolve_stored_agent(&session.agent_id, STUDIO_SYNTHETIC_AGENT_NAME)
                .map_err(map_runtime_safety_error)
            {
                Ok(agent) => agent,
                Err(error) => return error_response_with_idempotency(error),
            };
            if let Err(error) = ensure_agent_available(&state, agent.id) {
                return error_response_with_idempotency(error);
            }

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

            let runtime_ctx = RuntimeSafetyContext::from_state(
                &state,
                agent.clone(),
                parse_or_stable_uuid(&execution_state.session_id, "studio-session"),
                None,
            );
            if let Err(error) = runtime_ctx
                .ensure_execution_permitted()
                .map_err(map_runner_error)
            {
                return error_response_with_idempotency(error);
            }

            let mut runner = match RuntimeSafetyBuilder::new(&state).build_live_runner(
                &runtime_ctx,
                RunnerBuildOptions {
                    system_prompt: Some(session.system_prompt.clone()),
                    conversation_history: build_conversation_history(&history_cutoff),
                    skill_allowlist: None,
                },
            ) {
                Ok(runner) => runner,
                Err(error) => {
                    return error_response_with_idempotency(map_runtime_safety_error(error));
                }
            };

            let providers = provider_runtime::ordered_provider_configs(&state);
            if providers.is_empty() {
                let response_body = validation_error_body(
                    "No model providers configured. Add provider config to ghost.yml.",
                );
                return finalize_studio_message_terminal_response(
                    &state,
                    &execution_record.journal_id,
                    &operation_context,
                    &session_id,
                    actor,
                    &execution_record.id,
                    execution_state,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    response_body,
                )
                .await;
            }

            let mut ctx = match runner
                .pre_loop(
                    runtime_ctx.agent.id,
                    runtime_ctx.session_id,
                    "studio",
                    &req.content,
                )
                .await
            {
                Ok(ctx) => ctx,
                Err(error) => {
                    return error_response_with_idempotency(map_runner_error(error));
                }
            };

            {
                let db = state.db.write().await;
                if let Err(error) = update_live_execution_state(
                    &db,
                    &execution_record.id,
                    "running",
                    &execution_state,
                ) {
                    return error_response_with_idempotency(error);
                }
            }

            let mut fallback_chain = provider_runtime::build_fallback_chain(&providers);
            let result = runner
                .run_turn(&mut ctx, &mut fallback_chain, &req.content)
                .await;

            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    let response_body = studio_message_recovery_body(&execution_state);
                    let response = finalize_studio_message_recovery_response(
                        &state,
                        &execution_record.journal_id,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
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
            let inspector = OutputInspector::new();
            let output_inspection = inspector.scan(&response_content, runtime_ctx.agent.id);
            let output_safety_status = match &output_inspection {
                InspectionResult::Clean => "clean",
                InspectionResult::Warning { .. } => "warning",
                InspectionResult::KillAll { .. } => "blocked",
            };

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
                        &execution_record.journal_id,
                        &operation_context,
                        &session_id,
                        actor,
                        &execution_record.id,
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

            crate::api::websocket::broadcast_event(
                &state,
                WsEvent::ChatMessage {
                    session_id: execution_state.session_id.clone(),
                    message_id: execution_state.assistant_message_id.clone(),
                    role: "assistant".into(),
                    content: truncate_preview(&response_content, 200),
                    safety_status: output_safety_status.into(),
                },
            );

            let final_body =
                match reconstruct_studio_message_completed_body(&state.db, &execution_state) {
                    Ok(Some(body)) => body,
                    Ok(None) => studio_message_recovery_body(&execution_state),
                    Err(error) => return error_response_with_idempotency(error),
                };

            finalize_studio_message_terminal_response(
                &state,
                &execution_record.journal_id,
                &operation_context,
                &session_id,
                actor,
                &execution_record.id,
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
    session_id: String,
    user_message_id: String,
    assistant_message_id: String,
    accepted_response: serde_json::Value,
    final_status_code: Option<u16>,
    final_response: Option<serde_json::Value>,
}

fn studio_message_accepted_body(
    session_id: &str,
    user_message_id: &str,
    assistant_message_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "status": "accepted",
        "session_id": session_id,
        "user_message_id": user_message_id,
        "assistant_message_id": assistant_message_id,
    })
}

fn studio_message_recovery_body(state: &StudioMessageExecutionState) -> serde_json::Value {
    let mut body = state.accepted_response.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert("recovery_required".into(), serde_json::Value::Bool(true));
    }
    body
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
            status,
            state_json: &state_json,
        },
    )
    .map_err(|error| ApiError::db_error("insert_live_execution_record", error))
}

fn update_live_execution_state(
    conn: &rusqlite::Connection,
    execution_id: &str,
    status: &str,
    state: &StudioMessageExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::update_status_and_state(
        conn,
        execution_id,
        status,
        &state_json,
    )
    .map_err(|error| ApiError::db_error("update_live_execution_record", error))
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
    journal_id: &str,
    operation_context: &OperationContext,
    session_id: &str,
    actor: &str,
    execution_id: &str,
    mut execution_state: StudioMessageExecutionState,
    status: StatusCode,
    body: serde_json::Value,
) -> Response {
    execution_state.final_status_code = Some(status.as_u16());
    execution_state.final_response = Some(body.clone());

    let db = state.db.write().await;
    if let Err(error) =
        update_live_execution_state(&db, execution_id, "completed", &execution_state)
    {
        return error_response_with_idempotency(error);
    }

    match commit_prepared_json_operation(&db, operation_context, journal_id, status, &body) {
        Ok(outcome) => {
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

async fn finalize_studio_message_recovery_response(
    state: &Arc<AppState>,
    journal_id: &str,
    operation_context: &OperationContext,
    session_id: &str,
    actor: &str,
    execution_id: &str,
    execution_state: StudioMessageExecutionState,
    body: serde_json::Value,
) -> Response {
    let db = state.db.write().await;
    if let Err(error) =
        update_live_execution_state(&db, execution_id, "recovery_required", &execution_state)
    {
        return error_response_with_idempotency(error);
    }

    match commit_prepared_json_operation(
        &db,
        operation_context,
        journal_id,
        StatusCode::ACCEPTED,
        &body,
    ) {
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

                let agent = RuntimeSafetyBuilder::new(&state)
                    .resolve_stored_agent(&session.agent_id, STUDIO_SYNTHETIC_AGENT_NAME)
                    .map_err(map_runtime_safety_error)?;
                ensure_agent_available(&state, agent.id)?;

                let inspector = OutputInspector::new();
                let input_inspection = inspector.scan(&user_content, agent.id);
                let user_safety_status = match &input_inspection {
                    InspectionResult::Clean => "clean",
                    InspectionResult::Warning { .. } => "warning",
                    InspectionResult::KillAll { .. } => "blocked",
                };

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
                    "clean",
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

                if provider_runtime::ordered_provider_configs(&state).is_empty() {
                    return Ok((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        validation_error_body(
                            "No model providers configured. Add provider config to ghost.yml.",
                        ),
                    ));
                }

                let assistant_msg_id = Uuid::now_v7().to_string();
                let start_payload = serde_json::json!({
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

    match idempotency_status {
        IdempotencyStatus::Executed => {
            let (stream_rx, task_handle) = spawn_studio_stream_execution(
                Arc::clone(&state),
                acceptance.session_id.clone(),
                acceptance.user_message_id.clone(),
                acceptance.assistant_message_id.clone(),
                user_content,
            );
            state.background_tasks.lock().await.push(task_handle);
            studio_live_stream_response(
                Arc::clone(&state),
                acceptance,
                stream_rx,
                IdempotencyStatus::Executed,
            )
        }
        IdempotencyStatus::Replayed => {
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
                IdempotencyStatus::Replayed,
            )
        }
        IdempotencyStatus::InProgress | IdempotencyStatus::Mismatch => unreachable!(),
    }
}

#[derive(Debug, Clone)]
struct StudioStreamAcceptance {
    session_id: String,
    user_message_id: String,
    assistant_message_id: String,
    stream_start_seq: i64,
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
    session_id: &str,
    user_message_id: &str,
    assistant_message_id: &str,
    stream_start_seq: i64,
) -> serde_json::Value {
    serde_json::json!({
        "status": "accepted",
        "session_id": session_id,
        "user_message_id": user_message_id,
        "assistant_message_id": assistant_message_id,
        "stream_start_seq": stream_start_seq,
    })
}

fn parse_studio_stream_acceptance(
    body: &serde_json::Value,
) -> Result<StudioStreamAcceptance, ApiError> {
    Ok(StudioStreamAcceptance {
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
        assistant_message_id: body
            .get("assistant_message_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("missing accepted assistant_message_id"))?
            .to_string(),
        stream_start_seq: body
            .get("stream_start_seq")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| ApiError::internal("missing accepted stream_start_seq"))?,
    })
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
    if message.content.is_empty() {
        return None;
    }

    Some(
        Event::default().event("text_delta").data(
            serde_json::json!({
                "content": message.content,
            })
            .to_string(),
        ),
    )
}

fn replay_fallback_stream_end_event(
    message: &cortex_storage::queries::studio_chat_queries::StudioMessageRow,
) -> Event {
    Event::default().event("stream_end").data(
        serde_json::json!({
            "message_id": message.id,
            "token_count": message.token_count,
            "safety_status": message.safety_status,
        })
        .to_string(),
    )
}

fn studio_replay_stream_response(
    acceptance: StudioStreamAcceptance,
    persisted_events: Vec<cortex_storage::queries::stream_event_queries::StreamEventRow>,
    assistant_message: Option<cortex_storage::queries::studio_chat_queries::StudioMessageRow>,
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
        }
    };

    response_with_idempotency(
        Sse::new(sse_stream)
            .keep_alive(studio_stream_keep_alive())
            .into_response(),
        idempotency_status,
    )
}

fn studio_live_stream_response(
    state: Arc<AppState>,
    acceptance: StudioStreamAcceptance,
    mut rx: tokio::sync::mpsc::Receiver<AgentStreamEvent>,
    idempotency_status: IdempotencyStatus,
) -> Response {
    let session_id_sse = acceptance.session_id.clone();
    let assistant_msg_id_sse = acceptance.assistant_message_id.clone();
    let db_for_stream = Arc::clone(&state.db);
    let state_for_stream = Arc::clone(&state);
    let start_event = studio_stream_start_event(&acceptance);

    let sse_stream = async_stream::stream! {
        let mut text_buffer = String::new();
        const TEXT_FLUSH_THRESHOLD: usize = 2048;
        let mut consecutive_persist_failures: u32 = 0;
        const PERSIST_FAILURE_WARN_THRESHOLD: u32 = 3;

        let persist_event = |db: &Arc<crate::db_pool::DbPool>, sid: &str, mid: &str, etype: &str, payload: &serde_json::Value, fail_count: &mut u32| -> Option<i64> {
            match db.read() {
                Ok(conn) => {
                    match cortex_storage::queries::stream_event_queries::insert_stream_event(
                        &conn, sid, mid, etype, &payload.to_string(),
                    ) {
                        Ok(seq) => {
                            *fail_count = 0;
                            Some(seq)
                        }
                        Err(e) => {
                            *fail_count += 1;
                            tracing::warn!(
                                error = %e,
                                event_type = etype,
                                consecutive_failures = *fail_count,
                                "failed to persist stream event"
                            );
                            None
                        }
                    }
                }
                Err(e) => {
                    *fail_count += 1;
                    tracing::warn!(
                        error = %e,
                        event_type = etype,
                        consecutive_failures = *fail_count,
                        "failed to acquire DB for stream event persistence"
                    );
                    None
                }
            }
        };

        let flush_text = |db: &Arc<crate::db_pool::DbPool>, sid: &str, mid: &str, buf: &mut String, fail_count: &mut u32| -> Option<i64> {
            if buf.is_empty() {
                return None;
            }
            let payload = serde_json::json!({ "content": buf.as_str() });
            let seq = match db.read() {
                Ok(conn) => {
                    match cortex_storage::queries::stream_event_queries::insert_stream_event(
                        &conn, sid, mid, "text_chunk", &payload.to_string(),
                    ) {
                        Ok(seq) => {
                            *fail_count = 0;
                            Some(seq)
                        }
                        Err(e) => {
                            *fail_count += 1;
                            tracing::warn!(error = %e, consecutive_failures = *fail_count, "failed to persist text_chunk");
                            None
                        }
                    }
                }
                Err(e) => {
                    *fail_count += 1;
                    tracing::warn!(error = %e, consecutive_failures = *fail_count, "failed to acquire DB for text_chunk persistence");
                    None
                }
            };
            buf.clear();
            seq
        };

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
                        .data(serde_json::json!({ "message_id": message_id }).to_string()));
                }
                AgentStreamEvent::TextDelta { content } => {
                    text_buffer.push_str(&content);
                    let mut seq = None;
                    if text_buffer.len() >= TEXT_FLUSH_THRESHOLD {
                        seq = flush_text(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, &mut text_buffer, &mut consecutive_persist_failures);
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
                    flush_text(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, &mut text_buffer, &mut consecutive_persist_failures);

                    let payload = serde_json::json!({
                        "tool": tool,
                        "tool_id": tool_id,
                        "status": status,
                    });
                    let seq = persist_event(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, "tool_use", &payload, &mut consecutive_persist_failures);
                    if consecutive_persist_failures == PERSIST_FAILURE_WARN_THRESHOLD {
                        yield Ok(Event::default()
                            .event("warning")
                            .data(serde_json::json!({
                                "code": "db_persistence_degraded",
                                "message": format!("{} consecutive DB persistence failures — stream events may not be recoverable on reconnect", consecutive_persist_failures),
                            }).to_string()));
                    }

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
                    let seq = persist_event(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, "tool_result", &payload, &mut consecutive_persist_failures);
                    if consecutive_persist_failures == PERSIST_FAILURE_WARN_THRESHOLD {
                        yield Ok(Event::default()
                            .event("warning")
                            .data(serde_json::json!({
                                "code": "db_persistence_degraded",
                                "message": format!("{} consecutive DB persistence failures — stream events may not be recoverable on reconnect", consecutive_persist_failures),
                            }).to_string()));
                    }

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
                    flush_text(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, &mut text_buffer, &mut consecutive_persist_failures);

                    let payload = serde_json::json!({
                        "message_id": assistant_msg_id_sse,
                        "token_count": token_count,
                        "safety_status": safety_status,
                    });
                    let seq = persist_event(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, "turn_complete", &payload, &mut consecutive_persist_failures);

                    let mut ev = Event::default()
                        .event("stream_end")
                        .data(payload.to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    yield Ok(ev);
                    stream_ended = true;
                    break;
                }
                AgentStreamEvent::Error { message } => {
                    flush_text(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, &mut text_buffer, &mut consecutive_persist_failures);

                    let payload = serde_json::json!({ "message": message });
                    let seq = persist_event(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, "error", &payload, &mut consecutive_persist_failures);

                    let mut ev = Event::default()
                        .event("error")
                        .data(payload.to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    yield Ok(ev);
                    stream_ended = true;
                    break;
                }
            }
        }

        if !stream_ended {
            flush_text(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, &mut text_buffer, &mut consecutive_persist_failures);

            yield Ok(Event::default()
                .event("stream_end")
                .data(serde_json::json!({
                    "message_id": assistant_msg_id_sse,
                    "token_count": 0,
                    "safety_status": "unknown",
                }).to_string()));
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

fn spawn_studio_stream_execution(
    state: Arc<AppState>,
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
        let session = {
            let db = match state_for_task.db.read() {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::Error {
                            message: format!(
                                "failed to load session: {}",
                                ApiError::db_error("get_session", error)
                            ),
                        })
                        .await;
                    return;
                }
            };
            match cortex_storage::queries::studio_chat_queries::get_session(&db, &session_id) {
                Ok(Some(session)) => session,
                Ok(None) => {
                    let _ = tx
                        .send(AgentStreamEvent::Error {
                            message: format!("session {session_id} not found"),
                        })
                        .await;
                    return;
                }
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::Error {
                            message: format!(
                                "failed to load session: {}",
                                ApiError::db_error("get_session", error)
                            ),
                        })
                        .await;
                    return;
                }
            }
        };

        let agent = match RuntimeSafetyBuilder::new(&state_for_task)
            .resolve_stored_agent(&session.agent_id, STUDIO_SYNTHETIC_AGENT_NAME)
            .map_err(map_runtime_safety_error)
        {
            Ok(agent) => agent,
            Err(error) => {
                let _ = tx
                    .send(AgentStreamEvent::Error {
                        message: error.to_string(),
                    })
                    .await;
                return;
            }
        };
        if let Err(error) = ensure_agent_available(&state_for_task, agent.id) {
            let _ = tx
                .send(AgentStreamEvent::Error {
                    message: error.to_string(),
                })
                .await;
            return;
        }

        let history = {
            let db = match state_for_task.db.read() {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::Error {
                            message: format!(
                                "failed to load history: {}",
                                ApiError::db_error("list_messages", error)
                            ),
                        })
                        .await;
                    return;
                }
            };
            match cortex_storage::queries::studio_chat_queries::list_messages(&db, &session_id) {
                Ok(history) => history,
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::Error {
                            message: format!(
                                "failed to load history: {}",
                                ApiError::db_error("list_messages", error)
                            ),
                        })
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

        let runtime_session_id = parse_or_stable_uuid(&session_id, "studio-session");
        let runtime_ctx = RuntimeSafetyContext::from_state(
            &state_for_task,
            agent.clone(),
            runtime_session_id,
            None,
        );
        if let Err(error) = runtime_ctx
            .ensure_execution_permitted()
            .map_err(map_runner_error)
        {
            let _ = tx
                .send(AgentStreamEvent::Error {
                    message: error.to_string(),
                })
                .await;
            return;
        }

        let mut runner = match RuntimeSafetyBuilder::new(&state_for_task).build_live_runner(
            &runtime_ctx,
            RunnerBuildOptions {
                system_prompt: Some(session.system_prompt.clone()),
                conversation_history: build_conversation_history(&history_cutoff),
                skill_allowlist: None,
            },
        ) {
            Ok(runner) => runner,
            Err(error) => {
                let _ = tx
                    .send(AgentStreamEvent::Error {
                        message: map_runtime_safety_error(error).to_string(),
                    })
                    .await;
                return;
            }
        };

        let all_providers = provider_runtime::ordered_provider_configs(&state_for_task);
        if all_providers.is_empty() {
            let _ = tx
                .send(AgentStreamEvent::Error {
                    message: "No model providers configured. Add provider config to ghost.yml."
                        .into(),
                })
                .await;
            return;
        }

        let session_id_clone = session_id.clone();
        let assistant_msg_id_clone = assistant_msg_id.clone();
        let session_title = session.title.clone();
        let db_clone = Arc::clone(&state_for_task.db);
        let state_clone = Arc::clone(&state_for_task);
        let user_content_for_title = user_content.clone();
        let tx_timeout = tx.clone();

        let turn_result = tokio::time::timeout(std::time::Duration::from_secs(300), async move {
            let mut ctx = match runner
                .pre_loop(
                    runtime_ctx.agent.id,
                    runtime_ctx.session_id,
                    "studio",
                    &user_content,
                )
                .await
            {
                Ok(ctx) => ctx,
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::Error {
                            message: format!("agent pre-loop failed: {error}"),
                        })
                        .await;
                    return;
                }
            };

            let mut result = Err(ghost_agent_loop::runner::RunError::LLMError(
                "no providers configured".into(),
            ));

            for (provider_idx, provider_config) in all_providers.iter().enumerate() {
                let provider = provider_config.clone();
                let get_stream = move |messages: Vec<ghost_llm::provider::ChatMessage>,
                                       tools: Vec<ghost_llm::provider::ToolSchema>|
                      -> ghost_llm::streaming::StreamChunkStream {
                    provider_runtime::build_provider_stream(&provider, messages, tools)
                };

                tracing::info!(
                    provider = %provider_config.name,
                    index = provider_idx,
                    "attempting streaming with provider"
                );

                match runner
                    .run_turn_streaming(&mut ctx, &user_content, tx.clone(), get_stream)
                    .await
                {
                    Ok(run_result) => {
                        if provider_idx > 0 {
                            tracing::info!(
                                provider = %provider_config.name,
                                index = provider_idx,
                                "streaming succeeded via fallback provider"
                            );
                        }
                        result = Ok(run_result);
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(
                            provider = %provider_config.name,
                            index = provider_idx,
                            error = %error,
                            "provider failed, trying next"
                        );
                        ctx.recursion_depth = 0;
                        result = Err(error);
                    }
                }
            }

            match result {
                Ok(run_result) => {
                    let response_content = run_result.output.unwrap_or_default();
                    let token_count = run_result.total_tokens as i64;
                    let inspector = OutputInspector::new();
                    let output_inspection = inspector.scan(&response_content, runtime_ctx.agent.id);
                    let output_safety_status = match &output_inspection {
                        InspectionResult::Clean => "clean",
                        InspectionResult::Warning { .. } => "warning",
                        InspectionResult::KillAll { .. } => "blocked",
                    };

                    {
                        let db = db_clone.write().await;
                        let _ = cortex_storage::queries::studio_chat_queries::insert_message(
                            &db,
                            &assistant_msg_id_clone,
                            &session_id_clone,
                            "assistant",
                            &response_content,
                            token_count,
                            output_safety_status,
                        );

                        let audit_id = Uuid::now_v7().to_string();
                        let detail = match &output_inspection {
                            InspectionResult::Warning { pattern_name, .. } => {
                                Some(pattern_name.as_str())
                            }
                            InspectionResult::KillAll { pattern_name, .. } => {
                                Some(pattern_name.as_str())
                            }
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
                            let _ =
                                cortex_storage::queries::studio_chat_queries::update_session_title(
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

                    let _ = tx
                        .send(AgentStreamEvent::TurnComplete {
                            token_count: run_result.total_tokens,
                            safety_status: output_safety_status.to_string(),
                        })
                        .await;
                }
                Err(error) => {
                    let _ = tx
                        .send(AgentStreamEvent::Error {
                            message: format!("agent run failed: {error}"),
                        })
                        .await;
                }
            }
        })
        .await;

        if turn_result.is_err() {
            tracing::warn!("Agent turn timed out after 5 minutes");
            let _ = tx_timeout
                .send(AgentStreamEvent::Error {
                    message: "Agent turn timed out after 5 minutes".into(),
                })
                .await;
        }
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
        created_at: row.created_at,
        updated_at: row.updated_at,
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
        created_at: row.created_at,
    }
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

fn map_runtime_safety_error(error: RuntimeSafetyError) -> ApiError {
    match error {
        RuntimeSafetyError::AgentNotFound(message) => ApiError::bad_request(message),
        _ => ApiError::internal(error.to_string()),
    }
}

fn ensure_agent_available(state: &AppState, agent_id: Uuid) -> Result<(), ApiError> {
    match state.kill_switch.check(agent_id) {
        crate::safety::kill_switch::KillCheckResult::Ok => Ok(()),
        crate::safety::kill_switch::KillCheckResult::AgentPaused(_) => Err(ApiError::custom(
            StatusCode::LOCKED,
            "AGENT_PAUSED",
            "Agent is paused",
        )),
        crate::safety::kill_switch::KillCheckResult::AgentQuarantined(_) => Err(ApiError::custom(
            StatusCode::LOCKED,
            "AGENT_QUARANTINED",
            "Agent is quarantined",
        )),
        crate::safety::kill_switch::KillCheckResult::PlatformKilled => {
            Err(ApiError::KillSwitchActive)
        }
    }
}

fn map_runner_error(error: ghost_agent_loop::runner::RunError) -> ApiError {
    match error {
        ghost_agent_loop::runner::RunError::AgentPaused => {
            ApiError::custom(StatusCode::LOCKED, "AGENT_PAUSED", "Agent is paused")
        }
        ghost_agent_loop::runner::RunError::AgentQuarantined => ApiError::custom(
            StatusCode::LOCKED,
            "AGENT_QUARANTINED",
            "Agent is quarantined",
        ),
        ghost_agent_loop::runner::RunError::PlatformKilled => ApiError::KillSwitchActive,
        ghost_agent_loop::runner::RunError::KillGateClosed => ApiError::custom(
            StatusCode::SERVICE_UNAVAILABLE,
            "DISTRIBUTED_KILL_GATE_CLOSED",
            "Distributed kill gate is closed",
        ),
        ghost_agent_loop::runner::RunError::ConvergenceProtectionDegraded(status) => {
            ApiError::custom(
                StatusCode::SERVICE_UNAVAILABLE,
                "CONVERGENCE_PROTECTION_DEGRADED",
                format!("Convergence protection is {status}"),
            )
        }
        other => ApiError::internal(format!("agent run failed: {other}")),
    }
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
