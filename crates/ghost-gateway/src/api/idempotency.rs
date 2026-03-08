use std::time::Duration;

use axum::http::StatusCode;
use serde_json::Value;

use crate::api::error::ApiError;
use crate::api::operation_context::{IdempotencyStatus, OperationContext};

const OPERATION_LEASE_SECONDS: i64 = 30;

#[derive(Debug, Clone)]
pub struct RequiredOperationContext {
    pub request_id: String,
    pub operation_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct StoredJsonResponse {
    pub status: StatusCode,
    pub body: Value,
}

#[derive(Debug, Clone)]
pub enum PreparedOperation {
    Acquired { journal_id: String },
    Replay(StoredJsonResponse),
    InProgress,
    Mismatch,
}

#[derive(Debug, Clone)]
pub struct ExecutedJsonMutation {
    pub status: StatusCode,
    pub body: Value,
    pub idempotency_status: IdempotencyStatus,
}

fn canonical_json_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(string) => serde_json::to_string(string).unwrap_or_else(|_| "\"\"".into()),
        Value::Array(array) => {
            let mut out = String::from("[");
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json_string(item));
            }
            out.push(']');
            out
        }
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            let mut out = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_else(|_| "\"\"".into()));
                out.push(':');
                out.push_str(&canonical_json_string(&object[*key]));
            }
            out.push('}');
            out
        }
    }
}

pub fn fingerprint_json_request(
    method: &str,
    route_template: &str,
    actor_key: &str,
    body: &Value,
) -> String {
    let canonical_body = canonical_json_string(body);
    let mut hasher = blake3::Hasher::new();
    hasher.update(method.as_bytes());
    hasher.update(b"\n");
    hasher.update(route_template.as_bytes());
    hasher.update(b"\n");
    hasher.update(actor_key.as_bytes());
    hasher.update(b"\n");
    hasher.update(canonical_body.as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub fn require_operation_context(
    context: &OperationContext,
) -> Result<RequiredOperationContext, ApiError> {
    let operation_id = context.operation_id.clone().ok_or_else(|| {
        ApiError::custom(
            StatusCode::PRECONDITION_REQUIRED,
            "MISSING_OPERATION_ID",
            "Mutating requests must include X-Ghost-Operation-ID",
        )
    })?;
    let idempotency_key = context.idempotency_key.clone().ok_or_else(|| {
        ApiError::custom(
            StatusCode::PRECONDITION_REQUIRED,
            "MISSING_IDEMPOTENCY_KEY",
            "Mutating requests must include Idempotency-Key",
        )
    })?;

    Ok(RequiredOperationContext {
        request_id: context.request_id.clone(),
        operation_id,
        idempotency_key,
    })
}

fn lease_expires_at(now: chrono::DateTime<chrono::Utc>) -> String {
    (now + chrono::Duration::from_std(Duration::from_secs(OPERATION_LEASE_SECONDS as u64)).unwrap())
        .to_rfc3339()
}

pub fn prepare_json_operation(
    conn: &rusqlite::Connection,
    context: &OperationContext,
    actor_key: &str,
    method: &str,
    route_template: &str,
    body: &Value,
) -> Result<PreparedOperation, ApiError> {
    let required = require_operation_context(context)?;
    let now = chrono::Utc::now();
    let fingerprint = fingerprint_json_request(method, route_template, actor_key, body);

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| ApiError::db_error("operation_journal_begin", e))?;

    let existing =
        cortex_storage::queries::operation_journal_queries::get_by_actor_and_idempotency_key(
            conn,
            actor_key,
            &required.idempotency_key,
        )
        .map_err(|e| ApiError::db_error("operation_journal_lookup", e))?;

    let result = match existing {
        Some(entry) if entry.request_fingerprint != fingerprint => PreparedOperation::Mismatch,
        Some(entry) if entry.status == "committed" => {
            let status = StatusCode::from_u16(entry.response_status_code.unwrap_or(200) as u16)
                .unwrap_or(StatusCode::OK);
            let body = entry
                .response_body
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok())
                .unwrap_or(Value::Null);
            PreparedOperation::Replay(StoredJsonResponse { status, body })
        }
        Some(entry) => {
            let expired = entry
                .lease_expires_at
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&chrono::Utc) <= now)
                .unwrap_or(false);

            if expired {
                cortex_storage::queries::operation_journal_queries::take_over_in_progress(
                    conn,
                    &entry.id,
                    &required.operation_id,
                    Some(&required.request_id),
                    &now.to_rfc3339(),
                    &lease_expires_at(now),
                )
                .map_err(|e| ApiError::db_error("operation_journal_takeover", e))?;
                PreparedOperation::Acquired {
                    journal_id: entry.id,
                }
            } else {
                PreparedOperation::InProgress
            }
        }
        None => {
            let journal_id = uuid::Uuid::now_v7().to_string();
            let body_string = canonical_json_string(body);
            let created_at = now.to_rfc3339();
            let lease_expires_at = lease_expires_at(now);
            let entry =
                cortex_storage::queries::operation_journal_queries::NewOperationJournalEntry {
                    id: &journal_id,
                    actor_key,
                    method,
                    route_template,
                    operation_id: &required.operation_id,
                    request_id: Some(&required.request_id),
                    idempotency_key: &required.idempotency_key,
                    request_fingerprint: &fingerprint,
                    request_body: &body_string,
                    created_at: &created_at,
                    lease_expires_at: &lease_expires_at,
                };
            cortex_storage::queries::operation_journal_queries::insert_in_progress(conn, &entry)
                .map_err(|e| ApiError::db_error("operation_journal_insert", e))?;
            PreparedOperation::Acquired { journal_id }
        }
    };

    conn.execute_batch("COMMIT")
        .map_err(|e| ApiError::db_error("operation_journal_commit", e))?;

    Ok(result)
}

pub fn execute_idempotent_json_mutation<F>(
    conn: &rusqlite::Connection,
    context: &OperationContext,
    actor_key: &str,
    method: &str,
    route_template: &str,
    body: &Value,
    execute: F,
) -> Result<ExecutedJsonMutation, ApiError>
where
    F: FnOnce(&rusqlite::Connection) -> Result<(StatusCode, Value), ApiError>,
{
    match prepare_json_operation(conn, context, actor_key, method, route_template, body)? {
        PreparedOperation::Replay(stored) => {
            return Ok(ExecutedJsonMutation {
                status: stored.status,
                body: stored.body,
                idempotency_status: IdempotencyStatus::Replayed,
            });
        }
        PreparedOperation::Mismatch => {
            return Err(ApiError::with_details(
                StatusCode::CONFLICT,
                "IDEMPOTENCY_KEY_REUSED",
                "Idempotency key was reused with a different request payload",
                serde_json::json!({
                    "route_template": route_template,
                    "method": method,
                }),
            ));
        }
        PreparedOperation::InProgress => {
            return Err(ApiError::custom(
                StatusCode::CONFLICT,
                "IDEMPOTENCY_IN_PROGRESS",
                "An equivalent request is already in progress",
            ));
        }
        PreparedOperation::Acquired { journal_id } => {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| ApiError::db_error("operation_execute_begin", e))?;

            match execute(conn) {
                Ok((status, response_body)) => {
                    let outcome = mark_prepared_json_operation_committed(
                        conn,
                        context,
                        &journal_id,
                        status,
                        &response_body,
                    )?;
                    conn.execute_batch("COMMIT")
                        .map_err(|e| ApiError::db_error("operation_execute_commit", e))?;
                    Ok(outcome)
                }
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    let _ = abort_prepared_json_operation(conn, &journal_id);
                    Err(error)
                }
            }
        }
    }
}

pub fn commit_prepared_json_operation(
    conn: &rusqlite::Connection,
    context: &OperationContext,
    journal_id: &str,
    status: StatusCode,
    response_body: &Value,
) -> Result<ExecutedJsonMutation, ApiError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| ApiError::db_error("operation_commit_begin", e))?;
    let outcome =
        mark_prepared_json_operation_committed(conn, context, journal_id, status, response_body)?;
    conn.execute_batch("COMMIT")
        .map_err(|e| ApiError::db_error("operation_execute_commit", e))?;
    Ok(outcome)
}

fn mark_prepared_json_operation_committed(
    conn: &rusqlite::Connection,
    context: &OperationContext,
    journal_id: &str,
    status: StatusCode,
    response_body: &Value,
) -> Result<ExecutedJsonMutation, ApiError> {
    let required = require_operation_context(context)?;
    let response_body_string =
        serde_json::to_string(response_body).map_err(|e| ApiError::internal(e.to_string()))?;
    cortex_storage::queries::operation_journal_queries::mark_committed(
        conn,
        journal_id,
        Some(&required.request_id),
        i64::from(status.as_u16()),
        &response_body_string,
        "application/json",
        &chrono::Utc::now().to_rfc3339(),
    )
    .map_err(|e| ApiError::db_error("operation_journal_mark_committed", e))?;

    Ok(ExecutedJsonMutation {
        status,
        body: response_body.clone(),
        idempotency_status: IdempotencyStatus::Executed,
    })
}

pub fn abort_prepared_json_operation(
    conn: &rusqlite::Connection,
    journal_id: &str,
) -> Result<(), ApiError> {
    cortex_storage::queries::operation_journal_queries::delete_entry(conn, journal_id)
        .map_err(|e| ApiError::db_error("operation_journal_delete", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::{json, Map};

    fn operation_context() -> OperationContext {
        OperationContext {
            request_id: "request-1".into(),
            operation_id: Some("018f0f23-8c65-7abc-9def-1234567890ab".into()),
            idempotency_key: Some("idem-1".into()),
            idempotency_status: None,
            is_mutating: true,
        }
    }

    proptest! {
        #[test]
        fn fingerprint_is_stable_under_key_reordering(
            entries in prop::collection::btree_map("[a-z]{1,6}", any::<i64>(), 1..8)
        ) {
            let mut forward = Map::new();
            for (key, value) in &entries {
                forward.insert(key.clone(), json!(value));
            }

            let mut reverse = Map::new();
            for (key, value) in entries.iter().rev() {
                reverse.insert(key.clone(), json!(value));
            }

            let lhs = fingerprint_json_request("POST", "/api/goals/:id/approve", "actor", &Value::Object(forward));
            let rhs = fingerprint_json_request("POST", "/api/goals/:id/approve", "actor", &Value::Object(reverse));

            prop_assert_eq!(lhs, rhs);
        }
    }

    #[test]
    fn retry_after_commit_replays_stored_response() {
        let conn = cortex_storage::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();

        let context = operation_context();
        let result = execute_idempotent_json_mutation(
            &conn,
            &context,
            "legacy-token-user",
            "POST",
            "/api/goals/:id/approve",
            &Value::Null,
            |_conn| {
                Ok((
                    StatusCode::OK,
                    json!({"status": "approved", "id": "goal-1"}),
                ))
            },
        )
        .unwrap();

        assert_eq!(result.idempotency_status, IdempotencyStatus::Executed);

        let replay = execute_idempotent_json_mutation(
            &conn,
            &context,
            "legacy-token-user",
            "POST",
            "/api/goals/:id/approve",
            &Value::Null,
            |_conn| unreachable!("replay should not re-execute mutation"),
        )
        .unwrap();

        assert_eq!(replay.idempotency_status, IdempotencyStatus::Replayed);
        assert_eq!(replay.body["status"], "approved");
    }
}
