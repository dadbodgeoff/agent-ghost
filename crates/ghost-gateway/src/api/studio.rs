//! Studio / Prompt Playground endpoint (T-2.7.1).
//!
//! POST /api/studio/run — thin wrapper around ghost-llm for interactive prompt testing.
//! MVP: stores prompt, returns mock/simulated response.
//! Production: wires to ghost_llm::LlmClient.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Request body for studio prompt run.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StudioRunRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<StudioMessage>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

/// A single message in the conversation.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct StudioMessage {
    pub role: String,
    pub content: String,
}

/// Response from studio run.
#[derive(Debug, Serialize, ToSchema)]
pub struct StudioRunResponse {
    pub content: String,
    pub model: String,
    pub token_count: u32,
    pub finish_reason: String,
}

/// POST /api/studio/run
///
/// MVP implementation: returns a structured simulation response.
/// In production, this would call ghost_llm::LlmClient with the configured
/// model provider and stream the response back.
pub async fn run_prompt(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<StudioRunRequest>,
) -> ApiResult<StudioRunResponse> {
    // Validate request
    if req.messages.is_empty() {
        return Err(ApiError::bad_request("messages array must not be empty"));
    }

    let model = req.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let _temperature = req.temperature.unwrap_or(0.5);
    let _max_tokens = req.max_tokens.unwrap_or(4096);

    // Extract the last user message for simulation
    let _last_user_msg = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("(empty)");

    // T-5.7.2: Wire to ghost_llm provider if model providers are configured.
    // When no explicit system prompt is provided, inject SOUL.md + L4 environment context.
    let default_prompt;
    let system = match req.system_prompt {
        Some(ref s) if !s.is_empty() => s.as_str(),
        _ => {
            default_prompt = build_default_system_prompt();
            &default_prompt
        }
    };

    // Check if any model providers are configured.
    if _state.model_providers.is_empty() {
        return Err(ApiError::bad_request(
            "No model providers configured. Add provider config to ghost.yml \
             (e.g., anthropic with ANTHROPIC_API_KEY) to enable Studio.",
        ));
    }

    // Build LLM messages from the request.
    let mut llm_messages = Vec::new();
    llm_messages.push(ghost_llm::provider::ChatMessage {
        role: ghost_llm::provider::MessageRole::System,
        content: system.to_string(),
        tool_calls: None,
        tool_call_id: None,
    });
    for msg in &req.messages {
        let role = match msg.role.as_str() {
            "user" => ghost_llm::provider::MessageRole::User,
            "assistant" => ghost_llm::provider::MessageRole::Assistant,
            "system" => ghost_llm::provider::MessageRole::System,
            _ => ghost_llm::provider::MessageRole::User,
        };
        llm_messages.push(ghost_llm::provider::ChatMessage {
            role,
            content: msg.content.clone(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    // Construct provider from first configured model provider.
    let provider_config = &_state.model_providers[0];
    let api_key = provider_config
        .api_key_env
        .as_deref()
        .and_then(|env| std::env::var(env).ok())
        .unwrap_or_default();

    let provider: Arc<dyn ghost_llm::provider::LLMProvider> = match provider_config.name.as_str() {
        "ollama" => {
            let base_url = provider_config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let ollama_model = provider_config
                .model
                .clone()
                .unwrap_or_else(|| model.clone());
            Arc::new(ghost_llm::provider::OllamaProvider {
                model: ollama_model,
                base_url,
            })
        }
        "anthropic" => {
            if api_key.is_empty() {
                return Err(ApiError::bad_request(format!(
                    "Model provider 'anthropic' has no API key (set {} env var)",
                    provider_config
                        .api_key_env
                        .as_deref()
                        .unwrap_or("ANTHROPIC_API_KEY"),
                )));
            }
            Arc::new(ghost_llm::provider::AnthropicProvider {
                model: model.clone(),
                api_key: std::sync::RwLock::new(api_key),
            })
        }
        "openai" => {
            if api_key.is_empty() {
                return Err(ApiError::bad_request(format!(
                    "Model provider 'openai' has no API key (set {} env var)",
                    provider_config
                        .api_key_env
                        .as_deref()
                        .unwrap_or("OPENAI_API_KEY"),
                )));
            }
            Arc::new(ghost_llm::provider::OpenAIProvider {
                model: model.clone(),
                api_key: std::sync::RwLock::new(api_key),
            })
        }
        "gemini" => {
            if api_key.is_empty() {
                return Err(ApiError::bad_request(format!(
                    "Model provider 'gemini' has no API key (set {} env var)",
                    provider_config
                        .api_key_env
                        .as_deref()
                        .unwrap_or("GEMINI_API_KEY"),
                )));
            }
            Arc::new(ghost_llm::provider::GeminiProvider {
                model: provider_config.model.clone().unwrap_or(model.clone()),
                api_key: std::sync::RwLock::new(api_key),
            })
        }
        "openai_compat" => {
            if api_key.is_empty() {
                return Err(ApiError::bad_request(format!(
                    "Model provider 'openai_compat' has no API key (set {} env var)",
                    provider_config
                        .api_key_env
                        .as_deref()
                        .unwrap_or("OPENAI_API_KEY"),
                )));
            }
            let base_url = provider_config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            Arc::new(ghost_llm::provider::OpenAICompatProvider {
                model: provider_config.model.clone().unwrap_or(model.clone()),
                api_key: std::sync::RwLock::new(api_key),
                base_url,
                context_window_size: 128_000,
            })
        }
        other => {
            return Err(ApiError::bad_request(format!(
                "Unsupported model provider: {other}"
            )));
        }
    };

    // Call the LLM provider.
    match provider.complete(&llm_messages, &[]).await {
        Ok(result) => {
            let content = match &result.response {
                ghost_llm::provider::LLMResponse::Text(t) => t.clone(),
                ghost_llm::provider::LLMResponse::Mixed { text, .. } => text.clone(),
                ghost_llm::provider::LLMResponse::Empty => String::new(),
                ghost_llm::provider::LLMResponse::ToolCalls(_) => {
                    "[Model returned tool calls instead of text]".to_string()
                }
            };
            Ok(Json(StudioRunResponse {
                content,
                model: result.model,
                token_count: result.usage.total_tokens as u32,
                finish_reason: "stop".to_string(),
            }))
        }
        Err(e) => Err(ApiError::internal(format!("LLM completion failed: {e}"))),
    }
}

/// Build a default system prompt with SOUL.md identity and L4 environment context.
/// Falls back to a plain prompt if neither SOUL.md nor environment info is available.
pub fn build_default_system_prompt() -> String {
    let mut sections = Vec::new();

    // L2: SOUL.md
    let soul_path = crate::bootstrap::ghost_home()
        .join("config")
        .join("SOUL.md");
    if let Ok(content) = std::fs::read_to_string(&soul_path) {
        if !content.is_empty() {
            sections.push(content);
        }
    }

    // L4: Environment context
    let env_ctx = ghost_agent_loop::context::environment::build_environment_context(
        std::env::current_dir().ok().as_deref(),
    );
    if !env_ctx.is_empty() {
        sections.push(format!("## Environment\n{env_ctx}"));
    }

    if sections.is_empty() {
        "You are a helpful assistant.".to_string()
    } else {
        sections.join("\n\n")
    }
}

/// Truncate a string to at most `max_bytes` without splitting a UTF-8 codepoint.
#[allow(dead_code)]
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk backward from max_bytes to find a valid UTF-8 boundary.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
