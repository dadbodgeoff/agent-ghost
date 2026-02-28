use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Quality metrics for a consolidation operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConsolidationMetrics {
    /// Precision of the consolidation (0.0–1.0).
    pub precision: f64,
    /// Compression ratio achieved.
    pub compression_ratio: f64,
    /// Information lift from consolidation.
    pub lift: f64,
    /// Stability of the consolidated memory.
    pub stability: f64,
}
