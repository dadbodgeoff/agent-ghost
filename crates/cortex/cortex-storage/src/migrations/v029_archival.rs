//! Migration v029: Memory archival support
//! - Creates memory_archival_log table for tracking archived memories
//! - Index on memory_id for fast lookups

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS memory_archival_log (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            memory_id           TEXT NOT NULL,
            archived_at         TEXT NOT NULL DEFAULT (datetime('now')),
            reason              TEXT NOT NULL,
            decayed_confidence  REAL,
            original_confidence REAL
        );

        CREATE INDEX IF NOT EXISTS idx_archival_memory
            ON memory_archival_log(memory_id);
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
