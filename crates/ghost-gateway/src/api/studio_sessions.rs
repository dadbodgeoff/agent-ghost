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
use axum::response::sse::{Event, Sse};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use ghost_agent_loop::output_inspector::{InspectionResult, OutputInspector};
use ghost_agent_loop::runner::AgentStreamEvent;

use crate::api::error::{ApiError, ApiResult};
use crate::api::websocket::WsEvent;
use crate::state::AppState;

// ── Request / Response types ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: String,
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

/// POST /api/studio/sessions — create a new chat session.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> ApiResult<SessionResponse> {
    let id = Uuid::now_v7().to_string();
    let title = req.title.unwrap_or_else(|| "New Chat".into());
    let model = req.model.unwrap_or_else(|| "qwen3.5:9b".into());
    let system_prompt = req.system_prompt.unwrap_or_default();
    let temperature = req.temperature.unwrap_or(0.5);
    let max_tokens = req.max_tokens.unwrap_or(4096);

    {
        let db = state.db.write().await;
        cortex_storage::queries::studio_chat_queries::create_session(
            &db, &id, &title, &model, &system_prompt, temperature, max_tokens,
        )
        .map_err(|e| ApiError::db_error("create_session", e))?;
    }

    Ok(Json(SessionResponse {
        id,
        title,
        model,
        system_prompt,
        temperature,
        max_tokens,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }))
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
        let db = state.db.read().map_err(|e| ApiError::db_error("list_sessions", e))?;
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
    let db = state.db.read().map_err(|e| ApiError::db_error("get_session", e))?;

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
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let deleted = {
        let db = state.db.write().await;
        cortex_storage::queries::studio_chat_queries::delete_session(&db, &id)
            .map_err(|e| ApiError::db_error("delete_session", e))?
    };

    if !deleted {
        return Err(ApiError::not_found(format!("session {id} not found")));
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
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
    let db = state.db.read().map_err(|e| ApiError::db_error("recover_stream", e))?;

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
    Path(session_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> ApiResult<SendMessageResponse> {
    if req.content.trim().is_empty() {
        return Err(ApiError::bad_request("message content must not be empty"));
    }

    // 0. Verify session exists and load settings.
    let session = {
        let db = state.db.read().map_err(|e| ApiError::db_error("get_session", e))?;
        cortex_storage::queries::studio_chat_queries::get_session(&db, &session_id)
            .map_err(|e| ApiError::db_error("get_session", e))?
            .ok_or_else(|| ApiError::not_found(format!("session {session_id} not found")))?
    };

    let agent_id = Uuid::now_v7();
    let runner_session_id = Uuid::now_v7();
    let user_msg_id = Uuid::now_v7().to_string();

    // 2. Run OutputInspector on user input (before DB write to avoid orphans).
    let inspector = OutputInspector::new();
    let input_inspection = inspector.scan(&req.content, agent_id);
    let user_safety_status = match &input_inspection {
        InspectionResult::Clean => "clean",
        InspectionResult::Warning { .. } => "warning",
        InspectionResult::KillAll { .. } => "blocked",
    };

    // WP2-B: Insert user message + safety audit in a single transaction.
    {
        let db = state.db.write().await;
        db.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| ApiError::db_error("begin_transaction", e))?;

        let audit_id = Uuid::now_v7().to_string();
        let detail = match &input_inspection {
            InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::Clean => None,
        };

        let result = cortex_storage::queries::studio_chat_queries::insert_message(
            &db, &user_msg_id, &session_id, "user", &req.content, 0, "clean",
        )
        .and_then(|_| {
            cortex_storage::queries::studio_chat_queries::insert_safety_audit(
                &db, &audit_id, &session_id, &user_msg_id, "input_scan", user_safety_status, detail,
            )
        });

        match result {
            Ok(_) => {
                db.execute_batch("COMMIT")
                    .map_err(|e| ApiError::db_error("commit_transaction", e))?;
            }
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK");
                return Err(ApiError::db_error("insert_user_message_transaction", e));
            }
        }
    }

    // Block if input contains known credentials.
    if matches!(input_inspection, InspectionResult::KillAll { .. }) {
        return Err(ApiError::bad_request(
            "Message blocked: credential pattern detected in input",
        ));
    }

    // 3. Load session messages as conversation history for multi-turn.
    let history = {
        let db = state.db.read().map_err(|e| ApiError::db_error("list_messages", e))?;
        cortex_storage::queries::studio_chat_queries::list_messages(&db, &session_id)
            .map_err(|e| ApiError::db_error("list_messages", e))?
    };

    if state.model_providers.is_empty() {
        return Err(ApiError::bad_request(
            "No model providers configured. Add provider config to ghost.yml.",
        ));
    }

    // 4. Build AgentRunner with full tool/skill wiring (mirrors agent_chat.rs).
    let mut runner = ghost_agent_loop::runner::AgentRunner::new(128_000);
    ghost_agent_loop::tools::executor::register_builtin_tools(&mut runner.tool_registry);

    // Wire DB (legacy Mutex<Connection> for agent loop).
    runner.db = state.db.legacy_connection().ok();

    // Configure filesystem tool with cwd.
    if let Ok(cwd) = std::env::current_dir() {
        runner.tool_executor.set_workspace_root(cwd);
    }

    // Apply tool configurations from ghost.yml.
    crate::api::apply_tool_configs(&mut runner.tool_executor, &state.tools_config);

    // Load SOUL.md (L2) — or use session's custom system prompt.
    if !session.system_prompt.is_empty() {
        runner.soul_identity = session.system_prompt.clone();
    } else {
        let soul_path = crate::bootstrap::ghost_home().join("config").join("SOUL.md");
        if let Ok(content) = std::fs::read_to_string(&soul_path) {
            if !content.is_empty() {
                runner.soul_identity = content;
            }
        }
    }

    // Build environment context (L4).
    runner.environment = ghost_agent_loop::context::environment::build_environment_context(
        std::env::current_dir().ok().as_deref(),
    );

    // Wire skills via SkillBridge (legacy Mutex<Connection> for skill bridge).
    let legacy_db = state.db.legacy_connection()
        .map_err(|e| crate::api::error::ApiError::internal(format!("db pool: {e}")))?;
    let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
        Arc::clone(&state.safety_skills),
        legacy_db,
        state.convergence_profile.clone(),
    );
    ghost_agent_loop::tools::skill_bridge::register_skills(&bridge, &mut runner.tool_registry, None);
    runner.tool_executor.set_skill_bridge(bridge);

    // 5. Inject conversation history for multi-turn.
    // Exclude the just-inserted user message (last in history) — it's sent as the user_message param.
    let history_cutoff: Vec<_> = if !history.is_empty() && history.last().map(|m| m.id.as_str()) == Some(&user_msg_id) {
        history[..history.len() - 1].to_vec()
    } else {
        history
    };

    for msg in &history_cutoff {
        let role = match msg.role.as_str() {
            "user" => ghost_llm::provider::MessageRole::User,
            "assistant" => ghost_llm::provider::MessageRole::Assistant,
            "system" => ghost_llm::provider::MessageRole::System,
            _ => ghost_llm::provider::MessageRole::User,
        };
        runner.conversation_history.push(ghost_llm::provider::ChatMessage {
            role,
            content: msg.content.clone(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    // 6. Build LLM fallback chain and run the agent turn.
    let mut fallback_chain = super::agent_chat::build_fallback_chain_from_providers(&state.model_providers);

    let mut ctx = runner
        .pre_loop(agent_id, runner_session_id, "studio", &req.content)
        .await
        .map_err(|e| ApiError::internal(format!("agent pre-loop failed: {e}")))?;

    let result = runner
        .run_turn(&mut ctx, &mut fallback_chain, &req.content)
        .await
        .map_err(|e| ApiError::internal(format!("agent run failed: {e}")))?;

    let response_content = result.output.unwrap_or_default();
    let token_count = result.total_tokens as i64;

    // 7. Determine output safety status.
    // The AgentRunner's OutputInspector already scanned the output.
    // We do a secondary scan here for audit logging.
    let output_inspection = inspector.scan(&response_content, agent_id);
    let output_safety_status = match &output_inspection {
        InspectionResult::Clean => "clean",
        InspectionResult::Warning { .. } => "warning",
        InspectionResult::KillAll { .. } => "blocked",
    };

    let overall_safety = match (user_safety_status, output_safety_status) {
        (_, "blocked") | ("blocked", _) => "blocked",
        (_, "warning") | ("warning", _) => "warning",
        _ => "clean",
    };

    // 8. Insert assistant message into DB.
    let assistant_msg_id = Uuid::now_v7().to_string();
    {
        let db = state.db.write().await;
        cortex_storage::queries::studio_chat_queries::insert_message(
            &db,
            &assistant_msg_id,
            &session_id,
            "assistant",
            &response_content,
            token_count,
            output_safety_status,
        )
        .map_err(|e| ApiError::db_error("insert_assistant_message", e))?;

        // Log safety audit for output.
        let audit_id = Uuid::now_v7().to_string();
        let detail = match &output_inspection {
            InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::Clean => None,
        };
        cortex_storage::queries::studio_chat_queries::insert_safety_audit(
            &db,
            &audit_id,
            &session_id,
            &assistant_msg_id,
            "output_scan",
            output_safety_status,
            detail,
        )
        .map_err(|e| ApiError::db_error("insert_safety_audit", e))?;
    }

    // 9. Auto-title from first user message.
    if session.title == "New Chat" {
        let title = truncate_for_title(&req.content);
        let db = state.db.write().await;
        let _ = cortex_storage::queries::studio_chat_queries::update_session_title(
            &db, &session_id, &title,
        );
    }

    // 10. Broadcast WsEvent.
    crate::api::websocket::broadcast_event(&state, WsEvent::ChatMessage {
        session_id: session_id.clone(),
        message_id: assistant_msg_id.clone(),
        role: "assistant".into(),
        content: truncate_preview(&response_content, 200),
        safety_status: output_safety_status.into(),
    });

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    Ok(Json(SendMessageResponse {
        user_message: MessageResponse {
            id: user_msg_id,
            role: "user".into(),
            content: req.content,
            token_count: 0,
            safety_status: user_safety_status.into(),
            created_at: now.clone(),
        },
        assistant_message: MessageResponse {
            id: assistant_msg_id,
            role: "assistant".into(),
            content: response_content,
            token_count,
            safety_status: output_safety_status.into(),
            created_at: now,
        },
        safety_status: overall_safety.into(),
    }))
}

/// POST /api/studio/sessions/:id/messages/stream — send a message with SSE streaming.
///
/// Same pipeline as `send_message` but returns an SSE stream that yields
/// `text_delta`, `tool_use`, `tool_result`, and `stream_end` events as
/// the agent generates its response.
pub async fn send_message_stream(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    if req.content.trim().is_empty() {
        return Err(ApiError::bad_request("message content must not be empty"));
    }

    // 0. Verify session exists and load settings.
    let session = {
        let db = state.db.read().map_err(|e| ApiError::db_error("get_session", e))?;
        cortex_storage::queries::studio_chat_queries::get_session(&db, &session_id)
            .map_err(|e| ApiError::db_error("get_session", e))?
            .ok_or_else(|| ApiError::not_found(format!("session {session_id} not found")))?
    };

    let agent_id = Uuid::now_v7();
    let runner_session_id = Uuid::now_v7();
    let user_msg_id = Uuid::now_v7().to_string();
    let assistant_msg_id = Uuid::now_v7().to_string();

    // 2. Run OutputInspector on user input (before DB write to avoid orphans).
    let inspector = OutputInspector::new();
    let input_inspection = inspector.scan(&req.content, agent_id);
    let user_safety_status = match &input_inspection {
        InspectionResult::Clean => "clean",
        InspectionResult::Warning { .. } => "warning",
        InspectionResult::KillAll { .. } => "blocked",
    };

    // WP2-B: Insert user message + safety audit in a single transaction.
    {
        let db = state.db.write().await;
        db.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| ApiError::db_error("begin_transaction", e))?;

        let audit_id = Uuid::now_v7().to_string();
        let detail = match &input_inspection {
            InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::Clean => None,
        };

        let result = cortex_storage::queries::studio_chat_queries::insert_message(
            &db, &user_msg_id, &session_id, "user", &req.content, 0, "clean",
        )
        .and_then(|_| {
            cortex_storage::queries::studio_chat_queries::insert_safety_audit(
                &db, &audit_id, &session_id, &user_msg_id, "input_scan", user_safety_status, detail,
            )
        });

        match result {
            Ok(_) => {
                db.execute_batch("COMMIT")
                    .map_err(|e| ApiError::db_error("commit_transaction", e))?;
            }
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK");
                return Err(ApiError::db_error("insert_user_message_transaction", e));
            }
        }
    }

    // Block if input contains known credentials.
    if matches!(input_inspection, InspectionResult::KillAll { .. }) {
        return Err(ApiError::bad_request(
            "Message blocked: credential pattern detected in input",
        ));
    }

    // 3. Load session messages as conversation history for multi-turn.
    let history = {
        let db = state.db.read().map_err(|e| ApiError::db_error("list_messages", e))?;
        cortex_storage::queries::studio_chat_queries::list_messages(&db, &session_id)
            .map_err(|e| ApiError::db_error("list_messages", e))?
    };

    if state.model_providers.is_empty() {
        return Err(ApiError::bad_request(
            "No model providers configured. Add provider config to ghost.yml.",
        ));
    }

    // 4. Build AgentRunner with full tool/skill wiring.
    let mut runner = ghost_agent_loop::runner::AgentRunner::new(128_000);
    ghost_agent_loop::tools::executor::register_builtin_tools(&mut runner.tool_registry);

    runner.db = state.db.legacy_connection().ok();

    if let Ok(cwd) = std::env::current_dir() {
        runner.tool_executor.set_workspace_root(cwd);
    }

    // Apply tool configurations from ghost.yml.
    crate::api::apply_tool_configs(&mut runner.tool_executor, &state.tools_config);

    if !session.system_prompt.is_empty() {
        runner.soul_identity = session.system_prompt.clone();
    } else {
        let soul_path = crate::bootstrap::ghost_home().join("config").join("SOUL.md");
        if let Ok(content) = std::fs::read_to_string(&soul_path) {
            if !content.is_empty() {
                runner.soul_identity = content;
            }
        }
    }

    runner.environment = ghost_agent_loop::context::environment::build_environment_context(
        std::env::current_dir().ok().as_deref(),
    );

    let legacy_db = state.db.legacy_connection()
        .map_err(|e| crate::api::error::ApiError::internal(format!("db pool: {e}")))?;
    let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
        Arc::clone(&state.safety_skills),
        legacy_db,
        state.convergence_profile.clone(),
    );
    ghost_agent_loop::tools::skill_bridge::register_skills(&bridge, &mut runner.tool_registry, None);
    runner.tool_executor.set_skill_bridge(bridge);

    // 5. Inject conversation history for multi-turn.
    let history_cutoff: Vec<_> = if !history.is_empty() && history.last().map(|m| m.id.as_str()) == Some(&user_msg_id) {
        history[..history.len() - 1].to_vec()
    } else {
        history
    };

    for msg in &history_cutoff {
        let role = match msg.role.as_str() {
            "user" => ghost_llm::provider::MessageRole::User,
            "assistant" => ghost_llm::provider::MessageRole::Assistant,
            "system" => ghost_llm::provider::MessageRole::System,
            _ => ghost_llm::provider::MessageRole::User,
        };
        runner.conversation_history.push(ghost_llm::provider::ChatMessage {
            role,
            content: msg.content.clone(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    // 6. Collect all configured providers for fallback (WP2-A).
    let all_providers = state.model_providers.clone();

    // 7. Create channel for streaming events.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentStreamEvent>(256);

    let user_content = req.content.clone();
    let session_id_clone = session_id.clone();
    let assistant_msg_id_clone = assistant_msg_id.clone();
    let session_title = session.title.clone();
    let db_clone = Arc::clone(&state.db);
    let state_clone = Arc::clone(&state);

    // 8. Spawn agent run as background task with outer timeout.
    tokio::spawn(async move {
        let tx_timeout = tx.clone();
        let turn_result = tokio::time::timeout(std::time::Duration::from_secs(300), async move {
        let mut ctx = match runner
            .pre_loop(agent_id, runner_session_id, "studio", &user_content)
            .await
        {
            Ok(ctx) => ctx,
            Err(e) => {
                let _ = tx.send(AgentStreamEvent::Error {
                    message: format!("agent pre-loop failed: {e}"),
                }).await;
                return;
            }
        };

        // WP2-A: Provider fallback — try each provider until one succeeds.
        // If a provider fails before producing a first token, try the next.
        let mut _last_error: Option<String> = None;
        let mut result = Err(ghost_agent_loop::runner::RunError::LLMError(
            "no providers configured".into(),
        ));

        for (provider_idx, provider_config) in all_providers.iter().enumerate() {
            let pc = provider_config.clone();
            let get_stream = move |messages: Vec<ghost_llm::provider::ChatMessage>, tools: Vec<ghost_llm::provider::ToolSchema>| -> ghost_llm::streaming::StreamChunkStream {
                build_provider_stream(&pc, messages, tools)
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
                Ok(r) => {
                    if provider_idx > 0 {
                        tracing::info!(
                            provider = %provider_config.name,
                            index = provider_idx,
                            "streaming succeeded via fallback provider"
                        );
                    }
                    result = Ok(r);
                    break;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    tracing::warn!(
                        provider = %provider_config.name,
                        index = provider_idx,
                        error = %err_str,
                        "provider failed, trying next"
                    );
                    _last_error = Some(err_str);
                    // Reset context for retry with next provider.
                    ctx.recursion_depth = 0;
                    result = Err(e);
                    continue;
                }
            }
        }

        let result = result;

        match result {
            Ok(run_result) => {
                let response_content = run_result.output.unwrap_or_default();
                let token_count = run_result.total_tokens as i64;

                // Determine output safety status.
                let inspector = OutputInspector::new();
                let output_inspection = inspector.scan(&response_content, agent_id);
                let output_safety_status = match &output_inspection {
                    InspectionResult::Clean => "clean",
                    InspectionResult::Warning { .. } => "warning",
                    InspectionResult::KillAll { .. } => "blocked",
                };

                // Insert assistant message into DB.
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

                    // Log safety audit for output.
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

                    // Auto-title from first user message.
                    if session_title == "New Chat" {
                        let title = truncate_for_title(&user_content);
                        let _ = cortex_storage::queries::studio_chat_queries::update_session_title(
                            &db, &session_id_clone, &title,
                        );
                    }
                }

                // Broadcast WsEvent.
                crate::api::websocket::broadcast_event(&state_clone, WsEvent::ChatMessage {
                    session_id: session_id_clone.clone(),
                    message_id: assistant_msg_id_clone.clone(),
                    role: "assistant".into(),
                    content: truncate_preview(&response_content, 200),
                    safety_status: output_safety_status.into(),
                });

                // Send TurnComplete event.
                let _ = tx.send(AgentStreamEvent::TurnComplete {
                    token_count: run_result.total_tokens,
                    safety_status: output_safety_status.to_string(),
                }).await;
            }
            Err(e) => {
                let _ = tx.send(AgentStreamEvent::Error {
                    message: format!("agent run failed: {e}"),
                }).await;
            }
        }
        }).await; // end timeout

        if turn_result.is_err() {
            tracing::warn!("Agent turn timed out after 5 minutes");
            let _ = tx_timeout.send(AgentStreamEvent::Error {
                message: "Agent turn timed out after 5 minutes".into(),
            }).await;
        }
    });

    // 9. Build SSE stream from mpsc receiver with event persistence.
    let session_id_sse = session_id.clone();
    let assistant_msg_id_sse = assistant_msg_id.clone();
    let db_for_stream = Arc::clone(&state.db);
    let state_for_stream = Arc::clone(&state);

    let sse_stream = async_stream::stream! {
        // Text accumulation buffer for coalescing text_delta events.
        // Text deltas are NOT individually persisted — they're coalesced into
        // text_chunk events every 2KB to reduce write amplification.
        let mut text_buffer = String::new();
        const TEXT_FLUSH_THRESHOLD: usize = 2048;

        // WP2-C: Track consecutive DB persistence failures.
        // After 3 consecutive failures, yield an SSE warning event to the client.
        let mut consecutive_persist_failures: u32 = 0;
        const PERSIST_FAILURE_WARN_THRESHOLD: u32 = 3;

        // Helper closure: persist a milestone event to stream_event_log.
        // Returns the sequence ID (row id) on success.
        let persist_event = |db: &Arc<crate::db_pool::DbPool>, sid: &str, mid: &str, etype: &str, payload: &serde_json::Value, fail_count: &mut u32| -> Option<i64> {
            match db.read() {
                Ok(conn) => {
                    match cortex_storage::queries::stream_event_queries::insert_stream_event(
                        &conn, sid, mid, etype, &payload.to_string(),
                    ) {
                        Ok(seq) => {
                            *fail_count = 0; // Reset on success.
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

        // Helper: flush accumulated text buffer to DB as a text_chunk event.
        // WP2-C: Also tracks consecutive failures via fail_count.
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

        // Persist and send stream_start event.
        let start_payload = serde_json::json!({
            "session_id": session_id_sse,
            "message_id": assistant_msg_id_sse,
        });
        let start_seq = persist_event(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, "stream_start", &start_payload, &mut consecutive_persist_failures);
        let mut start_event = Event::default()
            .event("stream_start")
            .data(start_payload.to_string());
        if let Some(seq) = start_seq {
            start_event = start_event.id(seq.to_string());
        }
        yield Ok(start_event);

        // WP9-L: Register initial heartbeat so stream starts without delay.
        state_for_stream.client_heartbeats.insert(session_id_sse.clone(), std::time::Instant::now());
        const BACKPRESSURE_STALE_SECS: u64 = 90;

        // Read events from channel until it closes.
        let mut stream_ended = false;

        while let Some(event) = rx.recv().await {
            // WP9-L: Backpressure — if client hasn't heartbeated in 90s, pause briefly.
            // Read elapsed time and drop the DashMap ref immediately (avoid holding shard lock during sleep).
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

                    // Flush to DB if buffer exceeds threshold.
                    let mut seq = None;
                    if text_buffer.len() >= TEXT_FLUSH_THRESHOLD {
                        seq = flush_text(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, &mut text_buffer, &mut consecutive_persist_failures);
                    }

                    // Always forward the individual delta to SSE for real-time rendering.
                    let mut ev = Event::default()
                        .event("text_delta")
                        .data(serde_json::json!({ "content": content }).to_string());
                    if let Some(s) = seq {
                        ev = ev.id(s.to_string());
                    }
                    yield Ok(ev);
                }
                AgentStreamEvent::ToolUse { tool, tool_id, status } => {
                    // Flush any accumulated text before milestone.
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

                    // Broadcast milestone to WebSocket for cross-client awareness.
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

                    // Broadcast milestone to WebSocket.
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
                    // Heartbeats are ephemeral — NOT persisted to DB.
                    yield Ok(Event::default()
                        .event("heartbeat")
                        .data(serde_json::json!({ "phase": phase }).to_string()));
                }
                AgentStreamEvent::TurnComplete { token_count, safety_status } => {
                    // Flush any remaining accumulated text.
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
                    // Flush any remaining text before error.
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

        // Fallback: channel closed without TurnComplete or Error
        // (task panicked, was cancelled, or timed out before sending).
        if !stream_ended {
            // Flush remaining text.
            flush_text(&db_for_stream, &session_id_sse, &assistant_msg_id_sse, &mut text_buffer, &mut consecutive_persist_failures);

            yield Ok(Event::default()
                .event("stream_end")
                .data(serde_json::json!({
                    "message_id": assistant_msg_id_sse,
                    "token_count": 0,
                    "safety_status": "unknown",
                }).to_string()));
        }

        // WP9-L: Clean up heartbeat entry for this session.
        state_for_stream.client_heartbeats.remove(&session_id_sse);
    };

    Ok(Sse::new(sse_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
}

// ── Helpers ────────────────────────────────────────────────────────

fn session_row_to_response(
    row: cortex_storage::queries::studio_chat_queries::StudioSessionRow,
) -> SessionResponse {
    SessionResponse {
        id: row.id,
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
fn build_provider_stream(
    provider_config: &crate::config::ProviderConfig,
    messages: Vec<ghost_llm::provider::ChatMessage>,
    tools: Vec<ghost_llm::provider::ToolSchema>,
) -> ghost_llm::streaming::StreamChunkStream {
    let provider: Arc<dyn ghost_llm::provider::LLMProvider> = match provider_config.name.as_str() {
        "ollama" => {
            let base_url = provider_config.base_url.clone()
                .unwrap_or_else(|| "http://localhost:11434".into());
            let model = provider_config.model.clone()
                .unwrap_or_else(|| "llama3.1".into());
            let ollama = ghost_llm::provider::OllamaProvider { model, base_url };
            return ollama.stream_chat(&messages, &tools);
        }
        "anthropic" => {
            let key_env = provider_config.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY");
            let key = crate::state::get_api_key(key_env).unwrap_or_default();
            Arc::new(ghost_llm::provider::AnthropicProvider {
                model: provider_config.model.clone().unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
                api_key: std::sync::RwLock::new(key),
            })
        }
        "openai" => {
            let key_env = provider_config.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
            let key = crate::state::get_api_key(key_env).unwrap_or_default();
            Arc::new(ghost_llm::provider::OpenAIProvider {
                model: provider_config.model.clone().unwrap_or_else(|| "gpt-4o".into()),
                api_key: std::sync::RwLock::new(key),
            })
        }
        "gemini" => {
            let key_env = provider_config.api_key_env.as_deref().unwrap_or("GEMINI_API_KEY");
            let key = crate::state::get_api_key(key_env).unwrap_or_default();
            Arc::new(ghost_llm::provider::GeminiProvider {
                model: provider_config.model.clone().unwrap_or_else(|| "gemini-2.0-flash".into()),
                api_key: std::sync::RwLock::new(key),
            })
        }
        "openai_compat" => {
            let key_env = provider_config.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
            let key = crate::state::get_api_key(key_env).unwrap_or_default();
            let base_url = provider_config.base_url.clone()
                .unwrap_or_else(|| "http://localhost:8080".into());
            let compat = ghost_llm::provider::OpenAICompatProvider {
                model: provider_config.model.clone().unwrap_or_else(|| "default".into()),
                api_key: std::sync::RwLock::new(key),
                base_url,
                context_window_size: 128_000,
            };
            return compat.stream_chat(&messages, &tools);
        }
        _ => {
            Arc::new(ghost_llm::provider::OllamaProvider {
                model: provider_config.model.clone().unwrap_or_else(|| "llama3.1".into()),
                base_url: provider_config.base_url.clone().unwrap_or_else(|| "http://localhost:11434".into()),
            })
        }
    };
    ghost_llm::provider::complete_stream_shim(provider, messages, tools)
}

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
