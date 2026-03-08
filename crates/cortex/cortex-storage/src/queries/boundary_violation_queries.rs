//! Boundary violation queries.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[allow(clippy::too_many_arguments)]
pub fn insert_violation(
    conn: &Connection,
    id: &str,
    session_id: &str,
    violation_type: &str,
    severity: f64,
    trigger_text_hash: &str,
    matched_patterns: &str,
    action_taken: &str,
    convergence_score: Option<f64>,
    intervention_level: Option<i32>,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO boundary_violations (id, session_id, violation_type, severity,
         trigger_text_hash, matched_patterns, action_taken, convergence_score,
         intervention_level, event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            session_id,
            violation_type,
            severity,
            trigger_text_hash,
            matched_patterns,
            action_taken,
            convergence_score,
            intervention_level,
            event_hash,
            previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_agent_session(
    conn: &Connection,
    session_id: &str,
) -> CortexResult<Vec<ViolationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, violation_type, severity, action_taken,
                    convergence_score, intervention_level, created_at
             FROM boundary_violations WHERE session_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(ViolationRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                violation_type: row.get(2)?,
                severity: row.get(3)?,
                action_taken: row.get(4)?,
                convergence_score: row.get(5)?,
                intervention_level: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn query_by_type(conn: &Connection, violation_type: &str) -> CortexResult<Vec<ViolationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, violation_type, severity, action_taken,
                    convergence_score, intervention_level, created_at
             FROM boundary_violations WHERE violation_type = ?1
             ORDER BY severity DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![violation_type], |row| {
            Ok(ViolationRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                violation_type: row.get(2)?,
                severity: row.get(3)?,
                action_taken: row.get(4)?,
                convergence_score: row.get(5)?,
                intervention_level: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct ViolationRow {
    pub id: String,
    pub session_id: String,
    pub violation_type: String,
    pub severity: f64,
    pub action_taken: String,
    pub convergence_score: Option<f64>,
    pub intervention_level: Option<i32>,
    pub created_at: String,
}
