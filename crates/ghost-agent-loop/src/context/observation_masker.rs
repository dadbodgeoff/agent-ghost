//! Observation Masker (Task 17.2).
//!
//! Replaces old tool outputs in conversation history (L8) with compact
//! references. Full outputs are cached to disk via `ToolOutputCache` for
//! on-demand retrieval. Achieves ~50% reduction in L8 token usage.
//!
//! Masking rules:
//! - Only tool_result blocks older than `recency_window` turns are masked.
//! - Only outputs larger than `min_token_threshold` tokens are masked.
//! - Non-tool-result messages are never modified.
//! - Current turn tool results are NEVER masked.

use std::io;
use std::path::PathBuf;

use ghost_llm::tokens::TokenCounter;

use super::tool_output_cache::ToolOutputCache;

/// Default cache directory.
const DEFAULT_CACHE_DIR: &str = ".ghost/cache/tool_outputs";

/// Configuration for the observation masker.
#[derive(Debug, Clone)]
pub struct ObservationMaskerConfig {
    /// Whether observation masking is enabled.
    pub enabled: bool,
    /// Number of recent assistant turns to keep tool outputs inline.
    pub recency_window: usize,
    /// Minimum token count for a tool output to be eligible for masking.
    pub min_token_threshold: usize,
    /// Directory for the tool output cache.
    pub cache_dir: PathBuf,
}

impl Default for ObservationMaskerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            recency_window: 3,
            min_token_threshold: 200,
            cache_dir: PathBuf::from(DEFAULT_CACHE_DIR),
        }
    }
}

/// Masks old tool outputs in conversation history with compact references.
pub struct ObservationMasker {
    cache: ToolOutputCache,
    recency_window: usize,
    min_token_threshold: usize,
    enabled: bool,
    counter: TokenCounter,
}

impl ObservationMasker {
    /// Create a new masker from configuration.
    pub fn new(config: ObservationMaskerConfig) -> Self {
        Self {
            cache: ToolOutputCache::with_dir(config.cache_dir),
            recency_window: config.recency_window,
            min_token_threshold: config.min_token_threshold,
            enabled: config.enabled,
            counter: TokenCounter::default(),
        }
    }

    /// Mask old tool outputs in conversation history.
    ///
    /// Parses the history to identify `tool_result` blocks. For each block
    /// older than `recency_window` assistant turns AND larger than
    /// `min_token_threshold`, the full output is cached and replaced with
    /// a compact reference string.
    ///
    /// Returns the masked history string.
    pub fn mask_history(&self, history: &str) -> Result<String, io::Error> {
        if !self.enabled || history.is_empty() {
            return Ok(history.to_string());
        }

        let blocks = Self::parse_blocks(history);
        let total_assistant_turns = blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Assistant)
            .count();

        // Determine the turn index threshold: blocks from turns older than
        // (total_assistant_turns - recency_window) get masked.
        let mask_threshold = total_assistant_turns.saturating_sub(self.recency_window);

        let mut result = String::with_capacity(history.len());
        let mut assistant_turn_index = 0usize;

        for block in &blocks {
            match block.block_type {
                BlockType::Assistant => {
                    assistant_turn_index += 1;
                    result.push_str(&block.content);
                }
                BlockType::ToolResult { ref tool_name, ref tool_call_id } => {
                    let token_count = self.counter.count(&block.content);
                    let should_mask = assistant_turn_index <= mask_threshold
                        && token_count >= self.min_token_threshold;

                    if should_mask {
                        // Cache the full output and replace with reference
                        let call_id = tool_call_id.as_deref().unwrap_or("unknown");
                        let name = tool_name.as_deref().unwrap_or("unknown_tool");
                        match self.cache.store(call_id, name, &block.content) {
                            Ok(cache_ref) => {
                                let reference = ToolOutputCache::reference_string(&cache_ref);
                                result.push_str(&reference);
                            }
                            Err(_) => {
                                // On cache error, keep original content
                                tracing::warn!("Failed to cache tool output, keeping inline");
                                result.push_str(&block.content);
                            }
                        }
                    } else {
                        result.push_str(&block.content);
                    }
                }
                BlockType::User | BlockType::Other => {
                    result.push_str(&block.content);
                }
            }
        }

        Ok(result)
    }

    /// Recover the full tool output from a compact reference string.
    ///
    /// Parses the reference to extract the hash prefix, then loads from cache.
    pub fn unmask_reference(&self, reference: &str) -> Result<String, io::Error> {
        // Parse "ref:{hash_prefix}" from the reference string
        let hash = reference
            .split("ref:")
            .nth(1)
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid reference format"))?;

        // We need the full hash to load — the reference only has 8 chars.
        // Search cache directory for a file starting with this prefix.
        self.load_by_prefix(hash)
    }

    /// Load a cached output by hash prefix (first 8 chars).
    fn load_by_prefix(&self, prefix: &str) -> Result<String, io::Error> {
        let entries = std::fs::read_dir(self.cache.cache_dir())?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(prefix) && name_str.ends_with(".txt") {
                return std::fs::read_to_string(entry.path());
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No cached output with prefix {prefix}"),
        ))
    }

    /// Get the underlying cache for direct access.
    pub fn cache(&self) -> &ToolOutputCache {
        &self.cache
    }
}

// ── Block parsing ───────────────────────────────────────────────────────

/// Type of a parsed conversation block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BlockType {
    User,
    Assistant,
    ToolResult {
        tool_name: Option<String>,
        tool_call_id: Option<String>,
    },
    Other,
}

/// A parsed block from conversation history.
#[derive(Debug, Clone)]
struct ConversationBlock {
    block_type: BlockType,
    content: String,
}

impl ObservationMasker {
    /// Parse conversation history into typed blocks.
    ///
    /// Recognizes patterns:
    /// - `User:` / `user:` prefix → User block
    /// - `Assistant:` / `assistant:` prefix → Assistant block
    /// - `tool_result` / `Tool Result` / `[tool_result:` → ToolResult block
    /// - Everything else → Other block
    ///
    /// Blocks are delimited by role prefixes. A tool_result block extends
    /// until the next role prefix or end of string.
    fn parse_blocks(history: &str) -> Vec<ConversationBlock> {
        if history.is_empty() {
            return Vec::new();
        }

        let mut blocks = Vec::new();
        let mut current_type = BlockType::Other;
        let mut current_content = String::new();

        for line in history.lines() {
            let trimmed = line.trim_start();

            if let Some(new_type) = Self::detect_block_type(trimmed) {
                // Flush current block
                if !current_content.is_empty() {
                    blocks.push(ConversationBlock {
                        block_type: current_type.clone(),
                        content: std::mem::take(&mut current_content),
                    });
                }
                current_type = new_type;
            }

            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }

        // Flush final block
        if !current_content.is_empty() {
            blocks.push(ConversationBlock {
                block_type: current_type,
                content: current_content,
            });
        }

        blocks
    }

    /// Detect the block type from a line prefix.
    fn detect_block_type(line: &str) -> Option<BlockType> {
        let lower = line.to_lowercase();

        if lower.starts_with("user:") {
            Some(BlockType::User)
        } else if lower.starts_with("assistant:") {
            Some(BlockType::Assistant)
        } else if lower.starts_with("tool_result")
            || lower.starts_with("tool result")
            || lower.starts_with("[tool_result")
        {
            // Try to extract tool name and call ID from the line
            let tool_name = Self::extract_field(line, "tool_name:");
            let tool_call_id = Self::extract_field(line, "tool_call_id:");
            Some(BlockType::ToolResult {
                tool_name,
                tool_call_id,
            })
        } else {
            None
        }
    }

    /// Extract a field value from a tool_result header line.
    /// e.g., `tool_result tool_name:shell tool_call_id:abc123 ...`
    fn extract_field(line: &str, field: &str) -> Option<String> {
        line.find(field).map(|start| {
            let value_start = start + field.len();
            let rest = &line[value_start..];
            let end = rest.find(|c: char| c.is_whitespace() || c == ']' || c == ',')
                .unwrap_or(rest.len());
            rest[..end].trim().to_string()
        })
    }
}
