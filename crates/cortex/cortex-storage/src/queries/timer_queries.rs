//! Timer queries for the `timer_set` skill.

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn insert_timer(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    session_id: &str,
    label: &str,
    fire_at: &str,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO agent_timers (id, agent_id, session_id, label, fire_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, agent_id, session_id, label, fire_at],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn cancel_timer(conn: &Connection, id: &str, agent_id: &str) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE agent_timers SET status = 'cancelled'
             WHERE id = ?1 AND agent_id = ?2 AND status = 'pending'",
            params![id, agent_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn fire_timer(conn: &Connection, id: &str, agent_id: &str) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE agent_timers SET status = 'fired'
             WHERE id = ?1 AND agent_id = ?2 AND status = 'pending'",
            params![id, agent_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn list_timers(
    conn: &Connection,
    agent_id: &str,
    status_filter: Option<&str>,
    limit: u32,
) -> CortexResult<Vec<TimerRow>> {
    let (sql, filter_param) = match status_filter {
        Some(status) => (
            "SELECT id, agent_id, session_id, label, fire_at, status, created_at
             FROM agent_timers WHERE agent_id = ?1 AND status = ?2
             ORDER BY fire_at ASC LIMIT ?3",
            Some(status.to_string()),
        ),
        None => (
            "SELECT id, agent_id, session_id, label, fire_at, status, created_at
             FROM agent_timers WHERE agent_id = ?1
             ORDER BY fire_at ASC LIMIT ?2",
            None,
        ),
    };

    let mut stmt = conn.prepare(sql).map_err(|e| to_storage_err(e.to_string()))?;

    let rows = if let Some(ref status) = filter_param {
        stmt.query_map(params![agent_id, status, limit], map_timer_row)
            .map_err(|e| to_storage_err(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| to_storage_err(e.to_string()))?
    } else {
        stmt.query_map(params![agent_id, limit], map_timer_row)
            .map_err(|e| to_storage_err(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| to_storage_err(e.to_string()))?
    };

    Ok(rows)
}

pub fn pending_due(conn: &Connection, agent_id: &str, now: &str) -> CortexResult<Vec<TimerRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, label, fire_at, status, created_at
             FROM agent_timers
             WHERE agent_id = ?1 AND status = 'pending' AND fire_at <= ?2
             ORDER BY fire_at ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id, now], map_timer_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

fn map_timer_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TimerRow> {
    Ok(TimerRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        session_id: row.get(2)?,
        label: row.get(3)?,
        fire_at: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[derive(Debug, Clone)]
pub struct TimerRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub label: String,
    pub fire_at: String,
    pub status: String,
    pub created_at: String,
}
