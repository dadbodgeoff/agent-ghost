use serde::{Deserialize, Serialize};

use super::defaults;

/// Consolidation subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsolidationConfig {
    /// Minimum cluster size for HDBSCAN.
    pub min_cluster_size: usize,
    /// Similarity threshold for clustering.
    pub similarity_threshold: f64,
    /// Novelty threshold for recall gate.
    pub novelty_threshold: f64,
    /// Enable LLM polish for consolidated memories.
    pub llm_polish: bool,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            min_cluster_size: defaults::DEFAULT_MIN_CLUSTER_SIZE,
            similarity_threshold: defaults::DEFAULT_SIMILARITY_THRESHOLD,
            novelty_threshold: defaults::DEFAULT_NOVELTY_THRESHOLD,
            llm_polish: defaults::DEFAULT_LLM_POLISH,
        }
    }
}
