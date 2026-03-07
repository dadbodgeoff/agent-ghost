//! Migration v022: Create session_event_index table.
//!
//! Pre-computed cumulative gate state snapshots every 50 events,
//! enabling fast session replay without re-computing from scratch.
//!
//! Ref: ADE_DESIGN_PLAN §17.5, §17.2.1, tasks.md T-2.1.7

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS session_event_index (
            id              TEXT PRIMARY KEY,
            session_id      TEXT NOT NULL,
            snapshot_seq    INTEGER NOT NULL,
            gate_state      TEXT NOT NULL DEFAULT '{}',
            cumulative_cost REAL NOT NULL DEFAULT 0.0,
            event_count     INTEGER NOT NULL DEFAULT 0,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_sei_session_seq
            ON session_event_index(session_id, snapshot_seq);
    ",
    )
    .map_err(|e| to_storage_err(format!("v022 session_event_index: {e}")))?;

    Ok(())
}
