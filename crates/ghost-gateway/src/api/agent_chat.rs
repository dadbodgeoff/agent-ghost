//! Agent chat endpoint — full AgentRunner via HTTP.
//!
//! POST /api/agent/chat        — runs one turn of the agent loop (blocking JSON response)
//! POST /api/agent/chat/stream — runs one turn with SSE streaming + event persistence

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

use ghost_agent_loop::runner::{AgentStreamErrorType, AgentStreamEvent};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ErrorResponse};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation, prepare_json_operation,
    start_operation_lease_heartbeat, PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, response_with_idempotency,
    write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::runtime_execution::{
    execute_blocking_turn, inspect_text_safety, inspection_safety_status, map_runner_error,
    pre_loop_blocking_turn, prepare_requested_runtime_execution, PreparedRuntimeExecution,
};
use crate::api::stream_runtime::execute_streaming_turn;
use crate::api::websocket::WsEvent;
use crate::runtime_safety::{RunnerBuildOptions, RuntimeSafetyBuilder, API_SYNTHETIC_AGENT_NAME};
use crate::state::AppState;

const AGENT_CHAT_ROUTE_TEMPLATE: &str = "/api/agent/chat";
const AGENT_CHAT_STREAM_ROUTE_TEMPLATE: &str = "/api/agent/chat/stream";
const AGENT_CHAT_ROUTE_KIND: &str = "agent_chat";
const AGENT_CHAT_STREAM_ROUTE_KIND: &str = "agent_chat_stream";
const AGENT_CHAT_EXECUTION_STATE_VERSION: u32 = 1;
const AGENT_STREAM_EXECUTION_STATE_VERSION: u32 = 1;

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct AgentChatRequest {
    /// User message.
    pub message: String,
    /// Optional durable agent identity (UUID or registered agent name).
    pub agent_id: Option<String>,
    /// Optional session ID for multi-turn conversations.
    #[schema(value_type = String)]
    pub session_id: Option<Uuid>,
    /// Optional model override.
    pub model: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentChatResponse {
    pub content: String,
    pub session_id: String,
    pub tool_calls_made: u32,
    pub total_tokens: usize,
    pub total_cost: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentChatAcceptedResponse {
    pub status: String,
    pub session_id: String,
    pub agent_id: String,
    pub execution_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentChatExecutionState {
    version: u32,
    session_id: String,
    accepted_response: serde_json::Value,
    final_status_code: Option<u16>,
    final_response: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct AgentStreamAcceptance {
    session_id: String,
    message_id: String,
    stream_start_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentStreamExecutionState {
    version: u32,
    session_id: String,
    agent_id: String,
    message_id: String,
    stream_start_seq: i64,
    recovery_required: bool,
    terminal_event_type: Option<String>,
    terminal_payload: Option<serde_json::Value>,
}

/// POST /api/agent/chat
///
/// Runs one turn of the full agent loop with environment awareness, SOUL.md identity,
/// skills, tool execution, and safety gate checks.
pub async fn agent_chat(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<AgentChatRequest>,
) -> Response {
    if req.message.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request("message must not be empty"));
    }

    let actor = agent_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = agent_request_body(&req);
    let audit_agent_id = agent_audit_target_id(&state, req.agent_id.as_deref());
    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            AGENT_CHAT_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                &audit_agent_id,
                "agent_chat",
                "medium",
                actor,
                "replayed",
                agent_chat_audit_details(&stored.body),
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
                "route_template": AGENT_CHAT_ROUTE_TEMPLATE,
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
                .unwrap_or_else(|| operation_context.request_id.clone());

            let execution_record = {
                let db = state.db.write().await;
                match cortex_storage::queries::live_execution_queries::get_by_journal_id(
                    &db,
                    &lease.journal_id,
                ) {
                    Ok(Some(record)) => Some(record),
                    Ok(None) => {
                        let session_id = req.session_id.unwrap_or_else(Uuid::now_v7).to_string();
                        let accepted_response = agent_chat_accepted_body(
                            &session_id,
                            &audit_agent_id,
                            &Uuid::now_v7().to_string(),
                        );
                        let execution_id = accepted_response
                            .get("execution_id")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let execution_state = AgentChatExecutionState {
                            version: AGENT_CHAT_EXECUTION_STATE_VERSION,
                            session_id,
                            accepted_response: accepted_response.clone(),
                            final_status_code: None,
                            final_response: None,
                        };

                        if let Err(error) = persist_agent_execution_record(
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
                                route_kind: AGENT_CHAT_ROUTE_KIND.to_string(),
                                actor_key: actor.to_string(),
                                state_version: AGENT_CHAT_EXECUTION_STATE_VERSION as i64,
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
                            "get_live_execution_record",
                            error,
                        ));
                    }
                }
            };

            let execution_record =
                execution_record.expect("agent chat execution record must exist");
            let execution_state = match parse_agent_chat_execution_state(&execution_record) {
                Ok(state) => state,
                Err(error) => {
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(error);
                }
            };
            let execution_id = execution_record.id.clone();
            let agent_audit_id = execution_state
                .accepted_response
                .get("agent_id")
                .and_then(|value| value.as_str())
                .unwrap_or(audit_agent_id.as_str())
                .to_string();

            match execution_record.status.as_str() {
                "completed" => {
                    if let Some((status, body)) =
                        stored_agent_chat_terminal_response(&execution_state)
                    {
                        return finalize_agent_chat_terminal_response(
                            &state,
                            &lease,
                            &operation_context,
                            &agent_audit_id,
                            actor,
                            &execution_id,
                            execution_state,
                            status,
                            body,
                        )
                        .await;
                    }

                    let recovery_body = agent_chat_recovery_body(&execution_state);
                    return finalize_agent_chat_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        &agent_audit_id,
                        actor,
                        &execution_id,
                        execution_state,
                        recovery_body,
                    )
                    .await;
                }
                "running" | "recovery_required" => {
                    let recovery_body = agent_chat_recovery_body(&execution_state);
                    return finalize_agent_chat_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        &agent_audit_id,
                        actor,
                        &execution_id,
                        execution_state,
                        recovery_body,
                    )
                    .await;
                }
                "accepted" => {}
                other => {
                    return error_response_with_idempotency(ApiError::internal(format!(
                        "unexpected agent chat execution status: {other}"
                    )));
                }
            }

            let prepared_runtime = match prepare_requested_runtime_execution(
                &state,
                req.agent_id.as_deref(),
                API_SYNTHETIC_AGENT_NAME,
                req.session_id
                    .unwrap_or_else(|| parse_execution_session_id(&execution_state)),
                RunnerBuildOptions::default(),
            ) {
                Ok(prepared_runtime) => prepared_runtime,
                Err(error) => {
                    let (status, body) = api_error_status_and_body(error);
                    return finalize_agent_chat_terminal_response(
                        &state,
                        &lease,
                        &operation_context,
                        &agent_audit_id,
                        actor,
                        &execution_id,
                        execution_state,
                        status,
                        body,
                    )
                    .await;
                }
            };

            let PreparedRuntimeExecution {
                agent_id,
                runtime_ctx,
                mut runner,
                providers,
            } = prepared_runtime;

            if providers.is_empty() {
                let (status, body) = api_error_status_and_body(ApiError::bad_request(
                    "No model providers configured. Add provider config to ghost.yml to enable agent chat.",
                ));
                return finalize_agent_chat_terminal_response(
                    &state,
                    &lease,
                    &operation_context,
                    &agent_id,
                    actor,
                    &execution_id,
                    execution_state,
                    status,
                    body,
                )
                .await;
            }

            let heartbeat = start_operation_lease_heartbeat(Arc::clone(&state.db), lease.clone());
            let mut ctx = match pre_loop_blocking_turn(
                &mut runner,
                &runtime_ctx,
                "api",
                &req.message,
            )
            .await
            {
                Ok(ctx) => ctx,
                Err(error) => {
                    let heartbeat_result = heartbeat.stop().await;
                    if let Err(error) = heartbeat_result {
                        return error_response_with_idempotency(error);
                    }
                    let (status, body) = api_error_status_and_body(map_runner_error(error));
                    return finalize_agent_chat_terminal_response(
                        &state,
                        &lease,
                        &operation_context,
                        &agent_id,
                        actor,
                        &execution_id,
                        execution_state,
                        status,
                        body,
                    )
                    .await;
                }
            };

            if let Err(error) = {
                let db = state.db.write().await;
                update_agent_execution_state(&db, &execution_id, "running", &execution_state)
            } {
                return error_response_with_idempotency(error);
            }

            let result = match execute_blocking_turn(
                &mut runner,
                &mut ctx,
                &req.message,
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
                Err(error) => {
                    let heartbeat_result = heartbeat.stop().await;
                    if let Err(error) = heartbeat_result {
                        return error_response_with_idempotency(error);
                    }
                    let recovery_body = agent_chat_recovery_body(&execution_state);
                    tracing::warn!(
                        operation_id = %operation_id,
                        session_id = %execution_state.session_id,
                        error = %error,
                        "agent chat execution entered recovery-required state"
                    );
                    return finalize_agent_chat_recovery_response(
                        &state,
                        &lease,
                        &operation_context,
                        &agent_id,
                        actor,
                        &execution_id,
                        execution_state,
                        recovery_body,
                    )
                    .await;
                }
            };

            let response_content = result.output.clone().unwrap_or_default();
            let body = serde_json::to_value(AgentChatResponse {
                content: response_content,
                session_id: runtime_ctx.session_id.to_string(),
                tool_calls_made: result.tool_calls_made,
                total_tokens: result.total_tokens,
                total_cost: result.total_cost,
            })
            .unwrap_or(serde_json::Value::Null);
            {
                let db = state.db.write().await;
                if let Err(error) = persist_blocking_runtime_session_turn(
                    &db,
                    &runtime_ctx.session_id.to_string(),
                    &agent_id,
                    &req.message,
                    &result,
                ) {
                    return error_response_with_idempotency(error);
                }
            }
            finalize_agent_chat_terminal_response(
                &state,
                &lease,
                &operation_context,
                &agent_id,
                actor,
                &execution_id,
                execution_state,
                StatusCode::OK,
                body,
            )
            .await
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/agent/chat/stream
///
/// Streaming variant of agent_chat. Returns SSE events with event persistence
/// and WebSocket milestone broadcasts for cross-client awareness.
pub async fn agent_chat_stream(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<AgentChatRequest>,
) -> Response {
    if req.message.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request("message must not be empty"));
    }

    let actor = agent_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = agent_request_body(&req);
    let user_message = req.message.clone();
    let requested_agent_id = req.agent_id.clone();
    let audit_agent_id = agent_audit_target_id(&state, requested_agent_id.as_deref());
    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            AGENT_CHAT_STREAM_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let acceptance = match parse_agent_stream_acceptance(&stored.body) {
                Ok(acceptance) => acceptance,
                Err(error) => {
                    return response_with_idempotency(
                        error.into_response(),
                        IdempotencyStatus::Replayed,
                    );
                }
            };
            let (persisted_events, execution_state) = {
                let db = match state.db.read() {
                    Ok(db) => db,
                    Err(error) => {
                        return response_with_idempotency(
                            ApiError::db_error("replay_stream_read", error).into_response(),
                            IdempotencyStatus::Replayed,
                        );
                    }
                };
                let persisted_events =
                    match cortex_storage::queries::stream_event_queries::recover_events_after(
                        &db,
                        &acceptance.session_id,
                        &acceptance.message_id,
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
                let execution_state = operation_context
                    .operation_id
                    .as_deref()
                    .and_then(|operation_id| {
                        cortex_storage::queries::live_execution_queries::get_by_operation_id(
                            &db,
                            operation_id,
                        )
                        .ok()
                        .flatten()
                    })
                    .and_then(|record| parse_agent_stream_execution_state(&record).ok());
                (persisted_events, execution_state)
            };

            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                &audit_agent_id,
                "agent_chat_stream",
                "medium",
                actor,
                "replayed",
                agent_chat_audit_details(&stored.body),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            agent_replay_stream_response(
                acceptance,
                persisted_events,
                execution_state,
                IdempotencyStatus::Replayed,
            )
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": AGENT_CHAT_STREAM_ROUTE_TEMPLATE,
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

            let (acceptance, execution_id, execution_state) = {
                let db = state.db.write().await;
                match cortex_storage::queries::live_execution_queries::get_by_journal_id(
                    &db,
                    &lease.journal_id,
                ) {
                    Ok(Some(record)) => {
                        let execution_state = match parse_agent_stream_execution_state(&record) {
                            Ok(state) => state,
                            Err(error) => {
                                let _ =
                                    abort_prepared_json_operation(&db, &operation_context, &lease);
                                return error_response_with_idempotency(error);
                            }
                        };
                        let accepted_body = agent_stream_accepted_body(
                            &execution_state.session_id,
                            &execution_state.agent_id,
                            &execution_state.message_id,
                            execution_state.stream_start_seq,
                        );
                        match commit_prepared_json_operation(
                            &db,
                            &operation_context,
                            &lease,
                            StatusCode::OK,
                            &accepted_body,
                        ) {
                            Ok(outcome) => {
                                write_mutation_audit_entry(
                                    &db,
                                    &audit_agent_id,
                                    "agent_chat_stream",
                                    "medium",
                                    actor,
                                    "accepted",
                                    agent_chat_audit_details(&outcome.body),
                                    &operation_context,
                                    &outcome.idempotency_status,
                                );
                                let acceptance = parse_agent_stream_acceptance(&outcome.body)
                                    .expect("stored stream acceptance must parse");
                                (acceptance, record.id, execution_state)
                            }
                            Err(error) => return error_response_with_idempotency(error),
                        }
                    }
                    Ok(None) => {
                        let session_id_uuid = req.session_id.unwrap_or_else(Uuid::now_v7);
                        let prepared_runtime = match prepare_requested_runtime_execution(
                            &state,
                            requested_agent_id.as_deref(),
                            API_SYNTHETIC_AGENT_NAME,
                            session_id_uuid,
                            RunnerBuildOptions::default(),
                        ) {
                            Ok(prepared_runtime) => prepared_runtime,
                            Err(error) => {
                                let _ =
                                    abort_prepared_json_operation(&db, &operation_context, &lease);
                                return error_response_with_idempotency(error);
                            }
                        };
                        if prepared_runtime.providers.is_empty() {
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(ApiError::bad_request(
                                "No model providers configured. Add provider config to ghost.yml to enable agent chat.",
                            ));
                        }

                        let session_id = session_id_uuid.to_string();
                        let message_id = Uuid::now_v7().to_string();
                        let start_payload = serde_json::json!({
                            "session_id": session_id,
                            "message_id": message_id,
                        });
                        let start_seq =
                            match cortex_storage::queries::stream_event_queries::insert_stream_event(
                                &db,
                                &session_id,
                                &message_id,
                                "stream_start",
                                &start_payload.to_string(),
                            ) {
                                Ok(seq) => seq,
                                Err(error) => {
                                    let _ = abort_prepared_json_operation(
                                        &db,
                                        &operation_context,
                                        &lease,
                                    );
                                    return error_response_with_idempotency(ApiError::db_error(
                                        "insert_stream_start",
                                        error,
                                    ));
                                }
                            };
                        if let Err(error) = persist_runtime_session_event(
                            &db,
                            &session_id,
                            &prepared_runtime.agent_id,
                            "stream_start",
                            &start_payload,
                            None,
                        ) {
                            let _ = cortex_storage::queries::stream_event_queries::delete_events_for_message(
                                &db,
                                &message_id,
                            );
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(error);
                        }
                        let execution_state = agent_stream_execution_state(
                            &session_id,
                            &prepared_runtime.agent_id,
                            &message_id,
                            start_seq,
                        );
                        if let Err(error) = persist_agent_stream_execution_record(
                            &db,
                            &message_id,
                            &lease.journal_id,
                            &operation_id,
                            actor,
                            &execution_state,
                        ) {
                            let _ = cortex_storage::queries::stream_event_queries::delete_events_for_message(
                                &db,
                                &message_id,
                            );
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(error);
                        }

                        let accepted_body = agent_stream_accepted_body(
                            &session_id,
                            &prepared_runtime.agent_id,
                            &message_id,
                            start_seq,
                        );
                        match commit_prepared_json_operation(
                            &db,
                            &operation_context,
                            &lease,
                            StatusCode::OK,
                            &accepted_body,
                        ) {
                            Ok(outcome) => {
                                write_mutation_audit_entry(
                                    &db,
                                    &audit_agent_id,
                                    "agent_chat_stream",
                                    "medium",
                                    actor,
                                    "accepted",
                                    agent_chat_audit_details(&outcome.body),
                                    &operation_context,
                                    &outcome.idempotency_status,
                                );
                                let acceptance = parse_agent_stream_acceptance(&outcome.body)
                                    .expect("accepted stream body must parse");
                                (acceptance, message_id, execution_state)
                            }
                            Err(error) => return error_response_with_idempotency(error),
                        }
                    }
                    Err(error) => {
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(ApiError::db_error(
                            "load_agent_stream_execution_record",
                            error,
                        ));
                    }
                }
            };

            let (stream_rx, task_handle) = spawn_agent_chat_stream_execution(
                Arc::clone(&state),
                requested_agent_id,
                acceptance.session_id.clone(),
                acceptance.message_id.clone(),
                user_message,
            );
            state.background_tasks.lock().await.push(task_handle);
            agent_live_stream_response(
                Arc::clone(&state),
                acceptance,
                execution_id,
                execution_state,
                stream_rx,
                IdempotencyStatus::Executed,
            )
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

fn agent_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

fn agent_request_body(req: &AgentChatRequest) -> serde_json::Value {
    serde_json::json!({
        "message": req.message,
        "agent_id": req.agent_id,
        "session_id": req.session_id.map(|session_id| session_id.to_string()),
        "model": req.model,
    })
}

fn agent_audit_target_id(state: &AppState, requested_agent_id: Option<&str>) -> String {
    RuntimeSafetyBuilder::new(state)
        .resolve_agent(requested_agent_id, API_SYNTHETIC_AGENT_NAME)
        .map(|agent| agent.id.to_string())
        .unwrap_or_else(|_| {
            requested_agent_id
                .unwrap_or(API_SYNTHETIC_AGENT_NAME)
                .to_string()
        })
}

fn agent_chat_accepted_body(
    session_id: &str,
    agent_id: &str,
    execution_id: &str,
) -> serde_json::Value {
    serde_json::to_value(AgentChatAcceptedResponse {
        status: "accepted".to_string(),
        session_id: session_id.to_string(),
        agent_id: agent_id.to_string(),
        execution_id: execution_id.to_string(),
        recovery_required: None,
    })
    .unwrap_or(serde_json::Value::Null)
}

fn agent_chat_recovery_body(state: &AgentChatExecutionState) -> serde_json::Value {
    let mut body = state.accepted_response.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert("recovery_required".into(), serde_json::Value::Bool(true));
    }
    body
}

fn parse_agent_chat_execution_state(
    record: &cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
) -> Result<AgentChatExecutionState, ApiError> {
    if record.state_version != AGENT_CHAT_EXECUTION_STATE_VERSION as i64 {
        return Err(ApiError::internal(format!(
            "unsupported agent chat execution state version: {}",
            record.state_version
        )));
    }

    let state =
        serde_json::from_str::<AgentChatExecutionState>(&record.state_json).map_err(|error| {
            ApiError::internal(format!(
                "failed to parse agent chat execution state: {error}"
            ))
        })?;
    if state.version != AGENT_CHAT_EXECUTION_STATE_VERSION {
        return Err(ApiError::internal(format!(
            "unsupported agent chat execution state version: {}",
            state.version
        )));
    }

    Ok(state)
}

fn parse_execution_session_id(state: &AgentChatExecutionState) -> Uuid {
    Uuid::parse_str(&state.session_id).unwrap_or_else(|_| Uuid::now_v7())
}

fn api_error_status_and_body(error: ApiError) -> (StatusCode, serde_json::Value) {
    match error {
        ApiError::NotFound { entity, id } => (
            StatusCode::NOT_FOUND,
            serde_json::to_value(ErrorResponse::new(
                "NOT_FOUND",
                format!("Not found: {entity} {id}"),
            ))
            .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Validation(message) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            serde_json::to_value(ErrorResponse::new("VALIDATION_ERROR", message))
                .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Unauthorized(message) => (
            StatusCode::UNAUTHORIZED,
            serde_json::to_value(ErrorResponse::new("UNAUTHORIZED", message))
                .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Forbidden(message) => (
            StatusCode::FORBIDDEN,
            serde_json::to_value(ErrorResponse::new("FORBIDDEN", message))
                .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Conflict(message) => (
            StatusCode::CONFLICT,
            serde_json::to_value(ErrorResponse::new("CONFLICT", message))
                .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::KillSwitchActive => (
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::to_value(ErrorResponse::new(
                "KILL_SWITCH_ACTIVE",
                "Kill switch active",
            ))
            .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::to_value(ErrorResponse::new(
                "DATABASE_ERROR",
                "An internal database error occurred",
            ))
            .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::LockPoisoned(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::to_value(ErrorResponse::new(
                "INTERNAL_ERROR",
                "An internal error occurred — please retry or restart the service",
            ))
            .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Provider(_) => (
            StatusCode::BAD_GATEWAY,
            serde_json::to_value(ErrorResponse::new(
                "PROVIDER_ERROR",
                "An upstream provider error occurred",
            ))
            .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Internal(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::to_value(ErrorResponse::new(
                "INTERNAL_ERROR",
                "An internal error occurred",
            ))
            .unwrap_or(serde_json::Value::Null),
        ),
        ApiError::Custom {
            status,
            code,
            message,
            details,
        } => (
            status,
            serde_json::to_value(if let Some(details) = details {
                ErrorResponse::with_details(code, message, details)
            } else {
                ErrorResponse::new(code, message)
            })
            .unwrap_or(serde_json::Value::Null),
        ),
    }
}

fn persist_agent_execution_record(
    conn: &rusqlite::Connection,
    execution_id: &str,
    journal_id: &str,
    operation_id: &str,
    actor: &str,
    state: &AgentChatExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::insert(
        conn,
        &cortex_storage::queries::live_execution_queries::NewLiveExecutionRecord {
            id: execution_id,
            journal_id,
            operation_id,
            route_kind: AGENT_CHAT_ROUTE_KIND,
            actor_key: actor,
            state_version: AGENT_CHAT_EXECUTION_STATE_VERSION as i64,
            status: "accepted",
            state_json: &state_json,
        },
    )
    .map_err(|error| ApiError::db_error("insert_live_execution_record", error))
}

fn update_agent_execution_state(
    conn: &rusqlite::Connection,
    execution_id: &str,
    status: &str,
    state: &AgentChatExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::update_status_and_state(
        conn,
        execution_id,
        AGENT_CHAT_EXECUTION_STATE_VERSION as i64,
        status,
        &state_json,
    )
    .map_err(|error| ApiError::db_error("update_live_execution_record", error))
}

fn stored_agent_chat_terminal_response(
    state: &AgentChatExecutionState,
) -> Option<(StatusCode, serde_json::Value)> {
    let status = StatusCode::from_u16(state.final_status_code?).ok()?;
    let body = state.final_response.clone()?;
    Some((status, body))
}

async fn finalize_agent_chat_terminal_response(
    state: &Arc<AppState>,
    lease: &crate::api::idempotency::PreparedOperationLease,
    operation_context: &OperationContext,
    agent_id: &str,
    actor: &str,
    execution_id: &str,
    mut execution_state: AgentChatExecutionState,
    status: StatusCode,
    body: serde_json::Value,
) -> Response {
    execution_state.final_status_code = Some(status.as_u16());
    execution_state.final_response = Some(body.clone());

    let db = state.db.write().await;
    if let Err(error) =
        update_agent_execution_state(&db, execution_id, "completed", &execution_state)
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
                agent_id,
                "agent_chat",
                "medium",
                actor,
                audit_outcome,
                agent_chat_audit_details(&outcome.body),
                operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

async fn finalize_agent_chat_recovery_response(
    state: &Arc<AppState>,
    lease: &crate::api::idempotency::PreparedOperationLease,
    operation_context: &OperationContext,
    agent_id: &str,
    actor: &str,
    execution_id: &str,
    execution_state: AgentChatExecutionState,
    body: serde_json::Value,
) -> Response {
    let db = state.db.write().await;
    if let Err(error) =
        update_agent_execution_state(&db, execution_id, "recovery_required", &execution_state)
    {
        return error_response_with_idempotency(error);
    }

    match commit_prepared_json_operation(&db, operation_context, lease, StatusCode::ACCEPTED, &body)
    {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                agent_id,
                "agent_chat",
                "medium",
                actor,
                "accepted",
                agent_chat_audit_details(&outcome.body),
                operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

fn agent_chat_audit_details(body: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "status": body.get("status").cloned().unwrap_or(serde_json::Value::Null),
        "session_id": body.get("session_id").cloned().unwrap_or(serde_json::Value::Null),
        "agent_id": body.get("agent_id").cloned().unwrap_or(serde_json::Value::Null),
        "execution_id": body.get("execution_id").cloned().unwrap_or(serde_json::Value::Null),
        "message_id": body.get("message_id").cloned().unwrap_or(serde_json::Value::Null),
        "recovery_required": body.get("recovery_required").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "error": body.get("error").cloned().unwrap_or(serde_json::Value::Null),
    })
}

fn agent_stream_accepted_body(
    session_id: &str,
    agent_id: &str,
    message_id: &str,
    stream_start_seq: i64,
) -> serde_json::Value {
    serde_json::json!({
        "status": "accepted",
        "session_id": session_id,
        "agent_id": agent_id,
        "message_id": message_id,
        "stream_start_seq": stream_start_seq,
    })
}

fn agent_stream_execution_state(
    session_id: &str,
    agent_id: &str,
    message_id: &str,
    stream_start_seq: i64,
) -> AgentStreamExecutionState {
    AgentStreamExecutionState {
        version: AGENT_STREAM_EXECUTION_STATE_VERSION,
        session_id: session_id.to_string(),
        agent_id: agent_id.to_string(),
        message_id: message_id.to_string(),
        stream_start_seq,
        recovery_required: false,
        terminal_event_type: None,
        terminal_payload: None,
    }
}

fn parse_agent_stream_execution_state(
    record: &cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
) -> Result<AgentStreamExecutionState, ApiError> {
    if record.state_version != AGENT_STREAM_EXECUTION_STATE_VERSION as i64 {
        return Err(ApiError::internal(format!(
            "unsupported agent stream state version {}",
            record.state_version
        )));
    }
    let state =
        serde_json::from_str::<AgentStreamExecutionState>(&record.state_json).map_err(|error| {
            ApiError::internal(format!("failed to parse agent stream state: {error}"))
        })?;
    if state.version != AGENT_STREAM_EXECUTION_STATE_VERSION {
        return Err(ApiError::internal(format!(
            "unsupported agent stream state version {}",
            state.version
        )));
    }
    Ok(state)
}

fn persist_agent_stream_execution_record(
    conn: &rusqlite::Connection,
    execution_id: &str,
    journal_id: &str,
    operation_id: &str,
    actor: &str,
    state: &AgentStreamExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::insert(
        conn,
        &cortex_storage::queries::live_execution_queries::NewLiveExecutionRecord {
            id: execution_id,
            journal_id,
            operation_id,
            route_kind: AGENT_CHAT_STREAM_ROUTE_KIND,
            actor_key: actor,
            state_version: AGENT_STREAM_EXECUTION_STATE_VERSION as i64,
            status: "accepted",
            state_json: &state_json,
        },
    )
    .map_err(|error| ApiError::db_error("insert_agent_stream_execution_record", error))
}

fn update_agent_stream_execution_state(
    conn: &rusqlite::Connection,
    execution_id: &str,
    status: &str,
    state: &AgentStreamExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    cortex_storage::queries::live_execution_queries::update_status_and_state(
        conn,
        execution_id,
        AGENT_STREAM_EXECUTION_STATE_VERSION as i64,
        status,
        &state_json,
    )
    .map_err(|error| ApiError::db_error("update_agent_stream_execution_record", error))
}

fn parse_agent_stream_acceptance(
    body: &serde_json::Value,
) -> Result<AgentStreamAcceptance, ApiError> {
    Ok(AgentStreamAcceptance {
        session_id: body
            .get("session_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("missing accepted session_id"))?
            .to_string(),
        message_id: body
            .get("message_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("missing accepted message_id"))?
            .to_string(),
        stream_start_seq: body
            .get("stream_start_seq")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| ApiError::internal("missing accepted stream_start_seq"))?,
    })
}

fn agent_stream_keep_alive() -> axum::response::sse::KeepAlive {
    axum::response::sse::KeepAlive::new()
        .interval(std::time::Duration::from_secs(15))
        .text("ping")
}

fn agent_stream_start_event(acceptance: &AgentStreamAcceptance) -> Event {
    Event::default()
        .event("stream_start")
        .id(acceptance.stream_start_seq.to_string())
        .data(
            serde_json::json!({
                "session_id": acceptance.session_id.clone(),
                "message_id": acceptance.message_id.clone(),
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

fn agent_stream_recovery_payload(message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "message": message.into(),
        "recovery_required": true,
    })
}

fn mark_reconstructed_payload(mut payload: serde_json::Value) -> serde_json::Value {
    if let Some(object) = payload.as_object_mut() {
        object.insert("reconstructed".into(), serde_json::Value::Bool(true));
    }
    payload
}

fn agent_stream_error_payload(
    message: &str,
    error_type: Option<AgentStreamErrorType>,
    provider: Option<&str>,
    fallback: bool,
    terminal: bool,
    recovery_required: bool,
    reconstructed: bool,
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
    if recovery_required {
        payload["recovery_required"] = serde_json::json!(true);
    }
    if reconstructed {
        payload = mark_reconstructed_payload(payload);
    }
    payload
}

fn load_runtime_session_chain_state(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<(i64, [u8; 32]), ApiError> {
    let latest = conn
        .query_row(
            "SELECT sequence_number, event_hash
             FROM itp_events
             WHERE session_id = ?1
             ORDER BY sequence_number DESC
             LIMIT 1",
            rusqlite::params![session_id],
            |row| {
                let sequence_number = row.get::<_, i64>(0)?;
                let event_hash = row.get::<_, Vec<u8>>(1)?;
                Ok((sequence_number, event_hash))
            },
        )
        .optional()
        .map_err(|error| ApiError::db_error("load_runtime_session_chain_state", error))?;

    if let Some((sequence_number, event_hash)) = latest {
        let mut previous_hash = [0u8; 32];
        if event_hash.len() == previous_hash.len() {
            previous_hash.copy_from_slice(&event_hash);
        }
        Ok((sequence_number, previous_hash))
    } else {
        Ok((0, [0u8; 32]))
    }
}

fn compute_runtime_session_event_hash(
    content_hash_hex: &str,
    previous_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(content_hash_hex.as_bytes());
    hasher.update(previous_hash);
    *hasher.finalize().as_bytes()
}

fn persist_runtime_session_event(
    conn: &rusqlite::Connection,
    session_id: &str,
    sender: &str,
    event_type: &str,
    attributes: &serde_json::Value,
    token_count: Option<i64>,
) -> Result<i64, ApiError> {
    let (last_sequence_number, previous_hash) = load_runtime_session_chain_state(conn, session_id)?;
    let sequence_number = last_sequence_number + 1;
    let attributes_json =
        serde_json::to_string(attributes).map_err(|error| ApiError::internal(error.to_string()))?;
    let content_hash = blake3::hash(attributes_json.as_bytes())
        .to_hex()
        .to_string();
    let event_hash = compute_runtime_session_event_hash(&content_hash, &previous_hash);

    conn.execute(
        "INSERT INTO itp_events (
             id,
             session_id,
             event_type,
             sender,
             timestamp,
             sequence_number,
             content_hash,
             content_length,
             privacy_level,
             latency_ms,
             token_count,
             event_hash,
             previous_hash,
             attributes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            Uuid::now_v7().to_string(),
            session_id,
            event_type,
            sender,
            chrono::Utc::now().to_rfc3339(),
            sequence_number,
            content_hash,
            attributes_json.len() as i64,
            "standard",
            Option::<i64>::None,
            token_count,
            event_hash.to_vec(),
            previous_hash.to_vec(),
            attributes_json,
        ],
    )
    .map_err(|error| ApiError::db_error("persist_runtime_session_event", error))?;

    Ok(sequence_number)
}

fn persist_blocking_runtime_session_turn(
    conn: &rusqlite::Connection,
    session_id: &str,
    agent_id: &str,
    user_message: &str,
    result: &ghost_agent_loop::runner::RunResult,
) -> Result<i64, ApiError> {
    persist_runtime_session_event(
        conn,
        session_id,
        agent_id,
        "turn_complete",
        &serde_json::json!({
            "route": "agent_chat",
            "message": user_message,
            "content": result.output.clone().unwrap_or_default(),
            "tool_calls_made": result.tool_calls_made,
            "total_tokens": result.total_tokens,
            "total_cost": result.total_cost,
        }),
        Some(result.total_tokens as i64),
    )
}

async fn persist_stream_event_durable(
    db: &Arc<crate::db_pool::DbPool>,
    session_id: &str,
    message_id: &str,
    agent_id: &str,
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
    .map_err(|error| ApiError::db_error("persist_stream_event", error))?;

    persist_runtime_session_event(
        &conn,
        session_id,
        agent_id,
        event_type,
        payload,
        match event_type {
            "turn_complete" => payload.get("token_count").and_then(|value| value.as_i64()),
            _ => None,
        },
    )
}

async fn persist_agent_stream_terminal_state(
    db: &Arc<crate::db_pool::DbPool>,
    execution_id: &str,
    status: &str,
    execution_state: &AgentStreamExecutionState,
) {
    let conn = db.write().await;
    let _ = update_agent_stream_execution_state(&conn, execution_id, status, execution_state);
}

fn agent_replay_stream_response(
    acceptance: AgentStreamAcceptance,
    persisted_events: Vec<cortex_storage::queries::stream_event_queries::StreamEventRow>,
    execution_state: Option<AgentStreamExecutionState>,
    idempotency_status: IdempotencyStatus,
) -> Response {
    let replay_missing_start = persisted_events
        .first()
        .map(|row| row.event_type.as_str() != "stream_start")
        .unwrap_or(true);
    let replay_missing_terminal = !persisted_events
        .iter()
        .any(|row| matches!(row.event_type.as_str(), "turn_complete" | "error"));
    let sse_stream = async_stream::stream! {
        if replay_missing_start {
            yield Ok::<Event, Infallible>(agent_stream_start_event(&acceptance));
        }
        for event in persisted_events {
            if let Some(event) = replay_stream_event(event) {
                yield Ok(event);
            }
        }
        if replay_missing_terminal {
            if let Some(execution_state) = execution_state {
                if execution_state.recovery_required {
                    let payload = execution_state
                        .terminal_payload
                        .unwrap_or_else(|| agent_stream_recovery_payload(
                            "Stream replay requires recovery because durable persistence did not complete",
                        ));
                    yield Ok(Event::default().event("error").data(mark_reconstructed_payload(payload).to_string()));
                }
            }
        }
    };

    response_with_idempotency(
        Sse::new(sse_stream)
            .keep_alive(agent_stream_keep_alive())
            .into_response(),
        idempotency_status,
    )
}

fn agent_live_stream_response(
    state: Arc<AppState>,
    acceptance: AgentStreamAcceptance,
    execution_id: String,
    mut execution_state: AgentStreamExecutionState,
    mut rx: tokio::sync::mpsc::Receiver<AgentStreamEvent>,
    idempotency_status: IdempotencyStatus,
) -> Response {
    let session_id_sse = acceptance.session_id.clone();
    let message_id_sse = acceptance.message_id.clone();
    let db_for_stream = Arc::clone(&state.db);
    let state_for_stream = Arc::clone(&state);
    let start_event = agent_stream_start_event(&acceptance);

    let sse_stream = async_stream::stream! {
        {
            let conn = db_for_stream.write().await;
            if let Err(error) = update_agent_stream_execution_state(&conn, &execution_id, "running", &execution_state) {
                let payload = agent_stream_recovery_payload(format!(
                    "Failed to mark stream execution as running: {error}"
                ));
                execution_state.recovery_required = true;
                execution_state.terminal_event_type = Some("error".to_string());
                execution_state.terminal_payload = Some(payload.clone());
                drop(conn);
                persist_agent_stream_terminal_state(
                    &db_for_stream,
                    &execution_id,
                    "recovery_required",
                    &execution_state,
                ).await;
                yield Ok::<Event, Infallible>(Event::default().event("error").data(payload.to_string()));
                return;
            }
        }

        yield Ok::<Event, Infallible>(start_event);

        let mut stream_ended = false;
        while let Some(event) = rx.recv().await {
            match event {
                AgentStreamEvent::StreamStart { .. } => {}
                AgentStreamEvent::TextDelta { content } => {
                    let payload = serde_json::json!({ "content": content });
                    let seq = match persist_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &message_id_sse,
                        &execution_state.agent_id,
                        "text_chunk",
                        &payload,
                    ).await {
                        Ok(seq) => seq,
                        Err(error) => {
                            let recovery_payload = agent_stream_recovery_payload(format!(
                                "Failed to persist text chunk: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(recovery_payload.clone());
                            persist_agent_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                "recovery_required",
                                &execution_state,
                            ).await;
                            yield Ok(Event::default().event("error").data(recovery_payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };
                    let ev = Event::default()
                        .event("text_delta")
                        .id(seq.to_string())
                        .data(payload.to_string());
                    yield Ok(ev);
                }
                AgentStreamEvent::ToolUse { tool, tool_id, status } => {
                    let payload = serde_json::json!({
                        "tool": tool,
                        "tool_id": tool_id,
                        "status": status,
                    });
                    let seq = match persist_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &message_id_sse,
                        &execution_state.agent_id,
                        "tool_use",
                        &payload,
                    ).await {
                        Ok(seq) => seq,
                        Err(error) => {
                            let recovery_payload = agent_stream_recovery_payload(format!(
                                "Failed to persist tool_use event: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(recovery_payload.clone());
                            persist_agent_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                "recovery_required",
                                &execution_state,
                            ).await;
                            yield Ok(Event::default().event("error").data(recovery_payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };

                    crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                        session_id: session_id_sse.clone(),
                        event_id: tool_id.clone(),
                        event_type: format!("tool_use:{}", tool),
                        sender: Some(tool.clone()),
                        sequence_number: seq,
                    });

                    let ev = Event::default().event("tool_use").id(seq.to_string()).data(payload.to_string());
                    yield Ok(ev);
                }
                AgentStreamEvent::ToolResult { tool, tool_id, status, preview } => {
                    let payload = serde_json::json!({
                        "tool": tool,
                        "tool_id": tool_id,
                        "status": status,
                        "preview": preview,
                    });
                    let seq = match persist_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &message_id_sse,
                        &execution_state.agent_id,
                        "tool_result",
                        &payload,
                    ).await {
                        Ok(seq) => seq,
                        Err(error) => {
                            let recovery_payload = agent_stream_recovery_payload(format!(
                                "Failed to persist tool_result event: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(recovery_payload.clone());
                            persist_agent_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                "recovery_required",
                                &execution_state,
                            ).await;
                            yield Ok(Event::default().event("error").data(recovery_payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };

                    crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                        session_id: session_id_sse.clone(),
                        event_id: tool_id.clone(),
                        event_type: format!("tool_result:{}", tool),
                        sender: Some(tool.clone()),
                        sequence_number: seq,
                    });

                    let ev = Event::default().event("tool_result").id(seq.to_string()).data(payload.to_string());
                    yield Ok(ev);
                }
                AgentStreamEvent::Heartbeat { phase } => {
                    yield Ok(Event::default()
                        .event("heartbeat")
                        .data(serde_json::json!({ "phase": phase }).to_string()));
                }
                AgentStreamEvent::TurnComplete { token_count, safety_status } => {
                    let payload = serde_json::json!({
                        "message_id": message_id_sse,
                        "session_id": session_id_sse,
                        "token_count": token_count,
                        "safety_status": safety_status,
                    });
                    let seq = match persist_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &message_id_sse,
                        &execution_state.agent_id,
                        "turn_complete",
                        &payload,
                    ).await {
                        Ok(seq) => seq,
                        Err(error) => {
                            let recovery_payload = agent_stream_recovery_payload(format!(
                                "Failed to persist terminal stream event: {error}"
                            ));
                            execution_state.recovery_required = true;
                            execution_state.terminal_event_type = Some("error".to_string());
                            execution_state.terminal_payload = Some(recovery_payload.clone());
                            persist_agent_stream_terminal_state(
                                &db_for_stream,
                                &execution_id,
                                "recovery_required",
                                &execution_state,
                            ).await;
                            yield Ok(Event::default().event("error").data(recovery_payload.to_string()));
                            stream_ended = true;
                            break;
                        }
                    };
                    execution_state.recovery_required = false;
                    execution_state.terminal_event_type = Some("stream_end".to_string());
                    execution_state.terminal_payload = Some(payload.clone());
                    persist_agent_stream_terminal_state(
                        &db_for_stream,
                        &execution_id,
                        "completed",
                        &execution_state,
                    ).await;
                    let ev = Event::default().event("stream_end").id(seq.to_string()).data(payload.to_string());
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
                } => {
                    let payload = agent_stream_error_payload(
                        &message,
                        error_type,
                        provider.as_deref(),
                        fallback,
                        terminal,
                        terminal,
                        false,
                    );

                    if !terminal {
                        yield Ok(Event::default().event("error").data(payload.to_string()));
                        continue;
                    }

                    let seq = persist_stream_event_durable(
                        &db_for_stream,
                        &session_id_sse,
                        &message_id_sse,
                        &execution_state.agent_id,
                        "error",
                        &payload,
                    ).await.ok();
                    execution_state.recovery_required = true;
                    execution_state.terminal_event_type = Some("error".to_string());
                    execution_state.terminal_payload = Some(payload.clone());
                    persist_agent_stream_terminal_state(
                        &db_for_stream,
                        &execution_id,
                        "recovery_required",
                        &execution_state,
                    ).await;
                    let mut ev = Event::default().event("error").data(payload.to_string());
                    if let Some(seq) = seq {
                        ev = ev.id(seq.to_string());
                    }
                    yield Ok(ev);
                    stream_ended = true;
                    break;
                }
            }
        }

        if !stream_ended {
            let payload = agent_stream_recovery_payload(
                "Stream ended before a durable terminal event was persisted",
            );
            execution_state.recovery_required = true;
            execution_state.terminal_event_type = Some("error".to_string());
            execution_state.terminal_payload = Some(payload.clone());
            persist_agent_stream_terminal_state(
                &db_for_stream,
                &execution_id,
                "recovery_required",
                &execution_state,
            ).await;
            yield Ok(Event::default().event("error").data(payload.to_string()));
        }
    };

    response_with_idempotency(
        Sse::new(sse_stream)
            .keep_alive(agent_stream_keep_alive())
            .into_response(),
        idempotency_status,
    )
}

fn spawn_agent_chat_stream_execution(
    state: Arc<AppState>,
    requested_agent_id: Option<String>,
    session_id: String,
    message_id: String,
    user_message: String,
) -> (
    tokio::sync::mpsc::Receiver<AgentStreamEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel::<AgentStreamEvent>(256);
    let state_for_task = Arc::clone(&state);
    let handle = tokio::spawn(async move {
        let runtime_session_id = Uuid::parse_str(&session_id).unwrap_or_else(|_| Uuid::now_v7());
        let prepared_runtime = match prepare_requested_runtime_execution(
            &state_for_task,
            requested_agent_id.as_deref(),
            API_SYNTHETIC_AGENT_NAME,
            runtime_session_id,
            RunnerBuildOptions::default(),
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
            providers,
            ..
        } = prepared_runtime;

        if let Some(run_result) = execute_streaming_turn(
            &tx,
            &mut runner,
            &runtime_ctx,
            "api",
            &user_message,
            &providers,
            "No model providers configured. Add provider config to ghost.yml to enable agent chat.",
            &format!("Agent turn timed out after 5 minutes for message {message_id}"),
        )
        .await
        {
            let output_inspection = inspect_text_safety(
                run_result.output.as_deref().unwrap_or_default(),
                runtime_ctx.agent.id,
            );
            let safety_status = inspection_safety_status(&output_inspection);
            let _ = tx
                .send(AgentStreamEvent::TurnComplete {
                    token_count: run_result.total_tokens,
                    safety_status: safety_status.to_string(),
                })
                .await;
        }
    });

    (rx, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[test]
    fn agent_chat_stream_uses_real_output_inspection_for_terminal_safety() {
        let inspection = inspect_text_safety(
            "leaked credential sk-proj-1234567890abcdefghijklmn",
            Uuid::nil(),
        );

        assert_eq!(inspection_safety_status(&inspection), "warning");
    }

    #[tokio::test]
    async fn replayed_agent_stream_terminal_errors_are_marked_reconstructed() {
        let acceptance = AgentStreamAcceptance {
            session_id: "session-1".to_string(),
            message_id: "message-1".to_string(),
            stream_start_seq: 7,
        };
        let execution_state = AgentStreamExecutionState {
            version: AGENT_STREAM_EXECUTION_STATE_VERSION,
            session_id: "session-1".to_string(),
            agent_id: "agent-1".to_string(),
            message_id: "message-1".to_string(),
            stream_start_seq: 7,
            recovery_required: true,
            terminal_event_type: Some("error".to_string()),
            terminal_payload: Some(serde_json::json!({
                "message": "Recovered from execution state",
                "recovery_required": true,
            })),
        };

        let response = agent_replay_stream_response(
            acceptance,
            Vec::new(),
            Some(execution_state),
            IdempotencyStatus::Replayed,
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();

        assert!(body.contains("event: stream_start"));
        assert!(body.contains("event: error"));
        assert!(body.contains("\"message\":\"Recovered from execution state\""));
        assert!(body.contains("\"reconstructed\":true"));
    }
}
