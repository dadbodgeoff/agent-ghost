//! Snapshot lookup operations.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::errors::CortexResult;
use cortex_core::models::{MemorySnapshot, SnapshotReason};
use cortex_storage::pool::ReadPool;
use cortex_storage::queries::snapshot_ops::{self, RawSnapshot};

/// Get the nearest snapshot for a memory before a given time.
pub fn get_nearest_snapshot(
    readers: &Arc<ReadPool>,
    memory_id: &str,
    before: DateTime<Utc>,
) -> CortexResult<Option<MemorySnapshot>> {
    let mid = memory_id.to_string();
    let before_str = before.to_rfc3339();
    readers.with_conn(|conn| {
        let raw = snapshot_ops::get_nearest_snapshot(conn, &mid, &before_str)?;
        Ok(raw.map(raw_to_snapshot))
    })
}

/// Get all snapshots for a memory.
pub fn get_snapshots_for_memory(
    readers: &Arc<ReadPool>,
    memory_id: &str,
) -> CortexResult<Vec<MemorySnapshot>> {
    let mid = memory_id.to_string();
    readers.with_conn(|conn| {
        let raw = snapshot_ops::get_snapshots_for_memory(conn, &mid)?;
        Ok(raw.into_iter().map(raw_to_snapshot).collect())
    })
}

fn raw_to_snapshot(raw: RawSnapshot) -> MemorySnapshot {
    let snapshot_at = chrono::DateTime::parse_from_rfc3339(&raw.snapshot_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let reason = match raw.reason.as_str() {
        "event_threshold" => SnapshotReason::EventThreshold,
        "periodic" => SnapshotReason::Periodic,
        "pre_operation" => SnapshotReason::PreOperation,
        "on_demand" => SnapshotReason::OnDemand,
        _ => SnapshotReason::OnDemand,
    };

    MemorySnapshot {
        snapshot_id: raw.snapshot_id,
        memory_id: raw.memory_id,
        snapshot_at,
        state: raw.state,
        event_id: raw.event_id,
        snapshot_reason: reason,
    }
}
