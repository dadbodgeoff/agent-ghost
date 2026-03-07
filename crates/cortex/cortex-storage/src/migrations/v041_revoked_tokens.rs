//! v041: Revoked JWT tokens table for persistent revocation across restarts (WP0-B).

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS revoked_tokens (
            jti TEXT PRIMARY KEY,
            revoked_at TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expires
            ON revoked_tokens (expires_at);
        "
    ).map_err(|e| crate::to_storage_err(e.to_string()))?;
    Ok(())
}
