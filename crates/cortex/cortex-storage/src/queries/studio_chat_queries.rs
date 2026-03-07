//! Studio chat session persistence queries.

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

// ── Row types ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StudioSessionRow {
    pub id: String,
    pub title: String,
    pub model: String,
    pub system_prompt: String,
    pub temperature: f64,
    pub max_tokens: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StudioMessageRow {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub token_count: i64,
    pub safety_status: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct StudioSafetyAuditRow {
    pub id: String,
    pub session_id: String,
    pub message_id: String,
    pub check_type: String,
    pub result: String,
    pub detail: Option<String>,
    pub created_at: String,
}

// ── Sessions ───────────────────────────────────────────────────────

pub fn create_session(
    conn: &Connection,
    id: &str,
    title: &str,
    model: &str,
    system_prompt: &str,
    temperature: f64,
    max_tokens: i64,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO studio_chat_sessions (id, title, model, system_prompt, temperature, max_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, title, model, system_prompt, temperature, max_tokens],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn list_sessions(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<StudioSessionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, model, system_prompt, temperature, max_tokens, created_at, updated_at
             FROM studio_chat_sessions
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC
             LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![limit, offset], map_session_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn get_session(conn: &Connection, id: &str) -> CortexResult<Option<StudioSessionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, model, system_prompt, temperature, max_tokens, created_at, updated_at
             FROM studio_chat_sessions WHERE id = ?1 AND deleted_at IS NULL",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![id], map_session_row)
        .map_err(|e| to_storage_err(e.to_string()))?;

    match rows.next() {
        Some(Ok(row)) => Ok(Some(row)),
        Some(Err(e)) => Err(to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn update_session_title(conn: &Connection, id: &str, title: &str) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE studio_chat_sessions SET title = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, title],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn update_session_settings(
    conn: &Connection,
    id: &str,
    model: &str,
    system_prompt: &str,
    temperature: f64,
    max_tokens: i64,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE studio_chat_sessions
             SET model = ?2, system_prompt = ?3, temperature = ?4, max_tokens = ?5,
                 updated_at = datetime('now')
             WHERE id = ?1",
            params![id, model, system_prompt, temperature, max_tokens],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

/// List sessions with `active_since` filter (WP9-D).
/// Only returns non-deleted sessions active since the given datetime.
pub fn list_sessions_active_since(
    conn: &Connection,
    active_since: &str,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<StudioSessionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, model, system_prompt, temperature, max_tokens, created_at, updated_at
             FROM studio_chat_sessions
             WHERE deleted_at IS NULL AND last_activity_at >= ?1
             ORDER BY last_activity_at DESC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![active_since, limit, offset], map_session_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// Soft-delete sessions whose last_activity_at is older than the cutoff (WP9-D).
/// Returns the number of sessions soft-deleted.
pub fn soft_delete_inactive_sessions(
    conn: &Connection,
    cutoff: &str,
) -> CortexResult<usize> {
    let count = conn
        .execute(
            "UPDATE studio_chat_sessions
             SET deleted_at = datetime('now')
             WHERE deleted_at IS NULL AND last_activity_at < ?1",
            params![cutoff],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(count)
}

/// Hard-delete sessions that were soft-deleted before the given cutoff (WP9-D).
/// Also cascades to messages and safety audits within a single transaction
/// to prevent orphaned rows if the process crashes mid-operation.
/// Returns the number of sessions permanently removed.
pub fn hard_delete_old_sessions(
    conn: &Connection,
    deleted_before: &str,
) -> CortexResult<usize> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| to_storage_err(format!("begin hard_delete transaction: {e}")))?;

    let result = (|| -> CortexResult<usize> {
        // Delete messages and audits first (no FK CASCADE in SQLite without pragma).
        conn.execute(
            "DELETE FROM studio_chat_messages WHERE session_id IN (
                SELECT id FROM studio_chat_sessions
                WHERE deleted_at IS NOT NULL AND deleted_at < ?1
            )",
            params![deleted_before],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "DELETE FROM studio_chat_safety_audit WHERE session_id IN (
                SELECT id FROM studio_chat_sessions
                WHERE deleted_at IS NOT NULL AND deleted_at < ?1
            )",
            params![deleted_before],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        let count = conn
            .execute(
                "DELETE FROM studio_chat_sessions
                 WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                params![deleted_before],
            )
            .map_err(|e| to_storage_err(e.to_string()))?;
        Ok(count)
    })();

    match result {
        Ok(count) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| to_storage_err(format!("commit hard_delete transaction: {e}")))?;
            Ok(count)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

pub fn delete_session(conn: &Connection, id: &str) -> CortexResult<bool> {
    let deleted = conn
        .execute(
            "DELETE FROM studio_chat_sessions WHERE id = ?1",
            params![id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(deleted > 0)
}

// ── Messages ───────────────────────────────────────────────────────

pub fn insert_message(
    conn: &Connection,
    id: &str,
    session_id: &str,
    role: &str,
    content: &str,
    token_count: i64,
    safety_status: &str,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO studio_chat_messages (id, session_id, role, content, token_count, safety_status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, session_id, role, content, token_count, safety_status],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // Touch parent session's updated_at and last_activity_at (WP9-D).
    conn.execute(
        "UPDATE studio_chat_sessions SET updated_at = datetime('now'), last_activity_at = datetime('now') WHERE id = ?1",
        params![session_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}

pub fn list_messages(conn: &Connection, session_id: &str) -> CortexResult<Vec<StudioMessageRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, role, content, token_count, safety_status, created_at
             FROM studio_chat_messages
             WHERE session_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id], map_message_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

// ── Safety audit ───────────────────────────────────────────────────

pub fn insert_safety_audit(
    conn: &Connection,
    id: &str,
    session_id: &str,
    message_id: &str,
    check_type: &str,
    result: &str,
    detail: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO studio_chat_safety_audit (id, session_id, message_id, check_type, result, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, session_id, message_id, check_type, result, detail],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

// ── Row mappers ────────────────────────────────────────────────────

fn map_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StudioSessionRow> {
    Ok(StudioSessionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        model: row.get(2)?,
        system_prompt: row.get(3)?,
        temperature: row.get(4)?,
        max_tokens: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn map_message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StudioMessageRow> {
    Ok(StudioMessageRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        token_count: row.get(4)?,
        safety_status: row.get(5)?,
        created_at: row.get(6)?,
    })
}
