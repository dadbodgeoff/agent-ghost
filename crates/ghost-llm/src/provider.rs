//! LLM provider trait and response types (Req 21 AC1).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from LLM providers.
#[derive(Debug, Error)]
pub enum LLMError {
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("timeout after {0}s")]
    Timeout(u64),
    #[error("context window exceeded: {used} > {max}")]
    ContextWindowExceeded { used: usize, max: usize },
    #[error("provider error: {0}")]
    Other(String),
}

/// A tool call requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// LLM response variants (A22.3).
#[derive(Debug, Clone)]
pub enum LLMResponse {
    /// Pure text response.
    Text(String),
    /// One or more tool calls.
    ToolCalls(Vec<LLMToolCall>),
    /// Mixed: text followed by tool calls.
    Mixed { text: String, tool_calls: Vec<LLMToolCall> },
    /// Empty response — treated as NO_REPLY.
    Empty,
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LLMToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool schema for LLM function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Usage statistics from a completion.
#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Result of a completion call.
#[derive(Debug)]
pub struct CompletionResult {
    pub response: LLMResponse,
    pub usage: UsageStats,
    pub model: String,
}

/// Cost per token for a model.
#[derive(Debug, Clone, Copy)]
pub struct TokenPricing {
    pub input_per_1k: f64,
    pub output_per_1k: f64,
}

/// The core LLM provider trait.
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Provider name (e.g., "anthropic", "openai").
    fn name(&self) -> &str;

    /// Complete a conversation.
    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError>;

    /// Whether this provider supports streaming.
    fn supports_streaming(&self) -> bool;

    /// Context window size in tokens.
    fn context_window(&self) -> usize;

    /// Cost per token for the current model.
    fn token_pricing(&self) -> TokenPricing;
}

// ── Concrete provider stubs ─────────────────────────────────────────────
// Full HTTP implementations deferred — these are structural stubs that
// satisfy the trait and allow the agent loop to compile and test.

/// Anthropic Claude provider.
pub struct AnthropicProvider {
    pub model: String,
    pub api_key: String,
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str { "anthropic" }

    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        // Stub: real implementation calls Anthropic Messages API
        Ok(CompletionResult {
            response: LLMResponse::Empty,
            usage: UsageStats::default(),
            model: self.model.clone(),
        })
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 200_000 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.003, output_per_1k: 0.015 }
    }
}

/// OpenAI provider.
pub struct OpenAIProvider {
    pub model: String,
    pub api_key: String,
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn name(&self) -> &str { "openai" }

    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        Ok(CompletionResult {
            response: LLMResponse::Empty,
            usage: UsageStats::default(),
            model: self.model.clone(),
        })
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 128_000 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.005, output_per_1k: 0.015 }
    }
}

/// Google Gemini provider.
pub struct GeminiProvider {
    pub model: String,
    pub api_key: String,
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    fn name(&self) -> &str { "gemini" }

    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        Ok(CompletionResult {
            response: LLMResponse::Empty,
            usage: UsageStats::default(),
            model: self.model.clone(),
        })
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 1_000_000 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.00025, output_per_1k: 0.0005 }
    }
}

/// Ollama local provider.
pub struct OllamaProvider {
    pub model: String,
    pub base_url: String,
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }

    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        Ok(CompletionResult {
            response: LLMResponse::Empty,
            usage: UsageStats::default(),
            model: self.model.clone(),
        })
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 32_768 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.0, output_per_1k: 0.0 } // local
    }
}

/// OpenAI-compatible provider (e.g., vLLM, LiteLLM, Together).
pub struct OpenAICompatProvider {
    pub model: String,
    pub api_key: String,
    pub base_url: String,
    pub context_window_size: usize,
}

#[async_trait]
impl LLMProvider for OpenAICompatProvider {
    fn name(&self) -> &str { "openai_compat" }

    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        Ok(CompletionResult {
            response: LLMResponse::Empty,
            usage: UsageStats::default(),
            model: self.model.clone(),
        })
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { self.context_window_size }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.001, output_per_1k: 0.002 }
    }
}
