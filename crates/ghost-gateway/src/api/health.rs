//! Health and readiness endpoints.
//!
//! Phase 2b: Health now surfaces convergence monitor connectivity status
//! by reading per-agent convergence state files published by the monitor.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::gateway::GatewayState;
use crate::safety::kill_switch::PLATFORM_KILLED;
use crate::state::AppState;

/// GET /api/health — liveness probe.
///
/// Returns the actual gateway state from `GatewaySharedState`.
/// Returns 503 for non-operational states (Initializing, ShuttingDown, FatalError).
/// Includes convergence monitor connectivity and distributed gate state.
pub async fn health_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let gw_state = state.gateway.current_state();
    let platform_killed = PLATFORM_KILLED.load(std::sync::atomic::Ordering::SeqCst);

    let status_code = match gw_state {
        GatewayState::Healthy | GatewayState::Degraded | GatewayState::Recovering => {
            StatusCode::OK
        }
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };

    // Read convergence monitor connectivity from published state files.
    let monitor_status = read_monitor_status();

    // Read distributed gate state if available.
    let gate_state = state.kill_gate.as_ref().and_then(|gate| {
        let bridge = match gate.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = %e, "Kill gate RwLock poisoned in health check");
                return None;
            }
        };
        let snapshot = bridge.gate.snapshot();
        Some(serde_json::json!({
            "state": format!("{:?}", snapshot.state),
            "node_id": snapshot.node_id.to_string(),
            "closed_at": snapshot.closed_at.map(|t| t.to_rfc3339()),
            "close_reason": snapshot.close_reason,
            "acked_nodes": snapshot.acked_nodes.len(),
            "chain_length": snapshot.chain_length,
        }))
    });

    (
        status_code,
        Json(serde_json::json!({
            "status": if status_code == StatusCode::OK { "alive" } else { "unavailable" },
            "state": format!("{:?}", gw_state),
            "platform_killed": platform_killed,
            "convergence_monitor": monitor_status,
            "distributed_gate": gate_state,
        })),
    )
}

/// GET /api/ready — readiness probe.
///
/// Only returns 200 when the gateway is fully Healthy.
/// Degraded, Recovering, and all other states return 503.
pub async fn ready_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let gw_state = state.gateway.current_state();

    let (status_code, ready) = match gw_state {
        GatewayState::Healthy => (StatusCode::OK, true),
        _ => (StatusCode::SERVICE_UNAVAILABLE, false),
    };

    (
        status_code,
        Json(serde_json::json!({
            "status": if ready { "ready" } else { "not_ready" },
            "state": format!("{:?}", gw_state),
        })),
    )
}

/// Read convergence monitor status from published state files.
///
/// The convergence monitor publishes per-agent state to
/// `~/.ghost/data/convergence_state/{agent_id}.json` via atomic writes.
/// If any files exist and are recent, the monitor is considered connected.
fn read_monitor_status() -> serde_json::Value {
    let state_dir = crate::bootstrap::shellexpand_tilde("~/.ghost/data/convergence_state");
    let state_path = std::path::Path::new(&state_dir);

    if !state_path.exists() {
        return serde_json::json!({
            "connected": false,
            "reason": "convergence_state directory not found",
            "agents": [],
        });
    }

    let mut agent_states = Vec::new();
    let mut newest_update = None;

    if let Ok(entries) = std::fs::read_dir(state_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        match serde_json::from_str::<serde_json::Value>(&content) {
                            Ok(state) => {
                                if let Some(updated_at) = state.get("updated_at").and_then(|v| v.as_str()) {
                                    if newest_update.as_ref().map_or(true, |n: &String| updated_at > n.as_str()) {
                                        newest_update = Some(updated_at.to_string());
                                    }
                                }
                                agent_states.push(serde_json::json!({
                                    "agent_id": state.get("agent_id"),
                                    "score": state.get("score"),
                                    "level": state.get("level"),
                                    "updated_at": state.get("updated_at"),
                                }));
                            }
                            Err(e) => {
                                tracing::debug!(path = %path.display(), error = %e, "skipping malformed convergence state file");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!(path = %path.display(), error = %e, "failed to read convergence state file");
                    }
                }
            }
        }
    } else {
        tracing::debug!("failed to read convergence_state directory");
    }

    // Consider connected if we have state files updated within the last 2 minutes.
    let connected = if let Some(ref ts) = newest_update {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) {
            let age = chrono::Utc::now() - parsed.with_timezone(&chrono::Utc);
            age.num_seconds() < 120
        } else {
            false
        }
    } else {
        false
    };

    serde_json::json!({
        "connected": connected,
        "last_update": newest_update,
        "agent_count": agent_states.len(),
        "agents": agent_states,
    })
}
