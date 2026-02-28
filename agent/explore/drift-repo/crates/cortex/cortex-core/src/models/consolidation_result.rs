use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::consolidation_metrics::ConsolidationMetrics;

/// Result of a consolidation operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConsolidationResult {
    /// IDs of newly created consolidated memories.
    pub created: Vec<String>,
    /// IDs of memories archived after consolidation.
    pub archived: Vec<String>,
    /// Quality metrics for this consolidation.
    pub metrics: ConsolidationMetrics,
}
