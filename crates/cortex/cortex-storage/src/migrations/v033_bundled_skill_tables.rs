//! Migration v033: notes and timers tables for Phase 8 bundled skills.
//!
//! `agent_notes` — structured note storage for the `note_take` skill.
//! `agent_timers` — reminder/timer entries for the `timer_set` skill.

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_notes (
            id          TEXT PRIMARY KEY,
            agent_id    TEXT NOT NULL,
            session_id  TEXT NOT NULL,
            title       TEXT NOT NULL,
            content     TEXT NOT NULL,
            tags        TEXT NOT NULL DEFAULT '[]',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_agent_notes_agent ON agent_notes(agent_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_agent_notes_title ON agent_notes(title);

        CREATE TABLE IF NOT EXISTS agent_timers (
            id          TEXT PRIMARY KEY,
            agent_id    TEXT NOT NULL,
            session_id  TEXT NOT NULL,
            label       TEXT NOT NULL,
            fire_at     TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'pending',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_agent_timers_agent ON agent_timers(agent_id, status);
        CREATE INDEX IF NOT EXISTS idx_agent_timers_fire ON agent_timers(fire_at, status);"
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
