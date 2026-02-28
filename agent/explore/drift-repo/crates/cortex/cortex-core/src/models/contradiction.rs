use serde::{Deserialize, Serialize};

/// A detected contradiction between memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub contradiction_type: ContradictionType,
    /// IDs of the contradicting memories.
    pub memory_ids: Vec<String>,
    /// Confidence delta between the contradicting memories.
    pub confidence_delta: f64,
    /// Human-readable description of the contradiction.
    pub description: String,
    /// The detection strategy that found this contradiction.
    pub detected_by: DetectionStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionType {
    /// Direct opposition ("always X" vs "never X").
    Direct,
    /// Partial conflict (overlapping but not fully opposing).
    Partial,
    /// Newer memory supersedes older on same topic.
    Supersession,
    /// Temporal inconsistency.
    Temporal,
    /// Semantic similarity with negation.
    Semantic,
}

/// Which detection strategy identified the contradiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStrategy {
    /// Embedding similarity + negation patterns.
    Semantic,
    /// "always"/"never" absolute statement conflict.
    AbsoluteStatement,
    /// Newer supersedes older on same topic.
    TemporalSupersession,
    /// Feedback contradictions.
    Feedback,
    /// Same pattern, opposing content.
    CrossPattern,
}
