//! Convergence score watcher — polls for new scores and broadcasts
//! WsEvent::ScoreUpdate and WsEvent::InterventionChange (Findings #13, #14).
//!
//! Runs as a background task, checking for score changes every 5 seconds.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::api::websocket::WsEvent;
use crate::state::AppState;

/// Previous score state for change detection.
struct PreviousScore {
    score: f64,
    level: u8,
}

/// Start the convergence watcher background task.
/// Polls the DB for new convergence scores and broadcasts WsEvents.
///
/// When using `GatewayRuntime`, prefer `convergence_watcher_task()` with
/// `runtime.spawn_tracked()` instead of this function.
pub fn spawn_convergence_watcher(state: Arc<AppState>) {
    tokio::spawn(convergence_watcher_task(state));
}

/// The convergence watcher loop as a standalone future.
/// Designed to be wrapped by `GatewayRuntime::spawn_tracked()` which
/// adds cancellation handling.
pub async fn convergence_watcher_task(state: Arc<AppState>) {
    let mut previous: BTreeMap<String, PreviousScore> = BTreeMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        interval.tick().await;
        let _span = tracing::info_span!("convergence_watcher_poll").entered();

        let agent_ids: Vec<(String, String)> = {
            let agents = match state.agents.read() {
                Ok(a) => a,
                Err(_) => continue,
            };
            agents
                .all_agents()
                .iter()
                .map(|a| (a.id.to_string(), a.name.clone()))
                .collect()
        };

        let db = match state.db.read() {
            Ok(db) => db,
            Err(_) => continue,
        };

        for (agent_id, _agent_name) in &agent_ids {
            let row = match cortex_storage::queries::convergence_score_queries::latest_by_agent(
                &db, agent_id,
            ) {
                Ok(Some(row)) => row,
                _ => continue,
            };

            let new_level = row.level as u8;
            let new_score = row.composite_score;
            let signals: Vec<f64> = match serde_json::from_str(&row.signal_scores) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        agent_id = %agent_id,
                        error = %e,
                        raw = %row.signal_scores,
                        "Malformed signal_scores JSON in convergence_scores — using empty"
                    );
                    Vec::new()
                }
            };

            let changed = match previous.get(agent_id) {
                Some(prev) => {
                    (prev.score - new_score).abs() > f64::EPSILON
                        || prev.level != new_level
                }
                None => true,
            };

            if changed {
                let old_level = previous
                    .get(agent_id)
                    .map(|p| p.level)
                    .unwrap_or(0);

                // Broadcast ScoreUpdate (Finding #13).
                crate::api::websocket::broadcast_event(&state, WsEvent::ScoreUpdate {
                    agent_id: agent_id.clone(),
                    score: new_score,
                    level: new_level,
                    signals: signals.clone(),
                });

                // Broadcast InterventionChange if level changed (Finding #14).
                if old_level != new_level {
                    crate::api::websocket::broadcast_event(&state, WsEvent::InterventionChange {
                        agent_id: agent_id.clone(),
                        old_level,
                        new_level,
                    });
                }

                previous.insert(
                    agent_id.clone(),
                    PreviousScore {
                        score: new_score,
                        level: new_level,
                    },
                );
            }
        }
    }
}
