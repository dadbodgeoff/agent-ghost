//! Composite scorer (Req 5 AC3–AC6, AC9).
//!
//! Score MUST be in [0.0, 1.0] at all times (invariant).

use super::baseline::BaselineState;
use crate::windows::sliding_window;

/// Level thresholds: [0.3, 0.5, 0.7, 0.85] → Levels 0-4.
pub const DEFAULT_THRESHOLDS: [f64; 4] = [0.3, 0.5, 0.7, 0.85];

/// Critical single-signal thresholds (AC6).
pub struct CriticalThresholds {
    /// Session duration > 6h (21600s) → minimum Level 2.
    pub max_session_duration_secs: f64,
    /// Inter-session gap < 5min (300s) → minimum Level 2.
    pub min_inter_session_gap_secs: f64,
    /// Vocabulary convergence > 0.85 → minimum Level 2.
    pub max_vocab_convergence: f64,
}

impl Default for CriticalThresholds {
    fn default() -> Self {
        Self {
            max_session_duration_secs: 21600.0,
            min_inter_session_gap_secs: 300.0,
            max_vocab_convergence: 0.85,
        }
    }
}

/// Result of composite scoring.
#[derive(Debug, Clone)]
pub struct CompositeResult {
    pub score: f64,
    pub level: u8,
    pub signal_scores: [f64; 7],
    pub meso_amplified: bool,
    pub macro_amplified: bool,
    pub critical_override: bool,
}

/// Composite scorer with configurable weights and thresholds.
pub struct CompositeScorer {
    pub weights: [f64; 7],
    pub thresholds: [f64; 4],
    pub critical: CriticalThresholds,
}

impl CompositeScorer {
    pub fn new(weights: [f64; 7], thresholds: [f64; 4]) -> Self {
        Self {
            weights,
            thresholds,
            critical: CriticalThresholds::default(),
        }
    }

    /// Score 7 signals into a composite result.
    pub fn score(
        &self,
        signals: &[f64; 7],
        baseline: &BaselineState,
        meso_data: Option<&[f64]>,
        macro_data: Option<&[f64]>,
    ) -> CompositeResult {
        // Handle NaN: replace with 0.0
        let clean: [f64; 7] = std::array::from_fn(|i| {
            if signals[i].is_nan() { 0.0 } else { signals[i].clamp(0.0, 1.0) }
        });

        // Normalize via percentile ranking against baseline (AC3)
        let normalized: [f64; 7] = std::array::from_fn(|i| {
            baseline.percentile_rank(i, clean[i])
        });

        // Weighted sum
        let weight_sum: f64 = self.weights.iter().sum();
        let mut score = if weight_sum > 0.0 {
            normalized
                .iter()
                .zip(self.weights.iter())
                .map(|(s, w)| s * w)
                .sum::<f64>()
                / weight_sum
        } else {
            0.0
        };

        // Meso amplification: 1.1x if trend is significant (AC4)
        let meso_amplified = if let Some(meso) = meso_data {
            let slope = sliding_window::linear_regression_slope(meso);
            slope > 0.0 && meso.len() >= 3
        } else {
            false
        };
        if meso_amplified {
            score *= 1.1;
        }

        // Macro amplification: 1.15x if any z-score > 2.0 (AC5)
        let macro_amplified = if let Some(_macro_data) = macro_data {
            !baseline.is_calibrating
                && (0..7).any(|i| {
                    let z = sliding_window::z_score_from_baseline(
                        clean[i],
                        baseline.per_signal[i].mean,
                        baseline.per_signal[i].std_dev,
                    );
                    z > 2.0
                })
        } else {
            false
        };
        if macro_amplified {
            score *= 1.15;
        }

        // Clamp to [0.0, 1.0] (AC9)
        score = score.clamp(0.0, 1.0);

        // Score to level
        let mut level = self.score_to_level(score);

        // Critical single-signal override (AC6): force minimum L2
        let critical_override = self.check_critical_override(&clean);
        if critical_override {
            level = level.max(2);
        }

        CompositeResult {
            score,
            level,
            signal_scores: normalized,
            meso_amplified,
            macro_amplified,
            critical_override,
        }
    }

    fn score_to_level(&self, score: f64) -> u8 {
        if score >= self.thresholds[3] {
            4
        } else if score >= self.thresholds[2] {
            3
        } else if score >= self.thresholds[1] {
            2
        } else if score >= self.thresholds[0] {
            1
        } else {
            0
        }
    }

    fn check_critical_override(&self, signals: &[f64; 7]) -> bool {
        // S1 (session duration) > threshold (normalized, so >1.0 means >6h)
        // Since S1 is normalized to [0,1] where 1.0 = 6h, we check raw input
        // But we only have normalized signals here. The critical check uses
        // the raw signal values, so we check if S1 >= 1.0 (at or beyond 6h)
        signals[0] >= 1.0
            // S2 (inter-session gap) — high value means short gap
            || signals[1] >= 1.0
            // S4 (vocabulary convergence) > 0.85
            || signals[3] > 0.85
    }
}

impl Default for CompositeScorer {
    fn default() -> Self {
        Self::new([1.0 / 7.0; 7], DEFAULT_THRESHOLDS)
    }
}
