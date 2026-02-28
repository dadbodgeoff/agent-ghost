use crate::errors::CortexResult;
use crate::models::LearningResult;
use serde::{Deserialize, Serialize};

/// A correction event that the learning system can analyze.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub original_memory_id: Option<String>,
    pub correction_text: String,
    pub context: String,
    pub source: String,
}

/// Correction analysis and principle extraction.
pub trait ILearner: Send + Sync {
    /// Analyze a correction and extract learning principles.
    fn analyze(&self, correction: &Correction) -> CortexResult<LearningResult>;
}
