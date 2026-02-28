use crate::errors::CortexResult;

/// Embedding generation provider.
pub trait IEmbeddingProvider: Send + Sync {
    /// Embed a single text, returning a vector of floats.
    fn embed(&self, text: &str) -> CortexResult<Vec<f32>>;

    /// Embed a batch of texts.
    fn embed_batch(&self, texts: &[String]) -> CortexResult<Vec<Vec<f32>>>;

    /// The dimensionality of embeddings produced by this provider.
    fn dimensions(&self) -> usize;

    /// Human-readable provider name.
    fn name(&self) -> &str;

    /// Whether this provider is currently available.
    fn is_available(&self) -> bool;
}
