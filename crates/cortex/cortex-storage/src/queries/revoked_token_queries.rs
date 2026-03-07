//! Queries for the revoked_tokens table (WP0-B).
//!
//! Provides write-through revocation persistence and startup loading
//! so JWT revocations survive gateway restarts.

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

/// Insert a revoked token JTI with its expiration time.
pub fn revoke_token(conn: &Connection, jti: &str, expires_at: &str) -> CortexResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO revoked_tokens (jti, expires_at) VALUES (?1, ?2)",
        rusqlite::params![jti, expires_at],
    )
    .map_err(|e| to_storage_err(format!("revoke_token: {e}")))?;
    Ok(())
}

/// Check if a JTI has been revoked.
pub fn is_revoked(conn: &Connection, jti: &str) -> CortexResult<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM revoked_tokens WHERE jti = ?1",
            rusqlite::params![jti],
            |row| row.get(0),
        )
        .map_err(|e| to_storage_err(format!("is_revoked: {e}")))?;
    Ok(count > 0)
}

/// Load all non-expired revoked JTIs (for startup hydration).
pub fn load_active_revocations(conn: &Connection) -> CortexResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT jti FROM revoked_tokens WHERE expires_at > datetime('now')"
        )
        .map_err(|e| to_storage_err(format!("load_active_revocations: {e}")))?;

    let jtis: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| to_storage_err(format!("load_active_revocations: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(format!("load_active_revocations row: {e}")))?;

    Ok(jtis)
}

/// Remove expired revocations to prevent table bloat.
pub fn cleanup_expired(conn: &Connection) -> CortexResult<usize> {
    let deleted = conn
        .execute(
            "DELETE FROM revoked_tokens WHERE expires_at <= datetime('now')",
            [],
        )
        .map_err(|e| to_storage_err(format!("cleanup_expired: {e}")))?;
    Ok(deleted)
}
