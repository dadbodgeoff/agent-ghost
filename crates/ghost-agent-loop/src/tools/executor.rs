//! ToolExecutor — dispatch, timeout enforcement, audit logging (Req 11 AC5, AC12).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ghost_llm::provider::LLMToolCall;
use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::engine::{PolicyDecision, PolicyEngine};
use thiserror::Error;

use super::builtin::filesystem::FilesystemTool;
use super::builtin::http_request::{http_request, HttpRequestConfig};
use super::builtin::memory::{read_memories, MemoryReadResult};
use super::builtin::shell::{execute_shell, ShellToolConfig};
use super::builtin::web_fetch::{fetch_url, FetchConfig};
use super::builtin::web_search::{search, WebSearchConfig};
use super::plan_validator::{PlanValidationResult, PlanValidator, ToolCallPlan};
use super::registry::{RegisteredTool, ToolRegistry};
use super::skill_bridge::{ExecutionContext, SkillBridge};

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
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
}

impl From<ghost_skills::skill::SkillError> for ToolError {
    fn from(e: ghost_skills::skill::SkillError) -> Self {
        use ghost_skills::skill::SkillError;
        match &e {
            SkillError::InvalidInput(_) => ToolError::InvalidArguments(e.to_string()),
            SkillError::ConvergenceTooHigh { .. }
            | SkillError::BudgetExhausted { .. }
            | SkillError::AppNotAllowed { .. }
            | SkillError::UserDenied
            | SkillError::AuthorizationDenied(_)
            | SkillError::SandboxViolation(_)
            | SkillError::PcControlBlocked(_)
            | SkillError::CircuitBreakerOpen(_) => ToolError::PolicyDenied(e.to_string()),
            _ => ToolError::ExecutionFailed(e.to_string()),
        }
    }
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
    _default_timeout: Duration,
    plan_validator: PlanValidator,
    policy_engine: Option<Arc<Mutex<PolicyEngine>>>,
    filesystem: Option<FilesystemTool>,
    shell_config: ShellToolConfig,
    web_search_config: WebSearchConfig,
    fetch_config: FetchConfig,
    http_request_config: HttpRequestConfig,
    /// Snapshot memories set per-run for the memory tool.
    snapshot_memories: Vec<serde_json::Value>,
    /// Optional skill bridge for dispatching skill tool calls.
    skill_bridge: Option<SkillBridge>,
}

impl ToolExecutor {
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            _default_timeout: default_timeout,
            plan_validator: PlanValidator::default(),
            policy_engine: None,
            filesystem: None,
            shell_config: ShellToolConfig::default(),
            web_search_config: WebSearchConfig::default(),
            fetch_config: FetchConfig::default(),
            http_request_config: HttpRequestConfig::default(),
            snapshot_memories: Vec::new(),
            skill_bridge: None,
        }
    }

    /// Create a ToolExecutor with a custom PlanValidator.
    pub fn with_plan_validator(default_timeout: Duration, plan_validator: PlanValidator) -> Self {
        Self {
            _default_timeout: default_timeout,
            plan_validator,
            policy_engine: None,
            filesystem: None,
            shell_config: ShellToolConfig::default(),
            web_search_config: WebSearchConfig::default(),
            fetch_config: FetchConfig::default(),
            http_request_config: HttpRequestConfig::default(),
            snapshot_memories: Vec::new(),
            skill_bridge: None,
        }
    }

    /// Configure the filesystem tool with a workspace root.
    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.filesystem = Some(FilesystemTool::new(root));
    }

    /// Configure the runtime policy engine. Live execution must provide this.
    pub fn set_policy_engine(&mut self, engine: PolicyEngine) {
        self.policy_engine = Some(Arc::new(Mutex::new(engine)));
    }

    /// Configure the shell tool.
    pub fn set_shell_config(&mut self, config: ShellToolConfig) {
        self.shell_config = config;
    }

    /// Configure the web search tool.
    pub fn set_web_search_config(&mut self, config: WebSearchConfig) {
        self.web_search_config = config;
    }

    /// Configure the URL fetch tool.
    pub fn set_fetch_config(&mut self, config: FetchConfig) {
        self.fetch_config = config;
    }

    /// Configure the HTTP request tool.
    pub fn set_http_request_config(&mut self, config: HttpRequestConfig) {
        self.http_request_config = config;
    }

    /// Set snapshot memories for the memory tool (refreshed per-run).
    pub fn set_snapshot_memories(&mut self, memories: Vec<serde_json::Value>) {
        self.snapshot_memories = memories;
    }

    /// Set the skill bridge for dispatching skill tool calls.
    pub fn set_skill_bridge(&mut self, bridge: SkillBridge) {
        self.skill_bridge = Some(bridge);
    }

    /// Validate a plan of tool calls before executing any of them.
    pub fn validate_plan(&self, calls: &[LLMToolCall]) -> PlanValidationResult {
        let plan = ToolCallPlan::new(calls.to_vec());
        self.plan_validator.validate(&plan)
    }

    /// Record a tool denial for escalation tracking.
    pub fn record_denial(&mut self, tool_name: &str) {
        self.plan_validator.record_denial(tool_name);
    }

    /// Execute a tool call.
    pub async fn execute(
        &self,
        call: &LLMToolCall,
        registry: &ToolRegistry,
        exec_ctx: &ExecutionContext,
    ) -> Result<ToolResult, ToolError> {
        let tool = registry
            .lookup(&call.name)
            .ok_or_else(|| ToolError::NotFound(call.name.clone()))?;

        self.evaluate_policy(call, tool, exec_ctx)?;

        let timeout = Duration::from_secs(tool.timeout_secs);
        let start = std::time::Instant::now();

        // Execute with timeout
        let result = tokio::time::timeout(timeout, self.dispatch(call, tool, exec_ctx)).await;

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
        _tool: &RegisteredTool,
        exec_ctx: &ExecutionContext,
    ) -> Result<String, ToolError> {
        match call.name.as_str() {
            "read_file" => {
                let fs = self.filesystem.as_ref().ok_or_else(|| {
                    ToolError::ExecutionFailed("filesystem not configured".into())
                })?;
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("missing 'path' argument".into()))?;
                match fs.read_file(path) {
                    Ok(content) => Ok(serde_json::json!({"content": content}).to_string()),
                    Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                }
            }
            "write_file" => {
                let fs = self.filesystem.as_ref().ok_or_else(|| {
                    ToolError::ExecutionFailed("filesystem not configured".into())
                })?;
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("missing 'path' argument".into()))?;
                let content = call
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidArguments("missing 'content' argument".into())
                    })?;
                match fs.write_file(path, content) {
                    Ok(()) => {
                        Ok(serde_json::json!({"status": "written", "path": path}).to_string())
                    }
                    Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                }
            }
            "list_dir" => {
                let fs = self.filesystem.as_ref().ok_or_else(|| {
                    ToolError::ExecutionFailed("filesystem not configured".into())
                })?;
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                match fs.list_dir(path) {
                    Ok(entries) => Ok(serde_json::json!({"entries": entries}).to_string()),
                    Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                }
            }
            "shell" => {
                let command = call
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidArguments("missing 'command' argument".into())
                    })?;
                match execute_shell(command, &self.shell_config).await {
                    Ok((stdout, stderr)) => {
                        Ok(serde_json::json!({"stdout": stdout, "stderr": stderr}).to_string())
                    }
                    Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                }
            }
            "memory_read" => {
                let query = call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidArguments("missing 'query' argument".into())
                    })?;
                let limit = call
                    .arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                let result: MemoryReadResult = read_memories(query, limit, &self.snapshot_memories);
                Ok(serde_json::json!({
                    "memories": result.memories,
                    "total_count": result.total_count,
                })
                .to_string())
            }
            "web_search" => {
                let query = call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidArguments("missing 'query' argument".into())
                    })?;
                // Allow per-call max_results override, capped at 10.
                let max_results = call
                    .arguments
                    .get("max_results")
                    .and_then(|v| v.as_u64())
                    .map(|n| n.min(10) as usize);
                let config = if let Some(n) = max_results {
                    let mut c = self.web_search_config.clone();
                    c.max_results = n;
                    c
                } else {
                    self.web_search_config.clone()
                };
                match search(query, &config).await {
                    Ok(results) => {
                        let items: Vec<serde_json::Value> = results
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "title": r.title,
                                    "url": r.url,
                                    "snippet": r.snippet,
                                })
                            })
                            .collect();
                        Ok(serde_json::json!({"results": items}).to_string())
                    }
                    Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                }
            }
            "web_fetch" => {
                let url = call
                    .arguments
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("missing 'url' argument".into()))?;
                match fetch_url(url, &self.fetch_config).await {
                    Ok(result) => Ok(serde_json::json!({
                        "url": result.url,
                        "status": result.status,
                        "content": result.content,
                        "content_type": result.content_type,
                        "truncated": result.truncated,
                        "content_length": result.content_length,
                    })
                    .to_string()),
                    Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                }
            }
            "http_request" => {
                let url = call
                    .arguments
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("missing 'url' argument".into()))?;
                let method = call
                    .arguments
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("GET");
                let headers: std::collections::HashMap<String, String> = call
                    .arguments
                    .get("headers")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();
                let body = call.arguments.get("body").and_then(|v| v.as_str());
                match http_request(url, method, &headers, body, &self.http_request_config).await {
                    Ok(result) => Ok(serde_json::json!({
                        "url": result.url,
                        "method": result.method,
                        "status": result.status,
                        "headers": result.headers,
                        "body": result.body,
                        "content_type": result.content_type,
                        "truncated": result.truncated,
                        "body_length": result.body_length,
                    })
                    .to_string()),
                    Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                }
            }
            name if name.starts_with("skill_") => {
                let skill_name = &name[6..]; // strip "skill_" prefix
                let bridge = self.skill_bridge.as_ref().ok_or_else(|| {
                    ToolError::ExecutionFailed("skill bridge not configured".into())
                })?;
                let result = bridge
                    .execute(skill_name, &call.arguments, exec_ctx)
                    .map_err(ToolError::from)?;
                Ok(result.to_string())
            }
            _ => Err(ToolError::ExecutionFailed(format!(
                "tool '{}' has no runtime dispatch implementation",
                call.name
            ))),
        }
    }

    fn evaluate_policy(
        &self,
        call: &LLMToolCall,
        tool: &RegisteredTool,
        exec_ctx: &ExecutionContext,
    ) -> Result<(), ToolError> {
        let policy_engine = self.policy_engine.as_ref().ok_or_else(|| {
            ToolError::PolicyDenied(
                serde_json::json!({
                    "type": "policy_missing",
                    "message": "runtime policy evaluator not configured",
                })
                .to_string(),
            )
        })?;

        let policy_call = self.build_policy_call(call, tool, exec_ctx)?;
        let mut engine = policy_engine
            .lock()
            .map_err(|_| ToolError::ExecutionFailed("policy engine lock poisoned".into()))?;
        let session_denial_count = engine.session_denial_count(exec_ctx.session_id);
        let policy_ctx = PolicyContext {
            agent_id: exec_ctx.agent_id,
            session_id: exec_ctx.session_id,
            intervention_level: exec_ctx.intervention_level,
            session_duration: exec_ctx.session_duration,
            session_denial_count,
            is_compaction_flush: exec_ctx.is_compaction_flush,
            session_reflection_count: exec_ctx.session_reflection_count,
        };

        match engine.evaluate(&policy_call, &policy_ctx) {
            PolicyDecision::Permit => Ok(()),
            PolicyDecision::Deny(feedback) => {
                tracing::warn!(
                    tool = %call.name,
                    constraint = %feedback.constraint,
                    reason = %feedback.reason,
                    "tool denied by runtime policy"
                );
                Err(ToolError::PolicyDenied(
                    serde_json::json!({
                        "type": "policy_denied",
                        "tool": call.name,
                        "reason": feedback.reason,
                        "constraint": feedback.constraint,
                        "suggested_alternatives": feedback.suggested_alternatives,
                    })
                    .to_string(),
                ))
            }
            PolicyDecision::Escalate(message) => Err(ToolError::PolicyDenied(
                serde_json::json!({
                    "type": "policy_escalation",
                    "tool": call.name,
                    "message": message,
                })
                .to_string(),
            )),
        }
    }

    fn build_policy_call(
        &self,
        call: &LLMToolCall,
        tool: &RegisteredTool,
        exec_ctx: &ExecutionContext,
    ) -> Result<ToolCall, ToolError> {
        match call.name.as_str() {
            "read_file"
            | "write_file"
            | "list_dir"
            | "shell"
            | "memory_read"
            | "web_search"
            | "web_fetch"
            | "http_request"
            | "send_proactive_message"
            | "schedule_message"
            | "heartbeat"
            | "reflection_write" => {}
            name if name.starts_with("skill_") => {}
            _ => {
                return Err(ToolError::PolicyDenied(
                    serde_json::json!({
                        "type": "policy_mapping_missing",
                        "tool": call.name,
                        "message": "tool has no runtime policy mapping",
                    })
                    .to_string(),
                ));
            }
        }

        if tool.capability.trim().is_empty() {
            return Err(ToolError::PolicyDenied(
                serde_json::json!({
                    "type": "policy_mapping_missing",
                    "tool": call.name,
                    "message": "tool is missing a required capability mapping",
                })
                .to_string(),
            ));
        }

        Ok(ToolCall {
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
            capability: tool.capability.clone(),
            is_compaction_flush: exec_ctx.is_compaction_flush,
        })
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

/// Register all builtin tools in the given registry with proper schemas.
pub fn register_builtin_tools(registry: &mut ToolRegistry) {
    use super::registry::RegisteredTool;
    use ghost_llm::provider::ToolSchema;

    registry.register(RegisteredTool {
        name: "read_file".into(),
        description: "Read the contents of a file".into(),
        schema: ToolSchema {
            name: "read_file".into(),
            description: "Read the contents of a file at the given path".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative file path to read"}
                },
                "required": ["path"]
            }),
        },
        capability: "file_read".into(),
        hidden_at_level: 5,
        timeout_secs: 10,
    });

    registry.register(RegisteredTool {
        name: "write_file".into(),
        description: "Write content to a file".into(),
        schema: ToolSchema {
            name: "write_file".into(),
            description:
                "Write content to a file at the given path, creating directories as needed".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative file path to write"},
                    "content": {"type": "string", "description": "Content to write to the file"}
                },
                "required": ["path", "content"]
            }),
        },
        capability: "filesystem_write".into(),
        hidden_at_level: 4,
        timeout_secs: 10,
    });

    registry.register(RegisteredTool {
        name: "list_dir".into(),
        description: "List directory contents".into(),
        schema: ToolSchema {
            name: "list_dir".into(),
            description: "List the contents of a directory".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative directory path to list"}
                },
                "required": ["path"]
            }),
        },
        capability: "filesystem_read".into(),
        hidden_at_level: 5,
        timeout_secs: 10,
    });

    registry.register(RegisteredTool {
        name: "shell".into(),
        description: "Execute a shell command".into(),
        schema: ToolSchema {
            name: "shell".into(),
            description: "Execute a shell command within the agent's sandbox".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"}
                },
                "required": ["command"]
            }),
        },
        capability: "shell_execute".into(),
        hidden_at_level: 4,
        timeout_secs: 30,
    });

    registry.register(RegisteredTool {
        name: "memory_read".into(),
        description: "Search agent memories".into(),
        schema: ToolSchema {
            name: "memory_read".into(),
            description: "Search the agent's memory for relevant information".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "limit": {"type": "integer", "description": "Maximum results to return", "default": 10}
                },
                "required": ["query"]
            }),
        },
        capability: "memory_read".into(),
        hidden_at_level: 5,
        timeout_secs: 10,
    });

    registry.register(RegisteredTool {
        name: "web_search".into(),
        description: "Search the web".into(),
        schema: ToolSchema {
            name: "web_search".into(),
            description: "Search the web for information. Returns titles, URLs, and snippets. Use web_fetch to read full page content from the returned URLs.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "max_results": {"type": "integer", "description": "Maximum number of results (default 5, max 10)", "default": 5}
                },
                "required": ["query"]
            }),
        },
        capability: "web_search".into(),
        hidden_at_level: 3,
        timeout_secs: 15,
    });

    registry.register(RegisteredTool {
        name: "web_fetch".into(),
        description: "Fetch and extract text content from a URL".into(),
        schema: ToolSchema {
            name: "web_fetch".into(),
            description: "Fetch a web page and extract its text content. Use after web_search to read full page content. Only HTTPS URLs are allowed. Returns cleaned text with HTML stripped.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The HTTPS URL to fetch (e.g. https://example.com/page)"
                    }
                },
                "required": ["url"]
            }),
        },
        capability: "web_fetch".into(),
        hidden_at_level: 3,
        timeout_secs: 20,
    });

    registry.register(RegisteredTool {
        name: "http_request".into(),
        description: "Make an HTTP request to an API endpoint".into(),
        schema: ToolSchema {
            name: "http_request".into(),
            description: "Make an HTTP request to a REST API. Supports GET, POST, PUT, PATCH, DELETE with custom headers and JSON body. Only HTTPS by default. Use for API interactions, webhooks, and service calls.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The HTTPS URL to request (e.g. https://api.github.com/repos/owner/repo)"
                    },
                    "method": {
                        "type": "string",
                        "description": "HTTP method: GET, POST, PUT, PATCH, or DELETE (default: GET)",
                        "default": "GET"
                    },
                    "headers": {
                        "type": "object",
                        "description": "Custom request headers as key-value pairs (e.g. {\"Authorization\": \"Bearer token\"})",
                        "additionalProperties": {"type": "string"}
                    },
                    "body": {
                        "type": "string",
                        "description": "Request body (typically JSON string for POST/PUT/PATCH)"
                    }
                },
                "required": ["url"]
            }),
        },
        capability: "http_request".into(),
        hidden_at_level: 3,
        timeout_secs: 30,
    });
}
