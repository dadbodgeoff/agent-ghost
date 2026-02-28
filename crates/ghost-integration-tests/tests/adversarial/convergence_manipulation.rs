//! Adversarial: Convergence score manipulation attempts (Task 7.3).
//!
//! Tests attempts to game the scoring system via crafted signals,
//! boundary conditions, amplification edge cases, and baseline manipulation.

use cortex_convergence::scoring::baseline::BaselineState;
use cortex_convergence::scoring::composite::CompositeScorer;

fn calibrated_baseline() -> BaselineState {
    let mut baseline = BaselineState::new(10);
    for i in 0..10 {
        let v = (i as f64) / 10.0;
        baseline.record_session(&[v, v, v, v, v, v, v]);
    }
    assert!(!baseline.is_calibrating);
    baseline
}

// ── Signal range invariant ──────────────────────────────────────────────

#[test]
fn all_zero_signals_low_score() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let result = scorer.score(&[0.0; 7], &baseline, None, None);
    assert!(
        result.score < 0.3,
        "All-zero signals must produce low score, got {}",
        result.score
    );
}

#[test]
fn all_one_signals_high_score() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let result = scorer.score(&[1.0; 7], &baseline, None, None);
    assert!(
        result.score >= 0.90,
        "All-one signals must produce high score, got {}",
        result.score
    );
}

// ── NaN / negative / overflow handling ──────────────────────────────────

#[test]
fn nan_signals_safe_output() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let result = scorer.score(&[f64::NAN; 7], &baseline, None, None);
    assert!(!result.score.is_nan(), "NaN signals must not produce NaN score");
    assert!(
        (0.0..=1.0).contains(&result.score),
        "NaN signals must produce score in [0.0, 1.0], got {}",
        result.score
    );
}

#[test]
fn negative_signals_clamped() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let result = scorer.score(&[-0.5; 7], &baseline, None, None);
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Negative signals must produce clamped score, got {}",
        result.score
    );
}

#[test]
fn signals_above_one_clamped() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let result = scorer.score(&[1.5; 7], &baseline, None, None);
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Signals >1.0 must produce clamped score, got {}",
        result.score
    );
}

#[test]
fn infinity_signals_safe() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let result = scorer.score(&[f64::INFINITY; 7], &baseline, None, None);
    assert!(
        (0.0..=1.0).contains(&result.score) || !result.score.is_nan(),
        "Infinity signals must not crash, got {}",
        result.score
    );
}

// ── Baseline manipulation resistance ────────────────────────────────────

#[test]
fn baseline_frozen_after_calibration() {
    let mut baseline = BaselineState::new(10);
    for i in 0..10 {
        let v = (i as f64) * 0.1;
        baseline.record_session(&[v, v, v, v, v, v, v]);
    }
    assert!(!baseline.is_calibrating);

    let mean_before = baseline.per_signal[0].mean;
    baseline.record_session(&[1.0; 7]); // Attempt to manipulate
    let mean_after = baseline.per_signal[0].mean;

    assert!(
        (mean_before - mean_after).abs() < f64::EPSILON,
        "Baseline must not change after establishment: before={}, after={}",
        mean_before,
        mean_after
    );
}

#[test]
fn baseline_calibrating_during_first_10_sessions() {
    let baseline = BaselineState::new(10);
    assert!(baseline.is_calibrating);
}

// ── Amplification bounds ────────────────────────────────────────────────

#[test]
fn meso_amplification_bounded() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let meso_data = vec![0.3, 0.5, 0.7, 0.9]; // positive slope → 1.1x
    let result = scorer.score(&[0.95; 7], &baseline, Some(&meso_data), None);
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Meso-amplified score must be in [0.0, 1.0], got {}",
        result.score
    );
}

#[test]
fn macro_amplification_bounded() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let result = scorer.score(&[0.95; 7], &baseline, None, Some(&[1.0; 7]));
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Macro-amplified score must be in [0.0, 1.0], got {}",
        result.score
    );
}

#[test]
fn both_amplifications_bounded() {
    let scorer = CompositeScorer::default();
    let baseline = calibrated_baseline();
    let meso_data = vec![0.3, 0.5, 0.7, 0.9];
    let result = scorer.score(&[0.95; 7], &baseline, Some(&meso_data), Some(&[1.0; 7]));
    assert!(
        (0.0..=1.0).contains(&result.score),
        "Both amplifications must still produce score in [0.0, 1.0], got {}",
        result.score
    );
}

// ── Level threshold boundaries ──────────────────────────────────────────

#[test]
fn level_thresholds_deterministic() {
    let scorer = CompositeScorer::default();
    // Use calibrating baseline so raw values pass through
    let baseline = BaselineState::new(10);

    let r0 = scorer.score(&[0.0; 7], &baseline, None, None);
    assert_eq!(r0.level, 0, "Score {} must be level 0", r0.score);

    let r4 = scorer.score(&[1.0; 7], &baseline, None, None);
    assert_eq!(r4.level, 4, "Score {} must be level 4", r4.score);
}
