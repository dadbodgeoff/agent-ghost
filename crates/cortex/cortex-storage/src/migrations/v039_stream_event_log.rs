//! Migration v039: Stream event log for reliable SSE delivery.
//!
//! Write-ahead event log for agent streaming responses. Events are persisted
//! before SSE delivery so clients can recover after disconnect.
//! Text deltas are coalesced into chunks (not stored individually).
//! Heartbeats are ephemeral and not stored.

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS stream_event_log (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT    NOT NULL,
            message_id TEXT    NOT NULL,
            event_type TEXT    NOT NULL,
            payload    TEXT    NOT NULL,
            created_at TEXT    NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (session_id) REFERENCES studio_chat_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_stream_event_recovery ON stream_event_log(session_id, message_id, id);"
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
