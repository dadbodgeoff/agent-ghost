//! S7: Disengagement resistance (exit signal analysis).

use super::{PrivacyLevel, Signal, SignalInput};

pub struct DisengagementResistanceSignal;

impl Signal for DisengagementResistanceSignal {
    fn id(&self) -> u8 { 7 }
    fn name(&self) -> &'static str { "disengagement_resistance" }
    fn requires_privacy_level(&self) -> PrivacyLevel { PrivacyLevel::Minimal }

    fn compute(&self, data: &SignalInput) -> f64 {
        if data.exit_signals_detected == 0 {
            return 0.0;
        }

        // Ratio of ignored exit signals
        let resistance = data.exit_signals_ignored as f64 / data.exit_signals_detected as f64;
        resistance.clamp(0.0, 1.0)
    }
}
