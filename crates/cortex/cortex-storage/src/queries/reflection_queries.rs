//! Reflection entry queries.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

pub fn insert_reflection(
    conn: &Connection,
    id: &str,
    session_id: &str,
    chain_id: &str,
    depth: i32,
    trigger_type: &str,
    reflection_text: &str,
    self_reference_ratio: f64,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO reflection_entries (id, session_id, chain_id, depth,
         trigger_type, reflection_text, self_reference_ratio,
         event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            session_id,
            chain_id,
            depth,
            trigger_type,
            reflection_text,
            self_reference_ratio,
            event_hash,
            previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_session(conn: &Connection, session_id: &str) -> CortexResult<Vec<ReflectionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, chain_id, depth, trigger_type, reflection_text,
                    self_reference_ratio, created_at
             FROM reflection_entries WHERE session_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(ReflectionRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                chain_id: row.get(2)?,
                depth: row.get(3)?,
                trigger_type: row.get(4)?,
                reflection_text: row.get(5)?,
                self_reference_ratio: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn count_per_session(conn: &Connection, session_id: &str) -> CortexResult<u32> {
    conn.query_row(
        "SELECT COUNT(*) FROM reflection_entries WHERE session_id = ?1",
        params![session_id],
        |row| row.get(0),
    )
    .map_err(|e| to_storage_err(e.to_string()))
}

#[derive(Debug, Clone)]
pub struct ReflectionRow {
    pub id: String,
    pub session_id: String,
    pub chain_id: String,
    pub depth: i32,
    pub trigger_type: String,
    pub reflection_text: String,
    pub self_reference_ratio: f64,
    pub created_at: String,
}
