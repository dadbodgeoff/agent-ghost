//! Health and readiness endpoints.
//!
//! Phase 2b: Health now surfaces convergence monitor connectivity status
//! by reading per-agent convergence state files published by the monitor.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::gateway::GatewayState;
use crate::safety::kill_switch::PLATFORM_KILLED;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthMonitorStatus {
    pub enabled: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub state: String,
    pub platform_killed: bool,
    pub convergence_monitor: HealthMonitorStatus,
    pub convergence_protection: crate::api::observability::AdeConvergenceProtectionSnapshot,
    pub distributed_kill: crate::api::observability::AdeDistributedKillSnapshot,
    pub autonomy: crate::autonomy::AutonomyStatusResponse,
    pub speculative_context: crate::speculative_context::SpeculativeContextStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadyResponse {
    pub status: String,
    pub state: String,
}

/// GET /api/health — liveness probe.
///
/// Returns the actual gateway state from `GatewaySharedState`.
/// Returns 503 for non-operational states (Initializing, ShuttingDown, FatalError).
/// Includes convergence monitor connectivity and distributed gate state.
pub async fn health_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let gw_state = state.gateway.current_state();
    let platform_killed = PLATFORM_KILLED.load(std::sync::atomic::Ordering::SeqCst);

    let status_code = match gw_state {
        GatewayState::Healthy | GatewayState::Degraded | GatewayState::Recovering => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };

    // Read convergence monitor liveness from in-memory atomic — O(1), no lock, no disk I/O.
    let monitor_connected = state
        .monitor_healthy
        .load(std::sync::atomic::Ordering::Relaxed);
    let monitor_status = HealthMonitorStatus {
        enabled: state.monitor_enabled,
        connected: monitor_connected,
    };
    let agent_ids = state
        .agents
        .read()
        .map(|agents| {
            agents
                .all_agents()
                .iter()
                .map(|agent| agent.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let convergence_protection = crate::api::observability::convergence_protection_snapshot(
        agent_ids,
        state.monitor_enabled,
        state.monitor_block_on_degraded,
        state.convergence_state_stale_after,
    );
    let distributed_kill = crate::api::observability::distributed_kill_snapshot(
        state.distributed_kill_enabled,
        state.kill_gate.as_ref(),
    );
    let autonomy = state.autonomy.status(&state).await;
    let speculative_context = crate::speculative_context::status(&state).await;

    (
        status_code,
        Json(HealthResponse {
            status: if status_code == StatusCode::OK {
                "alive".into()
            } else {
                "unavailable".into()
            },
            state: format!("{gw_state:?}"),
            platform_killed,
            convergence_monitor: monitor_status,
            convergence_protection,
            distributed_kill,
            autonomy,
            speculative_context,
        }),
    )
}

/// GET /api/ready — readiness probe.
///
/// Only returns 200 when the gateway is fully Healthy.
/// Degraded, Recovering, and all other states return 503.
pub async fn ready_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let gw_state = state.gateway.current_state();

    let (status_code, ready) = match gw_state {
        GatewayState::Healthy => (StatusCode::OK, true),
        _ => (StatusCode::SERVICE_UNAVAILABLE, false),
    };

    (
        status_code,
        Json(ReadyResponse {
            status: if ready {
                "ready".into()
            } else {
                "not_ready".into()
            },
            state: format!("{gw_state:?}"),
        }),
    )
}
