//! Retrieval scorer with convergence as 11th factor.

use cortex_core::memory::BaseMemory;
use serde::{Deserialize, Serialize};

/// Weights for the 11-factor retrieval scorer.
///
/// Factors 1-10 are existing retrieval factors.
/// Factor 11 is the convergence score — memories associated with
/// high-convergence contexts are deprioritized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScorerWeights {
    pub relevance: f64,
    pub recency: f64,
    pub importance: f64,
    pub confidence: f64,
    pub access_frequency: f64,
    pub citation_count: f64,
    pub type_affinity: f64,
    pub tag_match: f64,
    pub embedding_similarity: f64,
    pub pattern_alignment: f64,
    /// Factor 11: Convergence-aware deprioritization.
    /// Higher convergence score → lower retrieval priority for
    /// emotional/attachment content.
    pub convergence: f64,
}

impl Default for ScorerWeights {
    fn default() -> Self {
        Self {
            relevance: 0.20,
            recency: 0.10,
            importance: 0.10,
            confidence: 0.05,
            access_frequency: 0.05,
            citation_count: 0.05,
            type_affinity: 0.10,
            tag_match: 0.05,
            embedding_similarity: 0.15,
            pattern_alignment: 0.10,
            convergence: 0.05,
        }
    }
}

/// Retrieval scorer that includes convergence as a factor.
pub struct RetrievalScorer {
    pub weights: ScorerWeights,
}

impl RetrievalScorer {
    pub fn new(weights: ScorerWeights) -> Self {
        Self { weights }
    }

    /// Score a memory for retrieval relevance.
    /// `convergence_score` is the current agent convergence score [0.0, 1.0].
    /// Higher convergence → emotional/attachment memories scored lower.
    pub fn score(&self, memory: &BaseMemory, convergence_score: f64) -> f64 {
        let base_score = self.base_score(memory);
        let convergence_factor = self.convergence_factor(memory, convergence_score);
        (base_score + self.weights.convergence * convergence_factor).clamp(0.0, 1.0)
    }

    fn base_score(&self, memory: &BaseMemory) -> f64 {
        // Simplified base scoring — in production this would use all 10 factors
        let importance_score = match memory.importance {
            cortex_core::memory::Importance::Critical => 1.0,
            cortex_core::memory::Importance::High => 0.8,
            cortex_core::memory::Importance::Normal => 0.5,
            cortex_core::memory::Importance::Low => 0.3,
            cortex_core::memory::Importance::Trivial => 0.1,
        };
        self.weights.importance * importance_score + self.weights.confidence * memory.confidence
    }

    /// Convergence factor: deprioritize emotional/attachment content at high convergence.
    fn convergence_factor(&self, memory: &BaseMemory, convergence_score: f64) -> f64 {
        use cortex_core::memory::types::MemoryType;
        let is_emotional = matches!(
            memory.memory_type,
            MemoryType::AttachmentIndicator
                | MemoryType::Conversation
                | MemoryType::Feedback
                | MemoryType::Preference
        );

        if is_emotional {
            // Higher convergence → lower score for emotional content
            1.0 - convergence_score
        } else {
            1.0 // Non-emotional content unaffected
        }
    }
}

impl Default for RetrievalScorer {
    fn default() -> Self {
        Self::new(ScorerWeights::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_core::memory::{BaseMemory, Importance};
    use cortex_core::memory::types::MemoryType;
    use uuid::Uuid;

    fn make_memory(memory_type: MemoryType) -> BaseMemory {
        BaseMemory {
            id: Uuid::now_v7(),
            memory_type,
            content: serde_json::json!({}),
            summary: "test".into(),
            importance: Importance::Normal,
            confidence: 0.8,
            created_at: chrono::Utc::now(),
            last_accessed: None,
            access_count: 0,
            tags: vec![],
            archived: false,
        }
    }

    #[test]
    fn retrieval_scorer_includes_convergence_factor() {
        let scorer = RetrievalScorer::default();
        let memory = make_memory(MemoryType::AttachmentIndicator);

        let score_low = scorer.score(&memory, 0.0);
        let score_high = scorer.score(&memory, 1.0);

        assert!(
            score_low >= score_high,
            "Emotional memory should score lower at high convergence"
        );
    }

    #[test]
    fn non_emotional_memory_unaffected_by_convergence() {
        let scorer = RetrievalScorer::default();
        let memory = make_memory(MemoryType::Core);

        let score_low = scorer.score(&memory, 0.0);
        let score_high = scorer.score(&memory, 1.0);

        assert!(
            (score_low - score_high).abs() < 0.01,
            "Non-emotional memory should be unaffected by convergence"
        );
    }
}
