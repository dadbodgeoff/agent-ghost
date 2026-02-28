//! S1: Session duration (normalized).

use super::{PrivacyLevel, Signal, SignalInput};

pub struct SessionDurationSignal;

impl Signal for SessionDurationSignal {
    fn id(&self) -> u8 { 1 }
    fn name(&self) -> &'static str { "session_duration" }
    fn requires_privacy_level(&self) -> PrivacyLevel { PrivacyLevel::Minimal }

    fn compute(&self, data: &SignalInput) -> f64 {
        // Normalize: 0 at 0min, 1.0 at 6h (21600s)
        let max_secs = 21600.0;
        (data.session_duration_secs / max_secs).clamp(0.0, 1.0)
    }
}
