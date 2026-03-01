//! Migration v026: webhooks table for webhook configuration (T-4.3.1).

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS webhooks (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            secret      TEXT NOT NULL DEFAULT '',
            events      TEXT NOT NULL DEFAULT '[]',
            active      INTEGER NOT NULL DEFAULT 1,
            headers     TEXT NOT NULL DEFAULT '{}',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_webhooks_active ON webhooks(active);"
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
