//! EmbeddingEngine — the main entry point for cortex-embeddings.
//!
//! Coordinates TF-IDF provider, cache, and enrichment into a single
//! coherent interface for embedding memories and queries.

use cortex_core::memory::BaseMemory;
use tracing::debug;

use crate::cache::EmbeddingCache;
use crate::enrichment;
use crate::tfidf::TfIdfProvider;

/// Configuration for the embedding engine.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Number of embedding dimensions.
    pub dimensions: usize,
    /// Maximum number of cached embeddings.
    pub cache_capacity: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            dimensions: 128,
            cache_capacity: 10_000,
        }
    }
}

/// The main embedding engine.
///
/// Wraps TF-IDF provider, caching, and enrichment into a single interface.
/// Designed for future extension with ONNX/Ollama/API providers.
pub struct EmbeddingEngine {
    provider: TfIdfProvider,
    cache: EmbeddingCache,
    config: EmbeddingConfig,
}

impl EmbeddingEngine {
    /// Create a new engine from configuration.
    pub fn new(config: EmbeddingConfig) -> Self {
        let provider = TfIdfProvider::new(config.dimensions);
        let cache = EmbeddingCache::new(config.cache_capacity);

        Self {
            provider,
            cache,
            config,
        }
    }

    /// Embed a `BaseMemory` with enrichment and caching.
    ///
    /// Uses the memory's content hash for cache lookups. Enriches the
    /// text with metadata before embedding.
    pub fn embed_memory(&mut self, memory: &BaseMemory) -> Vec<f32> {
        let hash = enrichment::content_hash(memory);

        // Check cache first.
        if let Some(vec) = self.cache.get(&hash) {
            debug!(hash = %hash, "cache hit for memory embedding");
            return vec.clone();
        }

        // Enrich and embed.
        let enriched = enrichment::enrich_for_embedding(memory);
        let embedding = self.provider.embed(&enriched);

        // Write to cache.
        self.cache.insert(hash, embedding.clone());

        embedding
    }

    /// Embed a raw query string (with query enrichment).
    pub fn embed_query(&mut self, query: &str) -> Vec<f32> {
        let enriched = enrichment::enrich_query(query);
        let hash = blake3::hash(enriched.as_bytes()).to_hex().to_string();

        // Check cache.
        if let Some(vec) = self.cache.get(&hash) {
            return vec.clone();
        }

        let embedding = self.provider.embed(&enriched);
        self.cache.insert(hash, embedding.clone());
        embedding
    }

    /// Get the configured dimensions.
    pub fn dimensions(&self) -> usize {
        self.config.dimensions
    }

    /// Get the active provider name.
    pub fn provider_name(&self) -> &str {
        "tfidf"
    }

    /// Get the number of cached embeddings.
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cortex_core::memory::{Importance, types::MemoryType};

    fn default_engine() -> EmbeddingEngine {
        EmbeddingEngine::new(EmbeddingConfig {
            dimensions: 128,
            cache_capacity: 100,
        })
    }

    fn make_memory() -> BaseMemory {
        BaseMemory {
            id: uuid::Uuid::new_v4(),
            memory_type: MemoryType::Conversation,
            content: serde_json::json!({"text": "test content"}),
            summary: "test summary".to_string(),
            importance: Importance::Normal,
            confidence: 0.8,
            created_at: Utc::now(),
            last_accessed: Some(Utc::now()),
            access_count: 3,
            tags: vec!["test".to_string()],
            archived: false,
        }
    }

    #[test]
    fn engine_creates_with_defaults() {
        let engine = default_engine();
        assert_eq!(engine.dimensions(), 128);
    }

    #[test]
    fn embed_query_returns_correct_dims() {
        let mut engine = default_engine();
        let vec = engine.embed_query("test query");
        assert_eq!(vec.len(), 128);
    }

    #[test]
    fn embed_query_caches() {
        let mut engine = default_engine();
        let a = engine.embed_query("cached query");
        let b = engine.embed_query("cached query");
        assert_eq!(a, b);
        assert_eq!(engine.cache_size(), 1);
    }

    #[test]
    fn embed_memory_returns_correct_dims() {
        let mut engine = default_engine();
        let mem = make_memory();
        let vec = engine.embed_memory(&mem);
        assert_eq!(vec.len(), 128);
    }

    #[test]
    fn embed_memory_caches() {
        let mut engine = default_engine();
        let mem = make_memory();
        let a = engine.embed_memory(&mem);
        let b = engine.embed_memory(&mem);
        assert_eq!(a, b);
        assert_eq!(engine.cache_size(), 1);
    }

    #[test]
    fn similar_memories_closer_than_dissimilar() {
        let mut engine = default_engine();

        let mut mem_a = make_memory();
        mem_a.summary = "rust programming language systems".to_string();
        mem_a.tags = vec!["rust".to_string(), "programming".to_string()];

        let mut mem_b = make_memory();
        mem_b.summary = "rust programming language design".to_string();
        mem_b.tags = vec!["rust".to_string(), "design".to_string()];
        mem_b.content = serde_json::json!({"text": "different content"});

        let mut mem_c = make_memory();
        mem_c.summary = "cooking pasta recipes italian".to_string();
        mem_c.tags = vec!["cooking".to_string(), "food".to_string()];
        mem_c.content = serde_json::json!({"text": "yet another"});

        let va = engine.embed_memory(&mem_a);
        let vb = engine.embed_memory(&mem_b);
        let vc = engine.embed_memory(&mem_c);

        let cos_ab: f32 = va.iter().zip(&vb).map(|(x, y)| x * y).sum();
        let cos_ac: f32 = va.iter().zip(&vc).map(|(x, y)| x * y).sum();
        assert!(
            cos_ab > cos_ac,
            "similar memories should have higher cosine similarity: {cos_ab} > {cos_ac}"
        );
    }

    #[test]
    fn query_embedding_aligns_with_memory() {
        let mut engine = default_engine();

        let mut mem = make_memory();
        mem.summary = "rust programming language".to_string();
        mem.tags = vec!["rust".to_string()];

        let vm = engine.embed_memory(&mem);
        let vq_relevant = engine.embed_query("rust programming");
        let vq_irrelevant = engine.embed_query("cooking pasta recipes");

        let cos_relevant: f32 = vm.iter().zip(&vq_relevant).map(|(x, y)| x * y).sum();
        let cos_irrelevant: f32 = vm.iter().zip(&vq_irrelevant).map(|(x, y)| x * y).sum();
        assert!(
            cos_relevant > cos_irrelevant,
            "relevant query should have higher cosine similarity"
        );
    }
}
