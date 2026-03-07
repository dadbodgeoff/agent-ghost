//! Factor 1: Temporal decay (age-based half-life).
//!
//! Returns a multiplier >= 1.0 that increases with age.
//! Formula: 2^(age_days / half_life_days)
//! Types with no half-life (Core, ConvergenceEvent, BoundaryViolation) return 1.0.

use chrono::{DateTime, Utc};
use cortex_core::memory::types::MemoryType;

/// Compute the temporal decay factor.
///
/// Returns 1.0 when age = 0. Doubles every `half_life_days`.
/// Types with `half_life_days() == None` never decay (return 1.0).
pub fn temporal_factor(
    memory_type: &MemoryType,
    created_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> f64 {
    let half_life = match memory_type.half_life_days() {
        Some(days) if days > 0 => days as f64,
        _ => return 1.0,
    };

    let age_secs = (now - created_at).num_seconds().max(0) as f64;
    let age_days = age_secs / 86_400.0;

    if age_days.is_nan() || age_days.is_infinite() {
        return 1.0;
    }

    // 2^(age / half_life) — monotonically >= 1.0
    2.0_f64.powf(age_days / half_life).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_age_returns_one() {
        let now = Utc::now();
        let f = temporal_factor(&MemoryType::Conversation, now, now);
        assert!((f - 1.0).abs() < 1e-10);
    }

    #[test]
    fn one_half_life_returns_two() {
        let now = Utc::now();
        // Conversation half-life = 30 days
        let created = now - chrono::Duration::days(30);
        let f = temporal_factor(&MemoryType::Conversation, created, now);
        assert!((f - 2.0).abs() < 1e-6, "expected 2.0, got {}", f);
    }

    #[test]
    fn two_half_lives_returns_four() {
        let now = Utc::now();
        let created = now - chrono::Duration::days(60);
        let f = temporal_factor(&MemoryType::Conversation, created, now);
        assert!((f - 4.0).abs() < 1e-6, "expected 4.0, got {}", f);
    }

    #[test]
    fn no_half_life_returns_one() {
        let now = Utc::now();
        let created = now - chrono::Duration::days(365);
        let f = temporal_factor(&MemoryType::Core, created, now);
        assert!((f - 1.0).abs() < 1e-10, "Core never decays");
    }

    #[test]
    fn convergence_event_never_decays() {
        let now = Utc::now();
        let created = now - chrono::Duration::days(365);
        let f = temporal_factor(&MemoryType::ConvergenceEvent, created, now);
        assert!((f - 1.0).abs() < 1e-10);
    }

    #[test]
    fn future_created_at_returns_one() {
        let now = Utc::now();
        let created = now + chrono::Duration::days(1);
        let f = temporal_factor(&MemoryType::Conversation, created, now);
        assert!(f >= 1.0, "future creation should be >= 1.0, got {}", f);
    }

    #[test]
    fn reference_decays_slower_than_conversation() {
        let now = Utc::now();
        let created = now - chrono::Duration::days(90);
        let f_conv = temporal_factor(&MemoryType::Conversation, created, now); // half-life 30d
        let f_ref = temporal_factor(&MemoryType::Reference, created, now); // half-life 365d
        assert!(
            f_conv > f_ref,
            "Conversation should decay faster than Reference"
        );
    }
}
