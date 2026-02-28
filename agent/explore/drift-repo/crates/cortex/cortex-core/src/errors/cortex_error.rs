use super::{CloudError, ConsolidationError, EmbeddingError, StorageError, TemporalError};

/// Top-level error type for the Cortex memory system.
/// All subsystem errors convert into this via `From` impls.
#[derive(Debug, thiserror::Error)]
pub enum CortexError {
    #[error("memory not found: {id}")]
    MemoryNotFound { id: String },

    #[error("invalid memory type: {type_name}")]
    InvalidType { type_name: String },

    #[error("embedding failed: {0}")]
    EmbeddingError(#[from] EmbeddingError),

    #[error("storage error: {0}")]
    StorageError(#[from] StorageError),

    #[error("causal cycle detected: {path}")]
    CausalCycle { path: String },

    #[error("token budget exceeded: needed {needed}, available {available}")]
    TokenBudgetExceeded { needed: usize, available: usize },

    #[error("migration error: {0}")]
    MigrationError(String),

    #[error("sanitization error: {0}")]
    SanitizationError(String),

    #[error("consolidation error: {0}")]
    ConsolidationError(#[from] ConsolidationError),

    #[error("validation error: {0}")]
    ValidationError(String),

    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("concurrency error: {0}")]
    ConcurrencyError(String),

    #[error("cloud sync error: {0}")]
    CloudSyncError(#[from] CloudError),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("degraded mode: {component} using fallback: {fallback}")]
    DegradedMode { component: String, fallback: String },

    #[error("temporal error: {0}")]
    TemporalError(#[from] TemporalError),

    #[error("multi-agent error: {0}")]
    MultiAgentError(#[from] super::MultiAgentError),

    #[error("authorization denied: {reason}")]
    AuthorizationDenied { reason: String },

    #[error("session boundary: {reason}")]
    SessionBoundary { reason: String },
}

/// Convenience type alias.
pub type CortexResult<T> = Result<T, CortexError>;
