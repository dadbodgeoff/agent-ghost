//! ITemporalEngine — the 13th trait in cortex-core.
//! Defines the complete temporal reasoning interface.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::CortexResult;
use crate::memory::BaseMemory;
use crate::models::{
    AsOfQuery, DecisionReplay, DecisionReplayQuery, DriftAlert, DriftSnapshot,
    MaterializedTemporalView, MemoryEvent, TemporalCausalQuery, TemporalDiff, TemporalDiffQuery,
    TemporalRangeQuery,
};

/// A node discovered during temporal causal traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalTraversalNode {
    pub memory_id: String,
    pub depth: usize,
    pub path_strength: f64,
}

/// Result of a temporal causal traversal.
///
/// This is the cortex-core equivalent of cortex-causal's TraversalResult,
/// avoiding a dependency from cortex-core → cortex-causal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalTraversalResult {
    /// The starting node.
    pub origin_id: String,
    /// Nodes discovered during traversal.
    pub nodes: Vec<TemporalTraversalNode>,
    /// Maximum depth actually reached.
    pub max_depth_reached: usize,
}

/// Temporal reasoning engine trait.
///
/// Phase A implements: record_event, get_events, reconstruct_at, reconstruct_all_at.
/// Phase B implements: query_as_of, query_range, query_diff.
/// Phase C implements: replay_decision, query_temporal_causal.
/// Phase D1 implements: compute_drift_metrics, get_drift_alerts.
/// Phase D2 implements: create_view, get_view.
#[allow(async_fn_in_trait)]
pub trait ITemporalEngine: Send + Sync {
    // Event store (TR1)
    async fn record_event(&self, event: MemoryEvent) -> CortexResult<u64>;
    async fn get_events(
        &self,
        memory_id: &str,
        before: Option<DateTime<Utc>>,
    ) -> CortexResult<Vec<MemoryEvent>>;

    // State reconstruction (TR2)
    async fn reconstruct_at(
        &self,
        memory_id: &str,
        as_of: DateTime<Utc>,
    ) -> CortexResult<Option<BaseMemory>>;
    async fn reconstruct_all_at(&self, as_of: DateTime<Utc>) -> CortexResult<Vec<BaseMemory>>;

    // Temporal queries (TR3 - Phase B)
    async fn query_as_of(&self, query: &AsOfQuery) -> CortexResult<Vec<BaseMemory>>;
    async fn query_range(&self, query: &TemporalRangeQuery) -> CortexResult<Vec<BaseMemory>>;
    async fn query_diff(&self, query: &TemporalDiffQuery) -> CortexResult<TemporalDiff>;

    // Decision replay + temporal causal (TR3 - Phase C)
    async fn replay_decision(&self, query: &DecisionReplayQuery) -> CortexResult<DecisionReplay>;
    async fn query_temporal_causal(
        &self,
        query: &TemporalCausalQuery,
    ) -> CortexResult<TemporalTraversalResult>;

    // Drift metrics + alerting (TR6, TR7 - Phase D1)
    async fn compute_drift_metrics(
        &self,
        window_hours: u64,
    ) -> CortexResult<DriftSnapshot>;
    async fn get_drift_alerts(&self) -> CortexResult<Vec<DriftAlert>>;

    // Materialized views (TR9 - Phase D2)
    async fn create_view(
        &self,
        label: &str,
        timestamp: DateTime<Utc>,
    ) -> CortexResult<MaterializedTemporalView>;
    async fn get_view(
        &self,
        label: &str,
    ) -> CortexResult<Option<MaterializedTemporalView>>;
}
