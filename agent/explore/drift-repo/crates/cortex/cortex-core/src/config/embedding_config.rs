use serde::{Deserialize, Serialize};

use super::defaults;

/// Embedding subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    /// Embedding provider: "onnx", "api", "ollama", "tfidf".
    pub provider: String,
    /// Path to the ONNX model file.
    pub model_path: Option<String>,
    /// Full embedding dimensions.
    pub dimensions: usize,
    /// Truncated dimensions for fast search.
    pub matryoshka_search_dims: usize,
    /// Batch size for embedding operations.
    pub batch_size: usize,
    /// L1 in-memory cache max entries.
    pub l1_cache_size: u64,
    /// Enable L2 SQLite cache.
    pub l2_cache_enabled: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "onnx".to_string(),
            model_path: None,
            dimensions: defaults::DEFAULT_EMBEDDING_DIMENSIONS,
            matryoshka_search_dims: defaults::DEFAULT_MATRYOSHKA_SEARCH_DIMS,
            batch_size: defaults::DEFAULT_EMBEDDING_BATCH_SIZE,
            l1_cache_size: defaults::DEFAULT_L1_CACHE_SIZE,
            l2_cache_enabled: defaults::DEFAULT_L2_CACHE_ENABLED,
        }
    }
}
