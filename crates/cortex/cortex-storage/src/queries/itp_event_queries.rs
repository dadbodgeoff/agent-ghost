//! ITP event queries.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[allow(clippy::too_many_arguments)]
pub fn insert_itp_event(
    conn: &Connection,
    id: &str,
    session_id: &str,
    event_type: &str,
    sender: Option<&str>,
    timestamp: &str,
    sequence_number: i64,
    content_hash: Option<&str>,
    content_length: Option<i64>,
    privacy_level: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO itp_events (id, session_id, event_type, sender, timestamp,
         sequence_number, content_hash, content_length, privacy_level,
         event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            session_id,
            event_type,
            sender,
            timestamp,
            sequence_number,
            content_hash,
            content_length,
            privacy_level,
            event_hash,
            previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_session(conn: &Connection, session_id: &str) -> CortexResult<Vec<ITPEventRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, event_type, sender, timestamp, sequence_number,
                    content_hash, event_hash, previous_hash
             FROM itp_events WHERE session_id = ?1
             ORDER BY sequence_number ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(ITPEventRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                event_type: row.get(2)?,
                sender: row.get(3)?,
                timestamp: row.get(4)?,
                sequence_number: row.get(5)?,
                content_hash: row.get(6)?,
                event_hash: row.get(7)?,
                previous_hash: row.get(8)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn query_by_time_range(
    conn: &Connection,
    start: &str,
    end: &str,
) -> CortexResult<Vec<ITPEventRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, event_type, sender, timestamp, sequence_number,
                    content_hash, event_hash, previous_hash
             FROM itp_events WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![start, end], |row| {
            Ok(ITPEventRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                event_type: row.get(2)?,
                sender: row.get(3)?,
                timestamp: row.get(4)?,
                sequence_number: row.get(5)?,
                content_hash: row.get(6)?,
                event_hash: row.get(7)?,
                previous_hash: row.get(8)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn query_recent_hydration_events(
    conn: &Connection,
    session_id: &str,
    limit: u32,
) -> CortexResult<Vec<ITPHydrationEventRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT event_type, timestamp, sequence_number, attributes
             FROM (
                 SELECT event_type, timestamp, sequence_number, attributes
                 FROM itp_events
                 WHERE session_id = ?1
                   AND event_type IN ('turn_complete', 'stream_start', 'text_chunk')
                 ORDER BY sequence_number DESC
                 LIMIT ?2
             )
             ORDER BY sequence_number ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id, limit], |row| {
            Ok(ITPHydrationEventRow {
                event_type: row.get(0)?,
                timestamp: row.get(1)?,
                sequence_number: row.get(2)?,
                attributes: row.get(3)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct ITPEventRow {
    pub id: String,
    pub session_id: String,
    pub event_type: String,
    pub sender: Option<String>,
    pub timestamp: String,
    pub sequence_number: i64,
    pub content_hash: Option<String>,
    pub event_hash: Vec<u8>,
    pub previous_hash: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ITPHydrationEventRow {
    pub event_type: String,
    pub timestamp: String,
    pub sequence_number: i64,
    pub attributes: String,
}
