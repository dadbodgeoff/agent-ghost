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

    // Read convergence monitor liveness from in-memory atomic — O(1), no lock, no disk I/O.
    let monitor_connected = state.monitor_healthy.load(std::sync::atomic::Ordering::Relaxed);
    let monitor_status = serde_json::json!({ "connected": monitor_connected });

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

