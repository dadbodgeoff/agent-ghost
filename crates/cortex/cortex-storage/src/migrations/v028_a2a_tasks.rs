//! Migration v028: A2A tasks and discovered agents tables (T-4.1.2).

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS a2a_tasks (
            id          TEXT PRIMARY KEY,
            target_agent TEXT NOT NULL DEFAULT 'unknown',
            target_url  TEXT NOT NULL,
            method      TEXT NOT NULL DEFAULT 'tasks/send',
            status      TEXT NOT NULL DEFAULT 'submitted',
            input       TEXT NOT NULL DEFAULT '{}',
            output      TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_a2a_tasks_status ON a2a_tasks(status);
        CREATE INDEX IF NOT EXISTS idx_a2a_tasks_created ON a2a_tasks(created_at);

        CREATE TABLE IF NOT EXISTS discovered_agents (
            name         TEXT PRIMARY KEY,
            description  TEXT NOT NULL DEFAULT '',
            endpoint_url TEXT NOT NULL,
            capabilities TEXT NOT NULL DEFAULT '[]',
            trust_score  REAL NOT NULL DEFAULT 0.0,
            version      TEXT NOT NULL DEFAULT '',
            discovered_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_discovered_agents_trust ON discovered_agents(trust_score);"
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
