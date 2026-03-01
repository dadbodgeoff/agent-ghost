//! Interactive chat REPL (Task 6.6).
//!
//! Wired to AgentRunner for live agent interaction via CLI.

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ghost_llm::fallback::AuthProfile;
use ghost_llm::provider::{
    AnthropicProvider, GeminiProvider, OllamaProvider, OpenAICompatProvider, OpenAIProvider,
};
use uuid::Uuid;

/// Build the LLM fallback chain from environment variables.
///
/// Checks for API keys in this order (first found becomes primary):
///   ANTHROPIC_API_KEY  → AnthropicProvider (claude-sonnet-4-20250514)
///   OPENAI_API_KEY     → OpenAIProvider (gpt-4o)
///   GEMINI_API_KEY     → GeminiProvider (gemini-2.0-flash)
///   OLLAMA_BASE_URL    → OllamaProvider (local, no key needed)
///
/// All found providers are added to the chain for fallback.
fn build_fallback_chain_from_env() -> ghost_agent_loop::runner::LLMFallbackChain {
    let mut chain = ghost_agent_loop::runner::LLMFallbackChain::new();
    let mut count = 0usize;

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
            tracing::info!(provider = "anthropic", model = %model, "LLM provider added");
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
            tracing::info!(provider = "openai", model = %model, "LLM provider added");
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
            tracing::info!(provider = "gemini", model = %model, "LLM provider added");
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
            tracing::info!(provider = "ollama", model = %model, base_url = %base_url, "LLM provider added");
            count += 1;
        }
    }

    if count == 0 {
        tracing::warn!(
            "No LLM providers configured. Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY, or OLLAMA_BASE_URL"
        );
    }

    chain
}

/// Run an interactive chat session via CLI.
///
/// Creates an AgentRunner and dispatches user messages through the
/// full agentic loop with real LLM providers from env vars.
pub async fn run_interactive_chat() {
    println!("GHOST Interactive Chat");
    println!("Type /quit to exit, /help for commands.\n");

    let stdin = io::stdin();
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

    // Configure filesystem tool with current working directory.
    if let Ok(cwd) = std::env::current_dir() {
        runner.tool_executor.set_workspace_root(cwd.clone());
        tracing::info!(workspace = %cwd.display(), "Filesystem tool configured");
    }

    // Build fallback chain from environment variables.
    let mut fallback_chain = build_fallback_chain_from_env();

    let agent_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    loop {
        print!("you> ");
        if let Err(e) = stdout.flush() {
            tracing::warn!(error = %e, "failed to flush stdout");
        }

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            break;
        }

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
