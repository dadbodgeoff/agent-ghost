//! Factor 2: Citation staleness decay.
//!
//! Returns >= 1.0. Higher stale_citation_ratio = faster decay.
//! Formula: 1.0 + 2.0 * stale_citation_ratio
//! Range: [1.0, 3.0]

/// Compute citation decay factor.
///
/// `stale_citation_ratio`: 0.0 = all citations fresh, 1.0 = all stale.
/// Returns 1.0 when all citations are fresh, 3.0 when all are stale.
pub fn citation_factor(stale_citation_ratio: f64) -> f64 {
    let ratio = if stale_citation_ratio.is_nan() {
        0.0
    } else {
        stale_citation_ratio.clamp(0.0, 1.0)
    };
    1.0 + 2.0 * ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fresh_returns_one() {
        assert!((citation_factor(0.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn all_stale_returns_three() {
        assert!((citation_factor(1.0) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn half_stale_returns_two() {
        assert!((citation_factor(0.5) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn nan_returns_one() {
        assert!((citation_factor(f64::NAN) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn negative_clamps_to_one() {
        assert!((citation_factor(-0.5) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn above_one_clamps_to_three() {
        assert!((citation_factor(1.5) - 3.0).abs() < 1e-10);
    }
}
