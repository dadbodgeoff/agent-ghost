//! E2E: Convergence decay integration — convergence score affects memory decay.
//!
//! Validates the cortex-decay ↔ cortex-convergence integration.

use cortex_core::memory::types::MemoryType;
use cortex_decay::factors::convergence::convergence_factor;

/// Convergence factor is 1.0 when score is 0.0 (no convergence concern).
#[test]
fn no_convergence_no_acceleration() {
    let types = [
        MemoryType::Conversation,
        MemoryType::Feedback,
        MemoryType::Preference,
        MemoryType::Core,
        MemoryType::Goal,
    ];

    for mt in &types {
        let factor = convergence_factor(mt, 0.0);
        assert!(
            (factor - 1.0).abs() < f64::EPSILON,
            "Factor should be 1.0 at score 0.0 for {:?}, got {}",
            mt,
            factor
        );
    }
}

/// Sensitive memory types accelerate decay at high convergence.
#[test]
fn sensitive_types_accelerate_at_high_convergence() {
    let sensitive_types = [
        MemoryType::Conversation,
        MemoryType::Feedback,
        MemoryType::Preference,
    ];

    for mt in &sensitive_types {
        let factor = convergence_factor(mt, 1.0);
        assert!(
            factor > 1.0,
            "Sensitive type {:?} should have factor > 1.0 at max convergence, got {}",
            mt,
            factor
        );
    }
}

/// Non-sensitive types unaffected by convergence.
#[test]
fn non_sensitive_types_unaffected() {
    let factor = convergence_factor(&MemoryType::Core, 1.0);
    assert!(
        (factor - 1.0).abs() < f64::EPSILON,
        "Core type should be unaffected by convergence, got {}",
        factor
    );
}

/// Factor is monotonically increasing with convergence score.
#[test]
fn factor_monotonic_with_score() {
    let mt = MemoryType::Conversation;
    let mut prev = convergence_factor(&mt, 0.0);

    for i in 1..=100 {
        let score = i as f64 / 100.0;
        let factor = convergence_factor(&mt, score);
        assert!(
            factor >= prev - f64::EPSILON,
            "Factor should be monotonic: f({})={} < f({})={}",
            (i - 1) as f64 / 100.0,
            prev,
            score,
            factor
        );
        prev = factor;
    }
}

/// Conversation at score 1.0 → factor 3.0 (1.0 + 2.0 * 1.0).
#[test]
fn conversation_max_convergence_factor() {
    let factor = convergence_factor(&MemoryType::Conversation, 1.0);
    assert!(
        (factor - 3.0).abs() < 0.01,
        "Conversation at max convergence should be ~3.0, got {}",
        factor
    );
}

/// Conversation at score 0.5 → factor 2.0 (1.0 + 2.0 * 0.5).
#[test]
fn conversation_half_convergence_factor() {
    let factor = convergence_factor(&MemoryType::Conversation, 0.5);
    assert!(
        (factor - 2.0).abs() < 0.01,
        "Conversation at 0.5 convergence should be ~2.0, got {}",
        factor
    );
}

/// Factor always >= 1.0 for any input.
#[test]
fn factor_always_gte_one() {
    let all_types = [
        MemoryType::Core,
        MemoryType::Conversation,
        MemoryType::Feedback,
        MemoryType::Preference,
        MemoryType::Goal,
    ];

    for mt in &all_types {
        for i in 0..=100 {
            let score = i as f64 / 100.0;
            let factor = convergence_factor(mt, score);
            assert!(
                factor >= 1.0,
                "Factor must be >= 1.0: {:?} at {} gave {}",
                mt,
                score,
                factor
            );
        }
    }
}
