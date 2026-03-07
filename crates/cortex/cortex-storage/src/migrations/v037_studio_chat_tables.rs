//! Migration v037: Studio chat session persistence tables.
//!
//! Three tables for DB-backed chat sessions in Studio:
//! - studio_chat_sessions: session metadata (model, system_prompt, etc.)
//! - studio_chat_messages: per-session message history
//! - studio_chat_safety_audit: safety pipeline audit trail per message

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE studio_chat_sessions (
            id          TEXT PRIMARY KEY,
            title       TEXT NOT NULL DEFAULT 'New Chat',
            model       TEXT NOT NULL DEFAULT 'qwen3.5:9b',
            system_prompt TEXT NOT NULL DEFAULT '',
            temperature REAL NOT NULL DEFAULT 0.5,
            max_tokens  INTEGER NOT NULL DEFAULT 4096,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    conn.execute_batch(
        "CREATE TABLE studio_chat_messages (
            id          TEXT PRIMARY KEY,
            session_id  TEXT NOT NULL REFERENCES studio_chat_sessions(id) ON DELETE CASCADE,
            role        TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
            content     TEXT NOT NULL,
            token_count INTEGER NOT NULL DEFAULT 0,
            safety_status TEXT NOT NULL DEFAULT 'clean',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX idx_studio_msg_session ON studio_chat_messages(session_id, created_at);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    conn.execute_batch(
        "CREATE TABLE studio_chat_safety_audit (
            id          TEXT PRIMARY KEY,
            session_id  TEXT NOT NULL,
            message_id  TEXT NOT NULL,
            check_type  TEXT NOT NULL,
            result      TEXT NOT NULL,
            detail      TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX idx_studio_safety_session ON studio_chat_safety_audit(session_id);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
