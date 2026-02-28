//! Convergence score queries.

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn insert_score(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    session_id: Option<&str>,
    composite_score: f64,
    signal_scores: &str,
    level: i32,
    profile: &str,
    computed_at: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO convergence_scores (id, agent_id, session_id, composite_score,
         signal_scores, level, profile, computed_at, event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id, agent_id, session_id, composite_score, signal_scores,
            level, profile, computed_at, event_hash, previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_agent(conn: &Connection, agent_id: &str) -> CortexResult<Vec<ScoreRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, composite_score, signal_scores, level,
                    profile, computed_at
             FROM convergence_scores WHERE agent_id = ?1
             ORDER BY computed_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id], |row| {
            Ok(ScoreRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                session_id: row.get(2)?,
                composite_score: row.get(3)?,
                signal_scores: row.get(4)?,
                level: row.get(5)?,
                profile: row.get(6)?,
                computed_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn latest_by_agent(conn: &Connection, agent_id: &str) -> CortexResult<Option<ScoreRow>> {
    let rows = query_by_agent(conn, agent_id)?;
    Ok(rows.into_iter().next())
}

#[derive(Debug, Clone)]
pub struct ScoreRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub composite_score: f64,
    pub signal_scores: String,
    pub level: i32,
    pub profile: String,
    pub computed_at: String,
}
