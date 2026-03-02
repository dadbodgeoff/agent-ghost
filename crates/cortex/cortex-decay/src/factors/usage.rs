//! Factor 3: Usage frequency decay.
//!
//! Memories accessed frequently and recently decay slower (factor closer to 1.0).
//! Unused or stale memories decay faster (higher factor).
//! Range: [1.0, ~4.0]

use chrono::{DateTime, Utc};

/// Compute usage frequency factor.
///
/// High `access_count` + recent `last_accessed` → lower factor (less decay).
/// Zero accesses + never accessed → higher factor (more decay).
pub fn usage_factor(
    access_count: u64,
    last_accessed: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> f64 {
    // Staleness component: grows with time since last access, caps at 4.0.
    let days_since = last_accessed
        .map(|la| {
            let secs = (now - la).num_seconds().max(0) as f64;
            secs / 86_400.0
        })
        .unwrap_or(365.0);

    let days_since = if days_since.is_nan() || days_since.is_infinite() {
        365.0
    } else {
        days_since
    };

    let staleness = (1.0 + days_since / 15.0).min(4.0);

    // Access discount: reduces the staleness penalty via diminishing returns.
    // access_count=0 → 0.0, access_count=100 → ~2.0
    let access_discount = (1.0 + access_count as f64).log2() * 0.3;

    // Combined: unused + old → up to 4.0; well-used + recent → 1.0
    (staleness - access_discount).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequently_accessed_recently_near_one() {
        let now = Utc::now();
        let f = usage_factor(50, Some(now), now);
        assert!(f < 1.3, "frequently accessed + just now should be near 1.0, got {}", f);
        assert!(f >= 1.0);
    }

    #[test]
    fn never_accessed_high_factor() {
        let now = Utc::now();
        let f = usage_factor(0, None, now);
        assert!(f > 2.0, "never accessed should have high factor, got {}", f);
    }

    #[test]
    fn old_access_increases_factor() {
        let now = Utc::now();
        let recent = usage_factor(5, Some(now), now);
        let old = usage_factor(5, Some(now - chrono::Duration::days(90)), now);
        assert!(old > recent, "older access should increase factor");
    }

    #[test]
    fn more_accesses_decreases_factor() {
        let now = Utc::now();
        let last = Some(now - chrono::Duration::days(7));
        let few = usage_factor(1, last, now);
        let many = usage_factor(100, last, now);
        assert!(many < few, "more accesses should decrease factor");
    }

    #[test]
    fn always_gte_one() {
        let now = Utc::now();
        for count in [0, 1, 10, 100, 1000] {
            for days_ago in [0, 1, 7, 30, 365] {
                let la = if days_ago == 0 {
                    Some(now)
                } else {
                    Some(now - chrono::Duration::days(days_ago))
                };
                let f = usage_factor(count, la, now);
                assert!(f >= 1.0, "factor must be >= 1.0, got {} for count={} days_ago={}", f, count, days_ago);
            }
            // Also test None
            let f = usage_factor(count, None, now);
            assert!(f >= 1.0);
        }
    }
}
