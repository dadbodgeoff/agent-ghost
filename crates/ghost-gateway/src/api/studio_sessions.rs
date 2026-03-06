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
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionResponse>,
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
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
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
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListSessionsQuery>,
) -> ApiResult<SessionListResponse> {
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let sessions = {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        cortex_storage::queries::studio_chat_queries::list_sessions(&db, limit, offset)
            .map_err(|e| ApiError::db_error("list_sessions", e))?
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
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

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
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        cortex_storage::queries::studio_chat_queries::delete_session(&db, &id)
            .map_err(|e| ApiError::db_error("delete_session", e))?
    };

    if !deleted {
        return Err(ApiError::not_found(format!("session {id} not found")));
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
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
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        cortex_storage::queries::studio_chat_queries::get_session(&db, &session_id)
            .map_err(|e| ApiError::db_error("get_session", e))?
            .ok_or_else(|| ApiError::not_found(format!("session {session_id} not found")))?
    };

    let agent_id = Uuid::now_v7();
    let runner_session_id = Uuid::now_v7();
    let user_msg_id = Uuid::now_v7().to_string();

    // 1. Insert user message into DB.
    {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        cortex_storage::queries::studio_chat_queries::insert_message(
            &db, &user_msg_id, &session_id, "user", &req.content, 0, "clean",
        )
        .map_err(|e| ApiError::db_error("insert_user_message", e))?;
    }

    // 2. Run OutputInspector on user input.
    let inspector = OutputInspector::new();
    let input_inspection = inspector.scan(&req.content, agent_id);
    let user_safety_status = match &input_inspection {
        InspectionResult::Clean => "clean",
        InspectionResult::Warning { .. } => "warning",
        InspectionResult::KillAll { .. } => "blocked",
    };

    // Log safety audit for user input.
    {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        let audit_id = Uuid::now_v7().to_string();
        let detail = match &input_inspection {
            InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::Clean => None,
        };
        cortex_storage::queries::studio_chat_queries::insert_safety_audit(
            &db, &audit_id, &session_id, &user_msg_id, "input_scan", user_safety_status, detail,
        )
        .map_err(|e| ApiError::db_error("insert_safety_audit", e))?;
    }

    // Block if input contains known credentials.
    if matches!(input_inspection, InspectionResult::KillAll { .. }) {
        return Err(ApiError::bad_request(
            "Message blocked: credential pattern detected in input",
        ));
    }

    // 3. Load session messages as conversation history for multi-turn.
    let history = {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
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

    // Wire DB.
    runner.db = Some(Arc::clone(&state.db));

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

    // Wire skills via SkillBridge.
    let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
        Arc::clone(&state.safety_skills),
        Arc::clone(&state.db),
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
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
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
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        let _ = cortex_storage::queries::studio_chat_queries::update_session_title(
            &db, &session_id, &title,
        );
    }

    // 10. Broadcast WsEvent.
    let _ = state.event_tx.send(WsEvent::ChatMessage {
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
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        cortex_storage::queries::studio_chat_queries::get_session(&db, &session_id)
            .map_err(|e| ApiError::db_error("get_session", e))?
            .ok_or_else(|| ApiError::not_found(format!("session {session_id} not found")))?
    };

    let agent_id = Uuid::now_v7();
    let runner_session_id = Uuid::now_v7();
    let user_msg_id = Uuid::now_v7().to_string();
    let assistant_msg_id = Uuid::now_v7().to_string();

    // 1. Insert user message into DB.
    {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        cortex_storage::queries::studio_chat_queries::insert_message(
            &db, &user_msg_id, &session_id, "user", &req.content, 0, "clean",
        )
        .map_err(|e| ApiError::db_error("insert_user_message", e))?;
    }

    // 2. Run OutputInspector on user input.
    let inspector = OutputInspector::new();
    let input_inspection = inspector.scan(&req.content, agent_id);
    let user_safety_status = match &input_inspection {
        InspectionResult::Clean => "clean",
        InspectionResult::Warning { .. } => "warning",
        InspectionResult::KillAll { .. } => "blocked",
    };

    // Log safety audit for user input.
    {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        let audit_id = Uuid::now_v7().to_string();
        let detail = match &input_inspection {
            InspectionResult::Warning { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::KillAll { pattern_name, .. } => Some(pattern_name.as_str()),
            InspectionResult::Clean => None,
        };
        cortex_storage::queries::studio_chat_queries::insert_safety_audit(
            &db, &audit_id, &session_id, &user_msg_id, "input_scan", user_safety_status, detail,
        )
        .map_err(|e| ApiError::db_error("insert_safety_audit", e))?;
    }

    // Block if input contains known credentials.
    if matches!(input_inspection, InspectionResult::KillAll { .. }) {
        return Err(ApiError::bad_request(
            "Message blocked: credential pattern detected in input",
        ));
    }

    // 3. Load session messages as conversation history for multi-turn.
    let history = {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
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

    runner.db = Some(Arc::clone(&state.db));

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

    let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
        Arc::clone(&state.safety_skills),
        Arc::clone(&state.db),
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

    // 6. Determine provider type for streaming.
    let first_provider = state.model_providers[0].clone();

    // 7. Create channel for streaming events.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentStreamEvent>(256);

    let user_content = req.content.clone();
    let session_id_clone = session_id.clone();
    let assistant_msg_id_clone = assistant_msg_id.clone();
    let session_title = session.title.clone();
    let db_clone = Arc::clone(&state.db);
    let event_tx = state.event_tx.clone();

    // 8. Spawn agent run as background task.
    tokio::spawn(async move {
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

        // Build the get_stream closure based on provider type.
        let get_stream = move |messages: Vec<ghost_llm::provider::ChatMessage>, tools: Vec<ghost_llm::provider::ToolSchema>| -> ghost_llm::streaming::StreamChunkStream {
            let provider: Arc<dyn ghost_llm::provider::LLMProvider> = match first_provider.name.as_str() {
                "ollama" => {
                    let base_url = first_provider.base_url.clone()
                        .unwrap_or_else(|| "http://localhost:11434".into());
                    let model = first_provider.model.clone()
                        .unwrap_or_else(|| "llama3.1".into());
                    // Ollama supports native streaming.
                    let ollama = ghost_llm::provider::OllamaProvider {
                        model,
                        base_url,
                    };
                    return ollama.stream_chat(&messages, &tools);
                }
                "anthropic" => {
                    let key_env = first_provider.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY");
                    let key = std::env::var(key_env).unwrap_or_default();
                    Arc::new(ghost_llm::provider::AnthropicProvider {
                        model: first_provider.model.clone().unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
                        api_key: std::sync::RwLock::new(key),
                    })
                }
                "openai" => {
                    let key_env = first_provider.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
                    let key = std::env::var(key_env).unwrap_or_default();
                    Arc::new(ghost_llm::provider::OpenAIProvider {
                        model: first_provider.model.clone().unwrap_or_else(|| "gpt-4o".into()),
                        api_key: std::sync::RwLock::new(key),
                    })
                }
                "gemini" => {
                    let key_env = first_provider.api_key_env.as_deref().unwrap_or("GEMINI_API_KEY");
                    let key = std::env::var(key_env).unwrap_or_default();
                    Arc::new(ghost_llm::provider::GeminiProvider {
                        model: first_provider.model.clone().unwrap_or_else(|| "gemini-2.0-flash".into()),
                        api_key: std::sync::RwLock::new(key),
                    })
                }
                "openai_compat" => {
                    let key_env = first_provider.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
                    let key = std::env::var(key_env).unwrap_or_default();
                    let base_url = first_provider.base_url.clone()
                        .unwrap_or_else(|| "http://localhost:8080".into());
                    // Use real SSE streaming for OpenAI-compatible providers.
                    // This is critical for reasoning models (Grok, etc.) where
                    // non-streaming calls block for 30-60s with zero feedback.
                    let compat = ghost_llm::provider::OpenAICompatProvider {
                        model: first_provider.model.clone().unwrap_or_else(|| "default".into()),
                        api_key: std::sync::RwLock::new(key),
                        base_url,
                        context_window_size: 128_000,
                    };
                    return compat.stream_chat(&messages, &tools);
                }
                _ => {
                    // Fallback to Ollama for unknown providers.
                    Arc::new(ghost_llm::provider::OllamaProvider {
                        model: first_provider.model.clone().unwrap_or_else(|| "llama3.1".into()),
                        base_url: first_provider.base_url.clone().unwrap_or_else(|| "http://localhost:11434".into()),
                    })
                }
            };
            // Use the complete_stream_shim for non-Ollama providers.
            ghost_llm::provider::complete_stream_shim(provider, messages, tools)
        };

        let result = runner
            .run_turn_streaming(&mut ctx, &user_content, tx.clone(), get_stream)
            .await;

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
                if let Ok(db) = db_clone.lock() {
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
                let _ = event_tx.send(WsEvent::ChatMessage {
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
    });

    // 9. Build SSE stream from mpsc receiver.
    let session_id_sse = session_id.clone();
    let assistant_msg_id_sse = assistant_msg_id.clone();

    let sse_stream = async_stream::stream! {
        // Send stream_start event.
        yield Ok(Event::default()
            .event("stream_start")
            .data(serde_json::json!({
                "session_id": session_id_sse,
                "message_id": assistant_msg_id_sse,
            }).to_string()));

        // Read events from channel until it closes.
        while let Some(event) = rx.recv().await {
            match event {
                AgentStreamEvent::StreamStart { message_id } => {
                    yield Ok(Event::default()
                        .event("stream_start")
                        .data(serde_json::json!({ "message_id": message_id }).to_string()));
                }
                AgentStreamEvent::TextDelta { content } => {
                    yield Ok(Event::default()
                        .event("text_delta")
                        .data(serde_json::json!({ "content": content }).to_string()));
                }
                AgentStreamEvent::ToolUse { tool, tool_id, status } => {
                    yield Ok(Event::default()
                        .event("tool_use")
                        .data(serde_json::json!({
                            "tool": tool,
                            "tool_id": tool_id,
                            "status": status,
                        }).to_string()));
                }
                AgentStreamEvent::ToolResult { tool, tool_id, status, preview } => {
                    yield Ok(Event::default()
                        .event("tool_result")
                        .data(serde_json::json!({
                            "tool": tool,
                            "tool_id": tool_id,
                            "status": status,
                            "preview": preview,
                        }).to_string()));
                }
                AgentStreamEvent::TurnComplete { token_count, safety_status } => {
                    yield Ok(Event::default()
                        .event("stream_end")
                        .data(serde_json::json!({
                            "message_id": assistant_msg_id_sse,
                            "token_count": token_count,
                            "safety_status": safety_status,
                        }).to_string()));
                    break;
                }
                AgentStreamEvent::Error { message } => {
                    yield Ok(Event::default()
                        .event("error")
                        .data(serde_json::json!({ "message": message }).to_string()));
                    break;
                }
            }
        }
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
