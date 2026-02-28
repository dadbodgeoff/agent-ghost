use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Result of analyzing a correction.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LearningResult {
    /// Category of the correction (e.g., "factual", "procedural", "preference").
    pub category: String,
    /// Extracted principle, if any.
    pub principle: Option<String>,
    /// ID of the memory created from this learning, if any.
    pub memory_created: Option<String>,
}
