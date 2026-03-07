//! v032: Memory embeddings table.
//!
//! Stores pre-computed embeddings for memories, enabling vector similarity
//! search alongside FTS5 text search.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS memory_embeddings (
            memory_id   TEXT PRIMARY KEY,
            embedding   BLOB NOT NULL,
            dimensions  INTEGER NOT NULL,
            provider    TEXT NOT NULL DEFAULT 'tfidf',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
