//! Config file watcher for hot-reload (T-3.6.1, T-3.6.2, WP4-B).
//!
//! Watches the convergence profile config file for changes and broadcasts
//! AgentConfigChange WS events to connected clients.
//!
//! WP4-B: Uses the `notify` crate for filesystem events (2s debounce)
//! with SIGHUP handler for manual reload. Falls back to polling if
//! `notify` watcher creation fails.

use std::sync::Arc;

use crate::api::websocket::WsEvent;
use crate::state::AppState;

/// Start the config file watcher background task.
///
/// Watches `GHOST_CONFIG_PATH` (default: `ghost.yml`) for modifications.
/// On change, reloads config and broadcasts WsEvent::AgentConfigChange.
///
/// When using `GatewayRuntime`, prefer `config_watcher_task()` with
/// `runtime.spawn_tracked()` instead of this function.
pub fn spawn_config_watcher(state: Arc<AppState>) {
    tokio::spawn(config_watcher_task(state));
}

/// The config watcher loop as a standalone future.
/// Designed to be wrapped by `GatewayRuntime::spawn_tracked()` which
/// adds cancellation handling.
pub async fn config_watcher_task(state: Arc<AppState>) {
    let config_path = std::env::var("GHOST_CONFIG_PATH").unwrap_or_else(|_| "ghost.yml".into());

    let path = std::path::PathBuf::from(&config_path);
    if !path.exists() {
        tracing::debug!(path = %config_path, "Config file not found — watcher inactive");
        return;
    }

    // Try notify-based watcher first, fall back to polling.
    if let Err(e) = run_notify_watcher(&path, &state).await {
        tracing::warn!(error = %e, "notify watcher failed — falling back to polling");
        run_polling_watcher(&path, &state).await;
    }
}

/// WP4-B: Use the `notify` crate for filesystem event watching with 2s debounce.
/// Also listens for SIGHUP to trigger manual config reload.
async fn run_notify_watcher(
    path: &std::path::Path,
    state: &Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use notify::{Event, EventKind, RecursiveMode, Watcher};

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(4);

    // Create the notify watcher with a debounced callback.
    let tx_clone = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                let _ = tx_clone.try_send(());
            }
        }
    })?;

    // Watch the parent directory (some editors write to a temp file then rename).
    let watch_path = path.parent().unwrap_or(path);
    watcher.watch(watch_path, RecursiveMode::NonRecursive)?;
    tracing::info!(path = %path.display(), "Config watcher started (notify + SIGHUP)");

    // SIGHUP handler for manual reload.
    let tx_sighup = tx.clone();
    #[cfg(unix)]
    {
        let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .map_err(|e| format!("SIGHUP handler: {e}"))?;
        tokio::spawn(async move {
            loop {
                sighup.recv().await;
                tracing::info!("SIGHUP received — triggering config reload");
                let _ = tx_sighup.try_send(());
            }
        });
    }

    // Debounce: wait 2s after last event before processing.
    let mut debounce_deadline: Option<tokio::time::Instant> = None;

    loop {
        tokio::select! {
            Some(()) = rx.recv() => {
                // Reset debounce timer on each event.
                debounce_deadline = Some(tokio::time::Instant::now() + std::time::Duration::from_secs(2));
            }
            _ = async {
                match debounce_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => {
                debounce_deadline = None;
                handle_config_change(path, state);
            }
        }
    }
}

/// Polling fallback: check modification time every 5 seconds.
async fn run_polling_watcher(path: &std::path::Path, state: &Arc<AppState>) {
    let mut last_modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        interval.tick().await;

        let current_modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();

        if current_modified != last_modified && current_modified.is_some() {
            last_modified = current_modified;
            handle_config_change(path, state);
        }
    }
}

/// Validate and broadcast config change.
fn handle_config_change(path: &std::path::Path, state: &Arc<AppState>) {
    let config_path = path.display();
    tracing::info!(path = %config_path, "Config file changed — validating and broadcasting update");

    // Parse and validate the config file before broadcasting.
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(path = %config_path, error = %e, "Failed to read changed config file — skipping reload");
            return;
        }
    };

    // Try to parse as full GhostConfig for secret rotation (WP9-K).
    match serde_yaml::from_str::<crate::config::GhostConfig>(&contents) {
        Ok(new_config) => {
            tracing::info!(path = %config_path, "Config file validated — broadcasting change");

            // WP9-K: Rotate provider API keys without restart.
            rotate_provider_keys(&new_config.models.providers);
        }
        Err(_) => {
            // Fall back to basic YAML validation.
            if serde_yaml::from_str::<serde_json::Value>(&contents).is_err() {
                tracing::warn!(path = %config_path, "Config file changed but is not valid YAML — skipping reload");
                return;
            }
            tracing::info!(path = %config_path, "Config file validated (partial parse) — broadcasting change");
        }
    }

    // T-5.3.9: Send a single ConfigReloaded event instead of one per agent.
    crate::api::websocket::broadcast_event(
        state,
        WsEvent::AgentConfigChange {
            agent_id: "system".to_string(),
            changed_fields: vec!["config_reloaded".into()],
        },
    );
}

/// WP9-K: Re-read provider API keys from environment and atomically swap them
/// in the thread-safe key store. This allows key rotation via:
///   1. Update the env var (e.g. ANTHROPIC_API_KEY)
///   2. Send SIGHUP or modify config file
fn rotate_provider_keys(providers: &[crate::config::ProviderConfig]) {
    for pc in providers {
        let key_env = match pc.api_key_env.as_deref() {
            Some(env) => env,
            None => match pc.name.as_str() {
                "anthropic" => "ANTHROPIC_API_KEY",
                "openai" | "openai_compat" => "OPENAI_API_KEY",
                "gemini" => "GEMINI_API_KEY",
                _ => continue,
            },
        };

        // Re-read from environment (the source of truth for rotation).
        if let Ok(new_key) = std::env::var(key_env) {
            if !new_key.is_empty() {
                let current = crate::state::get_api_key(key_env);
                if current.as_deref() != Some(&new_key) {
                    crate::state::set_api_key(key_env, &new_key);
                    tracing::info!(
                        provider = %pc.name,
                        key_env = %key_env,
                        "API key rotated (env var changed)"
                    );
                }
            }
        }
    }
}
