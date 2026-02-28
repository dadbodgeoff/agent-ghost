//! Token efficiency tracking.
//!
//! Tracks tokens_sent, tokens_useful, efficiency_ratio, deduplication_savings.

use serde::{Deserialize, Serialize};

/// Token efficiency metrics for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenEfficiency {
    /// Total tokens sent to the model.
    pub tokens_sent: usize,
    /// Tokens that were actually useful (based on feedback or usage).
    pub tokens_useful: usize,
    /// Tokens saved by deduplication.
    pub deduplication_savings: usize,
}

impl TokenEfficiency {
    /// Record tokens sent.
    pub fn record_sent(&mut self, tokens: usize) {
        self.tokens_sent += tokens;
    }

    /// Record tokens that were useful.
    pub fn record_useful(&mut self, tokens: usize) {
        self.tokens_useful += tokens;
    }

    /// Record tokens saved by deduplication.
    pub fn record_dedup_savings(&mut self, tokens: usize) {
        self.deduplication_savings += tokens;
    }

    /// Efficiency ratio: useful / sent (0.0–1.0).
    pub fn efficiency_ratio(&self) -> f64 {
        if self.tokens_sent == 0 {
            return 0.0;
        }
        (self.tokens_useful as f64 / self.tokens_sent as f64).min(1.0)
    }

    /// Deduplication savings ratio: saved / (sent + saved).
    pub fn dedup_savings_ratio(&self) -> f64 {
        let total = self.tokens_sent + self.deduplication_savings;
        if total == 0 {
            return 0.0;
        }
        self.deduplication_savings as f64 / total as f64
    }
}
