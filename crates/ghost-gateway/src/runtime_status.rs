use std::sync::{Arc, RwLock};

use ghost_agent_loop::runner::{inspect_convergence_shared_state, ConvergenceHealthState};
use serde_json::json;
use uuid::Uuid;

use crate::safety::kill_gate_bridge::KillGateBridge;

pub fn should_enable_distributed_kill(mesh_enabled: bool, distributed_kill_enabled: bool) -> bool {
    mesh_enabled && distributed_kill_enabled
}

pub fn distributed_kill_status_value(
    distributed_kill_enabled: bool,
    kill_gate: Option<&Arc<RwLock<KillGateBridge>>>,
) -> serde_json::Value {
    if !distributed_kill_enabled {
        return json!({
            "enabled": false,
            "status": "gated",
            "authoritative": false,
            "reason": "distributed kill is feature-gated for this remediation milestone",
        });
    }

    match kill_gate {
        Some(gate) => match gate.read() {
            Ok(guard) => {
                let snapshot = guard.gate.snapshot();
                json!({
                    "enabled": true,
                    "status": format!("{:?}", snapshot.state),
                    "authoritative": false,
                    "node_id": snapshot.node_id.to_string(),
                    "closed_at": snapshot.closed_at.map(|t| t.to_rfc3339()),
                    "close_reason": snapshot.close_reason,
                    "acked_nodes": snapshot.acked_nodes.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "chain_length": snapshot.chain_length,
                })
            }
            Err(error) => json!({
                "enabled": true,
                "status": "poisoned",
                "authoritative": false,
                "error": format!("kill gate lock poisoned: {error}"),
            }),
        },
        None => json!({
            "enabled": true,
            "status": "unavailable",
            "authoritative": false,
            "reason": "distributed kill enabled but no gate bridge is active",
        }),
    }
}

pub fn convergence_protection_summary_value(
    agent_ids: impl IntoIterator<Item = Uuid>,
    monitor_enabled: bool,
    block_on_degraded: bool,
    stale_after: std::time::Duration,
) -> serde_json::Value {
    if !monitor_enabled {
        return json!({
            "execution_mode": "disabled",
            "stale_after_secs": stale_after.as_secs(),
            "agents": {
                "healthy": 0,
                "missing": 0,
                "stale": 0,
                "corrupted": 0,
            }
        });
    }

    let mut healthy = 0usize;
    let mut missing = 0usize;
    let mut stale = 0usize;
    let mut corrupted = 0usize;

    for agent_id in agent_ids {
        match inspect_convergence_shared_state(agent_id, monitor_enabled, stale_after).status {
            ConvergenceHealthState::Healthy => healthy += 1,
            ConvergenceHealthState::Missing => missing += 1,
            ConvergenceHealthState::Stale => stale += 1,
            ConvergenceHealthState::Corrupted => corrupted += 1,
            ConvergenceHealthState::Disabled => {}
        }
    }

    json!({
        "execution_mode": if block_on_degraded { "block" } else { "allow" },
        "stale_after_secs": stale_after.as_secs(),
        "agents": {
            "healthy": healthy,
            "missing": missing,
            "stale": stale,
            "corrupted": corrupted,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distributed_kill_disabled_by_default_when_not_fully_implemented() {
        assert!(!should_enable_distributed_kill(true, false));
        assert!(!should_enable_distributed_kill(false, false));
    }

    #[test]
    fn distributed_kill_status_surface_honest_when_gated() {
        let status = distributed_kill_status_value(false, None);

        assert_eq!(status["enabled"], false);
        assert_eq!(status["status"], "gated");
        assert_eq!(status["authoritative"], false);
    }

    #[test]
    fn partial_distributed_kill_path_not_active_in_production_mode() {
        assert!(!should_enable_distributed_kill(true, false));
    }
}
