use std::time::Duration;

use axum::http::StatusCode;
use serde_json::Value;

use crate::api::error::ApiError;
use crate::api::operation_context::{IdempotencyStatus, OperationContext};

const OPERATION_LEASE_SECONDS: i64 = 30;
const OPERATION_LEASE_RENEW_INTERVAL_SECONDS: u64 = 10;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedOperationLease {
    pub journal_id: String,
    pub owner_token: String,
    pub lease_epoch: i64,
}

#[derive(Debug, Clone)]
pub enum PreparedOperation {
    Acquired { lease: PreparedOperationLease },
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

pub struct OperationLeaseHeartbeat {
    journal_id: String,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
    lost_rx: tokio::sync::watch::Receiver<bool>,
    handle: tokio::task::JoinHandle<()>,
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

fn ownership_lost_error(journal_id: &str) -> ApiError {
    ApiError::with_details(
        StatusCode::CONFLICT,
        "OPERATION_OWNERSHIP_LOST",
        "Operation ownership was lost before the mutation could be finalized",
        serde_json::json!({
            "journal_id": journal_id,
        }),
    )
}

fn parse_committed_replay(
    entry: &cortex_storage::queries::operation_journal_queries::OperationJournalRow,
) -> Result<StoredJsonResponse, ApiError> {
    let raw_status = entry.response_status_code.ok_or_else(|| {
        ApiError::internal(format!(
            "committed operation_journal row {} missing response_status_code",
            entry.id
        ))
    })?;
    let status = StatusCode::from_u16(raw_status as u16).map_err(|_| {
        ApiError::internal(format!(
            "committed operation_journal row {} has invalid status code {}",
            entry.id, raw_status
        ))
    })?;
    let raw_body = entry.response_body.as_deref().ok_or_else(|| {
        ApiError::internal(format!(
            "committed operation_journal row {} missing response_body",
            entry.id
        ))
    })?;
    let body = serde_json::from_str(raw_body).map_err(|error| {
        ApiError::internal(format!(
            "committed operation_journal row {} has invalid response_body: {error}",
            entry.id
        ))
    })?;
    Ok(StoredJsonResponse { status, body })
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
    let next_owner_token = uuid::Uuid::now_v7().to_string();

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| ApiError::db_error("operation_journal_begin", e))?;

    let existing =
        cortex_storage::queries::operation_journal_queries::get_by_actor_and_idempotency_key(
            conn,
            actor_key,
            &required.idempotency_key,
        )
        .map_err(|e| ApiError::db_error("operation_journal_lookup", e))?;

    let result = (|| -> Result<PreparedOperation, ApiError> {
        Ok(match existing {
            Some(entry) if entry.request_fingerprint != fingerprint => PreparedOperation::Mismatch,
            Some(entry) if entry.status == "committed" => {
                PreparedOperation::Replay(parse_committed_replay(&entry)?)
            }
            Some(entry) if entry.status == "aborted" => {
                let new_epoch = entry.lease_epoch + 1;
                let restarted =
                    cortex_storage::queries::operation_journal_queries::restart_aborted(
                        conn,
                        &entry.id,
                        &required.operation_id,
                        Some(&required.request_id),
                        &next_owner_token,
                        &now.to_rfc3339(),
                        &lease_expires_at(now),
                    )
                    .map_err(|e| ApiError::db_error("operation_journal_restart", e))?;
                if !restarted {
                    return Err(ApiError::custom(
                        StatusCode::CONFLICT,
                        "OPERATION_RESTART_FAILED",
                        "Failed to reacquire aborted operation journal entry",
                    ));
                }
                PreparedOperation::Acquired {
                    lease: PreparedOperationLease {
                        journal_id: entry.id,
                        owner_token: next_owner_token.clone(),
                        lease_epoch: new_epoch,
                    },
                }
            }
            Some(entry) => {
                let expired = entry.lease_expires_at.as_deref().ok_or_else(|| {
                    ApiError::internal(format!(
                        "in-progress operation_journal row {} missing lease_expires_at",
                        entry.id
                    ))
                })?;
                let expired = chrono::DateTime::parse_from_rfc3339(expired)
                .map_err(|error| {
                    ApiError::internal(format!(
                        "in-progress operation_journal row {} has invalid lease_expires_at: {error}",
                        entry.id
                    ))
                })?
                .with_timezone(&chrono::Utc)
                <= now;

                if expired {
                    let new_epoch = entry.lease_epoch + 1;
                    let taken_over =
                        cortex_storage::queries::operation_journal_queries::take_over_in_progress(
                            conn,
                            &entry.id,
                            &required.operation_id,
                            Some(&required.request_id),
                            &next_owner_token,
                            &now.to_rfc3339(),
                            &lease_expires_at(now),
                        )
                        .map_err(|e| ApiError::db_error("operation_journal_takeover", e))?;
                    if !taken_over {
                        return Err(ApiError::custom(
                            StatusCode::CONFLICT,
                            "OPERATION_TAKEOVER_FAILED",
                            "Failed to take ownership of expired operation journal entry",
                        ));
                    }
                    PreparedOperation::Acquired {
                        lease: PreparedOperationLease {
                            journal_id: entry.id,
                            owner_token: next_owner_token.clone(),
                            lease_epoch: new_epoch,
                        },
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
                        owner_token: &next_owner_token,
                        lease_epoch: 0,
                    };
                cortex_storage::queries::operation_journal_queries::insert_in_progress(
                    conn, &entry,
                )
                .map_err(|e| ApiError::db_error("operation_journal_insert", e))?;
                PreparedOperation::Acquired {
                    lease: PreparedOperationLease {
                        journal_id,
                        owner_token: next_owner_token,
                        lease_epoch: 0,
                    },
                }
            }
        })
    })();

    match result {
        Ok(result) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| ApiError::db_error("operation_journal_commit", e))?;
            Ok(result)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
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
        PreparedOperation::Acquired { lease } => {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| ApiError::db_error("operation_execute_begin", e))?;

            match execute(conn) {
                Ok((status, response_body)) => {
                    let outcome = mark_prepared_json_operation_committed(
                        conn,
                        context,
                        &lease,
                        status,
                        &response_body,
                    )?;
                    conn.execute_batch("COMMIT")
                        .map_err(|e| ApiError::db_error("operation_execute_commit", e))?;
                    Ok(outcome)
                }
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    let _ = abort_prepared_json_operation(conn, context, &lease);
                    Err(error)
                }
            }
        }
    }
}

pub fn commit_prepared_json_operation(
    conn: &rusqlite::Connection,
    context: &OperationContext,
    lease: &PreparedOperationLease,
    status: StatusCode,
    response_body: &Value,
) -> Result<ExecutedJsonMutation, ApiError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| ApiError::db_error("operation_commit_begin", e))?;
    match mark_prepared_json_operation_committed(conn, context, lease, status, response_body) {
        Ok(outcome) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| ApiError::db_error("operation_execute_commit", e))?;
            Ok(outcome)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn mark_prepared_json_operation_committed(
    conn: &rusqlite::Connection,
    context: &OperationContext,
    lease: &PreparedOperationLease,
    status: StatusCode,
    response_body: &Value,
) -> Result<ExecutedJsonMutation, ApiError> {
    let required = require_operation_context(context)?;
    let response_body_string =
        serde_json::to_string(response_body).map_err(|e| ApiError::internal(e.to_string()))?;
    let committed = cortex_storage::queries::operation_journal_queries::mark_committed(
        conn,
        &lease.journal_id,
        &lease.owner_token,
        lease.lease_epoch,
        Some(&required.request_id),
        i64::from(status.as_u16()),
        &response_body_string,
        "application/json",
        &chrono::Utc::now().to_rfc3339(),
    )
    .map_err(|e| ApiError::db_error("operation_journal_mark_committed", e))?;
    if !committed {
        return Err(ownership_lost_error(&lease.journal_id));
    }

    Ok(ExecutedJsonMutation {
        status,
        body: response_body.clone(),
        idempotency_status: IdempotencyStatus::Executed,
    })
}

pub fn abort_prepared_json_operation(
    conn: &rusqlite::Connection,
    context: &OperationContext,
    lease: &PreparedOperationLease,
) -> Result<(), ApiError> {
    let required = require_operation_context(context)?;
    let aborted = cortex_storage::queries::operation_journal_queries::mark_aborted(
        conn,
        &lease.journal_id,
        &lease.owner_token,
        lease.lease_epoch,
        Some(&required.request_id),
        &chrono::Utc::now().to_rfc3339(),
    )
    .map_err(|e| ApiError::db_error("operation_journal_abort", e))?;
    if !aborted {
        return Err(ownership_lost_error(&lease.journal_id));
    }
    Ok(())
}

pub fn renew_prepared_json_operation_lease(
    conn: &rusqlite::Connection,
    lease: &PreparedOperationLease,
) -> Result<(), ApiError> {
    let now = chrono::Utc::now();
    let renewed = cortex_storage::queries::operation_journal_queries::renew_lease(
        conn,
        &lease.journal_id,
        &lease.owner_token,
        lease.lease_epoch,
        &now.to_rfc3339(),
        &lease_expires_at(now),
    )
    .map_err(|e| ApiError::db_error("operation_journal_renew_lease", e))?;
    if !renewed {
        return Err(ownership_lost_error(&lease.journal_id));
    }
    Ok(())
}

pub fn start_operation_lease_heartbeat(
    db: std::sync::Arc<crate::db_pool::DbPool>,
    lease: PreparedOperationLease,
) -> OperationLeaseHeartbeat {
    let journal_id = lease.journal_id.clone();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
    let (lost_tx, lost_rx) = tokio::sync::watch::channel(false);
    let renewal_lease = lease.clone();
    let renewal_journal_id = journal_id.clone();
    let handle = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(Duration::from_secs(OPERATION_LEASE_RENEW_INTERVAL_SECONDS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                _ = interval.tick() => {
                    let conn = db.write().await;
                    if let Err(error) = renew_prepared_json_operation_lease(&conn, &renewal_lease) {
                        tracing::warn!(
                            journal_id = %renewal_journal_id,
                            error = %error,
                            "operation lease heartbeat lost ownership"
                        );
                        let _ = lost_tx.send(true);
                        break;
                    }
                }
            }
        }
    });

    OperationLeaseHeartbeat {
        journal_id,
        stop_tx: Some(stop_tx),
        lost_rx,
        handle,
    }
}

impl OperationLeaseHeartbeat {
    pub async fn stop(mut self) -> Result<(), ApiError> {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        match self.handle.await {
            Ok(()) => {}
            Err(error) => {
                return Err(ApiError::internal(format!(
                    "operation lease heartbeat task failed for {}: {error}",
                    self.journal_id
                )));
            }
        }

        if *self.lost_rx.borrow() {
            return Err(ownership_lost_error(&self.journal_id));
        }
        Ok(())
    }
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
            client_supplied_operation_id: true,
            client_supplied_idempotency_key: true,
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

    #[test]
    fn stale_owner_cannot_commit_after_takeover() {
        let conn = cortex_storage::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();

        let initial = operation_context();
        let body = serde_json::json!({"status": "approve"});
        let PreparedOperation::Acquired { lease: stale_lease } = prepare_json_operation(
            &conn,
            &initial,
            "legacy-token-user",
            "POST",
            "/api/goals/:id/approve",
            &body,
        )
        .unwrap() else {
            panic!("expected initial acquisition");
        };

        conn.execute(
            "UPDATE operation_journal
             SET last_seen_at = ?2, lease_expires_at = ?3
             WHERE id = ?1",
            rusqlite::params![
                stale_lease.journal_id,
                (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339(),
                (chrono::Utc::now() - chrono::Duration::minutes(4)).to_rfc3339(),
            ],
        )
        .unwrap();

        let takeover_context = OperationContext {
            request_id: "request-2".into(),
            operation_id: Some("018f0f23-8c65-7abc-9def-1234567890ac".into()),
            idempotency_key: Some("idem-1".into()),
            idempotency_status: None,
            is_mutating: true,
            client_supplied_operation_id: true,
            client_supplied_idempotency_key: true,
        };
        let PreparedOperation::Acquired {
            lease: winning_lease,
        } = prepare_json_operation(
            &conn,
            &takeover_context,
            "legacy-token-user",
            "POST",
            "/api/goals/:id/approve",
            &body,
        )
        .unwrap()
        else {
            panic!("expected takeover acquisition");
        };

        let stale_result = commit_prepared_json_operation(
            &conn,
            &initial,
            &stale_lease,
            StatusCode::OK,
            &serde_json::json!({"status": "stale"}),
        );
        assert!(stale_result.is_err());

        let winner = commit_prepared_json_operation(
            &conn,
            &takeover_context,
            &winning_lease,
            StatusCode::OK,
            &serde_json::json!({"status": "winner"}),
        )
        .unwrap();
        assert_eq!(winner.body["status"], "winner");
    }

    #[test]
    fn aborted_operation_can_be_reacquired_without_deleting_history() {
        let conn = cortex_storage::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();

        let context = operation_context();
        let body = serde_json::json!({"status": "retryable"});
        let PreparedOperation::Acquired { lease } = prepare_json_operation(
            &conn,
            &context,
            "legacy-token-user",
            "POST",
            "/api/goals/:id/approve",
            &body,
        )
        .unwrap() else {
            panic!("expected acquisition");
        };

        abort_prepared_json_operation(&conn, &context, &lease).unwrap();

        let reacquired = prepare_json_operation(
            &conn,
            &OperationContext {
                request_id: "request-3".into(),
                operation_id: Some("018f0f23-8c65-7abc-9def-1234567890ad".into()),
                idempotency_key: Some("idem-1".into()),
                idempotency_status: None,
                is_mutating: true,
                client_supplied_operation_id: true,
                client_supplied_idempotency_key: true,
            },
            "legacy-token-user",
            "POST",
            "/api/goals/:id/approve",
            &body,
        )
        .unwrap();

        assert!(matches!(reacquired, PreparedOperation::Acquired { .. }));
    }
}
