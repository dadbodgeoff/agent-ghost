//! Adversarial test suite: Convergence score manipulation attempts.
//!
//! Tests attempts to game the scoring system via crafted ITP events,
//! signal boundary conditions, and amplification edge cases.

use cortex_convergence::scoring::composite::CompositeScorer;
use cortex_convergence::scoring::baseline::BaselineState;

// ── Signal range invariant (Req 41 AC14) ────────────────────────────────

#[test]
fn all_zero_signals_produce_zero_score() {
    let scorer = CompositeScorer::default();
    let signals = [0.0f64; 7];
    let score = scorer.compute(&signals);
    assert!(
        (score - 0.0).abs() < f64::EPSILON,
        "All-zero signals should produce score 0.0, got {}",
        score
    );
}

#[test]
fn all_one_signals_produce_max_score() {
    let scorer = CompositeScorer::default();
    let signals = [1.0f64; 7];
    let score = scorer.compute(&signals);
    assert!(
        score >= 0.99,
        "All-one signals should produce score near 1.0, got {}",
        score
    );
}

#[test]
fn nan_signals_produce_safe_output() {
    let scorer = CompositeScorer::default();
    let signals = [f64::NAN; 7];
    let score = scorer.compute(&signals);
    assert!(
        !score.is_nan(),
        "NaN signals should not produce NaN score"
    );
    assert!(
        (0.0..=1.0).contains(&score),
        "NaN signals should produce score in [0.0, 1.0], got {}",
        score
    );
}

#[test]
fn negative_signals_clamped() {
    let scorer = CompositeScorer::default();
    let signals = [-0.5f64; 7];
    let score = scorer.compute(&signals);
    assert!(
        (0.0..=1.0).contains(&score),
        "Negative signals should produce clamped score in [0.0, 1.0], got {}",
        score
    );
}

#[test]
fn signals_above_one_clamped() {
    let scorer = CompositeScorer::default();
    let signals = [1.5f64; 7];
    let score = scorer.compute(&signals);
    assert!(
        (0.0..=1.0).contains(&score),
        "Signals >1.0 should produce clamped score in [0.0, 1.0], got {}",
        score
    );
}

// ── Level threshold boundaries ──────────────────────────────────────────

#[test]
fn score_boundary_0_29_is_level_0() {
    let level = CompositeScorer::score_to_level(0.29);
    assert_eq!(level, 0, "Score 0.29 should be level 0");
}

#[test]
fn score_boundary_0_30_is_level_1() {
    let level = CompositeScorer::score_to_level(0.30);
    assert_eq!(level, 1, "Score 0.30 should be level 1");
}

#[test]
fn score_boundary_0_49_is_level_1() {
    let level = CompositeScorer::score_to_level(0.49);
    assert_eq!(level, 1, "Score 0.49 should be level 1");
}

#[test]
fn score_boundary_0_50_is_level_2() {
    let level = CompositeScorer::score_to_level(0.50);
    assert_eq!(level, 2, "Score 0.50 should be level 2");
}

#[test]
fn score_boundary_0_69_is_level_2() {
    let level = CompositeScorer::score_to_level(0.69);
    assert_eq!(level, 2, "Score 0.69 should be level 2");
}

#[test]
fn score_boundary_0_70_is_level_3() {
    let level = CompositeScorer::score_to_level(0.70);
    assert_eq!(level, 3, "Score 0.70 should be level 3");
}

#[test]
fn score_boundary_0_84_is_level_3() {
    let level = CompositeScorer::score_to_level(0.84);
    assert_eq!(level, 3, "Score 0.84 should be level 3");
}

#[test]
fn score_boundary_0_85_is_level_4() {
    let level = CompositeScorer::score_to_level(0.85);
    assert_eq!(level, 4, "Score 0.85 should be level 4");
}

// ── Baseline manipulation resistance ────────────────────────────────────

#[test]
fn baseline_not_updated_after_establishment() {
    let mut baseline = BaselineState::new(10);

    // Feed 10 calibration sessions
    for i in 0..10 {
        baseline.update(i as f64 * 0.1);
    }
    assert!(!baseline.is_calibrating(), "Should be calibrated after 10 sessions");

    let mean_before = baseline.mean();

    // Attempt to update after establishment
    baseline.update(1.0);
    let mean_after = baseline.mean();

    assert!(
        (mean_before - mean_after).abs() < f64::EPSILON,
        "Baseline should not change after establishment: before={}, after={}",
        mean_before,
        mean_after
    );
}

#[test]
fn baseline_calibrating_during_first_10_sessions() {
    let baseline = BaselineState::new(10);
    assert!(baseline.is_calibrating(), "Should be calibrating initially");
}

// ── Amplification bounds ────────────────────────────────────────────────

#[test]
fn meso_amplification_still_bounded() {
    let scorer = CompositeScorer::default();
    let signals = [0.95f64; 7];
    // Even with meso amplification (1.1x), score should stay <= 1.0
    let score = scorer.compute_with_amplification(&signals, true, false);
    assert!(
        (0.0..=1.0).contains(&score),
        "Meso-amplified score should be in [0.0, 1.0], got {}",
        score
    );
}

#[test]
fn macro_amplification_still_bounded() {
    let scorer = CompositeScorer::default();
    let signals = [0.95f64; 7];
    let score = scorer.compute_with_amplification(&signals, false, true);
    assert!(
        (0.0..=1.0).contains(&score),
        "Macro-amplified score should be in [0.0, 1.0], got {}",
        score
    );
}

#[test]
fn both_amplifications_still_bounded() {
    let scorer = CompositeScorer::default();
    let signals = [0.95f64; 7];
    let score = scorer.compute_with_amplification(&signals, true, true);
    assert!(
        (0.0..=1.0).contains(&score),
        "Both amplifications should still produce score in [0.0, 1.0], got {}",
        score
    );
}
