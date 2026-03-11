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
    pub state_version: i64,
    pub attempt: i64,
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
    pub state_version: i64,
    pub attempt: i64,
    pub status: &'a str,
    pub state_json: &'a str,
}

pub fn insert(conn: &Connection, record: &NewLiveExecutionRecord<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO live_execution_records (
            id, journal_id, operation_id, route_kind, actor_key, state_version, attempt, status, state_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            record.id,
            record.journal_id,
            record.operation_id,
            record.route_kind,
            record.actor_key,
            record.state_version,
            record.attempt,
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
            "SELECT id, journal_id, operation_id, route_kind, actor_key, state_version, attempt, status, state_json, created_at, updated_at
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
            "SELECT id, journal_id, operation_id, route_kind, actor_key, state_version, attempt, status, state_json, created_at, updated_at
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
        "SELECT id, journal_id, operation_id, route_kind, actor_key, state_version, attempt, status, state_json, created_at, updated_at
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
    state_version: i64,
    status: &str,
    state_json: &str,
) -> CortexResult<()> {
    conn.execute(
        "UPDATE live_execution_records
         SET state_version = ?2, status = ?3, state_json = ?4, updated_at = datetime('now')
         WHERE id = ?1",
        params![id, state_version, status, state_json],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn update_status_and_state_if_in_statuses(
    conn: &Connection,
    id: &str,
    state_version: i64,
    expected_statuses: &[&str],
    status: &str,
    state_json: &str,
) -> CortexResult<bool> {
    if expected_statuses.is_empty() {
        return Ok(false);
    }

    let in_clause = expected_statuses
        .iter()
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "UPDATE live_execution_records
         SET state_version = ?2,
             status = ?3,
             state_json = ?4,
             updated_at = datetime('now')
         WHERE id = ?1
           AND status IN ({in_clause})"
    );

    let updated = conn
        .execute(&sql, params![id, state_version, status, state_json])
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn advance_attempt_and_update_status_if_in_statuses(
    conn: &Connection,
    id: &str,
    state_version: i64,
    expected_statuses: &[&str],
    status: &str,
    state_json: &str,
) -> CortexResult<Option<i64>> {
    if expected_statuses.is_empty() {
        return Ok(None);
    }

    let in_clause = expected_statuses
        .iter()
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "UPDATE live_execution_records
         SET attempt = attempt + 1,
             state_version = ?2,
             status = ?3,
             state_json = ?4,
             updated_at = datetime('now')
         WHERE id = ?1
           AND status IN ({in_clause})"
    );

    let updated = conn
        .execute(&sql, params![id, state_version, status, state_json])
        .map_err(|e| to_storage_err(e.to_string()))?;
    if updated == 0 {
        return Ok(None);
    }

    let attempt = conn
        .query_row(
            "SELECT attempt FROM live_execution_records WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(Some(attempt))
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LiveExecutionRecord> {
    Ok(LiveExecutionRecord {
        id: row.get(0)?,
        journal_id: row.get(1)?,
        operation_id: row.get(2)?,
        route_kind: row.get(3)?,
        actor_key: row.get(4)?,
        state_version: row.get(5)?,
        attempt: row.get(6)?,
        status: row.get(7)?,
        state_json: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_cas_refuses_stale_terminal_overwrite() {
        let conn = crate::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE live_execution_records (
                id TEXT PRIMARY KEY,
                journal_id TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                route_kind TEXT NOT NULL,
                actor_key TEXT NOT NULL,
                state_version INTEGER NOT NULL,
                attempt INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                state_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .unwrap();

        insert(
            &conn,
            &NewLiveExecutionRecord {
                id: "exec-1",
                journal_id: "journal-1",
                operation_id: "op-1",
                route_kind: "agent_chat",
                actor_key: "actor-1",
                state_version: 1,
                attempt: 0,
                status: "completed",
                state_json: "{}",
            },
        )
        .unwrap();

        let updated = update_status_and_state_if_in_statuses(
            &conn,
            "exec-1",
            1,
            &[
                "accepted",
                "preparing",
                "running",
                "recovery_required",
                "cancel_requested",
            ],
            "cancelled",
            "{\"cancelled\":true}",
        )
        .unwrap();

        assert!(!updated);
        let stored = get_by_id(&conn, "exec-1").unwrap().unwrap();
        assert_eq!(stored.status, "completed");
        assert_eq!(stored.state_json, "{}");
    }

    #[test]
    fn advance_attempt_moves_record_into_running_once() {
        let conn = crate::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE live_execution_records (
                id TEXT PRIMARY KEY,
                journal_id TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                route_kind TEXT NOT NULL,
                actor_key TEXT NOT NULL,
                state_version INTEGER NOT NULL,
                attempt INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                state_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .unwrap();

        insert(
            &conn,
            &NewLiveExecutionRecord {
                id: "exec-1",
                journal_id: "journal-1",
                operation_id: "op-1",
                route_kind: "agent_chat",
                actor_key: "actor-1",
                state_version: 1,
                attempt: 0,
                status: "accepted",
                state_json: "{}",
            },
        )
        .unwrap();

        let attempt = advance_attempt_and_update_status_if_in_statuses(
            &conn,
            "exec-1",
            1,
            &["accepted", "preparing"],
            "running",
            "{\"running\":true}",
        )
        .unwrap();

        assert_eq!(attempt, Some(1));
        let stored = get_by_id(&conn, "exec-1").unwrap().unwrap();
        assert_eq!(stored.attempt, 1);
        assert_eq!(stored.status, "running");
    }
}
