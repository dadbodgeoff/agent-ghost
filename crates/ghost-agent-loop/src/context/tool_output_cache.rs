//! Tool Output Cache (Task 17.1).
//!
//! Content-addressable cache for tool outputs. Full tool outputs are stored
//! to disk using blake3 hashes as filenames, enabling deduplication and
//! compact reference strings for observation masking.
//!
//! Cache is per-workspace (not per-session) — tool outputs may be reused
//! across sessions. Atomic writes (temp + rename) prevent corruption.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use blake3::Hasher;

use ghost_llm::tokens::TokenCounter;

/// Default cache directory relative to workspace root.
const DEFAULT_CACHE_DIR: &str = ".ghost/cache/tool_outputs";

/// Reference to a cached tool output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRef {
    /// blake3 hash of the output content (hex-encoded).
    pub hash: String,
    /// Name of the tool that produced the output.
    pub tool_name: String,
    /// Original tool call ID.
    pub tool_call_id: String,
    /// Approximate token count of the original output.
    pub token_count: usize,
    /// Byte count of the original output.
    pub byte_count: usize,
}

/// Content-addressable cache for tool outputs on disk.
pub struct ToolOutputCache {
    cache_dir: PathBuf,
    counter: TokenCounter,
}

impl ToolOutputCache {
    /// Create a new cache with the default directory (`.ghost/cache/tool_outputs/`).
    pub fn new() -> Self {
        Self {
            cache_dir: PathBuf::from(DEFAULT_CACHE_DIR),
            counter: TokenCounter::default(),
        }
    }

    /// Create a new cache with a custom directory.
    pub fn with_dir(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            counter: TokenCounter::default(),
        }
    }

    /// Store a tool output in the cache. Returns a `CacheRef` on success.
    ///
    /// Uses content-addressable storage: same output → same hash → deduplicated.
    /// Writes are atomic (temp file + rename) to prevent corruption.
    pub fn store(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        output: &str,
    ) -> Result<CacheRef, io::Error> {
        // Ensure cache directory exists
        fs::create_dir_all(&self.cache_dir)?;

        let hash = Self::compute_hash(output);
        let file_path = self.cache_dir.join(format!("{hash}.txt"));

        // Only write if not already cached (content-addressable dedup)
        if !file_path.exists() {
            // Atomic write: write to temp, then rename
            let temp_path = self.cache_dir.join(format!("{hash}.tmp"));
            fs::write(&temp_path, output)?;
            fs::rename(&temp_path, &file_path)?;
        }

        let token_count = self.counter.count(output);
        let byte_count = output.len();

        Ok(CacheRef {
            hash,
            tool_name: tool_name.to_string(),
            tool_call_id: tool_call_id.to_string(),
            token_count,
            byte_count,
        })
    }

    /// Load a cached tool output by its hash.
    pub fn load(&self, hash: &str) -> Result<String, io::Error> {
        let file_path = self.cache_dir.join(format!("{hash}.txt"));
        fs::read_to_string(&file_path)
    }

    /// Generate a compact reference string for a cached tool output.
    ///
    /// Format: `[tool_result: {tool_name} → {token_count} tokens, ref:{hash_prefix}]`
    /// where `hash_prefix` is the first 8 characters of the hash.
    pub fn reference_string(cache_ref: &CacheRef) -> String {
        let hash_prefix = &cache_ref.hash[..8.min(cache_ref.hash.len())];
        format!(
            "[tool_result: {} → {} tokens, ref:{}]",
            cache_ref.tool_name, cache_ref.token_count, hash_prefix
        )
    }

    /// Remove cache files older than the given duration.
    ///
    /// Returns the number of files removed.
    pub fn cleanup_older_than(&self, max_age: Duration) -> Result<u32, io::Error> {
        let mut removed = 0u32;

        let entries = match fs::read_dir(&self.cache_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }

            let metadata = entry.metadata()?;
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = modified.elapsed() {
                    if age > max_age {
                        fs::remove_file(&path)?;
                        removed += 1;
                    }
                }
            }
        }

        Ok(removed)
    }

    /// Compute blake3 hash of content, returned as hex string.
    fn compute_hash(content: &str) -> String {
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        hasher.finalize().to_hex().to_string()
    }

    /// Get the cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

impl Default for ToolOutputCache {
    fn default() -> Self {
        Self::new()
    }
}
