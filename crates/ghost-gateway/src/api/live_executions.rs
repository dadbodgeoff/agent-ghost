use axum::extract::State;
use axum::Extension;
use axum::Json;
use chrono::Utc;
use std::sync::Arc;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthorizedLiveExecutionRecord(
    pub cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
);

fn expected_live_execution_state_version(route_kind: &str) -> Option<i64> {
    match route_kind {
        "oauth_execute_api_call"
        | "studio_send_message"
        | "studio_send_message_stream"
        | "agent_chat"
        | "agent_chat_stream" => Some(1),
        _ => None,
    }
}

fn parse_live_execution_state(
    record: &cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
) -> Result<serde_json::Map<String, serde_json::Value>, ApiError> {
    let expected_state_version = expected_live_execution_state_version(&record.route_kind)
        .ok_or_else(|| {
            ApiError::internal(format!(
                "unsupported live execution route kind: {}",
                record.route_kind
            ))
        })?;
    if record.state_version != expected_state_version {
        return Err(ApiError::internal(format!(
            "unsupported live execution state version {} for route {}",
            record.state_version, record.route_kind
        )));
    }

    let state_json =
        serde_json::from_str::<serde_json::Value>(&record.state_json).map_err(|error| {
            ApiError::internal(format!("failed to parse live execution state: {error}"))
        })?;
    let Some(object) = state_json.as_object() else {
        return Err(ApiError::internal(
            "live execution state must be a JSON object".to_string(),
        ));
    };
    Ok(object.clone())
}

fn execution_cancelled_error_body() -> serde_json::Value {
    serde_json::json!({
        "error": {
            "code": "EXECUTION_CANCELLED",
            "message": "Execution cancelled by user",
        }
    })
}

fn cancelled_stream_payload() -> serde_json::Value {
    serde_json::json!({
        "message": "Execution cancelled by user",
        "cancelled": true,
    })
}

fn apply_cancelled_state(
    actor: &str,
    mut state: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    state.insert(
        "cancelled_at".into(),
        serde_json::json!(Utc::now().to_rfc3339()),
    );
    state.insert("cancelled_by".into(), serde_json::json!(actor));

    if state.contains_key("final_status_code") {
        state.insert("final_status_code".into(), serde_json::json!(409));
    }
    if state.contains_key("final_response") {
        state.insert("final_response".into(), execution_cancelled_error_body());
    }
    if state.contains_key("recovery_required") {
        state.insert("recovery_required".into(), serde_json::Value::Bool(false));
    }
    if state.contains_key("terminal_event_type") {
        state.insert("terminal_event_type".into(), serde_json::json!("error"));
    }
    if state.contains_key("terminal_payload") {
        state.insert("terminal_payload".into(), cancelled_stream_payload());
    }

    state
}

/// GET /api/live-executions/:execution_id — inspect accepted-boundary execution state.
pub async fn get_live_execution(
    Extension(authorized_record): Extension<AuthorizedLiveExecutionRecord>,
) -> ApiResult<serde_json::Value> {
    let record = authorized_record.0;
    let state_json = parse_live_execution_state(&record)?;

    Ok(Json(serde_json::json!({
        "execution_id": record.id,
        "route_kind": record.route_kind,
        "state_version": record.state_version,
        "status": record.status,
        "operation_id": record.operation_id,
        "accepted_response": state_json.get("accepted_response").cloned().unwrap_or(serde_json::Value::Null),
        "result_status_code": state_json.get("final_status_code").cloned().unwrap_or(serde_json::Value::Null),
        "result_body": state_json.get("final_response").cloned().unwrap_or(serde_json::Value::Null),
        "recovery_required": record.status == "recovery_required",
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })))
}

/// POST /api/live-executions/:execution_id/cancel — explicitly cancel a live execution.
pub async fn cancel_live_execution(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(authorized_record): Extension<AuthorizedLiveExecutionRecord>,
) -> ApiResult<serde_json::Value> {
    let record = authorized_record.0;
    let actor = claims
        .as_ref()
        .map(|claims| claims.0.sub.as_str())
        .unwrap_or(record.actor_key.as_str());

    if matches!(record.status.as_str(), "completed" | "cancelled") {
        return Ok(Json(serde_json::json!({
            "execution_id": record.id,
            "route_kind": record.route_kind,
            "status": record.status,
            "cancel_requested": record.status == "cancelled",
        })));
    }

    let state_json = apply_cancelled_state(actor, parse_live_execution_state(&record)?);
    let state_json = serde_json::to_string(&state_json)
        .map_err(|error| ApiError::internal(error.to_string()))?;

    {
        let db = state.db.write().await;
        cortex_storage::queries::live_execution_queries::update_status_and_state(
            &db,
            &record.id,
            record.state_version,
            "cancelled",
            &state_json,
        )
        .map_err(|error| ApiError::db_error("cancel_live_execution", error))?;
    }

    let cancel_signal_sent = state.cancel_live_execution(&record.id);

    Ok(Json(serde_json::json!({
        "execution_id": record.id,
        "route_kind": record.route_kind,
        "status": "cancelled",
        "cancel_signal_sent": cancel_signal_sent,
    })))
}
