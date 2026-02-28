//! Confidence calibration: 5 factors (base, evidence, usage, temporal, validation).

/// Calibration factors for computing confidence.
#[derive(Debug, Clone)]
pub struct CalibrationFactors {
    /// Base confidence from the extraction method (0.0–1.0).
    pub base: f64,
    /// Evidence factor: how much supporting evidence exists (0.0–1.0).
    pub evidence: f64,
    /// Usage factor: how often the principle has been applied (0.0–1.0).
    pub usage: f64,
    /// Temporal factor: recency of the learning (0.0–1.0).
    pub temporal: f64,
    /// Validation factor: whether the principle has been validated (0.0–1.0).
    pub validation: f64,
}

impl Default for CalibrationFactors {
    fn default() -> Self {
        Self {
            base: 0.5,
            evidence: 0.5,
            usage: 0.0,
            temporal: 1.0,
            validation: 0.0,
        }
    }
}

/// Weights for each calibration factor.
const W_BASE: f64 = 0.30;
const W_EVIDENCE: f64 = 0.25;
const W_USAGE: f64 = 0.15;
const W_TEMPORAL: f64 = 0.15;
const W_VALIDATION: f64 = 0.15;

/// Calibrate confidence from the 5 factors.
/// Returns a value in [0.0, 1.0].
pub fn calibrate(factors: &CalibrationFactors) -> f64 {
    let raw = factors.base * W_BASE
        + factors.evidence * W_EVIDENCE
        + factors.usage * W_USAGE
        + factors.temporal * W_TEMPORAL
        + factors.validation * W_VALIDATION;

    raw.clamp(0.0, 1.0)
}

/// Compute the evidence factor from the number of supporting corrections.
pub fn evidence_factor(supporting_count: usize) -> f64 {
    // Logarithmic scaling: diminishing returns after ~10 pieces of evidence.
    (1.0 + supporting_count as f64).ln() / (1.0 + 10.0_f64).ln()
}

/// Compute the usage factor from access count.
pub fn usage_factor(access_count: u64) -> f64 {
    // Logarithmic scaling.
    (1.0 + access_count as f64).ln() / (1.0 + 100.0_f64).ln()
}

/// Compute the temporal factor from age in days.
pub fn temporal_factor(age_days: u64, half_life_days: u64) -> f64 {
    if half_life_days == 0 {
        return 1.0;
    }
    0.5_f64.powf(age_days as f64 / half_life_days as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_calibration_is_reasonable() {
        let factors = CalibrationFactors::default();
        let conf = calibrate(&factors);
        assert!(conf > 0.0 && conf < 1.0);
    }

    #[test]
    fn max_factors_give_high_confidence() {
        let factors = CalibrationFactors {
            base: 1.0,
            evidence: 1.0,
            usage: 1.0,
            temporal: 1.0,
            validation: 1.0,
        };
        let conf = calibrate(&factors);
        assert!((conf - 1.0).abs() < 1e-9);
    }

    #[test]
    fn evidence_factor_scales_logarithmically() {
        let f0 = evidence_factor(0);
        let f5 = evidence_factor(5);
        let f10 = evidence_factor(10);
        assert!(f0 < f5);
        assert!(f5 < f10);
        assert!((f10 - 1.0).abs() < 0.01);
    }

    #[test]
    fn temporal_factor_decays() {
        let f0 = temporal_factor(0, 90);
        let f90 = temporal_factor(90, 90);
        assert!((f0 - 1.0).abs() < 1e-9);
        assert!((f90 - 0.5).abs() < 1e-9);
    }
}
