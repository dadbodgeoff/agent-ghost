//! Intervention history queries.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[allow(clippy::too_many_arguments)]
pub fn insert_intervention(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    session_id: &str,
    intervention_level: i32,
    previous_level: i32,
    trigger_score: f64,
    trigger_signals: &str,
    action_type: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO intervention_history (id, agent_id, session_id, intervention_level,
         previous_level, trigger_score, trigger_signals, action_type,
         event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            agent_id,
            session_id,
            intervention_level,
            previous_level,
            trigger_score,
            trigger_signals,
            action_type,
            event_hash,
            previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_agent(conn: &Connection, agent_id: &str) -> CortexResult<Vec<InterventionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, intervention_level, previous_level,
                    trigger_score, action_type, created_at
             FROM intervention_history WHERE agent_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id], |row| {
            Ok(InterventionRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                session_id: row.get(2)?,
                intervention_level: row.get(3)?,
                previous_level: row.get(4)?,
                trigger_score: row.get(5)?,
                action_type: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn query_by_level(conn: &Connection, level: i32) -> CortexResult<Vec<InterventionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, intervention_level, previous_level,
                    trigger_score, action_type, created_at
             FROM intervention_history WHERE intervention_level = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![level], |row| {
            Ok(InterventionRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                session_id: row.get(2)?,
                intervention_level: row.get(3)?,
                previous_level: row.get(4)?,
                trigger_score: row.get(5)?,
                action_type: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct InterventionRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub intervention_level: i32,
    pub previous_level: i32,
    pub trigger_score: f64,
    pub action_type: String,
    pub created_at: String,
}
