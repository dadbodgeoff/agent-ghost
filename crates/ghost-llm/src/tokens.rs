//! Token counting with model-specific tokenization (Req 21 AC4).

/// Token counter with model-specific strategies.
pub struct TokenCounter {
    strategy: TokenStrategy,
}

#[derive(Debug, Clone, Copy)]
pub enum TokenStrategy {
    /// Approximate: bytes / 4 (fallback for unknown models).
    ByteDiv4,
    /// OpenAI tiktoken-based (approximation without tiktoken-rs dep).
    OpenAI,
    /// Anthropic tokenizer (approximation).
    Anthropic,
}

impl TokenCounter {
    pub fn new(strategy: TokenStrategy) -> Self {
        Self { strategy }
    }

    /// Fallback counter using bytes/4 approximation.
    pub fn fallback() -> Self {
        Self::new(TokenStrategy::ByteDiv4)
    }

    /// Count tokens in a string.
    pub fn count(&self, text: &str) -> usize {
        match self.strategy {
            TokenStrategy::ByteDiv4 => {
                // Simple byte/4 approximation
                text.len().div_ceil(4)
            }
            TokenStrategy::OpenAI => {
                // Approximation: ~4 chars per token for English text
                // In production, use tiktoken-rs for exact counts
                let chars = text.chars().count();
                chars.div_ceil(4)
            }
            TokenStrategy::Anthropic => {
                // Anthropic uses a similar BPE tokenizer
                let chars = text.chars().count();
                chars.div_ceil(4)
            }
        }
    }

    /// Count tokens for a list of messages (includes role overhead).
    pub fn count_messages(&self, messages: &[crate::provider::ChatMessage]) -> usize {
        let mut total = 0;
        for msg in messages {
            // ~4 tokens overhead per message for role/formatting
            total += 4;
            total += self.count(&msg.content);
            if let Some(ref calls) = msg.tool_calls {
                for call in calls {
                    total += self.count(&call.name);
                    total += self.count(&call.arguments.to_string());
                }
            }
        }
        total
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::fallback()
    }
}
