// Single source of truth for all default values.

// --- Storage ---
pub const DEFAULT_DB_FILENAME: &str = "cortex.db";
pub const DEFAULT_WAL_MODE: bool = true;
pub const DEFAULT_MMAP_SIZE: u64 = 268_435_456; // 256 MB
pub const DEFAULT_CACHE_SIZE: i64 = -64_000; // 64 MB (negative = KB)
pub const DEFAULT_BUSY_TIMEOUT_MS: u32 = 5_000;
pub const DEFAULT_READ_POOL_SIZE: usize = 4;

// --- Embeddings ---
pub const DEFAULT_EMBEDDING_DIMENSIONS: usize = 1024;
pub const DEFAULT_MATRYOSHKA_SEARCH_DIMS: usize = 384;
pub const DEFAULT_EMBEDDING_BATCH_SIZE: usize = 50;
pub const DEFAULT_L1_CACHE_SIZE: u64 = 10_000;
pub const DEFAULT_L2_CACHE_ENABLED: bool = true;

// --- Retrieval ---
pub const DEFAULT_TOKEN_BUDGET: usize = 2_000;
pub const DEFAULT_RRF_K: u32 = 60;
pub const DEFAULT_RERANK_TOP_K: usize = 20;
pub const DEFAULT_QUERY_EXPANSION: bool = false;

// --- Consolidation ---
pub const DEFAULT_MIN_CLUSTER_SIZE: usize = 2;
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.75;
pub const DEFAULT_NOVELTY_THRESHOLD: f64 = 0.85;
pub const DEFAULT_LLM_POLISH: bool = false;

// --- Decay ---
pub const DEFAULT_ARCHIVAL_THRESHOLD: f64 = 0.15;
pub const DEFAULT_DECAY_PROCESSING_INTERVAL_SECS: u64 = 3600; // 1 hour

// --- Privacy ---
pub const DEFAULT_NER_ENABLED: bool = false;
pub const DEFAULT_CONTEXT_SCORING: bool = true;

// --- Cloud ---
pub const DEFAULT_SYNC_INTERVAL_SECS: u64 = 300; // 5 minutes
pub const DEFAULT_OFFLINE_MODE: bool = true;

// --- Observability ---
pub const DEFAULT_METRICS_EXPORT_INTERVAL_SECS: u64 = 60;
pub const DEFAULT_LOG_LEVEL: &str = "info";
pub const DEFAULT_TRACING_ENABLED: bool = false;
pub const DEFAULT_HEALTH_CHECK_INTERVAL_SECS: u64 = 300;
