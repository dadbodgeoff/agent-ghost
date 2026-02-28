//! Snapshot creation (single + batch).

use std::sync::Arc;

use chrono::Utc;

use cortex_core::errors::{CortexResult, TemporalError};
use cortex_core::memory::BaseMemory;
use cortex_core::models::SnapshotReason;
use cortex_core::CortexError;
use cortex_storage::pool::WriteConnection;
use cortex_storage::queries::snapshot_ops;

/// Create a snapshot for a single memory. Returns the snapshot_id.
pub async fn create_snapshot(
    writer: &Arc<WriteConnection>,
    memory_id: &str,
    current_state: &BaseMemory,
    reason: SnapshotReason,
) -> CortexResult<u64> {
    let state_json = serde_json::to_vec(current_state).map_err(|e| {
        CortexError::TemporalError(TemporalError::SnapshotCreationFailed(format!(
            "failed to serialize snapshot for memory_id='{}': {}",
            memory_id, e
        )))
    })?;
    let compressed = zstd::encode_all(state_json.as_slice(), 3).map_err(|e| {
        CortexError::TemporalError(TemporalError::SnapshotCreationFailed(format!(
            "zstd compress failed for memory_id='{}': {}",
            memory_id, e
        )))
    })?;

    let snapshot_at = Utc::now().to_rfc3339();
    let reason_str = serde_json::to_string(&reason)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    let mid = memory_id.to_string();

    writer
        .with_conn(move |conn| {
            // Get the current max event_id for this memory.
            let event_id = conn
                .query_row(
                    "SELECT COALESCE(MAX(event_id), 0) FROM memory_events WHERE memory_id = ?1",
                    [&mid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u64;

            snapshot_ops::insert_snapshot(
                conn,
                &mid,
                &snapshot_at,
                &compressed,
                event_id,
                &reason_str,
            )
        })
        .await
}

/// Create snapshots for a batch of memories.
pub async fn create_batch_snapshots(
    writer: &Arc<WriteConnection>,
    memories: &[(String, BaseMemory)],
    reason: SnapshotReason,
) -> CortexResult<Vec<u64>> {
    let prepared: Vec<_> = memories
        .iter()
        .filter_map(|(mid, state)| {
            let json = serde_json::to_vec(state).ok()?;
            let compressed = zstd::encode_all(json.as_slice(), 3).ok()?;
            Some((mid.clone(), compressed))
        })
        .collect();

    let reason_str = serde_json::to_string(&reason)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    let snapshot_at = Utc::now().to_rfc3339();

    writer
        .with_conn(move |conn| {
            let mut ids = Vec::with_capacity(prepared.len());
            for (mid, compressed) in &prepared {
                let event_id = conn
                    .query_row(
                        "SELECT COALESCE(MAX(event_id), 0) FROM memory_events WHERE memory_id = ?1",
                        [mid],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap_or(0) as u64;

                let id = snapshot_ops::insert_snapshot(
                    conn,
                    mid,
                    &snapshot_at,
                    compressed,
                    event_id,
                    &reason_str,
                )?;
                ids.push(id);
            }
            Ok(ids)
        })
        .await
}

/// Decompress a zstd-compressed BaseMemory snapshot.
pub fn decompress_snapshot(compressed: &[u8]) -> CortexResult<BaseMemory> {
    let decompressed = zstd::decode_all(compressed).map_err(|e| {
        CortexError::TemporalError(TemporalError::ReconstructionFailed(format!(
            "zstd decompress: {e}"
        )))
    })?;
    serde_json::from_slice(&decompressed).map_err(|e| {
        CortexError::TemporalError(TemporalError::ReconstructionFailed(format!(
            "deserialize snapshot: {e}"
        )))
    })
}
