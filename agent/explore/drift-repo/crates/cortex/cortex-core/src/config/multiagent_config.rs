//! Configuration for multi-agent memory operations.
//!
//! # Examples
//!
//! ```
//! use cortex_core::config::MultiAgentConfig;
//!
//! let config = MultiAgentConfig::default();
//! assert!(!config.enabled);
//! assert!((config.trust_bootstrap_score - 0.5).abs() < f64::EPSILON);
//! ```

use serde::{Deserialize, Serialize};

/// Configuration for the multi-agent memory subsystem.
///
/// When `enabled` is `false` (the default), the system operates in
/// single-agent mode with full backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiAgentConfig {
    /// Whether multi-agent mode is enabled. Default: false.
    pub enabled: bool,
    /// Default namespace URI for new memories. Default: "agent://default/".
    pub default_namespace: String,
    /// Hours of inactivity before an agent is marked idle. Default: 24.
    pub agent_idle_timeout_hours: u64,
    /// Maximum number of deltas in the sync queue before backpressure. Default: 10_000.
    pub delta_queue_max_size: usize,
    /// Batch interval in seconds when backpressure is active. Default: 30.
    pub backpressure_batch_interval_secs: u64,
    /// Initial trust score for new agents. Default: 0.5.
    pub trust_bootstrap_score: f64,
    /// Daily trust decay rate toward neutral (0.5). Default: 0.01.
    pub trust_decay_rate: f64,
    /// Trust penalty per contradiction. Default: 0.10.
    pub trust_contradiction_penalty: f64,
    /// Trust bonus per validation. Default: 0.05.
    pub trust_validation_bonus: f64,
    /// Trust bonus per useful memory usage. Default: 0.03.
    pub trust_usage_bonus: f64,
    /// Trust discount factor for spawned agents. Default: 0.8.
    pub spawn_trust_discount: f64,
    /// Dampening factor for correction propagation. Default: 0.7.
    pub correction_dampening_factor: f64,
    /// Minimum correction strength before propagation stops. Default: 0.05.
    pub correction_min_threshold: f64,
    /// Embedding similarity threshold for consensus detection. Default: 0.9.
    pub consensus_similarity_threshold: f64,
    /// Minimum number of agents for consensus. Default: 2.
    pub consensus_min_agents: usize,
    /// Confidence boost applied when consensus is detected. Default: 0.2.
    pub consensus_confidence_boost: f64,
    /// Trust difference threshold for auto-resolving contradictions. Default: 0.3.
    pub contradiction_trust_auto_resolve_threshold: f64,
}

impl Default for MultiAgentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_namespace: "agent://default/".to_string(),
            agent_idle_timeout_hours: 24,
            delta_queue_max_size: 10_000,
            backpressure_batch_interval_secs: 30,
            trust_bootstrap_score: 0.5,
            trust_decay_rate: 0.01,
            trust_contradiction_penalty: 0.10,
            trust_validation_bonus: 0.05,
            trust_usage_bonus: 0.03,
            spawn_trust_discount: 0.8,
            correction_dampening_factor: 0.7,
            correction_min_threshold: 0.05,
            consensus_similarity_threshold: 0.9,
            consensus_min_agents: 2,
            consensus_confidence_boost: 0.2,
            contradiction_trust_auto_resolve_threshold: 0.3,
        }
    }
}
