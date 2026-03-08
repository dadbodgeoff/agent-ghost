use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::error::ApiError;
use crate::api::operation_context::{
    IdempotencyStatus, OperationContext, IDEMPOTENCY_STATUS_HEADER,
};

pub fn response_with_idempotency(
    mut response: Response,
    idempotency_status: IdempotencyStatus,
) -> Response {
    if let Ok(value) = HeaderValue::from_str(idempotency_status.as_header_value()) {
        response
            .headers_mut()
            .insert(IDEMPOTENCY_STATUS_HEADER, value);
    }
    response
}

pub fn json_response_with_idempotency(
    status: StatusCode,
    body: serde_json::Value,
    idempotency_status: IdempotencyStatus,
) -> Response {
    response_with_idempotency((status, Json(body)).into_response(), idempotency_status)
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
        actor_id: Some(actor.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_audit_entries_populate_actor_id_column() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();

        let operation_context = OperationContext {
            request_id: "req-1".to_string(),
            operation_id: Some("op-1".to_string()),
            idempotency_key: Some("idem-1".to_string()),
            idempotency_status: None,
            is_mutating: true,
        };

        write_mutation_audit_entry(
            &conn,
            "agent-1",
            "assign_profile",
            "info",
            "operator-1",
            "assigned",
            serde_json::json!({ "profile_name": "research" }),
            &operation_context,
            &IdempotencyStatus::Executed,
        );

        let actor_id: Option<String> = conn
            .query_row("SELECT actor_id FROM audit_log LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(actor_id.as_deref(), Some("operator-1"));
    }
}
