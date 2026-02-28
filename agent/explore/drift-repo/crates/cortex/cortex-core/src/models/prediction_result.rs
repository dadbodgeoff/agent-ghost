use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Result of predictive preloading.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PredictionResult {
    /// IDs of predicted memories.
    pub memory_ids: Vec<String>,
    /// Signals that triggered the prediction.
    pub signals: Vec<String>,
    /// Overall prediction confidence.
    pub confidence: f64,
}
