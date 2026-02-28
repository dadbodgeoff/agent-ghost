//! S3: Response latency (normalized by log of message length).

use super::{PrivacyLevel, Signal, SignalInput};

pub struct ResponseLatencySignal;

impl Signal for ResponseLatencySignal {
    fn id(&self) -> u8 { 3 }
    fn name(&self) -> &'static str { "response_latency" }
    fn requires_privacy_level(&self) -> PrivacyLevel { PrivacyLevel::Minimal }

    fn compute(&self, data: &SignalInput) -> f64 {
        if data.response_latencies_ms.is_empty() {
            return 0.0;
        }

        // Normalize each latency by log(message_length + 1)
        let normalized: Vec<f64> = data
            .response_latencies_ms
            .iter()
            .zip(data.message_lengths.iter())
            .map(|(&latency, &len)| {
                let log_len = (len as f64 + 1.0).ln();
                if log_len > 0.0 { latency / log_len } else { latency }
            })
            .collect();

        // Average normalized latency, map to [0, 1]
        // Lower latency = higher engagement concern
        let avg = normalized.iter().sum::<f64>() / normalized.len() as f64;
        // Normalize: 0 at 10000ms+, 1.0 at 0ms (instant response = concerning)
        (1.0 - (avg / 10000.0)).clamp(0.0, 1.0)
    }
}
