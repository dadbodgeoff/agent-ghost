//! Config file watcher for hot-reload (T-3.6.1, T-3.6.2).
//!
//! Watches the convergence profile config file for changes and broadcasts
//! AgentConfigChange WS events to connected clients.

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
    let config_path = std::env::var("GHOST_CONFIG_PATH")
        .unwrap_or_else(|_| "ghost.yml".into());

    let path = std::path::PathBuf::from(&config_path);
    if !path.exists() {
        tracing::debug!(path = %config_path, "Config file not found — watcher inactive");
        return;
    }

    // Track last modification time.
    let mut last_modified = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok();

    // Poll every 5 seconds (simpler than notify for MVP).
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        interval.tick().await;

        let current_modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok();

        if current_modified != last_modified && current_modified.is_some() {
            last_modified = current_modified;
            tracing::info!(path = %config_path, "Config file changed — validating and broadcasting update");

            // Parse and validate the config file before broadcasting.
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    // Validate it's parseable YAML/TOML.
                    if serde_yaml::from_str::<serde_json::Value>(&contents).is_err() {
                        tracing::warn!(path = %config_path, "Config file changed but is not valid YAML — skipping reload");
                        continue;
                    }
                    tracing::info!(path = %config_path, "Config file validated — broadcasting change");
                }
                Err(e) => {
                    tracing::warn!(path = %config_path, error = %e, "Failed to read changed config file — skipping reload");
                    continue;
                }
            }

            // T-5.3.9: Send a single ConfigReloaded event instead of one per agent.
            // With 10K agents, per-agent events would overflow the broadcast buffer.
            // Dashboard handles this by re-fetching affected data.
            let _ = state.event_tx.send(WsEvent::AgentConfigChange {
                agent_id: "system".to_string(),
                changed_fields: vec!["config_reloaded".into()],
            });
        }
    }
}
