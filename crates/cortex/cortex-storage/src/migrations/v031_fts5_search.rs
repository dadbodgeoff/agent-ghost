//! Migration v031: FTS5 full-text search index for memory snapshots
//! - Creates virtual FTS5 table for fast text search
//! - Backfills from existing snapshots
//! - Auto-sync trigger for new snapshots

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    // Create FTS5 virtual table.
    conn.execute_batch("
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
            memory_id,
            summary,
            content,
            tags,
            tokenize='unicode61 remove_diacritics 2'
        );
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    // Backfill from existing snapshots.
    conn.execute_batch("
        INSERT INTO memory_fts(memory_id, summary, content, tags)
            SELECT memory_id,
                   COALESCE(json_extract(snapshot, '$.summary'), ''),
                   COALESCE(json_extract(snapshot, '$.content'), ''),
                   COALESCE(json_extract(snapshot, '$.tags'), '')
            FROM memory_snapshots;
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    // Auto-sync trigger: new snapshots are automatically indexed.
    conn.execute_batch("
        CREATE TRIGGER IF NOT EXISTS memory_fts_insert AFTER INSERT ON memory_snapshots
        BEGIN
            INSERT INTO memory_fts(memory_id, summary, content, tags)
            VALUES (
                NEW.memory_id,
                COALESCE(json_extract(NEW.snapshot, '$.summary'), ''),
                COALESCE(json_extract(NEW.snapshot, '$.content'), ''),
                COALESCE(json_extract(NEW.snapshot, '$.tags'), '')
            );
        END;
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
