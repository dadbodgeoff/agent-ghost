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
use axum::response::Response;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::state::AppState;

const CREATE_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels";
const RECONNECT_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels/:id/reconnect";
const DELETE_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels/:id";
const INJECT_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels/:type/inject";

fn load_channels(conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>, ApiError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, channel_type, status, status_message, agent_id, config, last_message_at, message_count \
             FROM channels ORDER BY channel_type",
        )
        .map_err(|e| ApiError::db_error("list_channels_prepare", e))?;

    let rows = stmt
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
        .map_err(|e| ApiError::db_error("list_channels_query", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::db_error("list_channels_row", e))
}

/// GET /api/channels — list all configured channels with status.
pub async fn list_channels(State(state): State<Arc<AppState>>) -> ApiResult<serde_json::Value> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_channels", e))?;
    let channels = load_channels(&db)?;

    Ok(Json(serde_json::json!({ "channels": channels })))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateChannelRequest {
    pub channel_type: String,
    pub agent_id: String,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
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

fn channel_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

fn resolve_agent_id(state: &AppState, requested: &str) -> Result<Uuid, ApiError> {
    if let Ok(id) = Uuid::parse_str(requested) {
        if let Ok(agents) = state.agents.read() {
            if agents.lookup_by_id(id).is_some() {
                return Ok(id);
            }
        }

        if let Ok(db) = state.db.read() {
            let exists = db
                .query_row(
                    "SELECT COUNT(*) FROM agents WHERE id = ?1",
                    [requested],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .unwrap_or(false);
            if exists {
                return Ok(id);
            }
        }

        return Err(ApiError::not_found(format!("agent {requested} not found")));
    }

    if let Ok(agents) = state.agents.read() {
        if let Some(agent) = agents.lookup_by_name(requested) {
            return Ok(agent.id);
        }
    }

    if let Ok(db) = state.db.read() {
        let found: Option<String> = db
            .query_row(
                "SELECT id FROM agents WHERE name = ?1 LIMIT 1",
                [requested],
                |row| row.get(0),
            )
            .ok();
        if let Some(id) = found {
            return Uuid::parse_str(&id)
                .map_err(|e| ApiError::internal(format!("stored agent id is invalid: {e}")));
        }
    }

    Err(ApiError::not_found(format!("agent {requested} not found")))
}

fn resolve_injection_target(
    state: &AppState,
    channel_type: &str,
    requested_agent: Option<&str>,
) -> Result<Uuid, ApiError> {
    let agents = state
        .agents
        .read()
        .map_err(|_| ApiError::internal("lock"))?;

    if let Some(id_str) = requested_agent {
        return match Uuid::parse_str(id_str) {
            Ok(id) => {
                if agents.lookup_by_id(id).is_some() {
                    Ok(id)
                } else {
                    Err(ApiError::not_found(format!("agent {id} not found")))
                }
            }
            Err(_) => match agents.lookup_by_name(id_str) {
                Some(agent) => Ok(agent.id),
                None => Err(ApiError::not_found(format!("agent {id_str} not found"))),
            },
        };
    }

    if let Some(agent) = agents.lookup_by_channel(channel_type) {
        return Ok(agent.id);
    }

    let all = agents.all_agents();
    if let Some(agent) = all.first() {
        return Ok(agent.id);
    }

    Err(ApiError::not_found(
        "no agent available for channel injection",
    ))
}

/// POST /api/channels — create a new channel binding.
pub async fn create_channel(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<CreateChannelRequest>,
) -> Response {
    if body.channel_type.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request("channel_type is required"));
    }
    if body.agent_id.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request("agent_id is required"));
    }

    let actor = channel_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::to_value(&body).unwrap_or(serde_json::Value::Null);
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_CHANNEL_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let channel_id = operation_context
                .operation_id
                .clone()
                .unwrap_or_else(|| Uuid::now_v7().to_string());
            let resolved_agent_id = resolve_agent_id(&state, &body.agent_id)?;
            let config_str =
                serde_json::to_string(&body.config.clone().unwrap_or(serde_json::json!({})))
                    .unwrap_or_else(|_| "{}".to_string());

            conn.execute(
                "INSERT INTO channels (id, channel_type, status, agent_id, config) VALUES (?1, ?2, 'connected', ?3, ?4)",
                rusqlite::params![channel_id, body.channel_type, resolved_agent_id.to_string(), config_str],
            )
            .map_err(|e| ApiError::db_error("create_channel", e))?;

            Ok((
                StatusCode::CREATED,
                serde_json::json!({
                    "id": channel_id,
                    "status": "created",
                    "channel_type": body.channel_type,
                    "agent_id": resolved_agent_id.to_string(),
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                outcome.body["agent_id"].as_str().unwrap_or("platform"),
                "create_channel",
                "medium",
                actor,
                "created",
                serde_json::json!({
                    "channel_id": outcome.body["id"],
                    "channel_type": outcome.body["channel_type"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/channels/:id/reconnect
pub async fn reconnect_channel(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(channel_id): Path<String>,
) -> Response {
    let actor = channel_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "channel_id": channel_id });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        RECONNECT_CHANNEL_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let affected = conn
                .execute(
                    "UPDATE channels SET status = 'connected', status_message = NULL, updated_at = datetime('now') WHERE id = ?1",
                    [&channel_id],
                )
                .map_err(|e| ApiError::db_error("reconnect_channel", e))?;
            if affected == 0 {
                return Err(ApiError::not_found(format!(
                    "channel {channel_id} not found"
                )));
            }
            Ok((
                StatusCode::OK,
                serde_json::json!({ "id": channel_id, "status": "reconnected" }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "reconnect_channel",
                "medium",
                actor,
                "reconnected",
                serde_json::json!({ "channel_id": outcome.body["id"] }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// DELETE /api/channels/:id
pub async fn delete_channel(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(channel_id): Path<String>,
) -> Response {
    let actor = channel_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "channel_id": channel_id });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "DELETE",
        DELETE_CHANNEL_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let affected = conn
                .execute("DELETE FROM channels WHERE id = ?1", [&channel_id])
                .map_err(|e| ApiError::db_error("delete_channel", e))?;
            if affected == 0 {
                return Err(ApiError::not_found(format!(
                    "channel {channel_id} not found"
                )));
            }
            Ok((
                StatusCode::OK,
                serde_json::json!({ "id": channel_id, "status": "deleted" }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "delete_channel",
                "medium",
                actor,
                "deleted",
                serde_json::json!({ "channel_id": outcome.body["id"] }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/channels/:type/inject — inject a synthetic message for debugging.
pub async fn inject_message(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(channel_type): Path<String>,
    Json(request): Json<InjectMessageRequest>,
) -> Result<Response, StatusCode> {
    let actor = claims
        .as_ref()
        .map(|claims| claims.0.sub.as_str())
        .unwrap_or(request.sender.as_str());
    let request_body = serde_json::to_value(&request).unwrap_or(serde_json::Value::Null);
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        INJECT_CHANNEL_ROUTE_TEMPLATE,
        &serde_json::json!({
            "channel_type": channel_type,
            "request": request_body,
        }),
        |_conn| {
            let agent_id =
                resolve_injection_target(&state, &channel_type, request.agent_id.as_deref())?;
            Ok((
                StatusCode::ACCEPTED,
                serde_json::json!(InjectMessageResponse {
                    message_id: operation_context
                        .operation_id
                        .clone()
                        .unwrap_or_else(|| Uuid::now_v7().to_string()),
                    agent_id: agent_id.to_string(),
                    routed: true,
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            if outcome.idempotency_status == IdempotencyStatus::Executed {
                crate::api::websocket::broadcast_event(
                    &state,
                    crate::api::websocket::WsEvent::AgentStateChange {
                        agent_id: outcome.body["agent_id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        new_state: format!("channel_inject:{channel_type}"),
                    },
                );
            }

            write_mutation_audit_entry(
                &db,
                outcome.body["agent_id"].as_str().unwrap_or("platform"),
                "inject_channel_message",
                "info",
                actor,
                "accepted",
                serde_json::json!({
                    "channel_type": channel_type,
                    "sender": request.sender,
                    "content_length": request.content.len(),
                    "message_id": outcome.body["message_id"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );

            Ok(json_response_with_idempotency(
                outcome.status,
                outcome.body,
                outcome.idempotency_status,
            ))
        }
        Err(error) => Ok(error_response_with_idempotency(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_channels_fails_on_malformed_rows() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO channels (id, channel_type, status, agent_id, config, message_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "channel-1",
                "slack",
                "connected",
                "agent-1",
                "{}",
                "not-an-integer"
            ],
        )
        .unwrap();

        let error = load_channels(&conn).unwrap_err();
        assert!(error.to_string().contains("list_channels_row"), "{error}");
    }
}
