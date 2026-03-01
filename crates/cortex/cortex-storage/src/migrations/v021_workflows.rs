//! Migration v021: Create workflows table.
//!
//! Stores saved workflow definitions for the visual workflow composer.
//! Each workflow is a DAG of agent, gate, and tool nodes with edges.
//!
//! Ref: ADE_DESIGN_PLAN §17.11, tasks.md T-2.1.9

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS workflows (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            nodes       TEXT NOT NULL DEFAULT '[]',
            edges       TEXT NOT NULL DEFAULT '[]',
            created_by  TEXT,
            updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_workflows_name ON workflows(name);
        CREATE INDEX IF NOT EXISTS idx_workflows_created_by ON workflows(created_by);
    ")
    .map_err(|e| to_storage_err(format!("v021 workflows: {e}")))?;

    Ok(())
}
