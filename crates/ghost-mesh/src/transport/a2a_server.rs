//! A2A server: JSON-RPC 2.0 dispatcher for incoming mesh requests.
//!
//! Provides the handler logic for:
//! - `GET /.well-known/agent.json` → serve this agent's signed AgentCard
//! - `POST /a2a` → JSON-RPC 2.0 dispatcher for tasks/send, tasks/get,
//!   tasks/cancel, tasks/sendSubscribe
//!
//! The actual axum route registration happens in ghost-gateway
//! (`src/api/mesh_routes.rs`). This module provides the handler logic
//! that the gateway wires into its router.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::protocol::{error_codes, methods};
use crate::types::{AgentCard, MeshMessage, MeshTask, TaskStatus};

/// A2A server state: holds the local agent card and active tasks.
pub struct A2AServerState {
    /// This agent's signed card.
    pub agent_card: AgentCard,
    /// Active tasks: task_id → MeshTask.
    pub tasks: BTreeMap<Uuid, MeshTask>,
}

impl A2AServerState {
    pub fn new(agent_card: AgentCard) -> Self {
        Self {
            agent_card,
            tasks: BTreeMap::new(),
        }
    }
}

/// A2A JSON-RPC 2.0 dispatcher.
///
/// Routes incoming JSON-RPC requests to the appropriate handler.
pub struct A2ADispatcher {
    state: Arc<Mutex<A2AServerState>>,
}

impl A2ADispatcher {
    pub fn new(state: Arc<Mutex<A2AServerState>>) -> Self {
        Self { state }
    }

    /// Get the agent card (for `GET /.well-known/agent.json`).
    ///
    /// Returns `None` if the state mutex is poisoned.
    pub fn agent_card(&self) -> Option<AgentCard> {
        match self.state.lock() {
            Ok(state) => Some(state.agent_card.clone()),
            Err(e) => {
                tracing::error!(error = %e, "A2A server state mutex poisoned in agent_card()");
                None
            }
        }
    }

    /// Dispatch a JSON-RPC 2.0 request to the appropriate handler.
    pub fn dispatch(&self, msg: &MeshMessage) -> MeshMessage {
        let id = msg.id.clone().unwrap_or(serde_json::json!(null));

        match msg.method.as_str() {
            methods::TASKS_SEND | methods::TASKS_SEND_SUBSCRIBE => self.handle_tasks_send(msg, id),
            methods::TASKS_GET => self.handle_tasks_get(msg, id),
            methods::TASKS_CANCEL => self.handle_tasks_cancel(msg, id),
            _ => MeshMessage::error_response(
                id,
                error_codes::METHOD_NOT_FOUND,
                &format!("unknown method: {}", msg.method),
            ),
        }
    }

    fn handle_tasks_send(&self, msg: &MeshMessage, id: serde_json::Value) -> MeshMessage {
        let params = match &msg.params {
            Some(p) => p,
            None => {
                return MeshMessage::error_response(
                    id,
                    error_codes::INVALID_PARAMS,
                    "missing params",
                )
            }
        };

        let initiator = Uuid::new_v4(); // In production, extracted from auth
        let target = {
            let state = match self.state.lock() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "A2A server state mutex poisoned in tasks/send");
                    return MeshMessage::error_response(
                        id,
                        error_codes::INTERNAL_ERROR,
                        "server state unavailable",
                    );
                }
            };
            // Derive a stable target ID from the agent card's public key hash.
            let hash = blake3::hash(&state.agent_card.public_key);
            let bytes: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap();
            Uuid::from_bytes(bytes)
        };

        let task = MeshTask::new(
            initiator,
            target,
            params.clone(),
            300, // default timeout
        );

        let task_json = match serde_json::to_value(&task) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize task for response");
                return MeshMessage::error_response(
                    id,
                    error_codes::INTERNAL_ERROR,
                    &format!("task serialization failed: {e}"),
                );
            }
        };
        match self.state.lock() {
            Ok(mut state) => {
                state.tasks.insert(task.id, task);
            }
            Err(e) => {
                tracing::error!(error = %e, "A2A server state mutex poisoned inserting task");
                return MeshMessage::error_response(
                    id,
                    error_codes::INTERNAL_ERROR,
                    "server state unavailable",
                );
            }
        }

        MeshMessage::success(id, task_json)
    }

    fn handle_tasks_get(&self, msg: &MeshMessage, id: serde_json::Value) -> MeshMessage {
        let task_id = msg
            .params
            .as_ref()
            .and_then(|p| p.get("task_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        let Some(task_id) = task_id else {
            return MeshMessage::error_response(
                id,
                error_codes::INVALID_PARAMS,
                "missing or invalid task_id",
            );
        };

        let state = match self.state.lock() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "A2A server state mutex poisoned in tasks/get");
                return MeshMessage::error_response(
                    id,
                    error_codes::INTERNAL_ERROR,
                    "server state unavailable",
                );
            }
        };
        match state.tasks.get(&task_id) {
            Some(task) => match serde_json::to_value(task) {
                Ok(task_json) => MeshMessage::success(id, task_json),
                Err(e) => {
                    tracing::error!(task_id = %task_id, error = %e, "failed to serialize task");
                    MeshMessage::error_response(
                        id,
                        error_codes::INTERNAL_ERROR,
                        &format!("task serialization failed: {e}"),
                    )
                }
            },
            None => MeshMessage::error_response(
                id,
                error_codes::TASK_NOT_FOUND,
                &format!("task not found: {task_id}"),
            ),
        }
    }

    fn handle_tasks_cancel(&self, msg: &MeshMessage, id: serde_json::Value) -> MeshMessage {
        let task_id = msg
            .params
            .as_ref()
            .and_then(|p| p.get("task_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        let Some(task_id) = task_id else {
            return MeshMessage::error_response(
                id,
                error_codes::INVALID_PARAMS,
                "missing or invalid task_id",
            );
        };

        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "A2A server state mutex poisoned in tasks/cancel");
                return MeshMessage::error_response(
                    id,
                    error_codes::INTERNAL_ERROR,
                    "server state unavailable",
                );
            }
        };
        match state.tasks.get_mut(&task_id) {
            Some(task) => {
                if task.status.is_terminal() {
                    return MeshMessage::error_response(
                        id,
                        error_codes::TASK_ALREADY_COMPLETED,
                        "task already in terminal state",
                    );
                }
                match task.transition(TaskStatus::Canceled) {
                    Ok(()) => match serde_json::to_value(&*task) {
                        Ok(task_json) => MeshMessage::success(id, task_json),
                        Err(e) => {
                            tracing::error!(task_id = %task_id, error = %e, "failed to serialize canceled task");
                            MeshMessage::error_response(
                                id,
                                error_codes::INTERNAL_ERROR,
                                &format!("task serialization failed: {e}"),
                            )
                        }
                    },
                    Err(e) => {
                        MeshMessage::error_response(id, error_codes::INTERNAL_ERROR, &e.to_string())
                    }
                }
            }
            None => MeshMessage::error_response(
                id,
                error_codes::TASK_NOT_FOUND,
                &format!("task not found: {task_id}"),
            ),
        }
    }
}
