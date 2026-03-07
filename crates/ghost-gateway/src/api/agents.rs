//! Agent API endpoints.
//!
//! Phase 2b: Added create/delete endpoints with identity + keypair management.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::agents::registry::{AgentLifecycleState, RegisteredAgent};
use crate::api::websocket::WsEvent;
use crate::state::AppState;

#[derive(Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub spending_cap: f64,
}

/// GET /api/agents — returns the live agent registry.
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AgentInfo>>, StatusCode> {
    let agents = match state.agents.read() {
        Ok(guard) => guard,
        Err(e) => {
            tracing::error!(error = %e, "Agent registry RwLock poisoned in list_agents");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let list: Vec<AgentInfo> = agents
        .all_agents()
        .iter()
        .map(|a| AgentInfo {
            id: a.id.to_string(),
            name: a.name.clone(),
            status: format!("{:?}", a.state),
            spending_cap: a.spending_cap,
        })
        .collect();
    Ok(Json(list))
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub spending_cap: Option<f64>,
    pub capabilities: Option<Vec<String>>,
    pub generate_keypair: Option<bool>,
}

/// POST /api/agents — create a new agent with optional keypair generation.
///
/// Registers the agent in the live registry and optionally generates
/// an Ed25519 keypair via ghost-identity's AgentKeypairManager.
pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    // Check for duplicate name.
    {
        let agents = match state.agents.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = %e, "Agent registry RwLock poisoned in create_agent");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                );
            }
        };
        if agents.lookup_by_name(&body.name).is_some() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "agent with this name already exists",
                    "name": body.name,
                })),
            );
        }
    }

    let agent_id = crate::agents::registry::durable_agent_id(&body.name);
    let spending_cap = body.spending_cap.unwrap_or(5.0);

    // Generate keypair if requested.
    let mut has_keypair = false;
    if body.generate_keypair.unwrap_or(true) {
        let keys_dir_str = crate::bootstrap::shellexpand_tilde(
            &format!("~/.ghost/agents/{}/keys", body.name),
        );
        let keys_dir = std::path::PathBuf::from(&keys_dir_str);
        let mut kpm = ghost_identity::keypair_manager::AgentKeypairManager::new(keys_dir);
        match kpm.generate() {
            Ok(_vk) => {
                has_keypair = true;
                tracing::info!(agent = %body.name, "Ed25519 keypair generated");
            }
            Err(e) => {
                tracing::warn!(agent = %body.name, error = %e, "Keypair generation failed");
            }
        }
    }

    let registered = RegisteredAgent {
        id: agent_id,
        name: body.name.clone(),
        state: AgentLifecycleState::Starting,
        channel_bindings: Vec::new(),
        capabilities: body.capabilities.unwrap_or_default(),
        spending_cap,
        template: None,
    };

    {
        let mut agents = match state.agents.write() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = %e, "Agent registry RwLock poisoned in create_agent (write)");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                );
            }
        };
        agents.register(registered);
    }

    // Broadcast agent creation event.
    crate::api::websocket::broadcast_event(&state, WsEvent::AgentStateChange {
        agent_id: agent_id.to_string(),
        new_state: "Starting".into(),
    });

    tracing::info!(
        agent_id = %agent_id,
        name = %body.name,
        spending_cap = spending_cap,
        "Agent created via API"
    );

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": agent_id.to_string(),
            "name": body.name,
            "status": "Starting",
            "spending_cap": spending_cap,
            "has_keypair": has_keypair,
        })),
    )
}

/// DELETE /api/agents/:id — remove an agent from the registry.
///
/// Transitions the agent to Stopping → Stopped, then unregisters it.
/// Refuses to delete agents that are quarantined (must resume first).
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
) -> impl IntoResponse {
    let agent_id = match uuid::Uuid::parse_str(&agent_id_str) {
        Ok(id) => id,
        Err(_) => {
            // Try lookup by name.
            let agents = match state.agents.read() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!(error = %e, "Agent registry RwLock poisoned in delete_agent");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal server error"})),
                    ).into_response();
                }
            };
            match agents.lookup_by_name(&agent_id_str) {
                Some(a) => a.id,
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error": "agent not found", "id": agent_id_str})),
                    ).into_response();
                }
            }
        }
    };

    // Check kill state — refuse to delete quarantined agents.
    let ks_state = state.kill_switch.current_state();
    if let Some(agent_ks) = ks_state.per_agent.get(&agent_id) {
        if agent_ks.level == crate::safety::kill_switch::KillLevel::Quarantine {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "cannot delete quarantined agent — resume first",
                    "id": agent_id.to_string(),
                })),
            ).into_response();
        }
    }

    let mut agents = match state.agents.write() {
        Ok(guard) => guard,
        Err(e) => {
            tracing::error!(error = %e, "Agent registry RwLock poisoned in delete_agent (write)");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            ).into_response();
        }
    };
    match agents.unregister(agent_id) {
        Some(agent) => {
            crate::api::websocket::broadcast_event(&state, WsEvent::AgentStateChange {
                agent_id: agent_id.to_string(),
                new_state: "Stopped".into(),
            });

            tracing::info!(
                agent_id = %agent_id,
                name = %agent.name,
                "Agent deleted via API"
            );

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "deleted",
                    "id": agent_id.to_string(),
                    "name": agent.name,
                })),
            ).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found", "id": agent_id.to_string()})),
        ).into_response(),
    }
}
