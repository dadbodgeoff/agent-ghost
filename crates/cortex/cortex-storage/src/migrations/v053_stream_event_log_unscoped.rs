//! Migration v053: remove studio-session foreign key from stream_event_log.
//!
//! The stream event log is used by both studio and direct agent chat streaming.
//! Tying it to `studio_chat_sessions` makes agent-chat replay impossible.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_stream_event_recovery;
         ALTER TABLE stream_event_log RENAME TO stream_event_log_v052;
         CREATE TABLE stream_event_log (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             session_id TEXT    NOT NULL,
             message_id TEXT    NOT NULL,
             event_type TEXT    NOT NULL,
             payload    TEXT    NOT NULL,
             created_at TEXT    NOT NULL DEFAULT (datetime('now'))
         );
         INSERT INTO stream_event_log (id, session_id, message_id, event_type, payload, created_at)
         SELECT id, session_id, message_id, event_type, payload, created_at
         FROM stream_event_log_v052;
         DROP TABLE stream_event_log_v052;
         CREATE INDEX idx_stream_event_recovery
             ON stream_event_log(session_id, message_id, id);",
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(())
}
