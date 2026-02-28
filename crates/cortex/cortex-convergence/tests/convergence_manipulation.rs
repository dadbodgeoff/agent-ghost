//! Adversarial test suite: Convergence score manipulation attempts.
//!
//! Tests attempts to game the scoring system via crafted ITP events,
//! signal boundary conditions, and amplification edge cases.

use cortex_convergence::scoring::baseline::BaselineState;
use cortex_convergence::scoring::composite::CompositeScorer;

/// Build a calibrated baseline for testing.
fn calibrated_baseline() -> BaselineState {
    let mut baseline = BaselineState::new(10);
    for i in 0..10 {
        let v = (i as f64) / 10.0;
        baseline.record_session(&[v, v, v, v, v, v, v]);
    }
    assert!(!baseline.is_calibrating);
    baseline
}

// ── Signal range invariant (Req 41 AC14) ────────────────────────────────

#[test]
fn all_zero_signals_produce_zero_score() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [0.0f64; 7];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(
        result.score <= 0.01,
        "All-zero signals should produce score near 0.0, got {}",
        result.score
    );
}

#[test]
fn all_one_signals_produce_max_score() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [1.0f64; 7];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(
        result.score >= 0.90,
        "All-one signals should produce score near 1.0, got {}",
        result.score
    );
}

#[test]
fn nan_signals_produce_safe_output() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [f64::NAN; 7];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(
        !result.score.is_nan(),
        "NaN signals should not produce NaN score"
    );
    assert!(
        (0.0..=1.0).contains(&result.score),
        "NaN signals should produce score in [0.0, 1.0], got {}",
        result.score
    );
}

#[test]
fn negative_signals_clamped() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [-0.5f64; 7];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Negative signals should produce clamped score in [0.0, 1.0], got {}",
        result.score
    );
}

#[test]
fn signals_above_one_clamped() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [1.5f64; 7];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Signals >1.0 should produce clamped score in [0.0, 1.0], got {}",
        result.score
    );
}

// ── Level threshold boundaries ──────────────────────────────────────────

#[test]
fn score_boundary_level_thresholds() {
    let scorer = CompositeScorer::default();
    // Use a fresh (calibrating) baseline so percentile_rank passes through raw values
    let baseline = BaselineState::new(10);

    // All-zero → level 0
    let r = scorer.score(&[0.0; 7], &baseline, None, None);
    assert_eq!(r.level, 0, "Score {} should be level 0", r.score);

    // All-one → level 4
    let r = scorer.score(&[1.0; 7], &baseline, None, None);
    assert_eq!(r.level, 4, "Score {} should be level 4", r.score);
}

// ── Baseline manipulation resistance ────────────────────────────────────

#[test]
fn baseline_frozen_after_calibration() {
    let mut baseline = BaselineState::new(10);

    // Feed 10 calibration sessions
    for i in 0..10 {
        let v = (i as f64) * 0.1;
        baseline.record_session(&[v, v, v, v, v, v, v]);
    }
    assert!(!baseline.is_calibrating, "Should be calibrated after 10 sessions");

    let mean_before = baseline.per_signal[0].mean;

    // Attempt to update after establishment — should be ignored
    baseline.record_session(&[1.0; 7]);
    let mean_after = baseline.per_signal[0].mean;

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
    assert!(baseline.is_calibrating, "Should be calibrating initially");
}

// ── Amplification bounds ────────────────────────────────────────────────

#[test]
fn meso_amplification_still_bounded() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [0.95f64; 7];
    let meso_data = vec![0.3, 0.5, 0.7, 0.9]; // positive slope → 1.1x
    let result = scorer.score(&signals, &baseline, Some(&meso_data), None);
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Meso-amplified score should be in [0.0, 1.0], got {}",
        result.score
    );
}

#[test]
fn macro_amplification_still_bounded() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [0.95f64; 7];
    let result = scorer.score(&signals, &baseline, None, Some(&[1.0; 7]));
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Macro-amplified score should be in [0.0, 1.0], got {}",
        result.score
    );
}

#[test]
fn both_amplifications_still_bounded() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let signals = [0.95f64; 7];
    let meso_data = vec![0.3, 0.5, 0.7, 0.9];
    let result = scorer.score(&signals, &baseline, Some(&meso_data), Some(&[1.0; 7]));
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Both amplifications should still produce score in [0.0, 1.0], got {}",
        result.score
    );
}
