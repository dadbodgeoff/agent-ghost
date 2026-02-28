//! TemporalEngine — central orchestrator implementing ITemporalEngine.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::config::TemporalConfig;
use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::models::{
    AsOfQuery, DecisionReplay, DecisionReplayQuery, DriftAlert, DriftSnapshot,
    MaterializedTemporalView, MemoryEvent, TemporalCausalQuery, TemporalDiff, TemporalDiffQuery,
    TemporalRangeQuery,
};
use cortex_core::traits::{ITemporalEngine, TemporalTraversalNode, TemporalTraversalResult};
use cortex_storage::pool::{ReadPool, WriteConnection};

use crate::event_store;
use crate::query;
use crate::snapshot;

/// The temporal reasoning engine.
///
/// Holds references to WriteConnection (event appends, snapshot creation)
/// and ReadPool (all temporal queries) per CR5.
pub struct TemporalEngine {
    pub writer: Arc<WriteConnection>,
    pub readers: Arc<ReadPool>,
    pub config: TemporalConfig,
}

impl TemporalEngine {
    /// Create a new TemporalEngine.
    pub fn new(
        writer: Arc<WriteConnection>,
        readers: Arc<ReadPool>,
        config: TemporalConfig,
    ) -> Self {
        Self {
            writer,
            readers,
            config,
        }
    }
}

impl ITemporalEngine for TemporalEngine {
    async fn record_event(&self, event: MemoryEvent) -> CortexResult<u64> {
        let writer = self.writer.clone();
        event_store::append::append(&writer, &event).await
    }

    async fn get_events(
        &self,
        memory_id: &str,
        before: Option<DateTime<Utc>>,
    ) -> CortexResult<Vec<MemoryEvent>> {
        let readers = self.readers.clone();
        let mid = memory_id.to_string();
        event_store::query::get_events(&readers, &mid, before)
    }

    async fn reconstruct_at(
        &self,
        memory_id: &str,
        as_of: DateTime<Utc>,
    ) -> CortexResult<Option<BaseMemory>> {
        let readers = self.readers.clone();
        let mid = memory_id.to_string();
        snapshot::reconstruct::reconstruct_at(&readers, &mid, as_of)
    }

    async fn reconstruct_all_at(&self, as_of: DateTime<Utc>) -> CortexResult<Vec<BaseMemory>> {
        let readers = self.readers.clone();
        snapshot::reconstruct::reconstruct_all_at(&readers, as_of)
    }

    // Phase B: Temporal queries
    async fn query_as_of(&self, query: &AsOfQuery) -> CortexResult<Vec<BaseMemory>> {
        let readers = self.readers.clone();
        readers.with_conn(|conn| query::as_of::execute_as_of(conn, query))
    }

    async fn query_range(&self, query: &TemporalRangeQuery) -> CortexResult<Vec<BaseMemory>> {
        let readers = self.readers.clone();
        readers.with_conn(|conn| query::range::execute_range(conn, query))
    }

    async fn query_diff(&self, query: &TemporalDiffQuery) -> CortexResult<TemporalDiff> {
        let readers = self.readers.clone();
        query::diff::execute_diff_reconstructed(&readers, query)
    }

    // Phase C: Decision replay + temporal causal
    async fn replay_decision(&self, query: &DecisionReplayQuery) -> CortexResult<DecisionReplay> {
        let readers = self.readers.clone();
        query::replay::execute_replay(&readers, query)
    }

    async fn query_temporal_causal(
        &self,
        query: &TemporalCausalQuery,
    ) -> CortexResult<TemporalTraversalResult> {
        let readers = self.readers.clone();
        let causal_result = query::temporal_causal::execute_temporal_causal(&readers, query)?;

        // Convert cortex_causal::TraversalResult → cortex_core::TemporalTraversalResult
        Ok(TemporalTraversalResult {
            origin_id: causal_result.origin_id,
            nodes: causal_result
                .nodes
                .into_iter()
                .map(|n| TemporalTraversalNode {
                    memory_id: n.memory_id,
                    depth: n.depth,
                    path_strength: n.path_strength,
                })
                .collect(),
            max_depth_reached: causal_result.max_depth_reached,
        })
    }

    // Phase D1: Drift metrics + alerting
    async fn compute_drift_metrics(
        &self,
        window_hours: u64,
    ) -> CortexResult<DriftSnapshot> {
        let readers = self.readers.clone();
        let now = Utc::now();
        let window_start = now - chrono::Duration::hours(window_hours as i64);
        crate::drift::metrics::compute_all_metrics(&readers, window_start, now)
    }

    async fn get_drift_alerts(&self) -> CortexResult<Vec<DriftAlert>> {
        let readers = self.readers.clone();
        let config = self.config.clone();
        let now = Utc::now();
        // F-06: Use configurable drift detection window instead of hardcoded 168h.
        let window_start = now - chrono::Duration::hours(config.drift_detection_window_hours as i64);

        let snapshot =
            crate::drift::metrics::compute_all_metrics(&readers, window_start, now)?;

        // Get recent alerts for dampening
        let recent_alerts = match crate::drift::snapshots::get_latest_drift_snapshot(&readers)? {
            Some(_) => vec![], // In production, we'd store alerts separately
            None => vec![],
        };

        Ok(crate::drift::alerting::evaluate_drift_alerts(
            &snapshot,
            &config,
            &recent_alerts,
        ))
    }

    // Phase D2: Materialized views
    async fn create_view(
        &self,
        label: &str,
        timestamp: DateTime<Utc>,
    ) -> CortexResult<MaterializedTemporalView> {
        crate::views::create::create_materialized_view(
            &self.writer,
            &self.readers,
            label,
            timestamp,
        )
        .await
    }

    async fn get_view(
        &self,
        label: &str,
    ) -> CortexResult<Option<MaterializedTemporalView>> {
        crate::views::query::get_view(&self.readers, label)
    }
}
