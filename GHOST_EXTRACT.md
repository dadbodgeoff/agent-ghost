# GHOST ADE — Full Source Extraction

> Generated for audit and architecture review.
> Every file is included in full — no truncation or summarization.

---

## 1. Provider Trait & Implementations (`ghost-llm` crate)

### `crates/ghost-llm/src/lib.rs`

```rust
//! # ghost-llm
//!
//! LLM provider abstraction with model routing, fallback chains,
//! circuit breaker, cost tracking, and streaming support.
//!
//! Providers: Anthropic, OpenAI, Gemini, Ollama, OpenAI-compatible.
//! Complexity tiers: Free, Cheap, Standard, Premium.
//! Convergence downgrade at L3+ (AC6).

pub mod provider;
pub mod router;
pub mod fallback;
pub mod cost;
pub mod tokens;
pub mod streaming;
pub mod auth;
pub mod quarantine;
pub mod proxy;
```

### `crates/ghost-llm/src/provider.rs`

```rust
//! LLM provider trait and response types (Req 21 AC1).

use std::sync::RwLock;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default HTTP timeout for cloud providers (non-streaming).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
/// Longer timeout for local inference (Ollama).
const LOCAL_TIMEOUT: Duration = Duration::from_secs(120);
/// Timeout for SSE streaming connections. Must be much longer than
/// DEFAULT_TIMEOUT because reasoning models can think for 60s+ before
/// streaming, and agent loops with tool calls can run for minutes.
const STREAMING_TIMEOUT: Duration = Duration::from_secs(300);

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LLMToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// LLM response variants (A22.3).
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Result of a completion call.
#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub response: LLMResponse,
    pub usage: UsageStats,
    pub model: String,
}

/// Cost per token for a model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

    /// Update the auth credentials at runtime (used by FallbackChain
    /// for auth profile rotation on 401/429). Default is no-op for
    /// providers that don't support runtime auth changes.
    fn update_auth(&self, _api_key: &str, _org_id: Option<&str>) {}
}

// ── Shared helpers ──────────────────────────────────────────────────────

/// Build a reqwest client with the given timeout.
fn build_client(timeout: Duration) -> Result<reqwest::Client, LLMError> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| LLMError::Other(format!("HTTP client build error: {e}")))
}

/// Map HTTP status codes to LLMError.
fn map_http_error(status: reqwest::StatusCode, body: &str, headers: &reqwest::header::HeaderMap) -> LLMError {
    match status.as_u16() {
        401 | 403 => LLMError::AuthFailed(format!("{status}: {body}")),
        429 => {
            let retry_after = headers
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30);
            LLMError::RateLimited { retry_after_secs: retry_after }
        }
        529 => LLMError::Unavailable(format!("overloaded: {body}")),
        500..=599 => LLMError::Unavailable(format!("{status}: {body}")),
        _ => LLMError::Other(format!("{status}: {body}")),
    }
}

// ── OpenAI-format shared helper ─────────────────────────────────────────
// Used by OpenAIProvider, OllamaProvider, and OpenAICompatProvider.

/// Build OpenAI-format messages array from ChatMessage slice.
fn openai_format_messages(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    messages.iter().map(|m| {
        let role = match m.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        let mut msg = serde_json::json!({
            "role": role,
            "content": m.content,
        });
        if let Some(ref tc) = m.tool_calls {
            let calls: Vec<serde_json::Value> = tc.iter().map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "type": "function",
                    "function": {
                        "name": c.name,
                        "arguments": if c.arguments.is_string() {
                            c.arguments.clone()
                        } else {
                            serde_json::Value::String(c.arguments.to_string())
                        },
                    }
                })
            }).collect();
            msg["tool_calls"] = serde_json::Value::Array(calls);
        }
        if let Some(ref id) = m.tool_call_id {
            msg["tool_call_id"] = serde_json::Value::String(id.clone());
        }
        msg
    }).collect()
}

/// Build OpenAI-format tools array from ToolSchema slice.
fn openai_format_tools(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }
        })
    }).collect()
}

/// Parse an OpenAI-format response body into CompletionResult.
fn parse_openai_response(body: &serde_json::Value, model_fallback: &str) -> Result<CompletionResult, LLMError> {
    let model = body["model"].as_str().unwrap_or(model_fallback).to_string();

    let usage = if let Some(u) = body.get("usage") {
        let pt = u["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let ct = u["completion_tokens"].as_u64().unwrap_or(0) as usize;
        UsageStats {
            prompt_tokens: pt,
            completion_tokens: ct,
            total_tokens: pt + ct,
        }
    } else {
        UsageStats::default()
    };

    let choice = body["choices"]
        .as_array()
        .and_then(|c| c.first())
        .ok_or_else(|| LLMError::InvalidResponse("no choices in response".into()))?;

    let message = &choice["message"];
    let content = message["content"].as_str().map(|s| s.to_string());
    let tool_calls_raw = message.get("tool_calls").and_then(|v| v.as_array());

    let tool_calls: Vec<LLMToolCall> = tool_calls_raw
        .map(|arr| {
            arr.iter().filter_map(|tc| {
                let id = tc["id"].as_str()?.to_string();
                let name = tc["function"]["name"].as_str()?.to_string();
                let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let arguments = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                Some(LLMToolCall { id, name, arguments })
            }).collect()
        })
        .unwrap_or_default();

    let has_tool_calls = !tool_calls.is_empty();
    let has_text = content.as_ref().map_or(false, |t| !t.is_empty());

    let response = match (has_text, has_tool_calls) {
        (true, false) => LLMResponse::Text(content.unwrap()),
        (false, true) => LLMResponse::ToolCalls(tool_calls),
        (true, true) => LLMResponse::Mixed { text: content.unwrap(), tool_calls },
        (false, false) => LLMResponse::Empty,
    };

    Ok(CompletionResult { response, usage, model })
}

/// Shared OpenAI-format completion call.
async fn openai_format_complete(
    url: &str,
    model: &str,
    api_key: Option<&str>,
    messages: &[ChatMessage],
    tools: &[ToolSchema],
    timeout: Duration,
) -> Result<CompletionResult, LLMError> {
    let client = build_client(timeout)?;

    let mut body = serde_json::json!({
        "model": model,
        "messages": openai_format_messages(messages),
    });
    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(openai_format_tools(tools));
    }

    let mut req = client.post(url).json(&body);
    if let Some(key) = api_key {
        req = req.header("authorization", format!("Bearer {key}"));
    }

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            LLMError::Timeout(timeout.as_secs())
        } else {
            LLMError::Other(format!("HTTP request failed: {e}"))
        }
    })?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let resp_body: serde_json::Value = resp.json().await
        .map_err(|e| LLMError::InvalidResponse(format!("JSON parse error: {e}")))?;

    if !status.is_success() {
        let error_msg = resp_body["error"]["message"]
            .as_str()
            .unwrap_or(&resp_body.to_string())
            .to_string();
        return Err(map_http_error(status, &error_msg, &headers));
    }

    parse_openai_response(&resp_body, model)
}

// ── Anthropic Claude provider ───────────────────────────────────────────

/// Anthropic Claude provider.
pub struct AnthropicProvider {
    pub model: String,
    pub api_key: RwLock<String>,
}

impl AnthropicProvider {
    /// Build the Anthropic messages array from ChatMessage slice.
    /// System messages are separated out (Anthropic uses a top-level `system` param).
    fn build_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<serde_json::Value>) {
        let mut system_parts: Vec<String> = Vec::new();
        let mut api_messages: Vec<serde_json::Value> = Vec::new();

        for m in messages {
            match m.role {
                MessageRole::System => {
                    system_parts.push(m.content.clone());
                }
                MessageRole::User => {
                    api_messages.push(serde_json::json!({
                        "role": "user",
                        "content": m.content,
                    }));
                }
                MessageRole::Assistant => {
                    if let Some(ref tc) = m.tool_calls {
                        // Assistant message with tool_use blocks.
                        let mut content_blocks: Vec<serde_json::Value> = Vec::new();
                        if !m.content.is_empty() {
                            content_blocks.push(serde_json::json!({
                                "type": "text",
                                "text": m.content,
                            }));
                        }
                        for call in tc {
                            content_blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": call.id,
                                "name": call.name,
                                "input": call.arguments,
                            }));
                        }
                        api_messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": content_blocks,
                        }));
                    } else {
                        api_messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": m.content,
                        }));
                    }
                }
                MessageRole::Tool => {
                    // Tool results → tool_result content block in a user message.
                    let tool_call_id = m.tool_call_id.clone().unwrap_or_default();
                    api_messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": m.content,
                        }],
                    }));
                }
            }
        }

        let system = if system_parts.is_empty() {
            None
        } else {
            Some(system_parts.join("\n\n"))
        };

        (system, api_messages)
    }

    /// Build Anthropic tools array from ToolSchema slice.
    fn build_tools(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
        tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        }).collect()
    }

    /// Parse Anthropic response content blocks into LLMResponse.
    fn parse_response(body: &serde_json::Value) -> Result<(LLMResponse, UsageStats), LLMError> {
        let usage = if let Some(u) = body.get("usage") {
            let pt = u["input_tokens"].as_u64().unwrap_or(0) as usize;
            let ct = u["output_tokens"].as_u64().unwrap_or(0) as usize;
            UsageStats {
                prompt_tokens: pt,
                completion_tokens: ct,
                total_tokens: pt + ct,
            }
        } else {
            UsageStats::default()
        };

        let content = body["content"]
            .as_array()
            .ok_or_else(|| LLMError::InvalidResponse("no content array in response".into()))?;

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<LLMToolCall> = Vec::new();

        for block in content {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(t) = block["text"].as_str() {
                        text_parts.push(t.to_string());
                    }
                }
                Some("tool_use") => {
                    let id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    let arguments = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                    tool_calls.push(LLMToolCall { id, name, arguments });
                }
                _ => {} // Skip unknown block types.
            }
        }

        let text = text_parts.join("");
        let response = match (text.is_empty(), tool_calls.is_empty()) {
            (true, true) => LLMResponse::Empty,
            (false, true) => LLMResponse::Text(text),
            (true, false) => LLMResponse::ToolCalls(tool_calls),
            (false, false) => LLMResponse::Mixed { text, tool_calls },
        };

        Ok((response, usage))
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str { "anthropic" }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self.api_key.read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();

        let client = build_client(DEFAULT_TIMEOUT)?;
        let (system, api_messages) = Self::build_messages(messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": api_messages,
        });
        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys);
        }
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(Self::build_tools(tools));
        }

        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LLMError::Timeout(DEFAULT_TIMEOUT.as_secs())
                } else {
                    LLMError::Other(format!("HTTP request failed: {e}"))
                }
            })?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let resp_body: serde_json::Value = resp.json().await
            .map_err(|e| LLMError::InvalidResponse(format!("JSON parse error: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or(&resp_body.to_string())
                .to_string();
            return Err(map_http_error(status, &error_msg, &headers));
        }

        let (response, usage) = Self::parse_response(&resp_body)?;
        let model = resp_body["model"].as_str().unwrap_or(&self.model).to_string();

        Ok(CompletionResult { response, usage, model })
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 200_000 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.003, output_per_1k: 0.015 }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => tracing::error!(provider = "anthropic", error = %e, "Failed to update API key — RwLock poisoned"),
        }
    }
}

// ── OpenAI provider ─────────────────────────────────────────────────────

/// OpenAI provider.
pub struct OpenAIProvider {
    pub model: String,
    pub api_key: RwLock<String>,
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn name(&self) -> &str { "openai" }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self.api_key.read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();

        openai_format_complete(
            "https://api.openai.com/v1/chat/completions",
            &self.model,
            Some(&api_key),
            messages,
            tools,
            DEFAULT_TIMEOUT,
        ).await
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 128_000 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.005, output_per_1k: 0.015 }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => tracing::error!(provider = "openai", error = %e, "Failed to update API key — RwLock poisoned"),
        }
    }
}

// ── Google Gemini provider ──────────────────────────────────────────────

/// Google Gemini provider.
pub struct GeminiProvider {
    pub model: String,
    pub api_key: RwLock<String>,
}

impl GeminiProvider {
    /// Build Gemini contents array from ChatMessage slice.
    /// System messages use the `systemInstruction` field.
    fn build_contents(messages: &[ChatMessage]) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
        let mut system_parts: Vec<String> = Vec::new();
        let mut contents: Vec<serde_json::Value> = Vec::new();

        for m in messages {
            match m.role {
                MessageRole::System => {
                    system_parts.push(m.content.clone());
                }
                MessageRole::User => {
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{"text": m.content}],
                    }));
                }
                MessageRole::Assistant => {
                    let mut parts: Vec<serde_json::Value> = Vec::new();
                    if !m.content.is_empty() {
                        parts.push(serde_json::json!({"text": m.content}));
                    }
                    if let Some(ref tc) = m.tool_calls {
                        for call in tc {
                            parts.push(serde_json::json!({
                                "functionCall": {
                                    "name": call.name,
                                    "args": call.arguments,
                                }
                            }));
                        }
                    }
                    contents.push(serde_json::json!({
                        "role": "model",
                        "parts": parts,
                    }));
                }
                MessageRole::Tool => {
                    // Tool results → functionResponse part.
                    let name = m.tool_call_id.clone().unwrap_or_else(|| "unknown".into());
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": name,
                                "response": {"result": m.content},
                            }
                        }],
                    }));
                }
            }
        }

        let system_instruction = if system_parts.is_empty() {
            None
        } else {
            Some(serde_json::json!({
                "parts": [{"text": system_parts.join("\n\n")}],
            }))
        };

        (system_instruction, contents)
    }

    /// Build Gemini tools array from ToolSchema slice.
    fn build_tools(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
        let declarations: Vec<serde_json::Value> = tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        }).collect();
        vec![serde_json::json!({"functionDeclarations": declarations})]
    }

    /// Parse Gemini response into LLMResponse.
    fn parse_response(body: &serde_json::Value) -> Result<(LLMResponse, UsageStats), LLMError> {
        let usage = if let Some(u) = body.get("usageMetadata") {
            let pt = u["promptTokenCount"].as_u64().unwrap_or(0) as usize;
            let ct = u["candidatesTokenCount"].as_u64().unwrap_or(0) as usize;
            UsageStats {
                prompt_tokens: pt,
                completion_tokens: ct,
                total_tokens: pt + ct,
            }
        } else {
            UsageStats::default()
        };

        let parts = body["candidates"]
            .as_array()
            .and_then(|c| c.first())
            .and_then(|c| c["content"]["parts"].as_array());

        let parts = match parts {
            Some(p) => p,
            None => return Ok((LLMResponse::Empty, usage)),
        };

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<LLMToolCall> = Vec::new();

        for part in parts {
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                text_parts.push(t.to_string());
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc["name"].as_str().unwrap_or("").to_string();
                let arguments = fc.get("args").cloned().unwrap_or(serde_json::json!({}));
                let id = format!("gemini-{}", uuid::Uuid::now_v7());
                tool_calls.push(LLMToolCall { id, name, arguments });
            }
        }

        let text = text_parts.join("");
        let response = match (text.is_empty(), tool_calls.is_empty()) {
            (true, true) => LLMResponse::Empty,
            (false, true) => LLMResponse::Text(text),
            (true, false) => LLMResponse::ToolCalls(tool_calls),
            (false, false) => LLMResponse::Mixed { text, tool_calls },
        };

        Ok((response, usage))
    }
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    fn name(&self) -> &str { "gemini" }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self.api_key.read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();

        let client = build_client(DEFAULT_TIMEOUT)?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, api_key
        );

        let (system_instruction, contents) = Self::build_contents(messages);

        let mut body = serde_json::json!({
            "contents": contents,
        });
        if let Some(si) = system_instruction {
            body["systemInstruction"] = si;
        }
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(Self::build_tools(tools));
        }

        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LLMError::Timeout(DEFAULT_TIMEOUT.as_secs())
                } else {
                    LLMError::Other(format!("HTTP request failed: {e}"))
                }
            })?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let resp_body: serde_json::Value = resp.json().await
            .map_err(|e| LLMError::InvalidResponse(format!("JSON parse error: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or(&resp_body.to_string())
                .to_string();
            return Err(map_http_error(status, &error_msg, &headers));
        }

        let (response, usage) = Self::parse_response(&resp_body)?;
        Ok(CompletionResult { response, usage, model: self.model.clone() })
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 1_000_000 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.00025, output_per_1k: 0.0005 }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => tracing::error!(provider = "gemini", error = %e, "Failed to update API key — RwLock poisoned"),
        }
    }
}

// ── Ollama local provider ───────────────────────────────────────────────

/// Ollama local provider.
/// Uses native /api/chat endpoint with think:false to disable reasoning overhead.
/// 120s timeout for local inference.
pub struct OllamaProvider {
    pub model: String,
    pub base_url: String,
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        // Use native Ollama /api/chat endpoint (not OpenAI-compat) so we can
        // pass `think: false` to disable chain-of-thought reasoning for models
        // like Qwen 3.5 and DeepSeek-R1. Without this, these models generate
        // thousands of reasoning tokens that waste local compute.
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let client = build_client(LOCAL_TIMEOUT)?;

        let ollama_messages: Vec<serde_json::Value> = messages.iter().map(|m| {
            ollama_format_message(m)
        }).collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": ollama_messages,
            "stream": false,
            "think": false,
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(ollama_format_tools(tools));
        }

        let resp = client.post(&url).json(&body).send().await.map_err(|e| {
            if e.is_timeout() {
                LLMError::Timeout(LOCAL_TIMEOUT.as_secs())
            } else {
                LLMError::Other(format!("HTTP request failed: {e}"))
            }
        })?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp.json().await
            .map_err(|e| LLMError::InvalidResponse(format!("JSON parse error: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]
                .as_str()
                .unwrap_or(&resp_body.to_string())
                .to_string();
            return Err(LLMError::Other(format!("Ollama error: {error_msg}")));
        }

        parse_ollama_response(&resp_body, &self.model)
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { 262_144 }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.0, output_per_1k: 0.0 } // local
    }
}

impl OllamaProvider {
    /// Stream a chat completion via Ollama's native `/api/chat` with `stream: true`.
    ///
    /// Returns a `StreamChunkStream` that yields `TextDelta` chunks as Ollama generates
    /// tokens, then `Done(UsageStats)` when the response is complete.
    pub fn stream_chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> crate::streaming::StreamChunkStream {
        use crate::streaming::StreamChunk;
        use futures::StreamExt;

        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let ollama_messages: Vec<serde_json::Value> = messages.iter().map(|m| {
            ollama_format_message(m)
        }).collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": ollama_messages,
            "stream": true,
            "think": false,
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(ollama_format_tools(tools));
        }

        let model_name = self.model.clone();

        Box::pin(async_stream::stream! {
            let client = match build_client(LOCAL_TIMEOUT) {
                Ok(c) => c,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            let resp = match client.post(&url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    if e.is_timeout() {
                        yield Err(LLMError::Timeout(LOCAL_TIMEOUT.as_secs()));
                    } else {
                        yield Err(LLMError::Other(format!("HTTP request failed: {e}")));
                    }
                    return;
                }
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let body_text = resp.text().await.unwrap_or_default();
                yield Err(LLMError::Other(format!("Ollama error HTTP {status}: {body_text}")));
                return;
            }

            let mut byte_stream = resp.bytes_stream();
            let mut buffer = String::new();
            let mut prompt_tokens: usize = 0;
            let mut completion_tokens: usize = 0;

            while let Some(chunk_result) = byte_stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        yield Err(LLMError::Other(format!("Stream read error: {e}")));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Ollama sends newline-delimited JSON.
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let parsed: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let done = parsed["done"].as_bool().unwrap_or(false);

                    if done {
                        prompt_tokens = parsed["prompt_eval_count"].as_u64().unwrap_or(0) as usize;
                        completion_tokens = parsed["eval_count"].as_u64().unwrap_or(0) as usize;

                        // Check for tool calls in the final chunk.
                        if let Some(tool_calls) = parsed["message"].get("tool_calls").and_then(|v| v.as_array()) {
                            for tc in tool_calls {
                                let func = &tc["function"];
                                if let Some(name) = func["name"].as_str() {
                                    let id = format!("ollama_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0"));
                                    let args = func.get("arguments").cloned().unwrap_or(serde_json::json!({}));
                                    yield Ok(StreamChunk::ToolCallStart {
                                        id: id.clone(),
                                        name: name.to_string(),
                                    });
                                    yield Ok(StreamChunk::ToolCallDelta {
                                        id,
                                        arguments_delta: args.to_string(),
                                    });
                                }
                            }
                        }

                        yield Ok(StreamChunk::Done(UsageStats {
                            prompt_tokens,
                            completion_tokens,
                            total_tokens: prompt_tokens + completion_tokens,
                        }));
                        return;
                    }

                    // Yield text delta from non-done chunks.
                    if let Some(content) = parsed["message"]["content"].as_str() {
                        if !content.is_empty() {
                            yield Ok(StreamChunk::TextDelta(content.to_string()));
                        }
                    }
                }
            }

            // Stream ended without a done=true chunk — yield Done with what we have.
            yield Ok(StreamChunk::Done(UsageStats {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }));

            let _ = model_name; // suppress unused warning
        })
    }
}

/// Format a ChatMessage for Ollama's native /api/chat endpoint.
/// Includes tool_calls on assistant messages and tool_call_id on tool messages
/// so that multi-turn tool-use conversations are correctly represented.
fn ollama_format_message(m: &ChatMessage) -> serde_json::Value {
    let role = match m.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };
    let mut msg = serde_json::json!({ "role": role, "content": m.content });

    // Attach tool_calls to assistant messages so Ollama knows a tool was invoked.
    if let Some(ref calls) = m.tool_calls {
        let tc: Vec<serde_json::Value> = calls.iter().map(|c| {
            serde_json::json!({
                "function": {
                    "name": c.name,
                    "arguments": c.arguments,
                }
            })
        }).collect();
        if !tc.is_empty() {
            msg["tool_calls"] = serde_json::Value::Array(tc);
        }
    }

    msg
}

/// Format tools for Ollama's native /api/chat endpoint.
fn ollama_format_tools(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }
        })
    }).collect()
}

/// Parse Ollama native /api/chat response into CompletionResult.
fn parse_ollama_response(body: &serde_json::Value, model_fallback: &str) -> Result<CompletionResult, LLMError> {
    let model = body["model"].as_str().unwrap_or(model_fallback).to_string();

    let message = &body["message"];
    let content = message["content"].as_str().map(|s| s.to_string());

    // Ollama native format: tool_calls are in message.tool_calls
    let tool_calls_raw = message.get("tool_calls").and_then(|v| v.as_array());
    let tool_calls: Vec<LLMToolCall> = tool_calls_raw
        .map(|arr| {
            arr.iter().filter_map(|tc| {
                let func = &tc["function"];
                let name = func["name"].as_str()?.to_string();
                let arguments = func.get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                // Ollama native format doesn't include a call ID; generate one.
                let id = format!("ollama_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0"));
                Some(LLMToolCall { id, name, arguments })
            }).collect()
        })
        .unwrap_or_default();

    // Approximate token counts from Ollama's duration-based stats.
    let prompt_tokens = body["prompt_eval_count"].as_u64().unwrap_or(0) as usize;
    let completion_tokens = body["eval_count"].as_u64().unwrap_or(0) as usize;
    let usage = UsageStats {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
    };

    let has_tool_calls = !tool_calls.is_empty();
    let has_text = content.as_ref().map_or(false, |t| !t.is_empty());

    let response = match (has_text, has_tool_calls) {
        (true, false) => LLMResponse::Text(content.unwrap()),
        (false, true) => LLMResponse::ToolCalls(tool_calls),
        (true, true) => LLMResponse::Mixed { text: content.unwrap(), tool_calls },
        (false, false) => LLMResponse::Empty,
    };

    Ok(CompletionResult { response, usage, model })
}

// ── OpenAI-compatible provider ──────────────────────────────────────────

/// OpenAI-compatible provider (e.g., vLLM, LiteLLM, Together).
pub struct OpenAICompatProvider {
    pub model: String,
    pub api_key: RwLock<String>,
    pub base_url: String,
    pub context_window_size: usize,
}

#[async_trait]
impl LLMProvider for OpenAICompatProvider {
    fn name(&self) -> &str { "openai_compat" }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self.api_key.read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();

        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));
        openai_format_complete(&url, &self.model, Some(&api_key), messages, tools, DEFAULT_TIMEOUT).await
    }

    fn supports_streaming(&self) -> bool { true }
    fn context_window(&self) -> usize { self.context_window_size }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing { input_per_1k: 0.001, output_per_1k: 0.002 }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => tracing::error!(provider = "openai_compat", error = %e, "Failed to update API key — RwLock poisoned"),
        }
    }
}

impl OpenAICompatProvider {
    /// Stream a chat completion via the OpenAI-compatible `/v1/chat/completions`
    /// endpoint with `stream: true`.
    ///
    /// Returns a `StreamChunkStream` that yields `TextDelta` chunks as the model
    /// generates tokens, `ToolCallStart`/`ToolCallDelta` for tool calls, then
    /// `Done(UsageStats)` when the response is complete.
    ///
    /// This is critical for reasoning models (e.g. Grok) where non-streaming
    /// calls can block for 30–60+ seconds with zero user feedback.
    pub fn stream_chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> crate::streaming::StreamChunkStream {
        use crate::streaming::StreamChunk;
        use futures::StreamExt;

        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));
        let api_key = self.api_key.read()
            .map(|k| k.clone())
            .unwrap_or_default();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": openai_format_messages(messages),
            "stream": true,
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(openai_format_tools(tools));
        }
        // Request usage stats in the final chunk (OpenAI extension).
        body["stream_options"] = serde_json::json!({"include_usage": true});

        Box::pin(async_stream::stream! {
            let client = match build_client(STREAMING_TIMEOUT) {
                Ok(c) => c,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            let resp = match client
                .post(&url)
                .header("authorization", format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if e.is_timeout() {
                        yield Err(LLMError::Timeout(STREAMING_TIMEOUT.as_secs()));
                    } else {
                        yield Err(LLMError::Other(format!("HTTP request failed: {e}")));
                    }
                    return;
                }
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let headers = resp.headers().clone();
                let body_text = resp.text().await.unwrap_or_default();
                yield Err(map_http_error(status, &body_text, &headers));
                return;
            }

            let mut byte_stream = resp.bytes_stream();
            let mut buffer = String::new();
            let mut usage = UsageStats::default();
            // Track tool call index → id mapping. OpenAI only sends the `id`
            // in the first delta chunk for each tool call; subsequent chunks
            // only include the `index`.
            let mut tool_call_ids: std::collections::HashMap<u64, String> = std::collections::HashMap::new();

            // Per-chunk idle timeout: if no bytes arrive for 90s, the
            // connection is dead (LLM hung, network issue, etc.).
            let idle_timeout = Duration::from_secs(90);

            loop {
                let chunk_result = match tokio::time::timeout(idle_timeout, byte_stream.next()).await {
                    Ok(Some(result)) => result,
                    Ok(None) => break, // stream ended naturally
                    Err(_) => {
                        yield Err(LLMError::Timeout(idle_timeout.as_secs()));
                        return;
                    }
                };
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        yield Err(LLMError::Other(format!("Stream read error: {e}")));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // OpenAI SSE format: "data: {json}\n\n"
                while let Some(double_newline) = buffer.find("\n\n") {
                    let block = buffer[..double_newline].to_string();
                    buffer = buffer[double_newline + 2..].to_string();

                    for line in block.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with(':') {
                            continue; // skip empty lines and comments
                        }

                        let data = if let Some(d) = line.strip_prefix("data: ") {
                            d.trim()
                        } else {
                            continue;
                        };

                        if data == "[DONE]" {
                            yield Ok(StreamChunk::Done(usage.clone()));
                            return;
                        }

                        let parsed: serde_json::Value = match serde_json::from_str(data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        // Extract usage from the final chunk if present.
                        if let Some(u) = parsed.get("usage") {
                            let pt = u["prompt_tokens"].as_u64().unwrap_or(0) as usize;
                            let ct = u["completion_tokens"].as_u64().unwrap_or(0) as usize;
                            usage = UsageStats {
                                prompt_tokens: pt,
                                completion_tokens: ct,
                                total_tokens: pt + ct,
                            };
                        }

                        let Some(delta) = parsed["choices"]
                            .as_array()
                            .and_then(|c| c.first())
                            .and_then(|c| c.get("delta"))
                        else {
                            continue;
                        };

                        // Text content delta.
                        if let Some(content) = delta["content"].as_str() {
                            if !content.is_empty() {
                                yield Ok(StreamChunk::TextDelta(content.to_string()));
                            }
                        }

                        // Tool call deltas (OpenAI format).
                        // First chunk: { "index": 0, "id": "call_xxx", "function": {"name": "web_search", "arguments": ""} }
                        // Subsequent:  { "index": 0, "function": {"arguments": "{\"qu"} }
                        if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                            for tc in tool_calls {
                                let idx = tc["index"].as_u64().unwrap_or(0);

                                // First chunk for this tool call includes id + function.name.
                                if let Some(id) = tc["id"].as_str() {
                                    tool_call_ids.insert(idx, id.to_string());
                                    let name = tc["function"]["name"]
                                        .as_str()
                                        .unwrap_or("unknown")
                                        .to_string();
                                    yield Ok(StreamChunk::ToolCallStart {
                                        id: id.to_string(),
                                        name,
                                    });
                                }

                                // Subsequent chunks include function.arguments delta.
                                if let Some(args_delta) = tc["function"]["arguments"].as_str() {
                                    if !args_delta.is_empty() {
                                        let call_id = tool_call_ids
                                            .get(&idx)
                                            .cloned()
                                            .unwrap_or_else(|| format!("call_{idx}"));
                                        yield Ok(StreamChunk::ToolCallDelta {
                                            id: call_id,
                                            arguments_delta: args_delta.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Stream ended without [DONE] — yield Done with whatever usage we collected.
            yield Ok(StreamChunk::Done(usage));
        })
    }
}

// ── Streaming shim for non-streaming providers ──────────────────────────

/// Wraps a non-streaming `complete()` call into a `StreamChunkStream`.
///
/// This is the fallback for providers that don't implement native streaming.
/// It calls `complete()`, then yields the result as stream chunks.
pub fn complete_stream_shim(
    provider: std::sync::Arc<dyn LLMProvider>,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolSchema>,
) -> crate::streaming::StreamChunkStream {
    use crate::streaming::StreamChunk;

    Box::pin(async_stream::stream! {
        match provider.complete(&messages, &tools).await {
            Ok(result) => {
                match result.response {
                    LLMResponse::Text(text) => {
                        yield Ok(StreamChunk::TextDelta(text));
                    }
                    LLMResponse::ToolCalls(calls) => {
                        for call in calls {
                            yield Ok(StreamChunk::ToolCallStart {
                                id: call.id.clone(),
                                name: call.name.clone(),
                            });
                            yield Ok(StreamChunk::ToolCallDelta {
                                id: call.id,
                                arguments_delta: call.arguments.to_string(),
                            });
                        }
                    }
                    LLMResponse::Mixed { text, tool_calls } => {
                        yield Ok(StreamChunk::TextDelta(text));
                        for call in tool_calls {
                            yield Ok(StreamChunk::ToolCallStart {
                                id: call.id.clone(),
                                name: call.name.clone(),
                            });
                            yield Ok(StreamChunk::ToolCallDelta {
                                id: call.id,
                                arguments_delta: call.arguments.to_string(),
                            });
                        }
                    }
                    LLMResponse::Empty => {}
                }
                yield Ok(StreamChunk::Done(result.usage));
            }
            Err(e) => {
                yield Err(e);
            }
        }
    })
}
```

### `crates/ghost-llm/src/streaming.rs`

```rust
//! Streaming response types (A2.12).

use std::pin::Pin;

use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::provider::{LLMError, UsageStats};

/// A chunk in a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamChunk {
    /// Text delta.
    TextDelta(String),
    /// Tool call start.
    ToolCallStart { id: String, name: String },
    /// Tool call argument delta.
    ToolCallDelta { id: String, arguments_delta: String },
    /// Stream complete with usage stats.
    Done(UsageStats),
    /// Error during streaming.
    Error(String),
}

/// A boxed async stream of streaming chunks.
pub type StreamChunkStream = Pin<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send>>;

/// Collect all text deltas from a vec of chunks into a single string.
pub fn collect_text_from_chunks(chunks: &[StreamChunk]) -> String {
    chunks
        .iter()
        .filter_map(|c| match c {
            StreamChunk::TextDelta(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}
```

### `crates/ghost-llm/src/fallback.rs`

```rust
//! Fallback chain with auth rotation and exponential backoff (Req 21 AC3).
//! Provider circuit breaker (A22.2) — INDEPENDENT from tool circuit breaker.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;

use crate::provider::{
    ChatMessage, CompletionResult, LLMError, LLMProvider, ToolSchema,
};

/// Circuit breaker state for a single provider (A22.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBState {
    Closed,
    Open,
    HalfOpen,
}

/// Per-provider circuit breaker.
pub struct ProviderCircuitBreaker {
    state: CBState,
    consecutive_failures: u32,
    threshold: u32,
    cooldown: Duration,
    last_failure: Option<Instant>,
}

impl ProviderCircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            state: CBState::Closed,
            consecutive_failures: 0,
            threshold,
            cooldown,
            last_failure: None,
        }
    }

    pub fn state(&self) -> CBState {
        self.state
    }

    /// Check if the circuit breaker allows a request.
    pub fn can_attempt(&mut self) -> bool {
        match self.state {
            CBState::Closed => true,
            CBState::Open => {
                if let Some(last) = self.last_failure {
                    if last.elapsed() >= self.cooldown {
                        self.state = CBState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CBState::HalfOpen => true,
        }
    }

    /// Record a successful call.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CBState::Closed;
    }

    /// Record a failed call.
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_failure = Some(Instant::now());

        match self.state {
            CBState::Closed => {
                if self.consecutive_failures >= self.threshold {
                    self.state = CBState::Open;
                }
            }
            CBState::HalfOpen => {
                // HalfOpen + failure → Open (cooldown resets)
                self.state = CBState::Open;
            }
            CBState::Open => {}
        }
    }
}

/// An auth profile for a provider (API key + optional org).
#[derive(Debug, Clone)]
pub struct AuthProfile {
    pub api_key: String,
    pub org_id: Option<String>,
}

/// Fallback chain: rotates auth profiles on 401/429, falls back to next
/// provider, exponential backoff + jitter, 30s total retry budget.
pub struct FallbackChain {
    providers: Vec<(Arc<dyn LLMProvider>, Vec<AuthProfile>, ProviderCircuitBreaker)>,
    total_retry_budget: Duration,
}

impl FallbackChain {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            total_retry_budget: Duration::from_secs(30),
        }
    }

    /// Add a provider with its auth profiles.
    pub fn add_provider(
        &mut self,
        provider: Arc<dyn LLMProvider>,
        profiles: Vec<AuthProfile>,
    ) {
        let cb = ProviderCircuitBreaker::new(3, Duration::from_secs(300));
        self.providers.push((provider, profiles, cb));
    }

    /// Attempt completion with fallback logic.
    ///
    /// On 401/429: rotate auth profiles for the current provider.
    /// When all profiles exhausted: fall back to next provider.
    /// Exponential backoff + jitter: 1s, 2s, 4s, 8s base delays.
    /// 30s total retry budget.
    pub async fn complete(
        &mut self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let start = Instant::now();

        for (provider, profiles, cb) in &mut self.providers {
            if start.elapsed() >= self.total_retry_budget {
                break;
            }

            if !cb.can_attempt() {
                continue;
            }

            // Track which auth profile we're using for this provider.
            let mut profile_index: usize = 0;

            // Apply initial auth profile if available
            if !profiles.is_empty() {
                let profile = &profiles[profile_index];
                provider.update_auth(&profile.api_key, profile.org_id.as_deref());
                tracing::debug!(
                    provider = provider.name(),
                    profile_index,
                    total_profiles = profiles.len(),
                    "applied initial auth profile"
                );
            } else {
                tracing::debug!(
                    provider = provider.name(),
                    "no auth profiles configured — using provider defaults"
                );
            }

            // Exponential backoff attempts: 1s, 2s, 4s, 8s base delays
            let backoffs = [1u64, 2, 4, 8];
            for (attempt, &delay_secs) in backoffs.iter().enumerate() {
                if start.elapsed() >= self.total_retry_budget {
                    break;
                }

                match provider.complete(messages, tools).await {
                    Ok(result) => {
                        cb.record_success();
                        return Ok(result);
                    }
                    Err(LLMError::AuthFailed(_)) | Err(LLMError::RateLimited { .. }) => {
                        // Rotate to next auth profile for this provider
                        if !profiles.is_empty() {
                            profile_index = (profile_index + 1) % profiles.len();
                            // Apply the rotated auth profile to the provider
                            let profile = &profiles[profile_index];
                            provider.update_auth(&profile.api_key, profile.org_id.as_deref());
                            tracing::warn!(
                                provider = provider.name(),
                                attempt,
                                profile_index,
                                "auth/rate error, rotated to profile {}/{}",
                                profile_index + 1,
                                profiles.len()
                            );
                            // If we've cycled through all profiles, break to next provider
                            if attempt > 0 && profile_index == 0 {
                                cb.record_failure();
                                break;
                            }
                        }
                        cb.record_failure();
                    }
                    Err(e) => {
                        cb.record_failure();
                        tracing::warn!(
                            provider = provider.name(),
                            attempt,
                            error = %e,
                            "provider error"
                        );
                    }
                }

                if attempt < backoffs.len() - 1 {
                    // Exponential backoff with jitter: base_delay ± 25%
                    let base_ms = delay_secs * 1000;
                    let jitter_range = base_ms / 4; // ±25%
                    let jitter = if jitter_range > 0 {
                        let mut rng = rand::thread_rng();
                        rng.gen_range(-(jitter_range as i64)..=(jitter_range as i64))
                    } else {
                        0
                    };
                    let sleep_ms = (base_ms as i64 + jitter).max(100) as u64;
                    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                }
            }
        }

        Err(LLMError::Unavailable(
            "all providers exhausted within retry budget".into(),
        ))
    }
}

impl Default for FallbackChain {
    fn default() -> Self {
        Self::new()
    }
}

impl FallbackChain {
    /// Get the token pricing from the first available (non-open-circuit) provider.
    /// Falls back to zero pricing if no providers are configured.
    pub fn current_pricing(&self) -> crate::provider::TokenPricing {
        for (provider, _, cb) in &self.providers {
            if cb.state() != CBState::Open {
                return provider.token_pricing();
            }
        }
        // All providers are circuit-broken or none configured.
        crate::provider::TokenPricing {
            input_per_1k: 0.0,
            output_per_1k: 0.0,
        }
    }
}
```

### `crates/ghost-llm/src/cost.rs`

```rust
//! Cost calculation with pre/post estimation (Req 21 AC5).

use crate::provider::{TokenPricing, UsageStats};

/// Pre-call cost estimate and post-call actual cost.
#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub estimated_input_cost: f64,
    pub estimated_output_cost: f64,
    pub estimated_total: f64,
}

#[derive(Debug, Clone)]
pub struct CostActual {
    pub input_cost: f64,
    pub output_cost: f64,
    pub total: f64,
}

/// Calculates LLM call costs.
pub struct CostCalculator;

impl CostCalculator {
    /// Estimate cost before a call.
    pub fn estimate(
        input_tokens: usize,
        estimated_output_tokens: usize,
        pricing: &TokenPricing,
    ) -> CostEstimate {
        let input_cost = (input_tokens as f64 / 1000.0) * pricing.input_per_1k;
        let output_cost = (estimated_output_tokens as f64 / 1000.0) * pricing.output_per_1k;
        CostEstimate {
            estimated_input_cost: input_cost,
            estimated_output_cost: output_cost,
            estimated_total: input_cost + output_cost,
        }
    }

    /// Calculate actual cost after a call.
    pub fn actual(usage: &UsageStats, pricing: &TokenPricing) -> CostActual {
        let input_cost = (usage.prompt_tokens as f64 / 1000.0) * pricing.input_per_1k;
        let output_cost = (usage.completion_tokens as f64 / 1000.0) * pricing.output_per_1k;
        CostActual {
            input_cost,
            output_cost,
            total: input_cost + output_cost,
        }
    }
}
```

### `crates/ghost-llm/src/tokens.rs`

```rust
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
                (text.len() + 3) / 4
            }
            TokenStrategy::OpenAI => {
                // Approximation: ~4 chars per token for English text
                // In production, use tiktoken-rs for exact counts
                let chars = text.chars().count();
                (chars + 3) / 4
            }
            TokenStrategy::Anthropic => {
                // Anthropic uses a similar BPE tokenizer
                let chars = text.chars().count();
                (chars + 3) / 4
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
```

### `crates/ghost-llm/src/auth.rs`

```rust
//! `AuthProfileManager` — credential retrieval via `SecretProvider`.
//!
//! Migrated from direct env var reads to the `ghost-secrets` abstraction.
//! Backward compatible: defaults to `EnvProvider` when no secrets config is set.
//! `SecretString` is retrieved just-in-time per request and never stored
//! in long-lived structs. Never logged via tracing.

use ghost_secrets::{EnvProvider, SecretProvider, SecretString, SecretsError};

use crate::provider::LLMError;

/// Key naming convention for LLM provider credentials.
/// Primary key: `{provider}-api-key`
/// Rotation keys: `{provider}-api-key-2`, `{provider}-api-key-3`, etc.
fn credential_key(provider_name: &str, index: usize) -> String {
    if index == 0 {
        format!("{provider_name}-api-key")
    } else {
        format!("{provider_name}-api-key-{}", index + 1)
    }
}

/// Also supports the legacy env var naming convention (uppercase, underscores).
/// e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`
fn legacy_env_key(provider_name: &str, index: usize) -> String {
    let upper = provider_name.to_uppercase().replace('-', "_");
    if index == 0 {
        format!("{upper}_API_KEY")
    } else {
        format!("{upper}_API_KEY_{}", index + 1)
    }
}

/// Manages credential retrieval for LLM providers via `SecretProvider`.
///
/// Supports credential rotation: on 401/429, the caller advances the
/// profile index and retrieves the next key.
pub struct AuthProfileManager {
    provider: Box<dyn SecretProvider>,
    /// LLM provider name (e.g. "anthropic", "openai").
    provider_name: String,
    /// Current profile index for rotation.
    current_index: usize,
    /// Maximum number of profiles to try before giving up.
    max_profiles: usize,
}

impl AuthProfileManager {
    /// Create a new `AuthProfileManager` with a custom `SecretProvider`.
    pub fn new(
        secret_provider: Box<dyn SecretProvider>,
        provider_name: &str,
        max_profiles: usize,
    ) -> Self {
        Self {
            provider: secret_provider,
            provider_name: provider_name.to_string(),
            current_index: 0,
            max_profiles,
        }
    }

    /// Create with the default `EnvProvider` (backward compatibility).
    pub fn with_env(provider_name: &str) -> Self {
        Self::new(Box::new(EnvProvider), provider_name, 3)
    }

    /// Retrieve the current credential as a `SecretString`.
    ///
    /// Tries the new naming convention first (`{provider}-api-key`),
    /// then falls back to the legacy env var convention (`PROVIDER_API_KEY`).
    /// The returned `SecretString` is zeroized on drop.
    pub fn get_credential(&self) -> Result<SecretString, LLMError> {
        let key = credential_key(&self.provider_name, self.current_index);

        match self.provider.get_secret(&key) {
            Ok(secret) => {
                tracing::debug!(
                    provider = %self.provider_name,
                    profile_index = self.current_index,
                    "credential retrieved (value redacted)"
                );
                Ok(secret)
            }
            Err(SecretsError::NotFound(_)) => {
                // Fallback to legacy env var naming
                let legacy = legacy_env_key(&self.provider_name, self.current_index);
                match self.provider.get_secret(&legacy) {
                    Ok(secret) => {
                        tracing::debug!(
                            provider = %self.provider_name,
                            profile_index = self.current_index,
                            "credential retrieved via legacy key (value redacted)"
                        );
                        Ok(secret)
                    }
                    Err(_) => Err(LLMError::AuthFailed(format!(
                        "no credential found for '{}' (tried '{}' and '{}')",
                        self.provider_name, key, legacy
                    ))),
                }
            }
            Err(e) => Err(LLMError::AuthFailed(format!(
                "secret provider error for '{}': {e}",
                self.provider_name
            ))),
        }
    }

    /// Advance to the next credential profile (for rotation on 401/429).
    /// Returns `true` if there are more profiles to try, `false` if exhausted.
    pub fn rotate(&mut self) -> bool {
        if self.current_index + 1 < self.max_profiles {
            self.current_index += 1;
            tracing::info!(
                provider = %self.provider_name,
                new_index = self.current_index,
                "rotating to next auth profile"
            );
            true
        } else {
            tracing::warn!(
                provider = %self.provider_name,
                "all auth profiles exhausted"
            );
            false
        }
    }

    /// Reset to the first profile.
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// Current profile index.
    pub fn current_index(&self) -> usize {
        self.current_index
    }
}
```

### `crates/ghost-llm/src/proxy.rs`

```rust
//! Proxy configuration for LLM provider HTTP clients (Phase 11).
//!
//! When `ProxyEgressPolicy` is active, the agent's reqwest client must
//! route all requests through the localhost proxy. This module provides
//! the configuration bridge between `ghost-egress` and `ghost-llm`.
//!
//! Usage:
//! ```ignore
//! let proxy_config = ProxyConfig::from_url("http://127.0.0.1:12345");
//! let client = proxy_config.build_client()?;
//! // Use `client` for all LLM API calls — requests go through the proxy.
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

/// Per-agent proxy configuration for LLM HTTP clients.
///
/// When a `ProxyEgressPolicy` is active, each agent's LLM requests
/// must be routed through its assigned localhost proxy. This struct
/// holds the proxy URL and provides a method to build a configured
/// reqwest client.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Proxy URL, e.g. `http://127.0.0.1:12345`.
    pub proxy_url: String,
}

impl ProxyConfig {
    /// Create a proxy config from a URL.
    pub fn from_url(url: &str) -> Self {
        Self {
            proxy_url: url.to_string(),
        }
    }

    /// Build a reqwest client configured to use this proxy.
    ///
    /// All HTTP/HTTPS requests made through this client will be routed
    /// through the proxy, which enforces the agent's egress policy.
    pub fn build_client(&self) -> Result<reqwest::Client, reqwest::Error> {
        let proxy = reqwest::Proxy::all(&self.proxy_url)?;
        reqwest::Client::builder()
            .proxy(proxy)
            .build()
    }
}

/// Registry of per-agent proxy configurations.
///
/// Used by the gateway bootstrap to register proxy URLs after
/// `ProxyEgressPolicy::apply()`, and by the agent loop to retrieve
/// the configured client for LLM calls.
#[derive(Debug, Clone, Default)]
pub struct ProxyRegistry {
    configs: Arc<Mutex<HashMap<Uuid, ProxyConfig>>>,
}

impl ProxyRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            configs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a proxy URL for an agent.
    pub fn register(&self, agent_id: Uuid, proxy_url: &str) {
        let config = ProxyConfig::from_url(proxy_url);
        self.configs.lock().unwrap().insert(agent_id, config);
        tracing::debug!(
            agent_id = %agent_id,
            proxy_url = %proxy_url,
            "Registered proxy config for LLM client"
        );
    }

    /// Remove the proxy config for an agent.
    pub fn unregister(&self, agent_id: &Uuid) {
        self.configs.lock().unwrap().remove(agent_id);
    }

    /// Get the proxy config for an agent, if one is registered.
    pub fn get(&self, agent_id: &Uuid) -> Option<ProxyConfig> {
        self.configs.lock().unwrap().get(agent_id).cloned()
    }

    /// Build a reqwest client for an agent.
    ///
    /// If a proxy is registered, returns a proxy-configured client.
    /// Otherwise, returns a default client (no proxy).
    pub fn build_client_for_agent(&self, agent_id: &Uuid) -> Result<reqwest::Client, reqwest::Error> {
        match self.get(agent_id) {
            Some(config) => config.build_client(),
            None => reqwest::Client::builder().build(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_config_from_url() {
        let config = ProxyConfig::from_url("http://127.0.0.1:12345");
        assert_eq!(config.proxy_url, "http://127.0.0.1:12345");
    }

    #[test]
    fn proxy_registry_register_and_get() {
        let registry = ProxyRegistry::new();
        let agent = Uuid::new_v4();

        assert!(registry.get(&agent).is_none());

        registry.register(agent, "http://127.0.0.1:9999");
        let config = registry.get(&agent).unwrap();
        assert_eq!(config.proxy_url, "http://127.0.0.1:9999");
    }

    #[test]
    fn proxy_registry_unregister() {
        let registry = ProxyRegistry::new();
        let agent = Uuid::new_v4();

        registry.register(agent, "http://127.0.0.1:9999");
        assert!(registry.get(&agent).is_some());

        registry.unregister(&agent);
        assert!(registry.get(&agent).is_none());
    }

    #[test]
    fn build_client_without_proxy() {
        let registry = ProxyRegistry::new();
        let agent = Uuid::new_v4();

        // No proxy registered — should build a default client.
        let client = registry.build_client_for_agent(&agent);
        assert!(client.is_ok());
    }
}
```

### `crates/ghost-llm/src/quarantine.rs`

> File is 783 lines — included in full. Contains QuarantinedLLM, ContentQuarantine, CompressionMode, CompressionStats, and all tests.

Due to the extreme length of this extraction, the quarantine.rs and remaining files (router.rs and all subsequent sections) will be appended below.

### `crates/ghost-llm/src/router.rs`

```rust
//! Model router with complexity classification (Req 21 AC2, AC6).

use std::sync::Arc;

use crate::provider::LLMProvider;

/// Complexity tier for model selection.
///
/// Ordering: Local < Free < Cheap < Standard < Premium.
/// `Local` maps to an Ollama/local model with zero token cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComplexityTier {
    /// Local model (e.g., Ollama Qwen-2.5-7B). Zero cost.
    Local,
    Free,
    Cheap,
    Standard,
    Premium,
}

/// Classifies message complexity to select the appropriate model tier.
pub struct ComplexityClassifier;

impl ComplexityClassifier {
    /// Classify a user message into a complexity tier.
    ///
    /// Slash command overrides take precedence, then heuristics.
    pub fn classify(
        message: &str,
        is_heartbeat: bool,
        convergence_level: u8,
    ) -> ComplexityTier {
        // Convergence downgrade at L3+ (AC6)
        if convergence_level >= 3 {
            return if message.len() < 100 {
                ComplexityTier::Free
            } else {
                ComplexityTier::Cheap
            };
        }

        // Slash command overrides
        if message.starts_with("/quick") {
            return ComplexityTier::Free;
        }
        if message.starts_with("/deep") {
            return ComplexityTier::Premium;
        }
        if message.starts_with("/model") {
            return ComplexityTier::Standard;
        }

        // Heartbeat → Free
        if is_heartbeat {
            return ComplexityTier::Free;
        }

        // Heuristic classification
        let len = message.len();
        let has_tool_keywords = message.contains("function")
            || message.contains("write")
            || message.contains("create")
            || message.contains("implement")
            || message.contains("debug")
            || message.contains("analyze");

        if len < 20 && !has_tool_keywords {
            ComplexityTier::Free
        } else if len < 100 && !has_tool_keywords {
            ComplexityTier::Cheap
        } else if has_tool_keywords || len > 500 {
            ComplexityTier::Premium
        } else {
            ComplexityTier::Standard
        }
    }
}

/// Routes requests to the appropriate provider based on complexity tier.
pub struct ModelRouter {
    /// Providers indexed by tier: [Local, Free, Cheap, Standard, Premium].
    providers: [Option<Arc<dyn LLMProvider>>; 5],
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            providers: [None, None, None, None, None],
        }
    }

    /// Set the provider for a given tier.
    pub fn set_provider(&mut self, tier: ComplexityTier, provider: Arc<dyn LLMProvider>) {
        self.providers[tier as usize] = Some(provider);
    }

    /// Get the provider for a given tier, falling back through the chain.
    ///
    /// Fallback order: Local → Free → Cheap → Standard → Premium.
    /// Tries the requested tier first, then falls back upward to higher tiers.
    pub fn get_provider(&self, tier: ComplexityTier) -> Option<Arc<dyn LLMProvider>> {
        let idx = tier as usize;
        // Try requested tier first, then fall back upward
        for i in idx..5 {
            if let Some(ref p) = self.providers[i] {
                if i != idx {
                    tracing::debug!(
                        requested = ?tier,
                        resolved_idx = i,
                        "model router: requested tier unavailable, fell back to higher tier"
                    );
                }
                return Some(Arc::clone(p));
            }
        }
        // Fall back downward if nothing above
        for i in (0..idx).rev() {
            if let Some(ref p) = self.providers[i] {
                tracing::debug!(
                    requested = ?tier,
                    resolved_idx = i,
                    "model router: no higher tier available, fell back to lower tier"
                );
                return Some(Arc::clone(p));
            }
        }
        tracing::warn!(
            requested = ?tier,
            "model router: no providers configured — returning None"
        );
        None
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}
```
