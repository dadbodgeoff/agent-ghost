//! Interactive chat REPL (Task 6.6).
//!
//! Wired to AgentRunner for live agent interaction via CLI.

use std::io::{self, Write};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use ghost_llm::fallback::AuthProfile;
use ghost_llm::provider::{
    AnthropicProvider, GeminiProvider, OllamaProvider, OpenAICompatProvider, OpenAIProvider,
};
use uuid::Uuid;

use crate::runtime_safety::{
    build_live_runner_with_dependencies, RunnerBuildOptions, RuntimeRunnerDependencies,
    RuntimeSafetyContext, CLI_SYNTHETIC_AGENT_NAME,
};

/// Build the LLM fallback chain from ghost.yml config and environment variables.
///
/// First checks ghost.yml for configured providers, then falls back to
/// environment variables:
///   ANTHROPIC_API_KEY  → AnthropicProvider (claude-sonnet-4-20250514)
///   OPENAI_API_KEY     → OpenAIProvider (gpt-4o)
///   GEMINI_API_KEY     → GeminiProvider (gemini-2.0-flash)
///   OLLAMA_BASE_URL    → OllamaProvider (local, no key needed)
///
/// All found providers are added to the chain for fallback.
fn build_fallback_chain() -> ghost_agent_loop::runner::LLMFallbackChain {
    let mut chain = ghost_agent_loop::runner::LLMFallbackChain::new();
    let mut count = 0usize;

    // Try loading providers from ghost.yml first.
    if let Some(config) = load_models_config() {
        for p in &config.providers {
            match p.name.as_str() {
                "ollama" => {
                    let base_url = p
                        .base_url
                        .clone()
                        .or_else(|| std::env::var("OLLAMA_BASE_URL").ok())
                        .unwrap_or_else(|| "http://localhost:11434".into());
                    let model = p
                        .model
                        .clone()
                        .or_else(|| std::env::var("OLLAMA_MODEL").ok())
                        .unwrap_or_else(|| "llama3.1".into());
                    let provider = OllamaProvider {
                        model: model.clone(),
                        base_url: base_url.clone(),
                    };
                    chain.add_provider(Arc::new(provider), vec![]);
                    tracing::info!(provider = "ollama", model = %model, base_url = %base_url, "LLM provider added (config)");
                    count += 1;
                }
                "anthropic" => {
                    let key_env = p.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY");
                    if let Some(key) = crate::state::get_api_key(key_env) {
                        if !key.is_empty() {
                            let model = p
                                .model
                                .clone()
                                .unwrap_or_else(|| "claude-sonnet-4-20250514".into());
                            let provider = AnthropicProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile {
                                    api_key: key,
                                    org_id: None,
                                }],
                            );
                            tracing::info!(provider = "anthropic", model = %model, "LLM provider added (config)");
                            count += 1;
                        }
                    }
                }
                "openai" => {
                    let key_env = p.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
                    if let Some(key) = crate::state::get_api_key(key_env) {
                        if !key.is_empty() {
                            let model = p.model.clone().unwrap_or_else(|| "gpt-4o".into());
                            let provider = OpenAIProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile {
                                    api_key: key,
                                    org_id: None,
                                }],
                            );
                            tracing::info!(provider = "openai", model = %model, "LLM provider added (config)");
                            count += 1;
                        }
                    }
                }
                "gemini" => {
                    let key_env = p.api_key_env.as_deref().unwrap_or("GEMINI_API_KEY");
                    if let Some(key) = crate::state::get_api_key(key_env) {
                        if !key.is_empty() {
                            let model =
                                p.model.clone().unwrap_or_else(|| "gemini-2.0-flash".into());
                            let provider = GeminiProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile {
                                    api_key: key,
                                    org_id: None,
                                }],
                            );
                            tracing::info!(provider = "gemini", model = %model, "LLM provider added (config)");
                            count += 1;
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
                            let model = p.model.clone().unwrap_or_else(|| "default".into());
                            let provider = OpenAICompatProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                                base_url: base_url.clone(),
                                context_window_size: 128_000,
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile {
                                    api_key: key,
                                    org_id: None,
                                }],
                            );
                            tracing::info!(provider = "openai_compat", model = %model, base_url = %base_url, "LLM provider added (config)");
                            count += 1;
                        }
                    }
                }
                other => {
                    tracing::warn!(provider = other, "Unknown provider in ghost.yml — skipped");
                }
            }
        }
    }

    // Fall back to env vars for any providers not already added via config.
    if count == 0 {
        // Anthropic
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                let model = std::env::var("ANTHROPIC_MODEL")
                    .unwrap_or_else(|_| "claude-sonnet-4-20250514".into());
                let provider = AnthropicProvider {
                    model: model.clone(),
                    api_key: std::sync::RwLock::new(key.clone()),
                };
                chain.add_provider(
                    Arc::new(provider),
                    vec![AuthProfile {
                        api_key: key,
                        org_id: None,
                    }],
                );
                tracing::info!(provider = "anthropic", model = %model, "LLM provider added (env)");
                count += 1;
            }
        }

        // OpenAI
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.is_empty() {
                let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".into());
                let provider = OpenAIProvider {
                    model: model.clone(),
                    api_key: std::sync::RwLock::new(key.clone()),
                };
                chain.add_provider(
                    Arc::new(provider),
                    vec![AuthProfile {
                        api_key: key,
                        org_id: None,
                    }],
                );
                tracing::info!(provider = "openai", model = %model, "LLM provider added (env)");
                count += 1;
            }
        }

        // Gemini
        if let Ok(key) = std::env::var("GEMINI_API_KEY") {
            if !key.is_empty() {
                let model =
                    std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".into());
                let provider = GeminiProvider {
                    model: model.clone(),
                    api_key: std::sync::RwLock::new(key.clone()),
                };
                chain.add_provider(
                    Arc::new(provider),
                    vec![AuthProfile {
                        api_key: key,
                        org_id: None,
                    }],
                );
                tracing::info!(provider = "gemini", model = %model, "LLM provider added (env)");
                count += 1;
            }
        }

        // Ollama (local — no API key needed)
        if let Ok(base_url) = std::env::var("OLLAMA_BASE_URL") {
            if !base_url.is_empty() {
                let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.1".into());
                let provider = OllamaProvider {
                    model: model.clone(),
                    base_url: base_url.clone(),
                };
                chain.add_provider(Arc::new(provider), vec![]);
                tracing::info!(provider = "ollama", model = %model, base_url = %base_url, "LLM provider added (env)");
                count += 1;
            }
        }
    }

    if count == 0 {
        tracing::warn!(
            "No LLM providers configured. Add providers to ghost.yml or set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY, OLLAMA_BASE_URL"
        );
    }

    chain
}

/// Load the models section from ghost.yml, if available.
fn load_models_config() -> Option<crate::config::ModelsConfig> {
    load_ghost_config()
        .filter(|c| !c.models.providers.is_empty())
        .map(|c| c.models)
}

/// Load the full GhostConfig from ghost.yml, if available.
fn load_ghost_config() -> Option<crate::config::GhostConfig> {
    let candidates = ["ghost.yml", "~/.ghost/ghost.yml"];
    for path in &candidates {
        let expanded = crate::bootstrap::shellexpand_tilde(path);
        if let Ok(contents) = std::fs::read_to_string(&expanded) {
            if let Ok(config) = serde_yaml::from_str::<crate::config::GhostConfig>(&contents) {
                return Some(config);
            }
        }
    }
    None
}

/// Run an interactive chat session via CLI.
///
/// Creates an AgentRunner and dispatches user messages through the
/// full agentic loop with real LLM providers from env vars.
pub async fn run_interactive_chat() -> Result<(), super::error::CliError> {
    run_interactive_chat_inner().await;
    Ok(())
}

async fn run_interactive_chat_inner() {
    println!("GHOST Interactive Chat");
    println!("Type /quit to exit, /help for commands.\n");

    let mut stdout = io::stdout();
    let ghost_config = load_ghost_config();
    let effective_config = ghost_config.clone().unwrap_or_default();
    let cli_agent = ghost_config
        .as_ref()
        .and_then(|cfg| cfg.agents.first().cloned());

    // Wire DB persistence and compiled skill resolution through the same
    // catalog path used by the gateway runtime.
    let db_path = crate::bootstrap::shellexpand_tilde("~/.ghost/data/ghost.db");
    let db_pool = match crate::db_pool::create_pool(std::path::PathBuf::from(&db_path)) {
        Ok(pool) => {
            let migration_result = {
                let writer = pool.writer_for_migrations().await;
                cortex_storage::migrations::run_migrations(&writer)
            };
            match migration_result {
                Ok(()) => {
                    tracing::info!(path = %db_path, "DB pool wired into CLI runtime");
                    Some(pool)
                }
                Err(error) => {
                    tracing::warn!(error = %error, path = %db_path, "DB migrations failed — persistence disabled");
                    None
                }
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, path = %db_path, "Could not open DB pool — persistence disabled");
            None
        }
    };
    let db = db_pool
        .as_ref()
        .and_then(|pool| pool.legacy_connection().ok());

    let cost_tracker = std::sync::Arc::new(crate::cost::tracker::CostTracker::new());
    let agent_id = cli_agent
        .as_ref()
        .map(|agent| crate::agents::registry::durable_agent_id(&agent.name))
        .unwrap_or_else(|| crate::agents::registry::durable_agent_id(CLI_SYNTHETIC_AGENT_NAME));
    let session_id = Uuid::now_v7();
    let runtime_ctx = RuntimeSafetyContext {
        agent: crate::runtime_safety::ResolvedRuntimeAgent {
            id: agent_id,
            name: cli_agent
                .as_ref()
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| CLI_SYNTHETIC_AGENT_NAME.to_string()),
            capabilities: cli_agent
                .as_ref()
                .map(|agent| agent.capabilities.clone())
                .unwrap_or_default(),
            skill_allowlist: cli_agent.as_ref().and_then(|agent| agent.skills.clone()),
            spending_cap: cli_agent
                .as_ref()
                .map(|agent| agent.spending_cap)
                .unwrap_or(10.0),
        },
        session_id,
        run_id: Uuid::now_v7(),
        message_id: None,
        kill_switch: Arc::new(crate::safety::kill_switch::KillSwitch::new()),
        kill_gate: None,
        convergence_profile: ghost_config
            .as_ref()
            .map(|c| c.convergence.profile.clone())
            .unwrap_or_else(|| "standard".into()),
        capability_scope: cli_agent
            .as_ref()
            .map(|agent| agent.capabilities.clone())
            .unwrap_or_default(),
    };
    let resolved_skills = if let Some(pool) = &db_pool {
        let compiled =
            crate::skill_catalog::definitions::build_compiled_skill_definitions(&effective_config);
        match crate::skill_catalog::service::SkillCatalogService::new(
            compiled.definitions,
            Arc::clone(pool),
        )
        .await
        {
            Ok(catalog) => catalog
                .resolve_for_runtime(&runtime_ctx.agent, None)
                .unwrap_or_default(),
            Err(error) => {
                tracing::warn!(error = %error, "Failed to initialize CLI skill catalog");
                crate::skill_catalog::ResolvedSkillSet::default()
            }
        }
    } else {
        crate::skill_catalog::ResolvedSkillSet::default()
    };
    let mut runner = build_live_runner_with_dependencies(
        &runtime_ctx,
        RuntimeRunnerDependencies {
            db: db.clone(),
            resolved_skills,
            tools_config: ghost_config
                .as_ref()
                .map(|cfg| cfg.tools.clone())
                .unwrap_or_default(),
            convergence_profile: runtime_ctx.convergence_profile.clone(),
            monitor_enabled: ghost_config
                .as_ref()
                .map(|cfg| cfg.convergence.monitor.enabled)
                .unwrap_or(false),
            monitor_block_on_degraded: ghost_config
                .as_ref()
                .map(|cfg| cfg.convergence.monitor.block_on_degraded)
                .unwrap_or(false),
            convergence_state_stale_after: std::time::Duration::from_secs(
                ghost_config
                    .as_ref()
                    .map(|cfg| cfg.convergence.monitor.stale_after_secs)
                    .unwrap_or(300),
            ),
            cost_tracker: Some(cost_tracker),
        },
        RunnerBuildOptions {
            system_prompt: None,
            conversation_history: Vec::new(),
            skill_allowlist: None,
        },
    )
    .expect("cli runtime safety runner construction should not fail");

    // Build fallback chain from ghost.yml config / environment variables.
    let mut fallback_chain = build_fallback_chain();

    loop {
        print!("you> ");
        if let Err(e) = stdout.flush() {
            tracing::warn!(error = %e, "failed to flush stdout");
        }

        // Use tokio::select! so Ctrl+C is handled cleanly (T-X.4).
        let input = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nGoodbye.");
                return;
            }
            line = super::signal::read_line_async() => {
                match line {
                    Some(l) => l,
                    None => {
                        // EOF (Ctrl+D) or I/O error.
                        eprintln!("\nGoodbye.");
                        return;
                    }
                }
            }
        };

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        match trimmed {
            "/quit" | "/exit" | "/q" => {
                println!("Goodbye.");
                break;
            }
            "/help" | "/h" => {
                println!("Commands:");
                println!("  /quit    — Exit chat");
                println!("  /status  — Show agent status");
                println!("  /model   — Show current model");
                println!("  /help    — Show this help");
            }
            "/status" => {
                println!("[status] Agent ID: {agent_id}");
                println!("[status] Session: {session_id}");
                println!(
                    "[status] Kill switch: {}",
                    runner.kill_switch.load(Ordering::SeqCst)
                );
                println!("[status] Daily spend: ${:.4}", runner.daily_spend);
            }
            "/model" => {
                println!("[model] Default model selection active");
                println!("[model] Configure providers in ghost.yml");
            }
            _ => {
                // Dispatch through the agentic loop.
                match runner.pre_loop(agent_id, session_id, "cli", trimmed).await {
                    Ok(mut ctx) => {
                        match runner
                            .run_turn(&mut ctx, &mut fallback_chain, trimmed)
                            .await
                        {
                            Ok(result) => {
                                if let Some(output) = &result.output {
                                    println!("ghost> {output}");
                                } else {
                                    println!("ghost> [no reply]");
                                }
                                if result.tool_calls_made > 0 {
                                    println!(
                                        "  [{} tool calls, ${:.4}]",
                                        result.tool_calls_made, result.total_cost
                                    );
                                }
                            }
                            Err(e) => {
                                println!("ghost> [error: {e}]");
                            }
                        }
                    }
                    Err(e) => {
                        println!("ghost> [pre-loop error: {e}]");
                    }
                }
            }
        }
    }
}
