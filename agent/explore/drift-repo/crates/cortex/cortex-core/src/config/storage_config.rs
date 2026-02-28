use serde::{Deserialize, Serialize};

use super::defaults;

/// Storage subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Path to the SQLite database file.
    pub db_path: String,
    /// Enable WAL journal mode.
    pub wal_mode: bool,
    /// Memory-mapped I/O size in bytes.
    pub mmap_size: u64,
    /// Page cache size (negative = KB).
    pub cache_size: i64,
    /// Busy timeout in milliseconds.
    pub busy_timeout_ms: u32,
    /// Number of read connections in the pool.
    pub read_pool_size: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: defaults::DEFAULT_DB_FILENAME.to_string(),
            wal_mode: defaults::DEFAULT_WAL_MODE,
            mmap_size: defaults::DEFAULT_MMAP_SIZE,
            cache_size: defaults::DEFAULT_CACHE_SIZE,
            busy_timeout_ms: defaults::DEFAULT_BUSY_TIMEOUT_MS,
            read_pool_size: defaults::DEFAULT_READ_POOL_SIZE,
        }
    }
}
