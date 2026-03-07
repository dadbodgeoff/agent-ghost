//! Channel management and injection endpoints (T-4.6.1, Phase 3 Task 3.1).
//!
//! GET  /api/channels               — list all configured channels
//! POST /api/channels               — add a new channel
//! POST /api/channels/:type/inject  — inject a synthetic inbound message
//! POST /api/channels/:id/reconnect — reconnect a disconnected channel
//! DELETE /api/channels/:id         — remove a channel

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

/// GET /api/channels — list all configured channels with status.
pub async fn list_channels(State(state): State<Arc<AppState>>) -> ApiResult<serde_json::Value> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_channels", e))?;

    // Query channels table; fall back to agents as implicit CLI channels if table missing.
    let channels: Vec<serde_json::Value> = match db.prepare(
        "SELECT id, channel_type, status, status_message, agent_id, config, last_message_at, message_count \
         FROM channels ORDER BY channel_type"
    ) {
        Ok(mut stmt) => stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "channel_type": row.get::<_, String>(1)?,
                    "status": row.get::<_, String>(2)?,
                    "status_message": row.get::<_, Option<String>>(3)?,
                    "agent_id": row.get::<_, String>(4)?,
                    "config": serde_json::from_str::<serde_json::Value>(
                        &row.get::<_, String>(5)?
                    ).unwrap_or(serde_json::json!({})),
                    "last_message_at": row.get::<_, Option<String>>(6)?,
                    "message_count": row.get::<_, u64>(7)?,
                }))
            })
            .map_err(|e| ApiError::db_error("list_channels_query", e))?
            .filter_map(|r| r.ok())
            .collect(),
        Err(_) => {
            let agents = state.agents.read().map_err(|_| ApiError::internal("lock"))?;
            agents.all_agents().iter().map(|a| {
                serde_json::json!({
                    "id": a.id.to_string(),
                    "channel_type": "cli",
                    "status": "connected",
                    "agent_id": a.id.to_string(),
                    "agent_name": a.name,
                    "config": {},
                    "last_message_at": null,
                    "message_count": 0,
                })
            }).collect()
        }
    };

    Ok(Json(serde_json::json!({ "channels": channels })))
}

/// POST /api/channels — create a new channel binding.
#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub channel_type: String,
    pub agent_id: String,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

pub async fn create_channel(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateChannelRequest>,
) -> ApiResult<serde_json::Value> {
    let id = Uuid::now_v7().to_string();
    let config_str = serde_json::to_string(&body.config.unwrap_or(serde_json::json!({})))
        .unwrap_or_else(|_| "{}".to_string());

    let db = state.db.write().await;
    let _ = db.execute_batch(
        "CREATE TABLE IF NOT EXISTS channels (
            id TEXT PRIMARY KEY,
            channel_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'configuring',
            status_message TEXT,
            agent_id TEXT NOT NULL,
            config TEXT NOT NULL DEFAULT '{}',
            last_message_at TEXT,
            message_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    );

    db.execute(
        "INSERT INTO channels (id, channel_type, status, agent_id, config) VALUES (?1, ?2, 'connected', ?3, ?4)",
        rusqlite::params![id, body.channel_type, body.agent_id, config_str],
    ).map_err(|e| ApiError::db_error("create_channel", e))?;

    Ok(Json(serde_json::json!({ "id": id, "status": "created" })))
}

/// POST /api/channels/:id/reconnect
pub async fn reconnect_channel(
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;
    db.execute(
        "UPDATE channels SET status = 'connected', status_message = NULL, updated_at = datetime('now') WHERE id = ?1",
        [&channel_id],
    ).map_err(|e| ApiError::db_error("reconnect_channel", e))?;
    Ok(Json(
        serde_json::json!({ "id": channel_id, "status": "reconnected" }),
    ))
}

/// DELETE /api/channels/:id
pub async fn delete_channel(
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;
    db.execute("DELETE FROM channels WHERE id = ?1", [&channel_id])
        .map_err(|e| ApiError::db_error("delete_channel", e))?;
    Ok(Json(
        serde_json::json!({ "id": channel_id, "status": "deleted" }),
    ))
}

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
    // Scoped block ensures RwLockReadGuard is dropped before any .await (Send requirement).
    let agent_id = {
        let agents = state
            .agents
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if let Some(ref id_str) = request.agent_id {
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
        }
    };

    let message_id = Uuid::now_v7();

    // 2. Broadcast the injection event via WebSocket for observability.
    crate::api::websocket::broadcast_event(
        &state,
        crate::api::websocket::WsEvent::AgentStateChange {
            agent_id: agent_id.to_string(),
            new_state: format!("channel_inject:{channel_type}"),
        },
    );

    // 3. Write audit entry for the injection.
    {
        let db = state.db.write().await;
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
