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
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::channel_manager::{ChannelManager, ChannelRecord};
use crate::state::AppState;

const CREATE_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels";
const RECONNECT_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels/:id/reconnect";
const DELETE_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels/:id";
const INJECT_CHANNEL_ROUTE_TEMPLATE: &str = "/api/channels/:type/inject";

#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelListItem {
    pub id: String,
    pub channel_type: String,
    pub status: String,
    pub status_message: Option<String>,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub routing_key: String,
    pub source: String,
    pub config: serde_json::Value,
    pub last_message_at: Option<String>,
    pub message_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelListResponse {
    pub channels: Vec<ChannelListItem>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateChannelRequest {
    pub channel_type: String,
    pub agent_id: String,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateChannelResponse {
    pub id: String,
    pub status: String,
    pub channel_type: String,
    pub agent_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelStatusResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct InjectMessageRequest {
    pub content: String,
    #[serde(default = "default_sender")]
    pub sender: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InjectMessageResponse {
    pub message_id: String,
    pub agent_id: String,
    pub routed: bool,
}

struct MutationSuccess {
    status: StatusCode,
    body: serde_json::Value,
    idempotency_status: IdempotencyStatus,
}

fn default_sender() -> String {
    "ghost-operator".to_string()
}

fn channel_actor(claims: Option<&Claims>) -> String {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
        .to_string()
}

fn agent_name_for(state: &AppState, agent_id: &str) -> Option<String> {
    let parsed = Uuid::parse_str(agent_id).ok()?;
    let agents = state.agents.read().ok()?;
    agents.lookup_by_id(parsed).map(|agent| agent.name.clone())
}

fn to_channel_list_item(state: &AppState, channel: ChannelRecord) -> ChannelListItem {
    ChannelListItem {
        id: channel.id,
        channel_type: channel.channel_type,
        status: channel.status,
        status_message: channel.status_message,
        agent_id: channel.agent_id.clone(),
        agent_name: agent_name_for(state, &channel.agent_id),
        routing_key: channel.routing_key,
        source: channel.source,
        config: channel.config,
        last_message_at: channel.last_message_at,
        message_count: channel.message_count,
    }
}

fn load_channels(state: &AppState) -> Result<Vec<ChannelListItem>, ApiError> {
    state
        .channel_manager
        .load_channels()
        .map(|channels| {
            channels
                .into_iter()
                .map(|channel| to_channel_list_item(state, channel))
                .collect()
        })
        .map_err(ApiError::internal)
}

/// GET /api/channels — list all configured channels with status.
pub async fn list_channels(State(state): State<Arc<AppState>>) -> ApiResult<ChannelListResponse> {
    let channels = load_channels(&state)?;
    Ok(Json(ChannelListResponse { channels }))
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
    if let Some(requested_agent) = requested_agent {
        return resolve_agent_id(state, requested_agent);
    }

    let matching = state
        .channel_manager
        .load_channels()
        .map_err(ApiError::internal)?
        .into_iter()
        .filter(|channel| channel.channel_type == channel_type)
        .collect::<Vec<_>>();

    match matching.as_slice() {
        [] => Err(ApiError::not_found(format!(
            "no channel configured for type {channel_type}"
        ))),
        [channel] => Uuid::parse_str(&channel.agent_id)
            .map_err(|error| ApiError::internal(format!("stored agent id is invalid: {error}"))),
        _ => Err(ApiError::bad_request(format!(
            "multiple channels of type {channel_type} exist; specify agent_id explicitly"
        ))),
    }
}

fn map_channel_insert_error(error: rusqlite::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("channels.routing_key") {
        ApiError::bad_request("channel routing key already exists")
    } else {
        ApiError::db_error("create_channel", error)
    }
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
    let resolved_agent_id = match resolve_agent_id(&state, &body.agent_id) {
        Ok(agent_id) => agent_id,
        Err(error) => return error_response_with_idempotency(error),
    };
    let config_json = ChannelManager::normalize_config(body.config.clone());
    let routing_key = match ChannelManager::derive_routing_key(
        &body.channel_type,
        &resolved_agent_id.to_string(),
        &config_json,
    ) {
        Ok(routing_key) => routing_key,
        Err(error) => return error_response_with_idempotency(ApiError::bad_request(error)),
    };
    let request_body = serde_json::json!({
        "channel_type": body.channel_type,
        "agent_id": resolved_agent_id.to_string(),
        "routing_key": routing_key,
        "config": config_json,
    });

    let mutation = {
        let db = state.db.write().await;
        match execute_idempotent_json_mutation(
            &db,
            &operation_context,
            &actor,
            "POST",
            CREATE_CHANNEL_ROUTE_TEMPLATE,
            &request_body,
            |conn| {
                let channel_id = operation_context
                    .operation_id
                    .clone()
                    .unwrap_or_else(|| Uuid::now_v7().to_string());

                conn.execute(
                    "INSERT INTO channels (
                        id,
                        channel_type,
                        status,
                        agent_id,
                        routing_key,
                        source,
                        config
                    ) VALUES (?1, ?2, 'configuring', ?3, ?4, 'operator_created', ?5)",
                    rusqlite::params![
                        channel_id,
                        body.channel_type,
                        resolved_agent_id.to_string(),
                        routing_key,
                        config_json.to_string(),
                    ],
                )
                .map_err(map_channel_insert_error)?;

                Ok((
                    StatusCode::CREATED,
                    serde_json::to_value(CreateChannelResponse {
                        id: channel_id,
                        status: "created".to_string(),
                        channel_type: body.channel_type.clone(),
                        agent_id: resolved_agent_id.to_string(),
                    })
                    .unwrap_or(serde_json::Value::Null),
                ))
            },
        ) {
            Ok(outcome) => {
                write_mutation_audit_entry(
                    &db,
                    outcome.body["agent_id"].as_str().unwrap_or("platform"),
                    "create_channel",
                    "medium",
                    &actor,
                    "created",
                    serde_json::json!({
                        "channel_id": outcome.body["id"],
                        "channel_type": outcome.body["channel_type"],
                        "routing_key": routing_key,
                    }),
                    &operation_context,
                    &outcome.idempotency_status,
                );
                Ok(MutationSuccess {
                    status: outcome.status,
                    body: outcome.body.clone(),
                    idempotency_status: outcome.idempotency_status,
                })
            }
            Err(error) => Err(error_response_with_idempotency(error)),
        }
    };

    let MutationSuccess {
        status,
        body,
        idempotency_status,
    } = match mutation {
        Ok(success) => success,
        Err(response) => return response,
    };

    if idempotency_status == IdempotencyStatus::Executed {
        let _ = state
            .channel_manager
            .activate_channel(body["id"].as_str().unwrap_or_default(), true)
            .await;
    }

    json_response_with_idempotency(status, body, idempotency_status)
}

/// POST /api/channels/:id/reconnect
#[axum::debug_handler]
pub async fn reconnect_channel(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(channel_id): Path<String>,
) -> Response {
    let actor = channel_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "channel_id": channel_id });

    let mutation = {
        let db = state.db.write().await;
        match execute_idempotent_json_mutation(
            &db,
            &operation_context,
            &actor,
            "POST",
            RECONNECT_CHANNEL_ROUTE_TEMPLATE,
            &request_body,
            |conn| {
                let affected = conn
                    .execute(
                        "UPDATE channels
                         SET status = 'configuring',
                             status_message = NULL,
                             updated_at = datetime('now')
                         WHERE id = ?1",
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
                    serde_json::to_value(ChannelStatusResponse {
                        id: channel_id.clone(),
                        status: "reconnected".to_string(),
                    })
                    .unwrap_or(serde_json::Value::Null),
                ))
            },
        ) {
            Ok(outcome) => {
                write_mutation_audit_entry(
                    &db,
                    "platform",
                    "reconnect_channel",
                    "medium",
                    &actor,
                    "reconnected",
                    serde_json::json!({ "channel_id": outcome.body["id"] }),
                    &operation_context,
                    &outcome.idempotency_status,
                );
                Ok(MutationSuccess {
                    status: outcome.status,
                    body: outcome.body.clone(),
                    idempotency_status: outcome.idempotency_status,
                })
            }
            Err(error) => Err(error_response_with_idempotency(error)),
        }
    };

    let MutationSuccess {
        status,
        body,
        idempotency_status,
    } = match mutation {
        Ok(success) => success,
        Err(response) => return response,
    };

    if idempotency_status == IdempotencyStatus::Executed {
        let _ = state
            .channel_manager
            .reconnect_channel(body["id"].as_str().unwrap_or_default())
            .await;
    }

    json_response_with_idempotency(status, body, idempotency_status)
}

/// DELETE /api/channels/:id
pub async fn delete_channel(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(channel_id): Path<String>,
) -> Response {
    let actor = channel_actor(claims.as_ref().map(|claims| &claims.0));
    let snapshot = match state.channel_manager.load_channel(&channel_id) {
        Ok(snapshot) => snapshot,
        Err(error) => return error_response_with_idempotency(ApiError::internal(error)),
    };
    let request_body = serde_json::json!({ "channel_id": channel_id });

    let mutation = {
        let db = state.db.write().await;
        match execute_idempotent_json_mutation(
            &db,
            &operation_context,
            &actor,
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
                    serde_json::to_value(ChannelStatusResponse {
                        id: channel_id.clone(),
                        status: "deleted".to_string(),
                    })
                    .unwrap_or(serde_json::Value::Null),
                ))
            },
        ) {
            Ok(outcome) => {
                write_mutation_audit_entry(
                    &db,
                    "platform",
                    "delete_channel",
                    "medium",
                    &actor,
                    "deleted",
                    serde_json::json!({ "channel_id": outcome.body["id"] }),
                    &operation_context,
                    &outcome.idempotency_status,
                );
                Ok(MutationSuccess {
                    status: outcome.status,
                    body: outcome.body.clone(),
                    idempotency_status: outcome.idempotency_status,
                })
            }
            Err(error) => Err(error_response_with_idempotency(error)),
        }
    };

    let MutationSuccess {
        status,
        body,
        idempotency_status,
    } = match mutation {
        Ok(success) => success,
        Err(response) => return response,
    };

    if idempotency_status == IdempotencyStatus::Executed {
        if let Some(snapshot) = snapshot.as_ref() {
            let _ = state.channel_manager.remove_channel_runtime(snapshot).await;
        }
    }

    json_response_with_idempotency(status, body, idempotency_status)
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
        .unwrap_or(request.sender.as_str())
        .to_string();
    let request_body = serde_json::to_value(&request).unwrap_or(serde_json::Value::Null);

    let mutation = {
        let db = state.db.write().await;
        match execute_idempotent_json_mutation(
            &db,
            &operation_context,
            &actor,
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
                write_mutation_audit_entry(
                    &db,
                    outcome.body["agent_id"].as_str().unwrap_or("platform"),
                    "inject_channel_message",
                    "info",
                    &actor,
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
                Ok(MutationSuccess {
                    status: outcome.status,
                    body: outcome.body.clone(),
                    idempotency_status: outcome.idempotency_status,
                })
            }
            Err(error) => Err(error_response_with_idempotency(error)),
        }
    };

    let MutationSuccess {
        status,
        body,
        idempotency_status,
    } = match mutation {
        Ok(success) => success,
        Err(response) => return Ok(response),
    };

    if idempotency_status == IdempotencyStatus::Executed {
        crate::api::websocket::broadcast_event(
            &state,
            crate::api::websocket::WsEvent::AgentStateChange {
                agent_id: body["agent_id"].as_str().unwrap_or_default().to_string(),
                new_state: format!("channel_inject:{channel_type}"),
            },
        );
    }

    Ok(json_response_with_idempotency(
        status,
        body,
        idempotency_status,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_routing_key_rejects_unsupported_types() {
        let error =
            ChannelManager::derive_routing_key("webhook", "agent-1", &serde_json::json!({}))
                .unwrap_err();
        assert!(error.contains("unsupported channel_type"), "{error}");
    }

    #[test]
    fn normalize_config_wraps_non_objects() {
        let normalized = ChannelManager::normalize_config(Some(serde_json::json!("value")));
        assert_eq!(normalized, serde_json::json!({ "value": "value" }));
    }
}
