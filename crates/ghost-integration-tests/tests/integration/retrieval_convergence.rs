//! E2E: Retrieval scoring with convergence factor.
//!
//! Validates cortex-retrieval ↔ cortex-convergence integration.

use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
use cortex_retrieval::{RetrievalScorer, ScorerWeights};
use uuid::Uuid;

fn make_memory(memory_type: MemoryType, importance: Importance) -> BaseMemory {
    BaseMemory {
        id: Uuid::now_v7(),
        memory_type,
        content: serde_json::json!({"text": "test memory"}),
        summary: "test memory".into(),
        importance,
        confidence: 0.8,
        created_at: chrono::Utc::now(),
        last_accessed: None,
        access_count: 0,
        tags: vec![],
        archived: false,
    }
}

/// Emotional memory deprioritized at high convergence.
#[test]
fn emotional_memory_deprioritized_at_high_convergence() {
    let scorer = RetrievalScorer::default();
    let memory = make_memory(MemoryType::AttachmentIndicator, Importance::Normal);

    let score_low_conv = scorer.score(&memory, 0.0);
    let score_high_conv = scorer.score(&memory, 1.0);

    assert!(
        score_low_conv >= score_high_conv,
        "Emotional memory should score lower at high convergence: {} vs {}",
        score_low_conv,
        score_high_conv
    );
}

/// Non-emotional memory unaffected by convergence.
#[test]
fn core_memory_unaffected() {
    let scorer = RetrievalScorer::default();
    let memory = make_memory(MemoryType::Core, Importance::High);

    let score_low = scorer.score(&memory, 0.0);
    let score_high = scorer.score(&memory, 1.0);

    assert!(
        (score_low - score_high).abs() < 0.01,
        "Core memory should be unaffected: {} vs {}",
        score_low,
        score_high
    );
}

/// Scorer weights sum to approximately 1.0.
#[test]
fn default_weights_sum_to_one() {
    let w = ScorerWeights::default();
    let sum = w.relevance
        + w.recency
        + w.importance
        + w.confidence
        + w.access_frequency
        + w.citation_count
        + w.type_affinity
        + w.tag_match
        + w.embedding_similarity
        + w.pattern_alignment
        + w.convergence;

    assert!(
        (sum - 1.0).abs() < 0.01,
        "Default weights should sum to ~1.0, got {}",
        sum
    );
}

/// All memory types produce scores in [0.0, 1.0].
#[test]
fn scores_bounded() {
    let scorer = RetrievalScorer::default();
    let types = [
        MemoryType::Core,
        MemoryType::Conversation,
        MemoryType::Feedback,
        MemoryType::Preference,
        MemoryType::Goal,
        MemoryType::AttachmentIndicator,
    ];

    for mt in &types {
        for imp in [
            Importance::Trivial,
            Importance::Low,
            Importance::Normal,
            Importance::High,
            Importance::Critical,
        ] {
            let memory = make_memory(*mt, imp);
            for conv in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let score = scorer.score(&memory, conv);
                assert!(
                    (0.0..=1.0).contains(&score),
                    "Score {} out of bounds for {:?}/{:?} at conv {}",
                    score,
                    mt,
                    imp,
                    conv
                );
            }
        }
    }
}
