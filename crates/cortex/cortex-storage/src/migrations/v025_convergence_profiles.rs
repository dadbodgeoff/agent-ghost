//! Migration v025: convergence_profiles table for custom scoring profiles.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS convergence_profiles (
            name TEXT PRIMARY KEY,
            description TEXT NOT NULL DEFAULT '',
            weights TEXT NOT NULL,
            thresholds TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
