//! Query helpers for speculative context validation records.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct NewContextAttemptValidation<'a> {
    pub id: &'a str,
    pub attempt_id: &'a str,
    pub gate_name: &'a str,
    pub decision: &'a str,
    pub reason: Option<&'a str>,
    pub score: Option<f64>,
    pub details_json: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ContextAttemptValidationRow {
    pub id: String,
    pub attempt_id: String,
    pub gate_name: String,
    pub decision: String,
    pub reason: Option<String>,
    pub score: Option<f64>,
    pub details_json: Option<String>,
    pub created_at: String,
}

pub fn insert_validation(
    conn: &Connection,
    record: &NewContextAttemptValidation<'_>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO context_attempt_validation (
            id, attempt_id, gate_name, decision, reason, score, details_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.id,
            record.attempt_id,
            record.gate_name,
            record.decision,
            record.reason,
            record.score,
            record.details_json,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn list_for_attempt(
    conn: &Connection,
    attempt_id: &str,
) -> CortexResult<Vec<ContextAttemptValidationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, attempt_id, gate_name, decision, reason, score, details_json, created_at
             FROM context_attempt_validation
             WHERE attempt_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![attempt_id], |row| {
            Ok(ContextAttemptValidationRow {
                id: row.get(0)?,
                attempt_id: row.get(1)?,
                gate_name: row.get(2)?,
                decision: row.get(3)?,
                reason: row.get(4)?,
                score: row.get(5)?,
                details_json: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}
