use crate::errors::CortexResult;
use crate::models::PredictionResult;
use serde::{Deserialize, Serialize};

/// Signals used for predictive preloading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionSignals {
    pub active_files: Vec<String>,
    pub recent_queries: Vec<String>,
    pub current_intent: Option<String>,
}

/// Predictive memory preloading.
pub trait IPredictor: Send + Sync {
    /// Predict which memories will be needed based on current signals.
    fn predict(&self, signals: &PredictionSignals) -> CortexResult<PredictionResult>;
}
