use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationJournalRow {
    pub id: String,
    pub actor_key: String,
    pub method: String,
    pub route_template: String,
    pub operation_id: String,
    pub request_id: Option<String>,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub request_body: String,
    pub status: String,
    pub response_status_code: Option<i64>,
    pub response_body: Option<String>,
    pub response_content_type: Option<String>,
    pub created_at: String,
    pub last_seen_at: String,
    pub committed_at: Option<String>,
    pub lease_expires_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewOperationJournalEntry<'a> {
    pub id: &'a str,
    pub actor_key: &'a str,
    pub method: &'a str,
    pub route_template: &'a str,
    pub operation_id: &'a str,
    pub request_id: Option<&'a str>,
    pub idempotency_key: &'a str,
    pub request_fingerprint: &'a str,
    pub request_body: &'a str,
    pub created_at: &'a str,
    pub lease_expires_at: &'a str,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperationJournalRow> {
    Ok(OperationJournalRow {
        id: row.get(0)?,
        actor_key: row.get(1)?,
        method: row.get(2)?,
        route_template: row.get(3)?,
        operation_id: row.get(4)?,
        request_id: row.get(5)?,
        idempotency_key: row.get(6)?,
        request_fingerprint: row.get(7)?,
        request_body: row.get(8)?,
        status: row.get(9)?,
        response_status_code: row.get(10)?,
        response_body: row.get(11)?,
        response_content_type: row.get(12)?,
        created_at: row.get(13)?,
        last_seen_at: row.get(14)?,
        committed_at: row.get(15)?,
        lease_expires_at: row.get(16)?,
    })
}

pub fn get_by_actor_and_idempotency_key(
    conn: &Connection,
    actor_key: &str,
    idempotency_key: &str,
) -> CortexResult<Option<OperationJournalRow>> {
    conn.query_row(
        "SELECT id, actor_key, method, route_template, operation_id, request_id,
                idempotency_key, request_fingerprint, request_body, status,
                response_status_code, response_body, response_content_type,
                created_at, last_seen_at, committed_at, lease_expires_at
         FROM operation_journal
         WHERE actor_key = ?1 AND idempotency_key = ?2",
        params![actor_key, idempotency_key],
        map_row,
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

pub fn insert_in_progress(
    conn: &Connection,
    entry: &NewOperationJournalEntry<'_>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO operation_journal (
            id, actor_key, method, route_template, operation_id, request_id,
            idempotency_key, request_fingerprint, request_body, status,
            created_at, last_seen_at, lease_expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'in_progress', ?10, ?10, ?11)",
        params![
            entry.id,
            entry.actor_key,
            entry.method,
            entry.route_template,
            entry.operation_id,
            entry.request_id,
            entry.idempotency_key,
            entry.request_fingerprint,
            entry.request_body,
            entry.created_at,
            entry.lease_expires_at,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn take_over_in_progress(
    conn: &Connection,
    id: &str,
    operation_id: &str,
    request_id: Option<&str>,
    last_seen_at: &str,
    lease_expires_at: &str,
) -> CortexResult<()> {
    conn.execute(
        "UPDATE operation_journal
         SET operation_id = ?2,
             request_id = ?3,
             last_seen_at = ?4,
             lease_expires_at = ?5
         WHERE id = ?1 AND status = 'in_progress'",
        params![id, operation_id, request_id, last_seen_at, lease_expires_at],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn mark_committed(
    conn: &Connection,
    id: &str,
    request_id: Option<&str>,
    response_status_code: i64,
    response_body: &str,
    response_content_type: &str,
    committed_at: &str,
) -> CortexResult<()> {
    conn.execute(
        "UPDATE operation_journal
         SET status = 'committed',
             request_id = ?2,
             response_status_code = ?3,
             response_body = ?4,
             response_content_type = ?5,
             last_seen_at = ?6,
             committed_at = ?6,
             lease_expires_at = NULL
         WHERE id = ?1",
        params![
            id,
            request_id,
            response_status_code,
            response_body,
            response_content_type,
            committed_at,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn delete_entry(conn: &Connection, id: &str) -> CortexResult<()> {
    conn.execute("DELETE FROM operation_journal WHERE id = ?1", params![id])
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_by_operation_id(
    conn: &Connection,
    operation_id: &str,
) -> CortexResult<Option<OperationJournalRow>> {
    conn.query_row(
        "SELECT id, actor_key, method, route_template, operation_id, request_id,
                idempotency_key, request_fingerprint, request_body, status,
                response_status_code, response_body, response_content_type,
                created_at, last_seen_at, committed_at, lease_expires_at
         FROM operation_journal
         WHERE operation_id = ?1",
        params![operation_id],
        map_row,
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}
