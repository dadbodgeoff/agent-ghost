use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Result of 4-dimension memory validation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ValidationResult {
    pub memory_id: String,
    /// Score per dimension (0.0–1.0).
    pub dimension_scores: DimensionScores,
    /// Overall validation score.
    pub overall_score: f64,
    /// Healing actions to apply.
    pub healing_actions: Vec<HealingAction>,
    /// Whether the memory passed validation.
    pub passed: bool,
}

/// Scores for each of the 4 validation dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DimensionScores {
    /// Citation validation: file existence, content hash drift, line validity.
    pub citation: f64,
    /// Temporal validation: expiry, age vs expected lifetime.
    pub temporal: f64,
    /// Contradiction validation: opposing memories, consensus support.
    pub contradiction: f64,
    /// Pattern alignment: linked patterns still exist and are consistent.
    pub pattern_alignment: f64,
}

impl DimensionScores {
    /// Compute the average across all 4 dimensions.
    pub fn average(&self) -> f64 {
        (self.citation + self.temporal + self.contradiction + self.pattern_alignment) / 4.0
    }
}

/// The type of healing action to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum HealingActionType {
    /// Adjust confidence based on validation score.
    ConfidenceAdjust,
    /// Auto-update citations via git rename detection.
    CitationUpdate,
    /// Re-embed memories whose context changed.
    EmbeddingRefresh,
    /// Archive with reason tracking.
    Archival,
    /// Flag for human review when auto-fix isn't safe.
    HumanReviewFlag,
}

/// An automatic healing action.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HealingAction {
    pub action_type: HealingActionType,
    pub description: String,
    pub applied: bool,
}
