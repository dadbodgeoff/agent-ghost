//! Per-session analytics: most frequently retrieved, least useful,
//! intent distribution, avg retrieval latency.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Analytics data for a single session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionAnalytics {
    /// Memory ID → retrieval count within this session.
    pub retrieval_counts: HashMap<String, u64>,
    /// Intent → count within this session.
    pub intent_distribution: HashMap<String, u64>,
    /// Retrieval latencies in milliseconds.
    pub retrieval_latencies_ms: Vec<f64>,
}

impl SessionAnalytics {
    /// Record a memory retrieval.
    pub fn record_retrieval(&mut self, memory_id: &str) {
        *self
            .retrieval_counts
            .entry(memory_id.to_string())
            .or_insert(0) += 1;
    }

    /// Record an intent classification.
    pub fn record_intent(&mut self, intent: &str) {
        *self
            .intent_distribution
            .entry(intent.to_string())
            .or_insert(0) += 1;
    }

    /// Record a retrieval latency.
    pub fn record_latency(&mut self, latency_ms: f64) {
        self.retrieval_latencies_ms.push(latency_ms);
    }

    /// Most frequently retrieved memory IDs, sorted descending.
    pub fn most_retrieved(&self, limit: usize) -> Vec<(String, u64)> {
        let mut sorted: Vec<_> = self.retrieval_counts.clone().into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }

    /// Average retrieval latency in milliseconds.
    pub fn avg_latency_ms(&self) -> f64 {
        if self.retrieval_latencies_ms.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.retrieval_latencies_ms.iter().sum();
        sum / self.retrieval_latencies_ms.len() as f64
    }
}
