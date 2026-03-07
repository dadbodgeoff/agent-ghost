//! Stream event log queries for reliable SSE delivery.
//!
//! Supports the Write-Ahead Event Log pattern: events are persisted before
//! SSE delivery, enabling client-side recovery after disconnect.

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

// ── Row types ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StreamEventRow {
    pub id: i64,
    pub session_id: String,
    pub message_id: String,
    pub event_type: String,
    pub payload: String,
    pub created_at: String,
}

// ── Queries ────────────────────────────────────────────────────────

/// Insert a stream event, returns the assigned sequence ID.
pub fn insert_stream_event(
    conn: &Connection,
    session_id: &str,
    message_id: &str,
    event_type: &str,
    payload: &str,
) -> CortexResult<i64> {
    conn.execute(
        "INSERT INTO stream_event_log (session_id, message_id, event_type, payload)
         VALUES (?1, ?2, ?3, ?4)",
        params![session_id, message_id, event_type, payload],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    let seq = conn.last_insert_rowid();
    Ok(seq)
}

/// Recover events after a given sequence ID for a specific message.
/// Used by the frontend to recover after SSE disconnect.
pub fn recover_events_after(
    conn: &Connection,
    session_id: &str,
    message_id: &str,
    after_seq: i64,
) -> CortexResult<Vec<StreamEventRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, message_id, event_type, payload, created_at
             FROM stream_event_log
             WHERE session_id = ?1 AND message_id = ?2 AND id > ?3
             ORDER BY id ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id, message_id, after_seq], |row| {
            Ok(StreamEventRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                message_id: row.get(2)?,
                event_type: row.get(3)?,
                payload: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// Delete events older than a given timestamp (periodic cleanup).
pub fn delete_events_before(
    conn: &Connection,
    before: &str,
) -> CortexResult<u64> {
    let deleted = conn
        .execute(
            "DELETE FROM stream_event_log WHERE created_at < ?1",
            params![before],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(deleted as u64)
}

/// Delete all events for a specific message (cleanup after confirmed delivery).
pub fn delete_events_for_message(
    conn: &Connection,
    message_id: &str,
) -> CortexResult<u64> {
    let deleted = conn
        .execute(
            "DELETE FROM stream_event_log WHERE message_id = ?1",
            params![message_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(deleted as u64)
}
