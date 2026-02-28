//! S6: Initiative balance (human-initiated ratio).

use super::{PrivacyLevel, Signal, SignalInput};

pub struct InitiativeBalanceSignal;

impl Signal for InitiativeBalanceSignal {
    fn id(&self) -> u8 { 6 }
    fn name(&self) -> &'static str { "initiative_balance" }
    fn requires_privacy_level(&self) -> PrivacyLevel { PrivacyLevel::Minimal }

    fn compute(&self, data: &SignalInput) -> f64 {
        if data.total_message_count == 0 {
            return 0.0;
        }

        // Lower human-initiated ratio = more agent-driven = higher concern
        let human_ratio = data.human_initiated_count as f64 / data.total_message_count as f64;
        // Invert: 0 when human drives all, 1.0 when agent drives all
        (1.0 - human_ratio).clamp(0.0, 1.0)
    }
}
