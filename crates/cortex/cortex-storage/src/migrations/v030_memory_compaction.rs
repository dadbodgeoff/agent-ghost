//! Migration v030: Memory compaction support
//! - Creates compaction_runs and compaction_event_ranges tables
//! - Tracks which event ranges have been summarized into snapshots
//! - Preserves append-only invariant on memory_events

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS compaction_runs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at          TEXT NOT NULL DEFAULT (datetime('now')),
            completed_at        TEXT,
            memories_processed  INTEGER NOT NULL DEFAULT 0,
            events_compacted    INTEGER NOT NULL DEFAULT 0,
            status              TEXT NOT NULL DEFAULT 'running'
        );

        CREATE TABLE IF NOT EXISTS compaction_event_ranges (
            compaction_run_id   INTEGER NOT NULL REFERENCES compaction_runs(id),
            memory_id           TEXT NOT NULL,
            min_event_id        INTEGER NOT NULL,
            max_event_id        INTEGER NOT NULL,
            summary_snapshot_id INTEGER,
            PRIMARY KEY (memory_id, min_event_id)
        );

        CREATE INDEX IF NOT EXISTS idx_compaction_ranges_memory
            ON compaction_event_ranges(memory_id);
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
