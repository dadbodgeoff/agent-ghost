//! Migration v043: Session lifecycle — add last_activity_at and deleted_at columns.
//!
//! WP9-D: Supports TTL-based cleanup of old sessions. `last_activity_at` is
//! updated on every message insert. Sessions beyond TTL are soft-deleted
//! (deleted_at set), then hard-deleted after 2x TTL.

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "-- Add lifecycle columns to studio_chat_sessions.
         ALTER TABLE studio_chat_sessions
             ADD COLUMN last_activity_at TEXT NOT NULL DEFAULT (datetime('now'));

         ALTER TABLE studio_chat_sessions
             ADD COLUMN deleted_at TEXT DEFAULT NULL;

         -- Backfill last_activity_at from the latest message or updated_at.
         UPDATE studio_chat_sessions
         SET last_activity_at = COALESCE(
             (SELECT MAX(created_at) FROM studio_chat_messages
              WHERE studio_chat_messages.session_id = studio_chat_sessions.id),
             updated_at,
             created_at
         );

         -- Index for efficient TTL cleanup queries.
         CREATE INDEX IF NOT EXISTS idx_studio_sessions_last_activity
             ON studio_chat_sessions(last_activity_at)
             WHERE deleted_at IS NULL;

         CREATE INDEX IF NOT EXISTS idx_studio_sessions_deleted_at
             ON studio_chat_sessions(deleted_at)
             WHERE deleted_at IS NOT NULL;
        "
    ).map_err(|e| to_storage_err(e.to_string()))
}
