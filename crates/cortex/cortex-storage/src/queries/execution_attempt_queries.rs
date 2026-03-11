use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionAttemptRow {
    pub execution_id: String,
    pub attempt: i64,
    pub owner_token: Option<String>,
    pub lease_epoch: Option<i64>,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub failure_class: Option<String>,
    pub failure_detail: Option<String>,
    pub updated_at: String,
}

pub struct NewExecutionAttempt<'a> {
    pub execution_id: &'a str,
    pub attempt: i64,
    pub owner_token: Option<&'a str>,
    pub lease_epoch: Option<i64>,
    pub status: &'a str,
    pub started_at: &'a str,
}

pub fn insert_or_ignore(conn: &Connection, attempt: &NewExecutionAttempt<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO execution_attempts (
            execution_id, attempt, owner_token, lease_epoch, status, started_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            attempt.execution_id,
            attempt.attempt,
            attempt.owner_token,
            attempt.lease_epoch,
            attempt.status,
            attempt.started_at,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn update_status(
    conn: &Connection,
    execution_id: &str,
    attempt: i64,
    status: &str,
    ended_at: Option<&str>,
    failure_class: Option<&str>,
    failure_detail: Option<&str>,
    updated_at: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE execution_attempts
             SET status = ?3,
                 ended_at = COALESCE(?4, ended_at),
                 failure_class = ?5,
                 failure_detail = ?6,
                 updated_at = ?7
             WHERE execution_id = ?1 AND attempt = ?2",
            params![
                execution_id,
                attempt,
                status,
                ended_at,
                failure_class,
                failure_detail,
                updated_at,
            ],
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(updated > 0)
}

pub fn list_for_execution(
    conn: &Connection,
    execution_id: &str,
) -> CortexResult<Vec<ExecutionAttemptRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT execution_id, attempt, owner_token, lease_epoch, status,
                    started_at, ended_at, failure_class, failure_detail, updated_at
             FROM execution_attempts
             WHERE execution_id = ?1
             ORDER BY attempt ASC",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;

    let rows = stmt
        .query_map(params![execution_id], map_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(rows)
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExecutionAttemptRow> {
    Ok(ExecutionAttemptRow {
        execution_id: row.get(0)?,
        attempt: row.get(1)?,
        owner_token: row.get(2)?,
        lease_epoch: row.get(3)?,
        status: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        failure_class: row.get(7)?,
        failure_detail: row.get(8)?,
        updated_at: row.get(9)?,
    })
}
