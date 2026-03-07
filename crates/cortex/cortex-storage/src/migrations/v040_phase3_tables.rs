//! v040: Phase 3 tables — workflow_executions, channels, session_bookmarks.
//!
//! - workflow_executions: Durable execution state for crash recovery (Task 3.5)
//! - channels: Channel adapter configuration and status (Task 3.1)
//! - session_bookmarks: Replay bookmarks (Task 3.9)

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS workflow_executions (
            id TEXT PRIMARY KEY,
            state TEXT NOT NULL DEFAULT '{}',
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS channels (
            id TEXT PRIMARY KEY,
            channel_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'configuring',
            status_message TEXT,
            agent_id TEXT NOT NULL,
            config TEXT NOT NULL DEFAULT '{}',
            last_message_at TEXT,
            message_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_channels_agent ON channels(agent_id);
        CREATE INDEX IF NOT EXISTS idx_channels_type ON channels(channel_type);

        CREATE TABLE IF NOT EXISTS session_bookmarks (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            event_index INTEGER NOT NULL,
            label TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_session_bookmarks_session ON session_bookmarks(session_id);
        ",
    )
    .map_err(|e| crate::to_storage_err(e.to_string()))?;

    Ok(())
}
