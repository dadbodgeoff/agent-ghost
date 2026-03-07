//! Migration v042: Cost snapshots table for persisting cost tracker state.
//!
//! Allows CostTracker to survive process restarts by persisting per-agent
//! daily totals, per-session totals, and compaction costs.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cost_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scope TEXT NOT NULL,          -- 'agent_daily', 'session', 'compaction'
            entity_id TEXT NOT NULL,       -- agent_id or session_id (UUID)
            amount REAL NOT NULL DEFAULT 0.0,
            snapshot_date TEXT NOT NULL DEFAULT (date('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(scope, entity_id, snapshot_date)
        );

        CREATE INDEX IF NOT EXISTS idx_cost_snapshots_scope_date
            ON cost_snapshots(scope, snapshot_date);
        ",
    )
    .map_err(|e| to_storage_err(e.to_string()))
}
