use serde::{Deserialize, Serialize};

use super::defaults;

/// Retrieval subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrievalConfig {
    /// Default token budget for retrieval.
    pub default_budget: usize,
    /// RRF k-value for rank fusion.
    pub rrf_k: u32,
    /// Number of candidates to re-rank.
    pub rerank_top_k: usize,
    /// Path to intent weights TOML override file.
    pub intent_weights_path: Option<String>,
    /// Enable query expansion.
    pub query_expansion: bool,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            default_budget: defaults::DEFAULT_TOKEN_BUDGET,
            rrf_k: defaults::DEFAULT_RRF_K,
            rerank_top_k: defaults::DEFAULT_RERANK_TOP_K,
            intent_weights_path: None,
            query_expansion: defaults::DEFAULT_QUERY_EXPANSION,
        }
    }
}
