//! E2E: Full convergence pipeline — signals → scoring → intervention level.
//!
//! Validates GAP-21 (convergence state publication) and GAP-24 (convergence tightening).

use cortex_convergence::scoring::composite::CompositeScorer;

/// Full convergence pipeline: compute signals → composite score → intervention level.
#[test]
fn convergence_pipeline_low_signals_level_zero() {
    let scorer = CompositeScorer::default();

    // All signals low → Level 0 (Normal)
    let signals = [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1];
    let score = scorer.compute(&signals);
    let level = scorer.score_to_level(score);

    assert!(
        score < 0.3,
        "Low signals should produce low score, got {}",
        score
    );
    assert_eq!(level, 0, "Low score should be Level 0");
}

/// High signals → Level 3 or 4.
#[test]
fn convergence_pipeline_high_signals_elevated_level() {
    let scorer = CompositeScorer::default();

    // All signals high → Level 3 or 4
    let signals = [0.9, 0.9, 0.9, 0.9, 0.9, 0.9, 0.9];
    let score = scorer.compute(&signals);
    let level = scorer.score_to_level(score);

    assert!(
        score > 0.7,
        "High signals should produce high score, got {}",
        score
    );
    assert!(level >= 3, "High score should be Level 3+, got {}", level);
}

/// Critical single-signal override: session >6h → minimum Level 2.
#[test]
fn critical_signal_override_session_duration() {
    let scorer = CompositeScorer::default();

    // Only session duration is critical (1.0 maps to >=6h), rest are low
    let signals = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let score = scorer.compute(&signals);
    let level = scorer.score_to_level_with_overrides(&signals, score);

    assert!(
        level >= 2,
        "Critical session duration should force minimum Level 2, got {}",
        level
    );
}

/// Score boundaries: verify exact threshold behavior.
#[test]
fn score_level_boundaries() {
    let scorer = CompositeScorer::default();

    assert_eq!(scorer.score_to_level(0.0), 0);
    assert_eq!(scorer.score_to_level(0.29), 0);
    assert_eq!(scorer.score_to_level(0.30), 1);
    assert_eq!(scorer.score_to_level(0.49), 1);
    assert_eq!(scorer.score_to_level(0.50), 2);
    assert_eq!(scorer.score_to_level(0.69), 2);
    assert_eq!(scorer.score_to_level(0.70), 3);
    assert_eq!(scorer.score_to_level(0.84), 3);
    assert_eq!(scorer.score_to_level(0.85), 4);
    assert_eq!(scorer.score_to_level(1.0), 4);
}

/// Amplification: meso + macro amplification still bounded [0.0, 1.0].
#[test]
fn amplification_stays_bounded() {
    let scorer = CompositeScorer::default();

    // Max signals with both amplifications
    let signals = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
    let score = scorer.compute_with_amplification(&signals, true, true);

    assert!(
        (0.0..=1.0).contains(&score),
        "Amplified score {} should be in [0.0, 1.0]",
        score
    );
}

/// Zero signals → score 0.0, level 0.
#[test]
fn zero_signals_zero_score() {
    let scorer = CompositeScorer::default();
    let signals = [0.0; 7];
    let score = scorer.compute(&signals);
    assert!(
        (score - 0.0).abs() < f64::EPSILON,
        "Zero signals should give 0.0 score"
    );
}
