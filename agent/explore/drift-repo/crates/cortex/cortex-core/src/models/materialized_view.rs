//! Materialized temporal view — pre-computed knowledge snapshots at significant time points.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::temporal_event::EventActor;

/// A materialized view of the knowledge base at a specific point in time.
///
/// Views are created infrequently (every 2 weeks or on-demand) and provide
/// convenient named time points for diffing and analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedTemporalView {
    /// Unique identifier.
    pub view_id: u64,
    /// Human-readable label (e.g. "sprint-12", "v2.0-release", "Q1-2026").
    pub label: String,
    /// The point in time this view represents.
    pub timestamp: DateTime<Utc>,
    /// Number of active memories at this timestamp.
    pub memory_count: usize,
    /// References to memory_snapshots created for this view.
    pub snapshot_ids: Vec<u64>,
    /// Associated drift metrics snapshot (if computed).
    pub drift_snapshot_id: Option<u64>,
    /// Who created this view.
    pub created_by: EventActor,
    /// Whether this view was auto-created by the scheduler.
    pub auto_refresh: bool,
}

impl PartialEq for MaterializedTemporalView {
    fn eq(&self, other: &Self) -> bool {
        self.view_id == other.view_id
            && self.label == other.label
            && self.timestamp == other.timestamp
            && self.memory_count == other.memory_count
            && self.snapshot_ids == other.snapshot_ids
            && self.drift_snapshot_id == other.drift_snapshot_id
            && self.auto_refresh == other.auto_refresh
    }
}
