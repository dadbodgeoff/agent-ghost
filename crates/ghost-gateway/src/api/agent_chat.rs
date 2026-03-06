//! Agent chat endpoint — full AgentRunner via HTTP.
//!
//! POST /api/agent/chat — runs one turn of the agent loop with SOUL.md identity,
//! L4 environment context, all skills, tool execution, and gate checks.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use ghost_llm::fallback::AuthProfile;
use ghost_llm::provider::{AnthropicProvider, GeminiProvider, OllamaProvider, OpenAICompatProvider, OpenAIProvider};

use crate::api::error::{ApiError, ApiResult};
use crate::config::ProviderConfig;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AgentChatRequest {
    /// User message.
    pub message: String,
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
                if let Ok(key) = std::env::var(key_env) {
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
                if let Ok(key) = std::env::var(key_env) {
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
                if let Ok(key) = std::env::var(key_env) {
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
                if let Ok(key) = std::env::var(key_env) {
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

    // 1. Build AgentRunner
    let mut runner = ghost_agent_loop::runner::AgentRunner::new(128_000);
    ghost_agent_loop::tools::executor::register_builtin_tools(&mut runner.tool_registry);

    // 2. Wire DB from AppState
    runner.db = Some(Arc::clone(&state.db));

    // 3. Configure filesystem tool with current working directory
    if let Ok(cwd) = std::env::current_dir() {
        runner.tool_executor.set_workspace_root(cwd);
    }

    // 3b. Apply tool configurations from ghost.yml.
    crate::api::apply_tool_configs(&mut runner.tool_executor, &state.tools_config);

    // 4. Load SOUL.md (L2)
    let soul_path = crate::bootstrap::ghost_home().join("config").join("SOUL.md");
    if let Ok(content) = std::fs::read_to_string(&soul_path) {
        if !content.is_empty() {
            runner.soul_identity = content;
        }
    }

    // 5. Build environment context (L4)
    runner.environment = ghost_agent_loop::context::environment::build_environment_context(
        std::env::current_dir().ok().as_deref(),
    );

    // 6. Wire skills via SkillBridge (shared Arc from AppState)
    let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
        Arc::clone(&state.safety_skills),
        Arc::clone(&state.db),
        state.convergence_profile.clone(),
    );
    ghost_agent_loop::tools::skill_bridge::register_skills(&bridge, &mut runner.tool_registry, None);
    runner.tool_executor.set_skill_bridge(bridge);

    // 7. Build LLM fallback chain from configured providers
    let mut fallback_chain = build_fallback_chain_from_providers(&state.model_providers);

    // 8. Run pre_loop + run_turn
    let agent_id = Uuid::now_v7();
    let session_id = req.session_id.unwrap_or_else(Uuid::now_v7);

    let mut ctx = runner
        .pre_loop(agent_id, session_id, "api", &req.message)
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
