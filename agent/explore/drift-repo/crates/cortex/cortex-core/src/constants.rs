/// Cortex system version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum number of versions retained per memory.
pub const MAX_VERSIONS_PER_MEMORY: usize = 10;

/// Maximum traversal depth for causal graph queries.
pub const MAX_CAUSAL_TRAVERSAL_DEPTH: usize = 50;

/// Maximum batch size for bulk operations.
pub const MAX_BULK_BATCH_SIZE: usize = 1000;

/// Default compression level for new memories.
pub const DEFAULT_COMPRESSION_LEVEL: u8 = 2;

/// Feature flags.
pub const FEATURE_CLOUD_SYNC: bool = false;
pub const FEATURE_PREDICTION: bool = true;
pub const FEATURE_LEARNING: bool = true;
