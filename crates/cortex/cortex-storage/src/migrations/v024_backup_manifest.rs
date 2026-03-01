//! Migration v024: Create backup_manifest table.
//!
//! Stores metadata for point-in-time SQLite backups including BLAKE3
//! checksums for integrity verification.
//!
//! Ref: tasks.md T-3.4.5, §17.10, §17.2.1

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS backup_manifest (
            id              TEXT PRIMARY KEY,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            size_bytes      INTEGER NOT NULL DEFAULT 0,
            entry_count     INTEGER NOT NULL DEFAULT 0,
            blake3_checksum TEXT NOT NULL DEFAULT '',
            status          TEXT NOT NULL DEFAULT 'complete',
            metadata        TEXT NOT NULL DEFAULT '{}'
        );
    ")
    .map_err(|e| to_storage_err(format!("v024 backup_manifest: {e}")))?;

    Ok(())
}
