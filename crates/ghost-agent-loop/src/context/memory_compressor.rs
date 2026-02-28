//! L7 Memory Compressor (Task 18.3).
//!
//! Post-filter compression step for L7 (MEMORY.md + daily logs).
//! After `ConvergenceAwareFilter` removes irrelevant memories by type,
//! this compressor summarizes the remaining entries into a condensed
//! block (500-1000 tokens instead of 4000).
//!
//! Requires either a local model (free) or API budget. Opt-in via config.
//! Compression is idempotent — compressing already-compressed text
//! produces similar output.

use std::sync::Arc;

use ghost_llm::provider::LLMError;
use ghost_llm::quarantine::ContentQuarantine;
use ghost_llm::tokens::TokenCounter;

// ── Configuration ───────────────────────────────────────────────────────

/// Configuration for the memory compressor.
#[derive(Debug, Clone)]
pub struct MemoryCompressorConfig {
    /// Whether memory compression is enabled (default false — opt-in).
    pub enabled: bool,
    /// Target compressed size in tokens (default 1000).
    pub target_tokens: usize,
    /// Compression prompt sent to the LLM.
    pub compression_prompt: String,
}

impl Default for MemoryCompressorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_tokens: 1000,
            compression_prompt: "Summarize these memory entries into a concise context block. \
                Preserve: active goals, recent decisions, key facts, unresolved items. \
                Remove: redundant entries, old completed tasks, verbose descriptions."
                .into(),
        }
    }
}

// ── Memory Compressor ───────────────────────────────────────────────────

/// Compresses filtered L7 memory content via the quarantine/compressor LLM.
pub struct MemoryCompressor {
    compressor: Arc<ContentQuarantine>,
    target_tokens: usize,
    enabled: bool,
    compression_prompt: String,
    counter: TokenCounter,
}

impl MemoryCompressor {
    /// Create a new memory compressor from configuration.
    pub fn new(compressor: Arc<ContentQuarantine>, config: MemoryCompressorConfig) -> Self {
        Self {
            compressor,
            target_tokens: config.target_tokens,
            enabled: config.enabled,
            compression_prompt: config.compression_prompt,
            counter: TokenCounter::default(),
        }
    }

    /// Compress filtered memories into a condensed summary.
    ///
    /// Returns input unchanged if:
    /// - Compression is disabled
    /// - Input is already below `target_tokens`
    /// - Compression fails (graceful fallback)
    pub async fn compress_memories(
        &self,
        filtered_memories: &str,
    ) -> Result<String, LLMError> {
        if !self.enabled {
            return Ok(filtered_memories.to_string());
        }

        if filtered_memories.is_empty() {
            return Ok(String::new());
        }

        let input_tokens = self.counter.count(filtered_memories);

        // Already below target — no compression needed
        if input_tokens <= self.target_tokens {
            return Ok(filtered_memories.to_string());
        }

        match self
            .compressor
            .quarantine_content(filtered_memories, &self.compression_prompt)
            .await
        {
            Ok((compressed, stats)) => {
                tracing::info!(
                    original_tokens = stats.original_tokens,
                    compressed_tokens = stats.compressed_tokens,
                    target_tokens = self.target_tokens,
                    "L7 memory compression applied"
                );
                Ok(compressed)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "L7 memory compression failed, using raw filtered memories"
                );
                Ok(filtered_memories.to_string())
            }
        }
    }

    /// Whether the compressor is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Target token count for compressed output.
    pub fn target_tokens(&self) -> usize {
        self.target_tokens
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ghost_llm::provider::*;
    use ghost_llm::quarantine::QuarantineConfig;

    /// Mock provider that returns a short summary.
    struct MockCompressorProvider {
        response: String,
    }

    #[async_trait]
    impl LLMProvider for MockCompressorProvider {
        fn name(&self) -> &str { "mock-compressor" }

        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolSchema],
        ) -> Result<CompletionResult, LLMError> {
            Ok(CompletionResult {
                response: LLMResponse::Text(self.response.clone()),
                usage: UsageStats::default(),
                model: "mock".into(),
            })
        }

        fn supports_streaming(&self) -> bool { false }
        fn context_window(&self) -> usize { 4096 }
        fn token_pricing(&self) -> TokenPricing {
            TokenPricing { input_per_1k: 0.0, output_per_1k: 0.0 }
        }
    }

    /// Mock provider that always fails.
    struct FailingProvider;

    #[async_trait]
    impl LLMProvider for FailingProvider {
        fn name(&self) -> &str { "failing" }

        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolSchema],
        ) -> Result<CompletionResult, LLMError> {
            Err(LLMError::Unavailable("mock failure".into()))
        }

        fn supports_streaming(&self) -> bool { false }
        fn context_window(&self) -> usize { 4096 }
        fn token_pricing(&self) -> TokenPricing {
            TokenPricing { input_per_1k: 0.0, output_per_1k: 0.0 }
        }
    }

    fn make_compressor(
        response: &str,
        enabled: bool,
        target_tokens: usize,
    ) -> MemoryCompressor {
        let provider: Arc<dyn LLMProvider> = Arc::new(MockCompressorProvider {
            response: response.into(),
        });
        let quarantine_config = QuarantineConfig {
            enabled: true,
            compression_mode: ghost_llm::quarantine::CompressionMode::CompressAll,
            ..Default::default()
        };
        let cq = Arc::new(ContentQuarantine::new(provider, quarantine_config));
        let config = MemoryCompressorConfig {
            enabled,
            target_tokens,
            ..Default::default()
        };
        MemoryCompressor::new(cq, config)
    }

    fn make_failing_compressor(enabled: bool, target_tokens: usize) -> MemoryCompressor {
        let provider: Arc<dyn LLMProvider> = Arc::new(FailingProvider);
        let quarantine_config = QuarantineConfig {
            enabled: true,
            compression_mode: ghost_llm::quarantine::CompressionMode::CompressAll,
            ..Default::default()
        };
        let cq = Arc::new(ContentQuarantine::new(provider, quarantine_config));
        let config = MemoryCompressorConfig {
            enabled,
            target_tokens,
            ..Default::default()
        };
        MemoryCompressor::new(cq, config)
    }

    #[tokio::test]
    async fn disabled_returns_unchanged() {
        let mc = make_compressor("compressed", false, 10);
        let input = "a ".repeat(100);
        let result = mc.compress_memories(&input).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn below_target_returns_unchanged() {
        let mc = make_compressor("compressed", true, 10_000);
        let input = "short memory";
        let result = mc.compress_memories(input).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn above_target_compresses() {
        let mc = make_compressor("Goals: X, Y. Decisions: A.", true, 5);
        let input = "a ".repeat(100); // ~50 tokens, well above target of 5
        let result = mc.compress_memories(&input).await.unwrap();
        assert!(result.contains("Goals"));
        // Compressed output should be shorter
        let counter = TokenCounter::default();
        assert!(counter.count(&result) < counter.count(&input));
    }

    #[tokio::test]
    async fn compressor_error_falls_back_to_raw() {
        let mc = make_failing_compressor(true, 5);
        let input = "a ".repeat(100);
        let result = mc.compress_memories(&input).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn empty_memory_returns_empty() {
        let mc = make_compressor("compressed", true, 5);
        let result = mc.compress_memories("").await.unwrap();
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn exactly_at_target_returns_unchanged() {
        // target_tokens = 3, input is exactly 3 tokens ("hi there friend" ≈ 3-4 tokens)
        let mc = make_compressor("compressed", true, 1000);
        let input = "hi there";
        let result = mc.compress_memories(input).await.unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn config_defaults() {
        let config = MemoryCompressorConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.target_tokens, 1000);
        assert!(config.compression_prompt.contains("active goals"));
    }
}
