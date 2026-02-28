//! Adaptive snapshot trigger evaluation.

use std::sync::Arc;

use cortex_core::config::TemporalConfig;
use cortex_core::errors::CortexResult;
use cortex_core::models::SnapshotReason;
use cortex_storage::pool::ReadPool;

/// Evaluates whether a memory needs a new snapshot.
pub struct AdaptiveSnapshotTrigger {
    config: TemporalConfig,
}

impl AdaptiveSnapshotTrigger {
    pub fn new(config: TemporalConfig) -> Self {
        Self { config }
    }

    /// Check if a memory should have a snapshot created.
    pub fn should_snapshot(
        &self,
        readers: &Arc<ReadPool>,
        memory_id: &str,
    ) -> CortexResult<Option<SnapshotReason>> {
        let events_since_snapshot = readers.with_conn(|conn| {
            let last_snapshot_event_id: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(event_id), 0) FROM memory_snapshots WHERE memory_id = ?1",
                    [memory_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memory_events WHERE memory_id = ?1 AND event_id > ?2",
                    rusqlite::params![memory_id, last_snapshot_event_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            Ok(count as u64)
        })?;

        if events_since_snapshot >= self.config.snapshot_event_threshold {
            return Ok(Some(SnapshotReason::EventThreshold));
        }

        Ok(None)
    }
}
