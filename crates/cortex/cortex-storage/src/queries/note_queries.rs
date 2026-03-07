//! Note queries for the `note_take` skill.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

pub fn insert_note(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    session_id: &str,
    title: &str,
    content: &str,
    tags: &str,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO agent_notes (id, agent_id, session_id, title, content, tags)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, agent_id, session_id, title, content, tags],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn update_note(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    title: &str,
    content: &str,
    tags: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE agent_notes SET title = ?3, content = ?4, tags = ?5,
                    updated_at = datetime('now')
             WHERE id = ?1 AND agent_id = ?2",
            params![id, agent_id, title, content, tags],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn delete_note(conn: &Connection, id: &str, agent_id: &str) -> CortexResult<bool> {
    let deleted = conn
        .execute(
            "DELETE FROM agent_notes WHERE id = ?1 AND agent_id = ?2",
            params![id, agent_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(deleted > 0)
}

pub fn get_note(conn: &Connection, id: &str, agent_id: &str) -> CortexResult<Option<NoteRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, title, content, tags, created_at, updated_at
             FROM agent_notes WHERE id = ?1 AND agent_id = ?2",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![id, agent_id], map_note_row)
        .map_err(|e| to_storage_err(e.to_string()))?;

    match rows.next() {
        Some(Ok(row)) => Ok(Some(row)),
        Some(Err(e)) => Err(to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn list_notes(
    conn: &Connection,
    agent_id: &str,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<NoteRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, title, content, tags, created_at, updated_at
             FROM agent_notes WHERE agent_id = ?1
             ORDER BY updated_at DESC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id, limit, offset], map_note_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn search_notes(
    conn: &Connection,
    agent_id: &str,
    query: &str,
    limit: u32,
) -> CortexResult<Vec<NoteRow>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, title, content, tags, created_at, updated_at
             FROM agent_notes
             WHERE agent_id = ?1 AND (title LIKE ?2 OR content LIKE ?2 OR tags LIKE ?2)
             ORDER BY updated_at DESC
             LIMIT ?3",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id, pattern, limit], map_note_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn count_notes(conn: &Connection, agent_id: &str) -> CortexResult<u32> {
    conn.query_row(
        "SELECT COUNT(*) FROM agent_notes WHERE agent_id = ?1",
        params![agent_id],
        |row| row.get(0),
    )
    .map_err(|e| to_storage_err(e.to_string()))
}

fn map_note_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteRow> {
    Ok(NoteRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        session_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        tags: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

#[derive(Debug, Clone)]
pub struct NoteRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub title: String,
    pub content: String,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
}
