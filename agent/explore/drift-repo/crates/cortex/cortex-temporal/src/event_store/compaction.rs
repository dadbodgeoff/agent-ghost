//! Event compaction: move old events to archive table (CR4).

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::errors::CortexResult;
use cortex_storage::pool::WriteConnection;
use cortex_storage::queries::event_ops;

/// Result of a compaction run.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub events_moved: u64,
}

/// Compact events older than `before_date` that have a verified snapshot after them.
pub async fn compact_events(
    writer: &Arc<WriteConnection>,
    before_date: DateTime<Utc>,
    verified_snapshot_event_id: u64,
) -> CortexResult<CompactionResult> {
    let before_str = before_date.to_rfc3339();
    writer
        .with_conn(move |conn| {
            let moved =
                event_ops::move_events_to_archive(conn, &before_str, verified_snapshot_event_id)?;
            Ok(CompactionResult {
                events_moved: moved,
            })
        })
        .await
}
