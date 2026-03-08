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
    Mixed {
        text: String,
        tool_calls: Vec<LLMToolCall>,
    },
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

    /// WP9-F: Lightweight health check to verify API key and connectivity.
    /// Called at startup to detect bad keys before first user message.
    /// Default implementation returns Ok (for providers without health endpoints).
    async fn health_check(&self) -> Result<(), LLMError> {
        Ok(())
    }
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
fn map_http_error(
    status: reqwest::StatusCode,
    body: &str,
    headers: &reqwest::header::HeaderMap,
) -> LLMError {
    match status.as_u16() {
        401 | 403 => LLMError::AuthFailed(format!("{status}: {body}")),
        429 => {
            let retry_after = headers
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30);
            LLMError::RateLimited {
                retry_after_secs: retry_after,
            }
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
    messages
        .iter()
        .map(|m| {
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
                let calls: Vec<serde_json::Value> = tc
                    .iter()
                    .map(|c| {
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
                    })
                    .collect();
                msg["tool_calls"] = serde_json::Value::Array(calls);
            }
            if let Some(ref id) = m.tool_call_id {
                msg["tool_call_id"] = serde_json::Value::String(id.clone());
            }
            msg
        })
        .collect()
}

/// Build OpenAI-format tools array from ToolSchema slice.
fn openai_format_tools(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}

/// Parse an OpenAI-format response body into CompletionResult.
fn parse_openai_response(
    body: &serde_json::Value,
    model_fallback: &str,
) -> Result<CompletionResult, LLMError> {
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
            arr.iter()
                .filter_map(|tc| {
                    let id = tc["id"].as_str()?.to_string();
                    let name = tc["function"]["name"].as_str()?.to_string();
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let arguments = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                    Some(LLMToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let has_tool_calls = !tool_calls.is_empty();
    let has_text = content.as_ref().is_some_and(|t| !t.is_empty());

    let response = match (has_text, has_tool_calls) {
        (true, false) => LLMResponse::Text(content.unwrap()),
        (false, true) => LLMResponse::ToolCalls(tool_calls),
        (true, true) => LLMResponse::Mixed {
            text: content.unwrap(),
            tool_calls,
        },
        (false, false) => LLMResponse::Empty,
    };

    Ok(CompletionResult {
        response,
        usage,
        model,
    })
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
    let resp_body: serde_json::Value = resp
        .json()
        .await
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
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect()
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
                    tool_calls.push(LLMToolCall {
                        id,
                        name,
                        arguments,
                    });
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
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self
            .api_key
            .read()
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
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LLMError::InvalidResponse(format!("JSON parse error: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or(&resp_body.to_string())
                .to_string();
            return Err(map_http_error(status, &error_msg, &headers));
        }

        let (response, usage) = Self::parse_response(&resp_body)?;
        let model = resp_body["model"]
            .as_str()
            .unwrap_or(&self.model)
            .to_string();

        Ok(CompletionResult {
            response,
            usage,
            model,
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }
    fn context_window(&self) -> usize {
        200_000
    }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing {
            input_per_1k: 0.003,
            output_per_1k: 0.015,
        }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => {
                tracing::error!(provider = "anthropic", error = %e, "Failed to update API key — RwLock poisoned")
            }
        }
    }

    async fn health_check(&self) -> Result<(), LLMError> {
        let api_key = self
            .api_key
            .read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();
        let client = build_client(Duration::from_secs(10))?;
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "hi"}],
        });
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Other(format!("health check request failed: {e}")))?;
        if resp.status().is_success() || resp.status().as_u16() == 200 {
            Ok(())
        } else {
            Err(LLMError::Other(format!(
                "Anthropic health check failed: HTTP {}",
                resp.status()
            )))
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
    fn name(&self) -> &str {
        "openai"
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self
            .api_key
            .read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();

        openai_format_complete(
            "https://api.openai.com/v1/chat/completions",
            &self.model,
            Some(&api_key),
            messages,
            tools,
            DEFAULT_TIMEOUT,
        )
        .await
    }

    fn supports_streaming(&self) -> bool {
        true
    }
    fn context_window(&self) -> usize {
        128_000
    }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing {
            input_per_1k: 0.005,
            output_per_1k: 0.015,
        }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => {
                tracing::error!(provider = "openai", error = %e, "Failed to update API key — RwLock poisoned")
            }
        }
    }

    async fn health_check(&self) -> Result<(), LLMError> {
        let api_key = self
            .api_key
            .read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();
        let client = build_client(Duration::from_secs(10))?;
        let resp = client
            .get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|e| LLMError::Other(format!("health check request failed: {e}")))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(LLMError::Other(format!(
                "OpenAI health check failed: HTTP {}",
                resp.status()
            )))
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
    fn build_contents(
        messages: &[ChatMessage],
    ) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
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
        let declarations: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
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
                tool_calls.push(LLMToolCall {
                    id,
                    name,
                    arguments,
                });
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
    fn name(&self) -> &str {
        "gemini"
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self
            .api_key
            .read()
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
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LLMError::InvalidResponse(format!("JSON parse error: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or(&resp_body.to_string())
                .to_string();
            return Err(map_http_error(status, &error_msg, &headers));
        }

        let (response, usage) = Self::parse_response(&resp_body)?;
        Ok(CompletionResult {
            response,
            usage,
            model: self.model.clone(),
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }
    fn context_window(&self) -> usize {
        1_000_000
    }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing {
            input_per_1k: 0.00025,
            output_per_1k: 0.0005,
        }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => {
                tracing::error!(provider = "gemini", error = %e, "Failed to update API key — RwLock poisoned")
            }
        }
    }

    async fn health_check(&self) -> Result<(), LLMError> {
        let api_key = self
            .api_key
            .read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();
        let client = build_client(Duration::from_secs(10))?;
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={api_key}");
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| LLMError::Other(format!("health check request failed: {e}")))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(LLMError::Other(format!(
                "Gemini health check failed: HTTP {}",
                resp.status()
            )))
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
    fn name(&self) -> &str {
        "ollama"
    }

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

        let ollama_messages: Vec<serde_json::Value> =
            messages.iter().map(ollama_format_message).collect();

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
        let resp_body: serde_json::Value = resp
            .json()
            .await
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

    fn supports_streaming(&self) -> bool {
        true
    }
    fn context_window(&self) -> usize {
        262_144
    }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing {
            input_per_1k: 0.0,
            output_per_1k: 0.0,
        } // local
    }

    async fn health_check(&self) -> Result<(), LLMError> {
        let client = build_client(Duration::from_secs(5))?;
        let url = format!("{}/api/tags", self.base_url);
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| LLMError::Other(format!("Ollama health check failed: {e}")))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(LLMError::Other(format!(
                "Ollama health check failed: HTTP {}",
                resp.status()
            )))
        }
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
        let ollama_messages: Vec<serde_json::Value> =
            messages.iter().map(ollama_format_message).collect();

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
        let tc: Vec<serde_json::Value> = calls
            .iter()
            .map(|c| {
                serde_json::json!({
                    "function": {
                        "name": c.name,
                        "arguments": c.arguments,
                    }
                })
            })
            .collect();
        if !tc.is_empty() {
            msg["tool_calls"] = serde_json::Value::Array(tc);
        }
    }

    msg
}

/// Format tools for Ollama's native /api/chat endpoint.
fn ollama_format_tools(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}

/// Parse Ollama native /api/chat response into CompletionResult.
fn parse_ollama_response(
    body: &serde_json::Value,
    model_fallback: &str,
) -> Result<CompletionResult, LLMError> {
    let model = body["model"].as_str().unwrap_or(model_fallback).to_string();

    let message = &body["message"];
    let content = message["content"].as_str().map(|s| s.to_string());

    // Ollama native format: tool_calls are in message.tool_calls
    let tool_calls_raw = message.get("tool_calls").and_then(|v| v.as_array());
    let tool_calls: Vec<LLMToolCall> = tool_calls_raw
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let func = &tc["function"];
                    let name = func["name"].as_str()?.to_string();
                    let arguments = func
                        .get("arguments")
                        .cloned()
                        .unwrap_or(serde_json::json!({}));
                    // Ollama native format doesn't include a call ID; generate one.
                    let id = format!(
                        "ollama_{}",
                        uuid::Uuid::new_v4()
                            .to_string()
                            .split('-')
                            .next()
                            .unwrap_or("0")
                    );
                    Some(LLMToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect()
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
    let has_text = content.as_ref().is_some_and(|t| !t.is_empty());

    let response = match (has_text, has_tool_calls) {
        (true, false) => LLMResponse::Text(content.unwrap()),
        (false, true) => LLMResponse::ToolCalls(tool_calls),
        (true, true) => LLMResponse::Mixed {
            text: content.unwrap(),
            tool_calls,
        },
        (false, false) => LLMResponse::Empty,
    };

    Ok(CompletionResult {
        response,
        usage,
        model,
    })
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
    fn name(&self) -> &str {
        "openai_compat"
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let api_key = self
            .api_key
            .read()
            .map_err(|e| LLMError::Other(format!("lock poisoned: {e}")))?
            .clone();

        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        openai_format_complete(
            &url,
            &self.model,
            Some(&api_key),
            messages,
            tools,
            DEFAULT_TIMEOUT,
        )
        .await
    }

    fn supports_streaming(&self) -> bool {
        true
    }
    fn context_window(&self) -> usize {
        self.context_window_size
    }
    fn token_pricing(&self) -> TokenPricing {
        TokenPricing {
            input_per_1k: 0.001,
            output_per_1k: 0.002,
        }
    }

    fn update_auth(&self, api_key: &str, _org_id: Option<&str>) {
        match self.api_key.write() {
            Ok(mut key) => *key = api_key.to_string(),
            Err(e) => {
                tracing::error!(provider = "openai_compat", error = %e, "Failed to update API key — RwLock poisoned")
            }
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

        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let api_key = self.api_key.read().map(|k| k.clone()).unwrap_or_default();

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
