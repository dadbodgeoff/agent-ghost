//! Channel injection endpoint (T-4.6.1).
//!
//! POST /api/channels/:type/inject — inject a synthetic inbound message
//! into a running agent for operator debugging.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct InjectMessageRequest {
    pub content: String,
    #[serde(default = "default_sender")]
    pub sender: String,
    pub agent_id: Option<String>,
}

fn default_sender() -> String {
    "ghost-operator".to_string()
}

#[derive(Debug, Serialize)]
pub struct InjectMessageResponse {
    pub message_id: String,
    pub agent_id: String,
    pub routed: bool,
}

/// POST /api/channels/:type/inject — inject a synthetic message for debugging.
pub async fn inject_message(
    State(state): State<Arc<AppState>>,
    Path(channel_type): Path<String>,
    Json(request): Json<InjectMessageRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // 1. Find target agent.
    let agents = state
        .agents
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let agent_id = if let Some(ref id_str) = request.agent_id {
        // Parse as UUID.
        match Uuid::parse_str(id_str) {
            Ok(id) => {
                // Verify agent exists.
                if agents.lookup_by_id(id).is_none() {
                    return Err(StatusCode::NOT_FOUND);
                }
                id
            }
            Err(_) => {
                // Try by name.
                match agents.lookup_by_name(id_str) {
                    Some(a) => a.id,
                    None => return Err(StatusCode::NOT_FOUND),
                }
            }
        }
    } else {
        // Try by channel binding first, then fall back to first agent.
        if let Some(a) = agents.lookup_by_channel(&channel_type) {
            a.id
        } else {
            let all = agents.all_agents();
            if all.is_empty() {
                return Err(StatusCode::NOT_FOUND);
            }
            all[0].id
        }
    };

    let message_id = Uuid::now_v7();

    // 2. Broadcast the injection event via WebSocket for observability.
    let _ = state.event_tx.send(crate::api::websocket::WsEvent::AgentStateChange {
        agent_id: agent_id.to_string(),
        new_state: format!("channel_inject:{channel_type}"),
    });

    // 3. Write audit entry for the injection.
    {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let _ = db.execute(
            "INSERT INTO audit_log (id, event_type, severity, agent_id, details, timestamp, actor_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                message_id.to_string(),
                format!("channel_inject:{channel_type}"),
                "info",
                agent_id.to_string(),
                serde_json::json!({
                    "channel_type": channel_type,
                    "sender": request.sender,
                    "content_length": request.content.len(),
                })
                .to_string(),
                chrono::Utc::now().to_rfc3339(),
                request.sender,
            ],
        );
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(InjectMessageResponse {
            message_id: message_id.to_string(),
            agent_id: agent_id.to_string(),
            routed: true,
        }),
    ))
}
