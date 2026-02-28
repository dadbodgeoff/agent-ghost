//! Temporal query module — point-in-time, range, diff, replay, and causal queries.

pub mod as_of;
pub mod diff;
pub mod integrity;
pub mod range;
pub mod replay;
pub mod temporal_causal;

pub use as_of::execute_as_of;
pub use diff::execute_diff;
pub use diff::execute_diff_reconstructed;
pub use integrity::enforce_temporal_integrity;
pub use range::execute_range;
pub use replay::execute_replay;
pub use temporal_causal::execute_temporal_causal;

use cortex_causal::TraversalResult;
use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::models::{
    AsOfQuery, DecisionReplay, DecisionReplayQuery, TemporalCausalQuery, TemporalDiff,
    TemporalDiffQuery, TemporalRangeQuery,
};
use cortex_storage::pool::ReadPool;
use rusqlite::Connection;
use std::sync::Arc;

/// Temporal query variants that the dispatcher can route.
#[derive(Debug)]
pub enum TemporalQuery {
    /// Point-in-time reconstruction
    AsOf(AsOfQuery),
    /// Range query using Allen's interval algebra
    Range(TemporalRangeQuery),
    /// Diff between two knowledge states
    Diff(TemporalDiffQuery),
    /// Decision replay with historical context
    Replay(DecisionReplayQuery),
    /// Temporal causal graph traversal
    TemporalCausal(TemporalCausalQuery),
}

/// Result of dispatching a temporal query.
#[derive(Debug)]
pub enum TemporalQueryResult {
    /// Memories matching the query
    Memories(Vec<BaseMemory>),
    /// Diff between two states
    Diff(TemporalDiff),
    /// Decision replay result
    Replay(Box<DecisionReplay>),
    /// Causal traversal result
    Traversal(TraversalResult),
}

/// Routes a `TemporalQuery` to the correct handler.
///
/// This dispatcher pattern follows the same approach as
/// `cortex-causal/src/inference/` strategy dispatch.
pub struct TemporalQueryDispatcher;

impl TemporalQueryDispatcher {
    /// Dispatch a temporal query to the appropriate handler.
    ///
    /// For connection-level queries (AsOf, Range, Diff), uses the provided connection.
    /// For pool-level queries (Replay, TemporalCausal), uses the ReadPool.
    pub fn dispatch(conn: &Connection, query: TemporalQuery) -> CortexResult<TemporalQueryResult> {
        match query {
            TemporalQuery::AsOf(q) => {
                let memories = as_of::execute_as_of(conn, &q)?;
                Ok(TemporalQueryResult::Memories(memories))
            }
            TemporalQuery::Range(q) => {
                let memories = range::execute_range(conn, &q)?;
                Ok(TemporalQueryResult::Memories(memories))
            }
            TemporalQuery::Diff(q) => {
                let diff = diff::execute_diff(conn, &q)?;
                Ok(TemporalQueryResult::Diff(diff))
            }
            TemporalQuery::Replay(_) | TemporalQuery::TemporalCausal(_) => {
                Err(cortex_core::CortexError::TemporalError(
                    cortex_core::errors::TemporalError::QueryFailed(
                        "Replay and TemporalCausal queries require ReadPool; use dispatch_with_pool".to_string(),
                    ),
                ))
            }
        }
    }

    /// Dispatch queries that require ReadPool access.
    pub fn dispatch_with_pool(
        readers: &Arc<ReadPool>,
        query: TemporalQuery,
    ) -> CortexResult<TemporalQueryResult> {
        match query {
            TemporalQuery::AsOf(q) => {
                let memories = readers.with_conn(|conn| as_of::execute_as_of(conn, &q))?;
                Ok(TemporalQueryResult::Memories(memories))
            }
            TemporalQuery::Range(q) => {
                let memories = readers.with_conn(|conn| range::execute_range(conn, &q))?;
                Ok(TemporalQueryResult::Memories(memories))
            }
            TemporalQuery::Diff(q) => {
                let diff = diff::execute_diff_reconstructed(readers, &q)?;
                Ok(TemporalQueryResult::Diff(diff))
            }
            TemporalQuery::Replay(q) => {
                let result = replay::execute_replay(readers, &q)?;
                Ok(TemporalQueryResult::Replay(Box::new(result)))
            }
            TemporalQuery::TemporalCausal(q) => {
                let result = temporal_causal::execute_temporal_causal(readers, &q)?;
                Ok(TemporalQueryResult::Traversal(result))
            }
        }
    }
}
