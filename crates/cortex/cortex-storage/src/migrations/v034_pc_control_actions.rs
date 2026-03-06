//! Migration v034: pc_control_actions table for Phase 9 audit trail.
//!
//! Every PC control action (mouse, keyboard, screenshot, etc.) is logged
//! here for post-incident analysis and safety auditing. Blocked actions
//! are also recorded with the denial reason.

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pc_control_actions (
            id          TEXT PRIMARY KEY,
            agent_id    TEXT NOT NULL,
            session_id  TEXT NOT NULL,
            skill_name  TEXT NOT NULL,
            action_type TEXT NOT NULL,
            input_json  TEXT NOT NULL DEFAULT '{}',
            result_json TEXT NOT NULL DEFAULT '{}',
            target_app  TEXT,
            coordinates TEXT,
            blocked     INTEGER NOT NULL DEFAULT 0,
            block_reason TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_pc_actions_agent
            ON pc_control_actions(agent_id, created_at);

        CREATE INDEX IF NOT EXISTS idx_pc_actions_session
            ON pc_control_actions(session_id, created_at);

        CREATE INDEX IF NOT EXISTS idx_pc_actions_skill
            ON pc_control_actions(skill_name, created_at);

        CREATE INDEX IF NOT EXISTS idx_pc_actions_blocked
            ON pc_control_actions(blocked, created_at);"
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
