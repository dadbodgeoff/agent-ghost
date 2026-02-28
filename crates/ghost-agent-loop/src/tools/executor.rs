//! ToolExecutor — dispatch, timeout enforcement, audit logging (Req 11 AC5, AC12).

use std::time::Duration;

use ghost_llm::provider::LLMToolCall;
use thiserror::Error;

use super::registry::ToolRegistry;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("tool execution timed out after {0}s")]
    Timeout(u64),
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("tool denied by policy: {0}")]
    PolicyDenied(String),
}

/// Result of a tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub output: String,
    pub success: bool,
    pub duration_ms: u64,
}

/// Executes tool calls with timeout enforcement and audit logging.
pub struct ToolExecutor {
    default_timeout: Duration,
}

impl ToolExecutor {
    pub fn new(default_timeout: Duration) -> Self {
        Self { default_timeout }
    }

    /// Execute a tool call.
    pub async fn execute(
        &self,
        call: &LLMToolCall,
        registry: &ToolRegistry,
    ) -> Result<ToolResult, ToolError> {
        let tool = registry
            .lookup(&call.name)
            .ok_or_else(|| ToolError::NotFound(call.name.clone()))?;

        let timeout = Duration::from_secs(tool.timeout_secs);
        let start = std::time::Instant::now();

        // Execute with timeout
        let result = tokio::time::timeout(timeout, self.dispatch(call, tool)).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(output)) => {
                tracing::info!(
                    tool = %call.name,
                    duration_ms,
                    "tool execution succeeded"
                );
                Ok(ToolResult {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    output,
                    success: true,
                    duration_ms,
                })
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    tool = %call.name,
                    error = %e,
                    duration_ms,
                    "tool execution failed"
                );
                Err(e)
            }
            Err(_) => {
                tracing::error!(
                    tool = %call.name,
                    timeout_secs = tool.timeout_secs,
                    "tool execution timed out"
                );
                Err(ToolError::Timeout(tool.timeout_secs))
            }
        }
    }

    async fn dispatch(
        &self,
        call: &LLMToolCall,
        _tool: &super::registry::RegisteredTool,
    ) -> Result<String, ToolError> {
        // Dispatch to the appropriate builtin or skill handler.
        // In production, this routes to shell, filesystem, web_search,
        // memory, or WASM sandbox based on tool type.
        //
        // Stub: return a placeholder result.
        Ok(format!(
            "{{\"result\": \"executed {}\", \"args\": {}}}",
            call.name, call.arguments
        ))
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}
