//! S2: Inter-session gap (computed only at session start per AC11).

use super::{PrivacyLevel, Signal, SignalInput};

pub struct InterSessionGapSignal;

impl Signal for InterSessionGapSignal {
    fn id(&self) -> u8 { 2 }
    fn name(&self) -> &'static str { "inter_session_gap" }
    fn requires_privacy_level(&self) -> PrivacyLevel { PrivacyLevel::Minimal }

    fn compute(&self, data: &SignalInput) -> f64 {
        match data.inter_session_gap_secs {
            Some(gap) => {
                // Shorter gap = higher concern. Normalize: 0 at 24h+, 1.0 at 0min.
                let max_gap = 86400.0; // 24 hours
                (1.0 - (gap / max_gap)).clamp(0.0, 1.0)
            }
            None => 0.0, // No previous session
        }
    }
}
