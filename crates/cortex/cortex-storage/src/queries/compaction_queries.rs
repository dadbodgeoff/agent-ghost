//! Memory compaction queries (v030 tables).
//!
//! Compaction works WITH the append-only invariant: original events
//! are never modified or deleted. Instead, we track which event ranges
//! have been "summarized" into a snapshot via `compaction_event_ranges`.

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

/// Find memory_ids with more than `threshold` uncompacted events.
pub fn memories_above_threshold(
    conn: &Connection,
    threshold: i64,
) -> CortexResult<Vec<(String, i64)>> {
    let mut stmt = conn
        .prepare(
            "SELECT me.memory_id, COUNT(*) as cnt
             FROM memory_events me
             LEFT JOIN compaction_event_ranges cer
               ON me.memory_id = cer.memory_id
               AND me.event_id BETWEEN cer.min_event_id AND cer.max_event_id
             WHERE cer.memory_id IS NULL
             GROUP BY me.memory_id
             HAVING cnt > ?1
             ORDER BY cnt DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![threshold], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// Get uncompacted events for a specific memory_id.
pub fn uncompacted_events(
    conn: &Connection,
    memory_id: &str,
) -> CortexResult<Vec<CompactableEvent>> {
    let mut stmt = conn
        .prepare(
            "SELECT me.event_id, me.event_type, me.delta, me.actor_id, me.recorded_at
             FROM memory_events me
             LEFT JOIN compaction_event_ranges cer
               ON me.memory_id = cer.memory_id
               AND me.event_id BETWEEN cer.min_event_id AND cer.max_event_id
             WHERE me.memory_id = ?1 AND cer.memory_id IS NULL
             ORDER BY me.event_id ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![memory_id], |row| {
            Ok(CompactableEvent {
                event_id: row.get(0)?,
                event_type: row.get(1)?,
                delta: row.get(2)?,
                actor_id: row.get(3)?,
                recorded_at: row.get(4)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// Start a new compaction run.
pub fn insert_compaction_run(conn: &Connection) -> CortexResult<i64> {
    conn.execute(
        "INSERT INTO compaction_runs (status) VALUES ('running')",
        [],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

/// Record a compacted event range.
pub fn insert_compaction_range(
    conn: &Connection,
    run_id: i64,
    memory_id: &str,
    min_event_id: i64,
    max_event_id: i64,
    summary_snapshot_id: Option<i64>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO compaction_event_ranges
         (compaction_run_id, memory_id, min_event_id, max_event_id, summary_snapshot_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![run_id, memory_id, min_event_id, max_event_id, summary_snapshot_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Complete a compaction run.
pub fn complete_compaction_run(
    conn: &Connection,
    run_id: i64,
    memories_processed: i64,
    events_compacted: i64,
) -> CortexResult<()> {
    conn.execute(
        "UPDATE compaction_runs SET
         completed_at = datetime('now'),
         memories_processed = ?1,
         events_compacted = ?2,
         status = 'completed'
         WHERE id = ?3",
        params![memories_processed, events_compacted, run_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Mark a compaction run as failed.
pub fn fail_compaction_run(conn: &Connection, run_id: i64) -> CortexResult<()> {
    conn.execute(
        "UPDATE compaction_runs SET
         completed_at = datetime('now'),
         status = 'failed'
         WHERE id = ?1",
        params![run_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Get compaction run history.
pub fn query_compaction_runs(conn: &Connection, limit: u32) -> CortexResult<Vec<CompactionRunRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, started_at, completed_at, memories_processed, events_compacted, status
             FROM compaction_runs
             ORDER BY started_at DESC
             LIMIT ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(CompactionRunRow {
                id: row.get(0)?,
                started_at: row.get(1)?,
                completed_at: row.get(2)?,
                memories_processed: row.get(3)?,
                events_compacted: row.get(4)?,
                status: row.get(5)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct CompactableEvent {
    pub event_id: i64,
    pub event_type: String,
    pub delta: String,
    pub actor_id: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone)]
pub struct CompactionRunRow {
    pub id: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub memories_processed: i64,
    pub events_compacted: i64,
    pub status: String,
}
