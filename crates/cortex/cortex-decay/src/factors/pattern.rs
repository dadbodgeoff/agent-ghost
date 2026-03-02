//! Factor 5: Pattern alignment decay.
//!
//! Memories linked to inactive patterns decay faster.

/// Compute pattern alignment factor.
///
/// Active patterns: 1.0 (no acceleration).
/// Inactive patterns: 1.5 (50% faster decay).
pub fn pattern_factor(has_active_patterns: bool) -> f64 {
    if has_active_patterns {
        1.0
    } else {
        1.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_returns_one() {
        assert!((pattern_factor(true) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn inactive_returns_one_point_five() {
        assert!((pattern_factor(false) - 1.5).abs() < 1e-10);
    }

    #[test]
    fn both_gte_one() {
        assert!(pattern_factor(true) >= 1.0);
        assert!(pattern_factor(false) >= 1.0);
    }
}
