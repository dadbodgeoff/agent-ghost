//! Tests for cortex-decay: convergence factor, monotonicity invariant, edge cases.

use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
use cortex_decay::factors::convergence::convergence_factor;
use cortex_decay::factors::DecayContext;
use cortex_decay::formula;

// ── Unit tests ──────────────────────────────────────────────────────────

#[test]
fn score_zero_returns_one_for_all_types() {
    let types = [
        MemoryType::Conversation,
        MemoryType::Feedback,
        MemoryType::Preference,
        MemoryType::Core,
        MemoryType::Episodic,
        MemoryType::Semantic,
    ];
    for mt in &types {
        let f = convergence_factor(mt, 0.0);
        assert!(
            (f - 1.0).abs() < 1e-10,
            "score=0.0, type={:?} should give factor=1.0, got {}",
            mt,
            f
        );
    }
}

#[test]
fn conversation_score_one_returns_three() {
    let f = convergence_factor(&MemoryType::Conversation, 1.0);
    assert!((f - 3.0).abs() < 1e-10, "expected 3.0, got {}", f);
}

#[test]
fn conversation_score_half_returns_two() {
    let f = convergence_factor(&MemoryType::Conversation, 0.5);
    assert!((f - 2.0).abs() < 1e-10, "expected 2.0, got {}", f);
}

#[test]
fn non_sensitive_type_score_one_returns_one() {
    let f = convergence_factor(&MemoryType::Core, 1.0);
    assert!(
        (f - 1.0).abs() < 1e-10,
        "Core type should have 0 sensitivity, got {}",
        f
    );
}

#[test]
fn decay_context_default_convergence_is_zero() {
    let ctx = DecayContext::default();
    assert!((ctx.convergence_score - 0.0).abs() < 1e-10);
}

#[test]
fn decay_breakdown_includes_convergence_field() {
    let memory = BaseMemory {
        id: uuid::Uuid::new_v4(),
        memory_type: MemoryType::Conversation,
        content: serde_json::json!({}),
        summary: "test".to_string(),
        importance: Importance::Normal,
        confidence: 1.0,
        created_at: chrono::Utc::now(),
        last_accessed: None,
        access_count: 0,
        tags: vec![],
        archived: false,
    };
    let ctx = DecayContext {
        convergence_score: 0.5,
        ..Default::default()
    };
    let breakdown = formula::compute_with_breakdown(&memory, &ctx);
    assert!(
        breakdown.convergence >= 1.0,
        "convergence factor should be >= 1.0"
    );
    assert!((breakdown.convergence - 2.0).abs() < 1e-10);
}

// ── Adversarial tests ───────────────────────────────────────────────────

#[test]
fn score_slightly_above_one_clamped() {
    let f = convergence_factor(&MemoryType::Conversation, 1.0001);
    // Should clamp to 1.0 internally, so factor = 1.0 + 2.0 * 1.0 = 3.0
    assert!(
        (f - 3.0).abs() < 1e-10,
        "should clamp to 1.0, got factor {}",
        f
    );
}

#[test]
fn negative_score_clamped_to_zero() {
    let f = convergence_factor(&MemoryType::Conversation, -0.1);
    assert!(
        (f - 1.0).abs() < 1e-10,
        "negative score should clamp to 0.0, factor=1.0, got {}",
        f
    );
}

#[test]
fn nan_score_returns_one() {
    let f = convergence_factor(&MemoryType::Conversation, f64::NAN);
    assert!(
        (f - 1.0).abs() < 1e-10,
        "NaN score should return factor=1.0, got {}",
        f
    );
}

// ── Proptest ────────────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_memory_type() -> impl Strategy<Value = MemoryType> {
        prop_oneof![
            Just(MemoryType::Conversation),
            Just(MemoryType::Feedback),
            Just(MemoryType::Preference),
            Just(MemoryType::Core),
            Just(MemoryType::Episodic),
            Just(MemoryType::Insight),
            Just(MemoryType::Semantic),
            Just(MemoryType::Procedural),
            Just(MemoryType::AttachmentIndicator),
        ]
    }

    proptest! {
        #[test]
        fn factor_always_gte_one(
            mt in arb_memory_type(),
            score in 0.0f64..=1.0,
        ) {
            let f = convergence_factor(&mt, score);
            prop_assert!(f >= 1.0, "factor must be >= 1.0, got {} for {:?} score={}", f, mt, score);
        }

        #[test]
        fn higher_score_higher_or_equal_factor(
            mt in arb_memory_type(),
            s1 in 0.0f64..=1.0,
            s2 in 0.0f64..=1.0,
        ) {
            let (lo, hi) = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
            let f_lo = convergence_factor(&mt, lo);
            let f_hi = convergence_factor(&mt, hi);
            prop_assert!(
                f_hi >= f_lo - 1e-10,
                "monotonicity: f({}) = {} should be >= f({}) = {} for {:?}",
                hi, f_hi, lo, f_lo, mt
            );
        }
    }
}
