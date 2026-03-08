use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::rbac::Role;
use crate::state::AppState;

fn expected_live_execution_state_version(route_kind: &str) -> Option<i64> {
    match route_kind {
        "oauth_execute_api_call" | "studio_send_message" | "agent_chat" | "agent_chat_stream" => {
            Some(1)
        }
        _ => None,
    }
}

fn can_view_execution(
    record: &cortex_storage::queries::live_execution_queries::LiveExecutionRecord,
    claims: Option<&Claims>,
) -> bool {
    let Some(claims) = claims else {
        return false;
    };

    match Role::from_str(&claims.role) {
        Some(Role::Admin) | Some(Role::SuperAdmin) => true,
        _ => record.actor_key == claims.sub,
    }
}

/// GET /api/live-executions/:execution_id — inspect accepted-boundary execution state.
pub async fn get_live_execution(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Path(execution_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state
        .db
        .read()
        .map_err(|error| ApiError::db_error("get_live_execution", error))?;

    let Some(record) =
        cortex_storage::queries::live_execution_queries::get_by_id(&db, &execution_id)
            .map_err(|error| ApiError::db_error("get_live_execution", error))?
    else {
        return Err(ApiError::not_found(format!(
            "live execution {execution_id} not found"
        )));
    };

    if !can_view_execution(&record, claims.as_ref().map(|claims| &claims.0)) {
        return Err(ApiError::not_found(format!(
            "live execution {execution_id} not found"
        )));
    }

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
    if !state_json.is_object() {
        return Err(ApiError::internal(
            "live execution state must be a JSON object".to_string(),
        ));
    }

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
