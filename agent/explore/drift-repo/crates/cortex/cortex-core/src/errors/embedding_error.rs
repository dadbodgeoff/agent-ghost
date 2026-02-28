/// Embedding subsystem errors.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("model load failed: {path}: {reason}")]
    ModelLoadFailed { path: String, reason: String },

    #[error("inference failed: {reason}")]
    InferenceFailed { reason: String },

    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("provider unavailable: {provider}")]
    ProviderUnavailable { provider: String },

    #[error("cache miss for hash: {hash}")]
    CacheMiss { hash: String },
}
