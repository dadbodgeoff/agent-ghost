//! Query helpers for speculative context background jobs.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct NewContextAttemptJob<'a> {
    pub id: &'a str,
    pub attempt_id: &'a str,
    pub job_type: &'a str,
    pub status: &'a str,
    pub retry_count: i64,
    pub last_error: Option<&'a str>,
    pub run_after: &'a str,
}

#[derive(Debug, Clone)]
pub struct ContextAttemptJobRow {
    pub id: String,
    pub attempt_id: String,
    pub job_type: String,
    pub status: String,
    pub retry_count: i64,
    pub last_error: Option<String>,
    pub run_after: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn insert_job(conn: &Connection, job: &NewContextAttemptJob<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO context_attempt_jobs (
            id, attempt_id, job_type, status, retry_count, last_error, run_after
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            job.id,
            job.attempt_id,
            job.job_type,
            job.status,
            job.retry_count,
            job.last_error,
            job.run_after,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_job(conn: &Connection, id: &str) -> CortexResult<Option<ContextAttemptJobRow>> {
    conn.query_row(
        "SELECT id, attempt_id, job_type, status, retry_count, last_error, run_after, created_at, updated_at
         FROM context_attempt_jobs
         WHERE id = ?1",
        params![id],
        map_row,
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

pub fn select_due_jobs(
    conn: &Connection,
    job_type: &str,
    now: &str,
    limit: u32,
) -> CortexResult<Vec<ContextAttemptJobRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, attempt_id, job_type, status, retry_count, last_error, run_after, created_at, updated_at
             FROM context_attempt_jobs
             WHERE job_type = ?1
               AND status = 'pending'
               AND run_after <= ?2
             ORDER BY run_after ASC, created_at ASC
             LIMIT ?3",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![job_type, now, limit], map_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(rows)
}

pub fn mark_job_running(conn: &Connection, id: &str) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE context_attempt_jobs
             SET status = 'running', updated_at = datetime('now')
             WHERE id = ?1 AND status = 'pending'",
            params![id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn mark_job_succeeded(conn: &Connection, id: &str) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE context_attempt_jobs
             SET status = 'succeeded', updated_at = datetime('now')
             WHERE id = ?1 AND status IN ('pending', 'running', 'failed')",
            params![id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn mark_job_failed(
    conn: &Connection,
    id: &str,
    last_error: &str,
    retry_count: i64,
    next_status: &str,
    run_after: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE context_attempt_jobs
             SET status = ?2,
                 retry_count = ?3,
                 last_error = ?4,
                 run_after = ?5,
                 updated_at = datetime('now')
             WHERE id = ?1",
            params![id, next_status, retry_count, last_error, run_after],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextAttemptJobRow> {
    Ok(ContextAttemptJobRow {
        id: row.get(0)?,
        attempt_id: row.get(1)?,
        job_type: row.get(2)?,
        status: row.get(3)?,
        retry_count: row.get(4)?,
        last_error: row.get(5)?,
        run_after: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
