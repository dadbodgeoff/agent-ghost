//! Migration v036: Add citation_count column to memory_snapshots.
//!
//! Tracks how many times a memory is cited by other memories.
//! Used by the retrieval scorer (Factor 6) to boost frequently-cited content.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "ALTER TABLE memory_snapshots ADD COLUMN citation_count INTEGER NOT NULL DEFAULT 0;",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_memory_snapshots_citation_count
            ON memory_snapshots(citation_count) WHERE citation_count > 0;",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
