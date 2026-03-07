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
                    let base_url = p.base_url.clone()
                        .or_else(|| std::env::var("OLLAMA_BASE_URL").ok())
                        .unwrap_or_else(|| "http://localhost:11434".into());
                    let model = p.model.clone()
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
                            let model = p.model.clone()
                                .unwrap_or_else(|| "claude-sonnet-4-20250514".into());
                            let provider = AnthropicProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile { api_key: key, org_id: None }],
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
                            let model = p.model.clone()
                                .unwrap_or_else(|| "gpt-4o".into());
                            let provider = OpenAIProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile { api_key: key, org_id: None }],
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
                            let model = p.model.clone()
                                .unwrap_or_else(|| "gemini-2.0-flash".into());
                            let provider = GeminiProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile { api_key: key, org_id: None }],
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
                            let base_url = p.base_url.clone()
                                .unwrap_or_else(|| "http://localhost:8080".into());
                            let model = p.model.clone()
                                .unwrap_or_else(|| "default".into());
                            let provider = OpenAICompatProvider {
                                model: model.clone(),
                                api_key: std::sync::RwLock::new(key.clone()),
                                base_url: base_url.clone(),
                                context_window_size: 128_000,
                            };
                            chain.add_provider(
                                Arc::new(provider),
                                vec![AuthProfile { api_key: key, org_id: None }],
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
                    vec![AuthProfile { api_key: key, org_id: None }],
                );
                tracing::info!(provider = "anthropic", model = %model, "LLM provider added (env)");
                count += 1;
            }
        }

        // OpenAI
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.is_empty() {
                let model = std::env::var("OPENAI_MODEL")
                    .unwrap_or_else(|_| "gpt-4o".into());
                let provider = OpenAIProvider {
                    model: model.clone(),
                    api_key: std::sync::RwLock::new(key.clone()),
                };
                chain.add_provider(
                    Arc::new(provider),
                    vec![AuthProfile { api_key: key, org_id: None }],
                );
                tracing::info!(provider = "openai", model = %model, "LLM provider added (env)");
                count += 1;
            }
        }

        // Gemini
        if let Ok(key) = std::env::var("GEMINI_API_KEY") {
            if !key.is_empty() {
                let model = std::env::var("GEMINI_MODEL")
                    .unwrap_or_else(|_| "gemini-2.0-flash".into());
                let provider = GeminiProvider {
                    model: model.clone(),
                    api_key: std::sync::RwLock::new(key.clone()),
                };
                chain.add_provider(
                    Arc::new(provider),
                    vec![AuthProfile { api_key: key, org_id: None }],
                );
                tracing::info!(provider = "gemini", model = %model, "LLM provider added (env)");
                count += 1;
            }
        }

        // Ollama (local — no API key needed)
        if let Ok(base_url) = std::env::var("OLLAMA_BASE_URL") {
            if !base_url.is_empty() {
                let model = std::env::var("OLLAMA_MODEL")
                    .unwrap_or_else(|_| "llama3.1".into());
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
    let candidates = [
        "ghost.yml",
        "~/.ghost/ghost.yml",
    ];
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

    // Set up agent runner for live mode.
    let mut runner = ghost_agent_loop::runner::AgentRunner::new(128_000);
    ghost_agent_loop::tools::executor::register_builtin_tools(&mut runner.tool_registry);

    // Wire DB connection for proposal/violation/reflection persistence.
    let db_path = crate::bootstrap::shellexpand_tilde("~/.ghost/data/ghost.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;") {
            tracing::warn!(error = %e, path = %db_path, "PRAGMA setup failed — DB may have degraded performance");
        }
        let db = std::sync::Arc::new(std::sync::Mutex::new(conn));
        runner.db = Some(db);
        tracing::info!(path = %db_path, "DB wired into AgentRunner for persistence");
    } else {
        tracing::warn!(path = %db_path, "Could not open DB — persistence disabled");
    }

    // Wire CostTracker.record() into the agent runner (T-1.2.1).
    let cost_tracker = std::sync::Arc::new(crate::cost::tracker::CostTracker::new());
    let ct = cost_tracker.clone();
    runner.cost_recorder = Some(std::sync::Arc::new(move |agent_id, session_id, cost, is_compaction| {
        ct.record(agent_id, session_id, cost, is_compaction);
    }));

    // Configure filesystem tool with current working directory.
    let workspace_root = std::env::current_dir().ok();
    if let Some(ref cwd) = workspace_root {
        runner.tool_executor.set_workspace_root(cwd.clone());
        tracing::info!(workspace = %cwd.display(), "Filesystem tool configured");
    }

    // ── L2: Load SOUL.md identity ──────────────────────────────────────
    let soul_path = crate::bootstrap::ghost_home().join("config").join("SOUL.md");
    if !soul_path.exists() {
        // Auto-init SOUL.md if missing (first run).
        if let Some(parent) = soul_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = ghost_identity::soul_manager::SoulManager::create_template(&soul_path) {
            tracing::warn!(error = %e, "Failed to auto-create SOUL.md");
        } else {
            tracing::info!(path = %soul_path.display(), "Auto-created SOUL.md template");
        }
    }
    {
        let mut soul_mgr = ghost_identity::soul_manager::SoulManager::new();
        match soul_mgr.load(&soul_path) {
            Ok(doc) => {
                runner.soul_identity = doc.content.clone();
                tracing::info!(path = %soul_path.display(), "L2 SOUL.md loaded");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load SOUL.md — L2 will be empty");
            }
        }
    }

    // ── L4: Build environment context ──────────────────────────────────
    runner.environment = ghost_agent_loop::context::environment::build_environment_context(
        workspace_root.as_deref(),
    );
    tracing::info!(tokens = runner.environment.len() / 4, "L4 environment context built");

    // Wire skills into the agent loop as LLM-callable tools.
    if let Some(ref db) = runner.db {
        let mut all_skills: std::collections::HashMap<String, Box<dyn ghost_skills::skill::Skill>> =
            ghost_skills::safety_skills::all_safety_skills()
                .into_iter()
                .map(|s| (s.name().to_string(), s))
                .collect();

        for skill in ghost_skills::git_skills::all_git_skills() {
            all_skills.insert(skill.name().to_string(), skill);
        }
        for skill in ghost_skills::code_analysis::all_code_analysis_skills() {
            all_skills.insert(skill.name().to_string(), skill);
        }
        for skill in ghost_skills::bundled_skills::all_bundled_skills() {
            all_skills.insert(skill.name().to_string(), skill);
        }
        for skill in ghost_skills::delegation_skills::all_delegation_skills() {
            all_skills.insert(skill.name().to_string(), skill);
        }

        // Load PC control skills if enabled in config.
        if let Some(config) = load_ghost_config() {
            let pc_skills = ghost_pc_control::all_pc_control_skills(&config.pc_control);
            for skill in pc_skills {
                all_skills.insert(skill.name().to_string(), skill);
            }
        }

        let skills = std::sync::Arc::new(all_skills);
        let convergence_profile = load_ghost_config()
            .map(|c| c.convergence.profile.clone())
            .unwrap_or_else(|| "standard".into());

        let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
            skills,
            std::sync::Arc::clone(db),
            convergence_profile,
        );

        // Load skill allowlist from agent config if available.
        let allowlist = load_ghost_config()
            .and_then(|cfg| cfg.agents.first().and_then(|a| a.skills.clone()));

        ghost_agent_loop::tools::skill_bridge::register_skills(
            &bridge,
            &mut runner.tool_registry,
            allowlist.as_deref(),
        );

        runner.tool_executor.set_skill_bridge(bridge);
    }

    // Build fallback chain from ghost.yml config / environment variables.
    let mut fallback_chain = build_fallback_chain();

    let cli_agent = load_ghost_config().and_then(|cfg| cfg.agents.first().cloned());
    if let Some(agent) = &cli_agent {
        runner.spending_cap = agent.spending_cap;
    }

    let agent_id = cli_agent
        .as_ref()
        .map(|agent| crate::agents::registry::durable_agent_id(&agent.name))
        .unwrap_or_else(|| {
            crate::agents::registry::durable_agent_id(
                crate::runtime_safety::CLI_SYNTHETIC_AGENT_NAME,
            )
        });
    let session_id = Uuid::now_v7();

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
                println!("[status] Kill switch: {}", runner.kill_switch.load(Ordering::SeqCst));
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
                        match runner.run_turn(&mut ctx, &mut fallback_chain, trimmed).await {
                            Ok(result) => {
                                if let Some(output) = &result.output {
                                    println!("ghost> {output}");
                                } else {
                                    println!("ghost> [no reply]");
                                }
                                if result.tool_calls_made > 0 {
                                    println!("  [{} tool calls, ${:.4}]",
                                        result.tool_calls_made, result.total_cost);
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
