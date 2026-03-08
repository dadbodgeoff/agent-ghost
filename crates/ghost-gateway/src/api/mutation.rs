use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::error::ApiError;
use crate::api::operation_context::{
    IdempotencyStatus, OperationContext, IDEMPOTENCY_STATUS_HEADER,
};

pub fn json_response_with_idempotency(
    status: StatusCode,
    body: serde_json::Value,
    idempotency_status: IdempotencyStatus,
) -> Response {
    let mut response = (status, Json(body)).into_response();
    if let Ok(value) = HeaderValue::from_str(idempotency_status.as_header_value()) {
        response
            .headers_mut()
            .insert(IDEMPOTENCY_STATUS_HEADER, value);
    }
    response
}

pub fn error_response_with_idempotency(error: ApiError) -> Response {
    let idempotency_status = match &error {
        ApiError::Custom { code, .. } if code == "IDEMPOTENCY_KEY_REUSED" => {
            Some(IdempotencyStatus::Mismatch)
        }
        ApiError::Custom { code, .. } if code == "IDEMPOTENCY_IN_PROGRESS" => {
            Some(IdempotencyStatus::InProgress)
        }
        _ => None,
    };

    let mut response = error.into_response();
    if let Some(idempotency_status) = idempotency_status {
        if let Ok(value) = HeaderValue::from_str(idempotency_status.as_header_value()) {
            response
                .headers_mut()
                .insert(IDEMPOTENCY_STATUS_HEADER, value);
        }
    }
    response
}

pub fn write_mutation_audit_entry(
    conn: &rusqlite::Connection,
    agent_id: &str,
    event_type: &str,
    severity: &str,
    actor: &str,
    outcome: &str,
    details: serde_json::Value,
    operation_context: &OperationContext,
    idempotency_status: &IdempotencyStatus,
) {
    let engine = ghost_audit::AuditQueryEngine::new(conn);
    let entry = ghost_audit::AuditEntry {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.to_string(),
        event_type: event_type.to_string(),
        severity: severity.to_string(),
        tool_name: None,
        details: serde_json::json!({
            "actor": actor,
            "outcome": outcome,
            "details": details,
        })
        .to_string(),
        session_id: None,
        operation_id: operation_context.operation_id.clone(),
        request_id: Some(operation_context.request_id.clone()),
        idempotency_key: operation_context.idempotency_key.clone(),
        idempotency_status: Some(idempotency_status.as_header_value().to_string()),
    };

    if let Err(error) = engine.insert(&entry) {
        tracing::warn!(
            event_type = %event_type,
            agent_id = %agent_id,
            error = %error,
            "failed to write mutation audit entry"
        );
    }
}
