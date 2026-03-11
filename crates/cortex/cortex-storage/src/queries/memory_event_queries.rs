//! Memory event queries (v016 memory_events table).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_event(
    conn: &Connection,
    memory_id: &str,
    event_type: &str,
    delta: &str,
    actor_id: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO memory_events (memory_id, event_type, delta, actor_id,
         event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            memory_id,
            event_type,
            delta,
            actor_id,
            event_hash,
            previous_hash
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn insert_event_at(
    conn: &Connection,
    memory_id: &str,
    event_type: &str,
    delta: &str,
    actor_id: &str,
    recorded_at: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO memory_events (memory_id, event_type, delta, actor_id,
         recorded_at, event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            memory_id,
            event_type,
            delta,
            actor_id,
            recorded_at,
            event_hash,
            previous_hash
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_memory(conn: &Connection, memory_id: &str) -> CortexResult<Vec<MemoryEventRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT event_id, memory_id, event_type, delta, actor_id, recorded_at
             FROM memory_events WHERE memory_id = ?1
             ORDER BY recorded_at ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![memory_id], |row| {
            Ok(MemoryEventRow {
                event_id: row.get(0)?,
                memory_id: row.get(1)?,
                event_type: row.get(2)?,
                delta: row.get(3)?,
                actor_id: row.get(4)?,
                recorded_at: row.get(5)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn latest_event_hash(conn: &Connection, memory_id: &str) -> CortexResult<Option<Vec<u8>>> {
    conn.query_row(
        "SELECT event_hash
         FROM memory_events
         WHERE memory_id = ?1
         ORDER BY recorded_at DESC, event_id DESC
         LIMIT 1",
        params![memory_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

#[derive(Debug, Clone)]
pub struct MemoryEventRow {
    pub event_id: i64,
    pub memory_id: String,
    pub event_type: String,
    pub delta: String,
    pub actor_id: String,
    pub recorded_at: String,
}
