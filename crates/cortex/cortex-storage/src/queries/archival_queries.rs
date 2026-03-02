//! Memory archival queries (v029 memory_archival_log table).

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

/// Insert an archival record for a memory.
pub fn insert_archival_record(
    conn: &Connection,
    memory_id: &str,
    reason: &str,
    decayed_confidence: f64,
    original_confidence: f64,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO memory_archival_log (memory_id, reason, decayed_confidence, original_confidence)
         VALUES (?1, ?2, ?3, ?4)",
        params![memory_id, reason, decayed_confidence, original_confidence],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Remove archival record (unarchive a memory).
pub fn remove_archival_record(conn: &Connection, memory_id: &str) -> CortexResult<()> {
    conn.execute(
        "DELETE FROM memory_archival_log WHERE memory_id = ?1",
        params![memory_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Check if a memory is archived.
pub fn is_archived(conn: &Connection, memory_id: &str) -> CortexResult<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_archival_log WHERE memory_id = ?1",
            params![memory_id],
            |row| row.get(0),
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(count > 0)
}

/// List all archived memory_ids.
pub fn query_archived_ids(conn: &Connection) -> CortexResult<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT memory_id FROM memory_archival_log ORDER BY archived_at DESC")
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<String>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// List archived memories with details.
pub fn query_archived(conn: &Connection, limit: u32) -> CortexResult<Vec<ArchivalRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, memory_id, archived_at, reason, decayed_confidence, original_confidence
             FROM memory_archival_log
             ORDER BY archived_at DESC
             LIMIT ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(ArchivalRow {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                archived_at: row.get(2)?,
                reason: row.get(3)?,
                decayed_confidence: row.get(4)?,
                original_confidence: row.get(5)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct ArchivalRow {
    pub id: i64,
    pub memory_id: String,
    pub archived_at: String,
    pub reason: String,
    pub decayed_confidence: Option<f64>,
    pub original_confidence: Option<f64>,
}
