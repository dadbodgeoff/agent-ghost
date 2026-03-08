//! Durable execution state for live mutations that cannot always replay a final response.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct LiveExecutionRecord {
    pub id: String,
    pub journal_id: String,
    pub operation_id: String,
    pub route_kind: String,
    pub actor_key: String,
    pub status: String,
    pub state_json: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewLiveExecutionRecord<'a> {
    pub id: &'a str,
    pub journal_id: &'a str,
    pub operation_id: &'a str,
    pub route_kind: &'a str,
    pub actor_key: &'a str,
    pub status: &'a str,
    pub state_json: &'a str,
}

pub fn insert(conn: &Connection, record: &NewLiveExecutionRecord<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO live_execution_records (
            id, journal_id, operation_id, route_kind, actor_key, status, state_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.id,
            record.journal_id,
            record.operation_id,
            record.route_kind,
            record.actor_key,
            record.status,
            record.state_json,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_by_journal_id(
    conn: &Connection,
    journal_id: &str,
) -> CortexResult<Option<LiveExecutionRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, journal_id, operation_id, route_kind, actor_key, status, state_json, created_at, updated_at
             FROM live_execution_records
             WHERE journal_id = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![journal_id], map_row)
        .map_err(|e| to_storage_err(e.to_string()))?;

    match rows.next() {
        Some(Ok(row)) => Ok(Some(row)),
        Some(Err(e)) => Err(to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn get_by_operation_id(
    conn: &Connection,
    operation_id: &str,
) -> CortexResult<Option<LiveExecutionRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, journal_id, operation_id, route_kind, actor_key, status, state_json, created_at, updated_at
             FROM live_execution_records
             WHERE operation_id = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![operation_id], map_row)
        .map_err(|e| to_storage_err(e.to_string()))?;

    match rows.next() {
        Some(Ok(row)) => Ok(Some(row)),
        Some(Err(e)) => Err(to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn get_by_id(conn: &Connection, id: &str) -> CortexResult<Option<LiveExecutionRecord>> {
    conn.query_row(
        "SELECT id, journal_id, operation_id, route_kind, actor_key, status, state_json, created_at, updated_at
         FROM live_execution_records
         WHERE id = ?1",
        params![id],
        map_row,
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

pub fn update_status_and_state(
    conn: &Connection,
    id: &str,
    status: &str,
    state_json: &str,
) -> CortexResult<()> {
    conn.execute(
        "UPDATE live_execution_records
         SET status = ?2, state_json = ?3, updated_at = datetime('now')
         WHERE id = ?1",
        params![id, status, state_json],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LiveExecutionRecord> {
    Ok(LiveExecutionRecord {
        id: row.get(0)?,
        journal_id: row.get(1)?,
        operation_id: row.get(2)?,
        route_kind: row.get(3)?,
        actor_key: row.get(4)?,
        status: row.get(5)?,
        state_json: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
