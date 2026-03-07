//! Agent chat endpoint — full AgentRunner via HTTP.
//!
//! POST /api/agent/chat        — runs one turn of the agent loop (blocking JSON response)
//! POST /api/agent/chat/stream — runs one turn with SSE streaming + event persistence

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use uuid::Uuid;

use ghost_agent_loop::runner::AgentStreamEvent;
use ghost_llm::fallback::AuthProfile;
use ghost_llm::provider::{AnthropicProvider, GeminiProvider, OllamaProvider, OpenAICompatProvider, OpenAIProvider};

use crate::api::error::{ApiError, ApiResult};
use crate::api::websocket::WsEvent;
use crate::config::ProviderConfig;
use crate::runtime_safety::{
    RuntimeSafetyBuilder, RuntimeSafetyContext, RuntimeSafetyError, RunnerBuildOptions,
    API_SYNTHETIC_AGENT_NAME,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AgentChatRequest {
    /// User message.
    pub message: String,
    /// Optional durable agent identity (UUID or registered agent name).
    pub agent_id: Option<String>,
    /// Optional session ID for multi-turn conversations.
    pub session_id: Option<Uuid>,
    /// Optional model override.
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentChatResponse {
    pub content: String,
    pub session_id: String,
    pub tool_calls_made: u32,
    pub total_tokens: usize,
    pub total_cost: f64,
}

/// Build an LLM fallback chain from provider configs stored in AppState.
pub fn build_fallback_chain_from_providers(
    providers: &[ProviderConfig],
) -> ghost_agent_loop::runner::LLMFallbackChain {
    let mut chain = ghost_agent_loop::runner::LLMFallbackChain::new();

    for p in providers {
        match p.name.as_str() {
            "ollama" => {
                let base_url = p
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434".into());
                let model = p
                    .model
                    .clone()
                    .unwrap_or_else(|| "llama3.1".into());
                chain.add_provider(
                    Arc::new(OllamaProvider { model, base_url }),
                    vec![],
                );
            }
            "anthropic" => {
                let key_env = p.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY");
                if let Some(key) = crate::state::get_api_key(key_env) {
                    if !key.is_empty() {
                        let model = p
                            .model
                            .clone()
                            .unwrap_or_else(|| "claude-sonnet-4-20250514".into());
                        chain.add_provider(
                            Arc::new(AnthropicProvider {
                                model,
                                api_key: std::sync::RwLock::new(key.clone()),
                            }),
                            vec![AuthProfile { api_key: key, org_id: None }],
                        );
                    }
                }
            }
            "openai" => {
                let key_env = p.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
                if let Some(key) = crate::state::get_api_key(key_env) {
                    if !key.is_empty() {
                        let model = p
                            .model
                            .clone()
                            .unwrap_or_else(|| "gpt-4o".into());
                        chain.add_provider(
                            Arc::new(OpenAIProvider {
                                model,
                                api_key: std::sync::RwLock::new(key.clone()),
                            }),
                            vec![AuthProfile { api_key: key, org_id: None }],
                        );
                    }
                }
            }
            "gemini" => {
                let key_env = p.api_key_env.as_deref().unwrap_or("GEMINI_API_KEY");
                if let Some(key) = crate::state::get_api_key(key_env) {
                    if !key.is_empty() {
                        let model = p
                            .model
                            .clone()
                            .unwrap_or_else(|| "gemini-2.0-flash".into());
                        chain.add_provider(
                            Arc::new(GeminiProvider {
                                model,
                                api_key: std::sync::RwLock::new(key.clone()),
                            }),
                            vec![AuthProfile { api_key: key, org_id: None }],
                        );
                    }
                }
            }
            "openai_compat" => {
                let key_env = p.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
                if let Some(key) = crate::state::get_api_key(key_env) {
                    if !key.is_empty() {
                        let base_url = p
                            .base_url
                            .clone()
                            .unwrap_or_else(|| "http://localhost:8080".into());
                        let model = p
                            .model
                            .clone()
                            .unwrap_or_else(|| "default".into());
                        chain.add_provider(
                            Arc::new(OpenAICompatProvider {
                                model,
                                api_key: std::sync::RwLock::new(key.clone()),
                                base_url,
                                context_window_size: 128_000,
                            }),
                            vec![AuthProfile { api_key: key, org_id: None }],
                        );
                    }
                }
            }
            _ => {}
        }
    }

    chain
}

/// POST /api/agent/chat
///
/// Runs one turn of the full agent loop with environment awareness, SOUL.md identity,
/// skills, tool execution, and safety gate checks.
pub async fn agent_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentChatRequest>,
) -> ApiResult<AgentChatResponse> {
    if req.message.is_empty() {
        return Err(ApiError::bad_request("message must not be empty"));
    }

    if state.model_providers.is_empty() {
        return Err(ApiError::bad_request(
            "No model providers configured. Add provider config to ghost.yml to enable agent chat.",
        ));
    }

    let builder = RuntimeSafetyBuilder::new(&state);
    let agent = builder
        .resolve_agent(req.agent_id.as_deref(), API_SYNTHETIC_AGENT_NAME)
        .map_err(map_runtime_safety_error)?;
    let session_id = req.session_id.unwrap_or_else(Uuid::now_v7);
    let runtime_ctx = RuntimeSafetyContext::from_state(&state, agent, session_id, None);
    let mut runner = builder
        .build_live_runner(&runtime_ctx, RunnerBuildOptions::default())
        .map_err(map_runtime_safety_error)?;

    // 1. Build LLM fallback chain from configured providers
    let mut fallback_chain = build_fallback_chain_from_providers(&state.model_providers);

    // 2. Run pre_loop + run_turn
    let mut ctx = runner
        .pre_loop(runtime_ctx.agent.id, runtime_ctx.session_id, "api", &req.message)
        .await
        .map_err(|e| ApiError::internal(format!("agent pre-loop failed: {e}")))?;

    let result = runner
        .run_turn(&mut ctx, &mut fallback_chain, &req.message)
        .await
        .map_err(|e| ApiError::internal(format!("agent run failed: {e}")))?;

    Ok(Json(AgentChatResponse {
        content: result.output.unwrap_or_default(),
        session_id: session_id.to_string(),
        tool_calls_made: result.tool_calls_made,
        total_tokens: result.total_tokens,
        total_cost: result.total_cost,
    }))
}

/// POST /api/agent/chat/stream
///
/// Streaming variant of agent_chat. Returns SSE events with event persistence
/// and WebSocket milestone broadcasts for cross-client awareness.
pub async fn agent_chat_stream(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentChatRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if req.message.is_empty() {
        return Err(ApiError::bad_request("message must not be empty"));
    }

    if state.model_providers.is_empty() {
        return Err(ApiError::bad_request(
            "No model providers configured. Add provider config to ghost.yml to enable agent chat.",
        ));
    }

    let builder = RuntimeSafetyBuilder::new(&state);
    let agent = builder
        .resolve_agent(req.agent_id.as_deref(), API_SYNTHETIC_AGENT_NAME)
        .map_err(map_runtime_safety_error)?;
    let session_id = req.session_id.unwrap_or_else(Uuid::now_v7);
    let runtime_ctx = RuntimeSafetyContext::from_state(&state, agent, session_id, None);
    let mut runner = builder
        .build_live_runner(&runtime_ctx, RunnerBuildOptions::default())
        .map_err(map_runtime_safety_error)?;

    // 2. IDs
    let session_id_str = session_id.to_string();
    let message_id = Uuid::now_v7().to_string();

    // 3. Determine provider for streaming
    let first_provider = state.model_providers[0].clone();

    // 4. Create channel for streaming events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentStreamEvent>(64);
    let user_message = req.message.clone();

    // 5. Spawn agent run as background task with timeout
    tokio::spawn(async move {
        let tx_timeout = tx.clone();
        let turn_result = tokio::time::timeout(std::time::Duration::from_secs(300), async move {
            let mut ctx = match runner
                .pre_loop(runtime_ctx.agent.id, runtime_ctx.session_id, "api", &user_message)
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

            // Build get_stream closure based on provider type (mirrors studio_sessions pattern).
            let get_stream = move |messages: Vec<ghost_llm::provider::ChatMessage>, tools: Vec<ghost_llm::provider::ToolSchema>| -> ghost_llm::streaming::StreamChunkStream {
                let provider: Arc<dyn ghost_llm::provider::LLMProvider> = match first_provider.name.as_str() {
                    "ollama" => {
                        let ollama = ghost_llm::provider::OllamaProvider {
                            model: first_provider.model.clone().unwrap_or_else(|| "llama3.1".into()),
                            base_url: first_provider.base_url.clone().unwrap_or_else(|| "http://localhost:11434".into()),
                        };
                        return ollama.stream_chat(&messages, &tools);
                    }
                    "anthropic" => {
                        let key = crate::state::get_api_key(first_provider.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY")).unwrap_or_default();
                        Arc::new(ghost_llm::provider::AnthropicProvider {
                            model: first_provider.model.clone().unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
                            api_key: std::sync::RwLock::new(key),
                        })
                    }
                    "openai" => {
                        let key = crate::state::get_api_key(first_provider.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY")).unwrap_or_default();
                        Arc::new(ghost_llm::provider::OpenAIProvider {
                            model: first_provider.model.clone().unwrap_or_else(|| "gpt-4o".into()),
                            api_key: std::sync::RwLock::new(key),
                        })
                    }
                    "gemini" => {
                        let key = crate::state::get_api_key(first_provider.api_key_env.as_deref().unwrap_or("GEMINI_API_KEY")).unwrap_or_default();
                        Arc::new(ghost_llm::provider::GeminiProvider {
                            model: first_provider.model.clone().unwrap_or_else(|| "gemini-2.0-flash".into()),
                            api_key: std::sync::RwLock::new(key),
                        })
                    }
                    "openai_compat" => {
                        let key = crate::state::get_api_key(first_provider.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY")).unwrap_or_default();
                        let compat = ghost_llm::provider::OpenAICompatProvider {
                            model: first_provider.model.clone().unwrap_or_else(|| "default".into()),
                            api_key: std::sync::RwLock::new(key),
                            base_url: first_provider.base_url.clone().unwrap_or_else(|| "http://localhost:8080".into()),
                            context_window_size: 128_000,
                        };
                        return compat.stream_chat(&messages, &tools);
                    }
                    _ => {
                        Arc::new(ghost_llm::provider::OllamaProvider {
                            model: first_provider.model.clone().unwrap_or_else(|| "llama3.1".into()),
                            base_url: first_provider.base_url.clone().unwrap_or_else(|| "http://localhost:11434".into()),
                        })
                    }
                };
                ghost_llm::provider::complete_stream_shim(provider, messages, tools)
            };

            let result = runner.run_turn_streaming(&mut ctx, &user_message, tx.clone(), get_stream).await;
            match result {
                Ok(_) => {} // TurnComplete already sent
                Err(e) => {
                    let _ = tx.send(AgentStreamEvent::Error {
                        message: format!("agent run failed: {e}"),
                    }).await;
                }
            }
        }).await;

        if turn_result.is_err() {
            let _ = tx_timeout.send(AgentStreamEvent::Error {
                message: "Agent turn timed out after 5 minutes".into(),
            }).await;
        }
    });

    // 6. Build SSE stream with event persistence
    let db_for_stream = Arc::clone(&state.db);
    let state_for_stream = Arc::clone(&state);
    let session_id_sse = session_id_str.clone();
    let message_id_sse = message_id.clone();

    let sse_stream = async_stream::stream! {
        let mut text_buffer = String::new();
        const TEXT_FLUSH_THRESHOLD: usize = 2048;

        let persist_event = |db: &Arc<crate::db_pool::DbPool>, sid: &str, mid: &str, etype: &str, payload: &serde_json::Value| -> Option<i64> {
            match db.read() {
                Ok(conn) => {
                    cortex_storage::queries::stream_event_queries::insert_stream_event(
                        &conn, sid, mid, etype, &payload.to_string(),
                    ).ok()
                }
                Err(_) => None,
            }
        };

        let flush_text = |db: &Arc<crate::db_pool::DbPool>, sid: &str, mid: &str, buf: &mut String| -> Option<i64> {
            if buf.is_empty() { return None; }
            let payload = serde_json::json!({ "content": buf.as_str() });
            let seq = match db.read() {
                Ok(conn) => cortex_storage::queries::stream_event_queries::insert_stream_event(
                    &conn, sid, mid, "text_chunk", &payload.to_string(),
                ).ok(),
                Err(_) => None,
            };
            buf.clear();
            seq
        };

        // stream_start
        let start_payload = serde_json::json!({
            "session_id": session_id_sse,
            "message_id": message_id_sse,
        });
        let start_seq = persist_event(&db_for_stream, &session_id_sse, &message_id_sse, "stream_start", &start_payload);
        let mut start_event = Event::default().event("stream_start").data(start_payload.to_string());
        if let Some(seq) = start_seq { start_event = start_event.id(seq.to_string()); }
        yield Ok(start_event);

        let mut stream_ended = false;

        while let Some(event) = rx.recv().await {
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
                        seq = flush_text(&db_for_stream, &session_id_sse, &message_id_sse, &mut text_buffer);
                    }
                    let mut ev = Event::default()
                        .event("text_delta")
                        .data(serde_json::json!({ "content": content }).to_string());
                    if let Some(s) = seq { ev = ev.id(s.to_string()); }
                    yield Ok(ev);
                }
                AgentStreamEvent::ToolUse { tool, tool_id, status } => {
                    flush_text(&db_for_stream, &session_id_sse, &message_id_sse, &mut text_buffer);
                    let payload = serde_json::json!({ "tool": tool, "tool_id": tool_id, "status": status });
                    let seq = persist_event(&db_for_stream, &session_id_sse, &message_id_sse, "tool_use", &payload);

                    crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                        session_id: session_id_sse.clone(),
                        event_id: tool_id.clone(),
                        event_type: format!("tool_use:{}", tool),
                        sender: Some(tool.clone()),
                        sequence_number: seq.unwrap_or(0),
                    });

                    let mut ev = Event::default().event("tool_use").data(payload.to_string());
                    if let Some(s) = seq { ev = ev.id(s.to_string()); }
                    yield Ok(ev);
                }
                AgentStreamEvent::ToolResult { tool, tool_id, status, preview } => {
                    let payload = serde_json::json!({ "tool": tool, "tool_id": tool_id, "status": status, "preview": preview });
                    let seq = persist_event(&db_for_stream, &session_id_sse, &message_id_sse, "tool_result", &payload);

                    crate::api::websocket::broadcast_event(&state_for_stream, WsEvent::SessionEvent {
                        session_id: session_id_sse.clone(),
                        event_id: tool_id.clone(),
                        event_type: format!("tool_result:{}", tool),
                        sender: Some(tool.clone()),
                        sequence_number: seq.unwrap_or(0),
                    });

                    let mut ev = Event::default().event("tool_result").data(payload.to_string());
                    if let Some(s) = seq { ev = ev.id(s.to_string()); }
                    yield Ok(ev);
                }
                AgentStreamEvent::Heartbeat { phase } => {
                    yield Ok(Event::default()
                        .event("heartbeat")
                        .data(serde_json::json!({ "phase": phase }).to_string()));
                }
                AgentStreamEvent::TurnComplete { token_count, safety_status } => {
                    flush_text(&db_for_stream, &session_id_sse, &message_id_sse, &mut text_buffer);
                    let payload = serde_json::json!({
                        "message_id": message_id_sse,
                        "session_id": session_id_sse,
                        "token_count": token_count,
                        "safety_status": safety_status,
                    });
                    let seq = persist_event(&db_for_stream, &session_id_sse, &message_id_sse, "turn_complete", &payload);
                    let mut ev = Event::default().event("stream_end").data(payload.to_string());
                    if let Some(s) = seq { ev = ev.id(s.to_string()); }
                    yield Ok(ev);
                    stream_ended = true;
                    break;
                }
                AgentStreamEvent::Error { message } => {
                    flush_text(&db_for_stream, &session_id_sse, &message_id_sse, &mut text_buffer);
                    let payload = serde_json::json!({ "message": message });
                    let seq = persist_event(&db_for_stream, &session_id_sse, &message_id_sse, "error", &payload);
                    let mut ev = Event::default().event("error").data(payload.to_string());
                    if let Some(s) = seq { ev = ev.id(s.to_string()); }
                    yield Ok(ev);
                    stream_ended = true;
                    break;
                }
            }
        }

        if !stream_ended {
            flush_text(&db_for_stream, &session_id_sse, &message_id_sse, &mut text_buffer);
            yield Ok(Event::default()
                .event("stream_end")
                .data(serde_json::json!({
                    "message_id": message_id_sse,
                    "session_id": session_id_sse,
                    "token_count": 0,
                    "safety_status": "unknown",
                }).to_string()));
        }
    };

    Ok(Sse::new(sse_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
}

fn map_runtime_safety_error(error: RuntimeSafetyError) -> ApiError {
    match error {
        RuntimeSafetyError::AgentNotFound(message) => ApiError::bad_request(message),
        _ => ApiError::internal(error.to_string()),
    }
}
