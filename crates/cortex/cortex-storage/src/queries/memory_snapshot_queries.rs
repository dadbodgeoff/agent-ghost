//! Memory snapshot queries (v016 memory_snapshots table).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

pub fn insert_snapshot(
    conn: &Connection,
    memory_id: &str,
    snapshot: &str,
    state_hash: Option<&[u8]>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO memory_snapshots (memory_id, snapshot, state_hash)
         VALUES (?1, ?2, ?3)",
        params![memory_id, snapshot, state_hash],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_memory(conn: &Connection, memory_id: &str) -> CortexResult<Vec<SnapshotRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, memory_id, snapshot, created_at
             FROM memory_snapshots WHERE memory_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![memory_id], |row| {
            Ok(SnapshotRow {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                snapshot: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn latest_by_memory(conn: &Connection, memory_id: &str) -> CortexResult<Option<SnapshotRow>> {
    let rows = query_by_memory(conn, memory_id)?;
    Ok(rows.into_iter().next())
}

pub fn latest_for_actor(
    conn: &Connection,
    actor_id: &str,
    limit: u32,
) -> CortexResult<Vec<SnapshotRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at
             FROM memory_snapshots ms
             JOIN (
                 SELECT memory_id, MAX(id) AS max_id
                 FROM memory_snapshots
                 GROUP BY memory_id
             ) latest ON latest.max_id = ms.id
             WHERE EXISTS (
                 SELECT 1
                 FROM memory_events me
                 WHERE me.memory_id = ms.memory_id
                   AND me.actor_id = ?1
             )
             ORDER BY ms.created_at DESC
             LIMIT ?2",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![actor_id, limit], |row| {
            Ok(SnapshotRow {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                snapshot: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct SnapshotRow {
    pub id: i64,
    pub memory_id: String,
    pub snapshot: String,
    pub created_at: String,
}
