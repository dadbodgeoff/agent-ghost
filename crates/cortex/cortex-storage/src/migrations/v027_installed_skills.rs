//! Migration v027: installed_skills table for skill management (T-4.2.1).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS installed_skills (
            id              TEXT PRIMARY KEY,
            skill_name      TEXT NOT NULL UNIQUE,
            version         TEXT NOT NULL,
            description     TEXT NOT NULL DEFAULT '',
            capabilities    TEXT NOT NULL DEFAULT '[]',
            source          TEXT NOT NULL DEFAULT 'bundled',
            state           TEXT NOT NULL DEFAULT 'active',
            installed_at    TEXT NOT NULL DEFAULT (datetime('now')),
            installed_by    TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_installed_skills_state ON installed_skills(state);
        CREATE INDEX IF NOT EXISTS idx_installed_skills_name ON installed_skills(skill_name);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
