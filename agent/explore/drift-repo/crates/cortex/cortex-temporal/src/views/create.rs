//! Materialized view creation — snapshots all active memories at a timestamp.
//!
//! This is an expensive operation: it reconstructs all memories and creates
//! snapshots. Views are created infrequently (every 2 weeks or on-demand).

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use cortex_core::errors::CortexResult;
use cortex_core::models::{EventActor, MaterializedTemporalView, SnapshotReason};
use cortex_storage::pool::{ReadPool, WriteConnection};
use cortex_storage::queries::view_ops;

use crate::drift;
use crate::snapshot;

/// Create a materialized view at the given timestamp.
///
/// Steps:
/// 1. Reconstruct all active memories at timestamp via `reconstruct_all_at`
/// 2. Create snapshots for each memory (reason: OnDemand)
/// 3. Compute drift metrics at that point (2-week window)
/// 4. Store drift snapshot, associate with view
/// 5. Store view via `view_ops::insert_materialized_view`
pub async fn create_materialized_view(
    writer: &Arc<WriteConnection>,
    readers: &Arc<ReadPool>,
    label: &str,
    timestamp: DateTime<Utc>,
) -> CortexResult<MaterializedTemporalView> {
    // 1. Reconstruct all active memories at the given timestamp
    let memories = snapshot::reconstruct::reconstruct_all_at(readers, timestamp)?;

    // 2. Create snapshots for each memory
    let memory_pairs: Vec<(String, cortex_core::memory::BaseMemory)> = memories
        .iter()
        .map(|m| (m.id.clone(), m.clone()))
        .collect();

    let snapshot_ids = if memory_pairs.is_empty() {
        vec![]
    } else {
        snapshot::create::create_batch_snapshots(writer, &memory_pairs, SnapshotReason::OnDemand)
            .await?
    };

    // 3. Compute drift metrics at that point (2-week window)
    let window_start = timestamp - Duration::weeks(2);
    let drift_snapshot = drift::metrics::compute_all_metrics(readers, window_start, timestamp)?;

    // 4. Store drift snapshot
    let drift_snapshot_id = drift::snapshots::store_drift_snapshot(writer, &drift_snapshot).await?;

    // 5. Store the view
    let memory_count = memories.len();
    let created_by = EventActor::System("materialized_view_engine".to_string());
    let snapshot_ids_json = serde_json::to_string(&snapshot_ids)
        .map_err(cortex_core::CortexError::SerializationError)?;
    let created_by_json = serde_json::to_string(&created_by)
        .map_err(cortex_core::CortexError::SerializationError)?;
    let ts_str = timestamp.to_rfc3339();
    let lbl = label.to_string();

    let view_id = writer
        .with_conn(move |conn| {
            view_ops::insert_materialized_view(
                conn,
                &view_ops::InsertViewParams {
                    label: &lbl,
                    timestamp: &ts_str,
                    memory_count,
                    snapshot_ids_json: &snapshot_ids_json,
                    drift_snapshot_id: Some(drift_snapshot_id),
                    created_by_json: &created_by_json,
                    auto_refresh: false,
                },
            )
        })
        .await?;

    Ok(MaterializedTemporalView {
        view_id,
        label: label.to_string(),
        timestamp,
        memory_count,
        snapshot_ids,
        drift_snapshot_id: Some(drift_snapshot_id),
        created_by,
        auto_refresh: false,
    })
}
