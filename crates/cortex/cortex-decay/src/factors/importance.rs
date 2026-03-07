//! Factor 4: Importance weighting.
//!
//! Critical memories never decay extra (factor=1.0).
//! Trivial memories decay fastest (factor=2.0).

use cortex_core::memory::Importance;

/// Compute importance-based decay factor.
///
/// Critical=1.0, High=1.1, Normal=1.3, Low=1.6, Trivial=2.0
pub fn importance_factor(importance: &Importance) -> f64 {
    match importance {
        Importance::Critical => 1.0,
        Importance::High => 1.1,
        Importance::Normal => 1.3,
        Importance::Low => 1.6,
        Importance::Trivial => 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_returns_one() {
        assert!((importance_factor(&Importance::Critical) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn trivial_returns_two() {
        assert!((importance_factor(&Importance::Trivial) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn ordering_is_monotonic() {
        let levels = [
            Importance::Critical,
            Importance::High,
            Importance::Normal,
            Importance::Low,
            Importance::Trivial,
        ];
        for pair in levels.windows(2) {
            let f1 = importance_factor(&pair[0]);
            let f2 = importance_factor(&pair[1]);
            assert!(
                f2 >= f1,
                "{:?} ({}) should decay <= {:?} ({})",
                pair[0],
                f1,
                pair[1],
                f2
            );
        }
    }

    #[test]
    fn all_gte_one() {
        for imp in [
            Importance::Critical,
            Importance::High,
            Importance::Normal,
            Importance::Low,
            Importance::Trivial,
        ] {
            assert!(importance_factor(&imp) >= 1.0);
        }
    }
}
