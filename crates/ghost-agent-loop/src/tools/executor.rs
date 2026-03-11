//! ToolExecutor — dispatch, timeout enforcement, audit logging (Req 11 AC5, AC12).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use cortex_core::safety::trigger::TriggerEvent;
use ghost_llm::provider::LLMToolCall;
use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::engine::{PolicyDecision, PolicyEngine};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::builtin::filesystem::{FilesystemExecutionBackend, FilesystemTool};
use super::builtin::http_request::{finalize_http_request_result, http_request, HttpRequestConfig};
use super::builtin::memory::{read_memories, MemoryReadResult};
use super::builtin::shell::{execute_shell, ShellToolConfig};
use super::builtin::web_fetch::{fetch_url, finalize_fetch_result, FetchConfig};
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
    #[error("sandbox review unavailable: {0}")]
    SandboxReviewUnavailable(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSandboxMode {
    Off,
    ReadOnly,
    WorkspaceWrite,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSandboxViolationAction {
    Warn,
    Pause,
    Quarantine,
    KillAll,
}

#[derive(Debug, Clone)]
pub struct BuiltinSandboxPolicy {
    pub enabled: bool,
    pub mode: BuiltinSandboxMode,
    pub on_violation: BuiltinSandboxViolationAction,
    pub network_access: bool,
}

#[derive(Debug, Clone)]
pub enum NetworkExecutionBackend {
    InProcess,
    ProcessHelper { helper_executable: String },
    Container { image: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxReviewDecision {
    Approved,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxReviewRequest {
    pub review_id: String,
    pub agent_id: Uuid,
    pub session_id: Uuid,
    pub execution_id: Option<String>,
    pub route_kind: Option<String>,
    pub tool_name: String,
    pub violation_reason: String,
    pub sandbox_mode: String,
    pub timeout_secs: u64,
}

#[derive(Debug)]
pub struct SandboxReviewRequestEnvelope {
    pub request: SandboxReviewRequest,
    pub decision_tx: tokio::sync::oneshot::Sender<SandboxReviewDecision>,
}

impl BuiltinSandboxPolicy {
    pub fn is_active(&self) -> bool {
        self.enabled && self.mode != BuiltinSandboxMode::Off
    }
}

impl BuiltinSandboxMode {
    fn as_str(self) -> &'static str {
        match self {
            BuiltinSandboxMode::Off => "off",
            BuiltinSandboxMode::ReadOnly => "read_only",
            BuiltinSandboxMode::WorkspaceWrite => "workspace_write",
            BuiltinSandboxMode::Strict => "strict",
        }
    }
}

impl Default for BuiltinSandboxPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: BuiltinSandboxMode::WorkspaceWrite,
            on_violation: BuiltinSandboxViolationAction::Pause,
            network_access: false,
        }
    }
}

/// Executes tool calls with timeout enforcement and audit logging.
pub struct ToolExecutor {
    _default_timeout: Duration,
    plan_validator: PlanValidator,
    policy_engine: Option<Arc<Mutex<PolicyEngine>>>,
    filesystem: Option<FilesystemTool>,
    filesystem_backend: FilesystemExecutionBackend,
    network_backend: NetworkExecutionBackend,
    shell_config: ShellToolConfig,
    web_search_config: WebSearchConfig,
    fetch_config: FetchConfig,
    http_request_config: HttpRequestConfig,
    builtin_sandbox: BuiltinSandboxPolicy,
    trigger_sender: Option<tokio::sync::mpsc::Sender<TriggerEvent>>,
    sandbox_review_sender: Option<tokio::sync::mpsc::Sender<SandboxReviewRequestEnvelope>>,
    sandbox_review_timeout: Duration,
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
            filesystem_backend: FilesystemExecutionBackend::InProcess,
            network_backend: NetworkExecutionBackend::InProcess,
            shell_config: ShellToolConfig::default(),
            web_search_config: WebSearchConfig::default(),
            fetch_config: FetchConfig::default(),
            http_request_config: HttpRequestConfig::default(),
            builtin_sandbox: BuiltinSandboxPolicy::default(),
            trigger_sender: None,
            sandbox_review_sender: None,
            sandbox_review_timeout: Duration::from_secs(600),
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
            filesystem_backend: FilesystemExecutionBackend::InProcess,
            network_backend: NetworkExecutionBackend::InProcess,
            shell_config: ShellToolConfig::default(),
            web_search_config: WebSearchConfig::default(),
            fetch_config: FetchConfig::default(),
            http_request_config: HttpRequestConfig::default(),
            builtin_sandbox: BuiltinSandboxPolicy::default(),
            trigger_sender: None,
            sandbox_review_sender: None,
            sandbox_review_timeout: Duration::from_secs(600),
            snapshot_memories: Vec::new(),
            skill_bridge: None,
        }
    }

    /// Configure the filesystem tool with a workspace root.
    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.filesystem =
            Some(FilesystemTool::new(root).with_execution_backend(self.filesystem_backend.clone()));
    }

    /// Configure the filesystem tool without workspace path restrictions.
    pub fn set_unrestricted_workspace_root(&mut self, root: PathBuf) {
        self.filesystem = Some(
            FilesystemTool::new_unrestricted(root)
                .with_execution_backend(self.filesystem_backend.clone()),
        );
    }

    /// Configure the execution backend used by the filesystem tool.
    pub fn set_filesystem_execution_backend(&mut self, backend: FilesystemExecutionBackend) {
        self.filesystem_backend = backend.clone();
        if let Some(filesystem) = self.filesystem.as_mut() {
            filesystem.set_execution_backend(backend);
        }
    }

    /// Configure the backend used to isolate networked builtin tools.
    pub fn set_network_execution_backend(&mut self, backend: NetworkExecutionBackend) {
        self.network_backend = backend;
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

    /// Configure the builtin sandbox policy for runtime tools.
    pub fn set_builtin_sandbox_policy(&mut self, policy: BuiltinSandboxPolicy) {
        self.builtin_sandbox = policy;
    }

    /// Configure the trigger sender used for sandbox escalation.
    pub fn set_trigger_sender(&mut self, sender: tokio::sync::mpsc::Sender<TriggerEvent>) {
        self.trigger_sender = Some(sender);
    }

    /// Configure the sandbox review sender for interactive approval workflows.
    pub fn set_sandbox_review_sender(
        &mut self,
        sender: tokio::sync::mpsc::Sender<SandboxReviewRequestEnvelope>,
    ) {
        self.sandbox_review_sender = Some(sender);
    }

    /// Configure how long interactive sandbox reviews remain pending.
    pub fn set_sandbox_review_timeout(&mut self, timeout: Duration) {
        self.sandbox_review_timeout = timeout;
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
        self.enforce_builtin_sandbox(call, exec_ctx).await?;

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
                match self.dispatch_web_search(query, &config).await {
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
                match self.dispatch_web_fetch(url, &self.fetch_config).await {
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
                match self
                    .dispatch_http_request(url, method, &headers, body, &self.http_request_config)
                    .await
                {
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

    async fn dispatch_web_search(
        &self,
        query: &str,
        config: &WebSearchConfig,
    ) -> Result<Vec<super::builtin::web_search::SearchResult>, ToolError> {
        match &self.network_backend {
            NetworkExecutionBackend::InProcess => search(query, config)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),
            NetworkExecutionBackend::ProcessHelper { helper_executable } => {
                let config_json = serde_json::to_string(config)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                let output = self
                    .run_network_helper(
                        helper_executable,
                        "sandbox-web-search",
                        &[("--query", query), ("--config-json", config_json.as_str())],
                        None,
                    )
                    .await?;
                serde_json::from_slice(&output).map_err(|e| {
                    ToolError::ExecutionFailed(format!("parse sandbox web search output: {e}"))
                })
            }
            NetworkExecutionBackend::Container { image } => {
                self.dispatch_web_search_via_container(image, query, config)
                    .await
            }
        }
    }

    async fn dispatch_web_fetch(
        &self,
        url: &str,
        config: &FetchConfig,
    ) -> Result<super::builtin::web_fetch::FetchResult, ToolError> {
        match &self.network_backend {
            NetworkExecutionBackend::InProcess => fetch_url(url, config)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),
            NetworkExecutionBackend::ProcessHelper { helper_executable } => {
                let config_json = serde_json::to_string(config)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                let output = self
                    .run_network_helper(
                        helper_executable,
                        "sandbox-web-fetch",
                        &[("--url", url), ("--config-json", config_json.as_str())],
                        None,
                    )
                    .await?;
                serde_json::from_slice(&output).map_err(|e| {
                    ToolError::ExecutionFailed(format!("parse sandbox web fetch output: {e}"))
                })
            }
            NetworkExecutionBackend::Container { image } => {
                self.dispatch_web_fetch_via_container(image, url, config)
                    .await
            }
        }
    }

    async fn dispatch_http_request(
        &self,
        url: &str,
        method: &str,
        headers: &std::collections::HashMap<String, String>,
        body: Option<&str>,
        config: &HttpRequestConfig,
    ) -> Result<super::builtin::http_request::HttpRequestResult, ToolError> {
        match &self.network_backend {
            NetworkExecutionBackend::InProcess => http_request(url, method, headers, body, config)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string())),
            NetworkExecutionBackend::ProcessHelper { helper_executable } => {
                let request_json = serde_json::to_vec(&serde_json::json!({
                    "url": url,
                    "method": method,
                    "headers": headers,
                    "body": body,
                }))
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                let config_json = serde_json::to_string(config)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                let output = self
                    .run_network_helper(
                        helper_executable,
                        "sandbox-http-request",
                        &[("--config-json", config_json.as_str())],
                        Some(request_json),
                    )
                    .await?;
                serde_json::from_slice(&output).map_err(|e| {
                    ToolError::ExecutionFailed(format!("parse sandbox http request output: {e}"))
                })
            }
            NetworkExecutionBackend::Container { image } => {
                self.dispatch_http_request_via_container(image, url, method, headers, body, config)
                    .await
            }
        }
    }

    async fn run_network_helper(
        &self,
        helper_executable: &str,
        subcommand: &str,
        args: &[(&str, &str)],
        stdin_bytes: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, ToolError> {
        use std::process::Stdio;

        let mut process = tokio::process::Command::new(helper_executable);
        process
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        process.arg(subcommand);
        for (flag, value) in args {
            process.arg(flag).arg(value);
        }
        if stdin_bytes.is_some() {
            process.stdin(Stdio::piped());
        }

        let mut child = process
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("spawn network helper: {e}")))?;
        if let Some(bytes) = stdin_bytes {
            use tokio::io::AsyncWriteExt;
            let mut stdin = child.stdin.take().ok_or_else(|| {
                ToolError::ExecutionFailed("network helper stdin unavailable".into())
            })?;
            stdin.write_all(&bytes).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("write network helper stdin: {e}"))
            })?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("wait for network helper: {e}")))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(ToolError::ExecutionFailed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }

    async fn dispatch_web_search_via_container(
        &self,
        image: &str,
        query: &str,
        config: &WebSearchConfig,
    ) -> Result<Vec<super::builtin::web_search::SearchResult>, ToolError> {
        use super::builtin::web_search::{SearchBackend, SearchResult};

        let response = match config.backend {
            SearchBackend::SearXNG => {
                if config.searxng_url.is_empty() {
                    return Err(ToolError::ExecutionFailed(
                        "SearXNG URL not configured. Set `searxng_url` in search config \
                         (e.g. \"http://localhost:8888\"). Run SearXNG via: \
                         docker run -p 8888:8080 searxng/searxng"
                            .into(),
                    ));
                }
                let base_url = config.searxng_url.trim_end_matches('/');
                let url = format!(
                    "{}/search?q={}&format=json&categories=general",
                    base_url,
                    url_encode_component(query),
                );
                self.run_network_container_request(
                    image,
                    "GET",
                    &url,
                    &[("Accept", "application/json")],
                    None,
                    config.timeout_secs,
                )
                .await?
            }
            SearchBackend::Tavily => {
                if config.tavily_api_key.is_empty() {
                    return Err(ToolError::ExecutionFailed(
                        "Tavily API key not configured. Get a free key at https://tavily.com \
                         (1,000 credits/month free, no credit card)."
                            .into(),
                    ));
                }
                let body = serde_json::to_vec(&serde_json::json!({
                    "api_key": config.tavily_api_key,
                    "query": query,
                    "max_results": config.max_results,
                    "search_depth": "basic",
                }))
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                self.run_network_container_request(
                    image,
                    "POST",
                    "https://api.tavily.com/search",
                    &[("Content-Type", "application/json")],
                    Some(&body),
                    config.timeout_secs,
                )
                .await?
            }
            SearchBackend::Brave => {
                if config.brave_api_key.is_empty() {
                    return Err(ToolError::ExecutionFailed(
                        "Brave Search API key not configured. Get $5/month free credits \
                         at https://brave.com/search/api/"
                            .into(),
                    ));
                }
                let url = format!(
                    "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
                    url_encode_component(query),
                    config.max_results,
                );
                self.run_network_container_request(
                    image,
                    "GET",
                    &url,
                    &[
                        ("Accept", "application/json"),
                        ("Accept-Encoding", "gzip"),
                        ("X-Subscription-Token", &config.brave_api_key),
                    ],
                    None,
                    config.timeout_secs,
                )
                .await?
            }
        };

        if matches!(config.backend, SearchBackend::Tavily)
            && (response.status == 401 || response.status == 403)
        {
            return Err(ToolError::ExecutionFailed(
                "Tavily API key is invalid or expired.".into(),
            ));
        }
        if matches!(config.backend, SearchBackend::Brave)
            && (response.status == 401 || response.status == 403)
        {
            return Err(ToolError::ExecutionFailed(
                "Brave Search API key is invalid.".into(),
            ));
        }
        if response.status == 429 {
            let retry_after = response
                .headers
                .get("retry-after")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(60);
            return Err(ToolError::ExecutionFailed(format!(
                "search rate limited — retry after {retry_after}s"
            )));
        }
        if !(200..300).contains(&response.status) {
            return Err(ToolError::ExecutionFailed(format!(
                "search backend returned HTTP {}: {}",
                response.status,
                truncate_for_error(&String::from_utf8_lossy(&response.body), 200),
            )));
        }

        let parsed: serde_json::Value = serde_json::from_slice(&response.body)
            .map_err(|e| ToolError::ExecutionFailed(format!("search JSON parse: {e}")))?;
        let results = match config.backend {
            SearchBackend::SearXNG => parsed
                .get("results")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .map(|entry| SearchResult {
                    title: entry
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    url: entry
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    snippet: entry
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect(),
            SearchBackend::Tavily => parsed
                .get("results")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .map(|entry| SearchResult {
                    title: entry
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    url: entry
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    snippet: entry
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect(),
            SearchBackend::Brave => parsed
                .get("web")
                .and_then(|v| v.get("results"))
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .map(|entry| SearchResult {
                    title: entry
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    url: entry
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    snippet: entry
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect(),
        };

        Ok(results)
    }

    async fn dispatch_web_fetch_via_container(
        &self,
        image: &str,
        url: &str,
        config: &FetchConfig,
    ) -> Result<super::builtin::web_fetch::FetchResult, ToolError> {
        validate_fetch_url(url, config)?;
        let response = self
            .run_network_container_request(
                image,
                "GET",
                url,
                &[(
                    "Accept",
                    "text/html,application/xhtml+xml,text/plain,application/json",
                )],
                None,
                config.timeout_secs,
            )
            .await?;
        finalize_fetch_result(
            response.final_url,
            response.status,
            response.content_type,
            response.body,
            config,
        )
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }

    async fn dispatch_http_request_via_container(
        &self,
        image: &str,
        url: &str,
        method: &str,
        headers: &std::collections::HashMap<String, String>,
        body: Option<&str>,
        config: &HttpRequestConfig,
    ) -> Result<super::builtin::http_request::HttpRequestResult, ToolError> {
        let method_upper = validate_http_request(url, method, body, config)?;
        let request_headers: Vec<(&str, &str)> = headers
            .iter()
            .filter(|(key, _)| !key.eq_ignore_ascii_case("host"))
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let response = self
            .run_network_container_request(
                image,
                &method_upper,
                url,
                &request_headers,
                body.map(str::as_bytes),
                config.timeout_secs,
            )
            .await?;
        let safe_headers = extract_safe_headers_from_map(&response.headers);
        finalize_http_request_result(
            response.final_url,
            method_upper,
            response.status,
            safe_headers,
            response.body,
            response.content_type,
            config,
        )
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }

    async fn run_network_container_request(
        &self,
        image: &str,
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
        timeout_secs: u64,
    ) -> Result<ContainerHttpResponse, ToolError> {
        use std::process::Stdio;

        const META_MARKER: &str = "\n__GHOST_NET_META__";

        let mut process = tokio::process::Command::new("docker");
        process.kill_on_drop(true).arg("run").arg("--rm");
        if body.is_some() {
            process.arg("-i");
        }
        process
            .arg(image)
            .arg("-sS")
            .arg("-L")
            .arg("--compressed")
            .arg("--max-time")
            .arg(timeout_secs.to_string())
            .arg("-X")
            .arg(method)
            .arg(url)
            .arg("-D")
            .arg("/dev/stderr")
            .arg("-o")
            .arg("-")
            .arg("-w")
            .arg(format!(
                "{META_MARKER}%{{http_code}}\t%{{url_effective}}\t%{{content_type}}"
            ))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in headers {
            process.arg("-H").arg(format!("{key}: {value}"));
        }
        if body.is_some() {
            process.arg("--data-binary").arg("@-").stdin(Stdio::piped());
        }

        let mut child = process
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("spawn network container: {e}")))?;
        if let Some(bytes) = body {
            use tokio::io::AsyncWriteExt;
            let mut stdin = child.stdin.take().ok_or_else(|| {
                ToolError::ExecutionFailed("network container stdin unavailable".into())
            })?;
            stdin.write_all(bytes).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("write network container stdin: {e}"))
            })?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("wait for network container: {e}")))?;
        if !output.status.success() {
            return Err(ToolError::ExecutionFailed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        let marker = META_MARKER.as_bytes();
        let meta_start = output
            .stdout
            .windows(marker.len())
            .rposition(|window| window == marker)
            .ok_or_else(|| {
                ToolError::ExecutionFailed("network container metadata missing".into())
            })?;
        let body_bytes = output.stdout[..meta_start].to_vec();
        let meta = String::from_utf8_lossy(&output.stdout[meta_start + marker.len()..]).to_string();
        let mut parts = meta.splitn(3, '\t');
        let status = parts
            .next()
            .and_then(|value| value.trim().parse::<u16>().ok())
            .ok_or_else(|| ToolError::ExecutionFailed("invalid network container status".into()))?;
        let final_url = parts.next().unwrap_or(url).trim().to_string();
        let content_type = parts.next().unwrap_or("").trim().to_string();
        let headers = parse_final_header_block(&String::from_utf8_lossy(&output.stderr));

        Ok(ContainerHttpResponse {
            status,
            final_url,
            content_type,
            headers,
            body: body_bytes,
        })
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

    async fn enforce_builtin_sandbox(
        &self,
        call: &LLMToolCall,
        exec_ctx: &ExecutionContext,
    ) -> Result<(), ToolError> {
        if !self.builtin_sandbox.is_active() {
            return Ok(());
        }

        let violation = match (self.builtin_sandbox.mode, call.name.as_str()) {
            (BuiltinSandboxMode::ReadOnly, "write_file")
            | (BuiltinSandboxMode::Strict, "write_file") => {
                Some("write_file is blocked by read-only sandbox mode")
            }
            (BuiltinSandboxMode::ReadOnly, "shell") | (BuiltinSandboxMode::Strict, "shell") => {
                Some("shell is blocked by sandbox mode")
            }
            (_, "web_search" | "web_fetch" | "http_request")
                if !self.builtin_sandbox.network_access =>
            {
                Some("network access is disabled by sandbox policy")
            }
            _ => None,
        };

        if let Some(reason) = violation {
            if matches!(
                self.builtin_sandbox.on_violation,
                BuiltinSandboxViolationAction::Pause
            ) && exec_ctx.interactive
                && self.sandbox_review_sender.is_some()
            {
                match self.request_sandbox_review(call, exec_ctx, reason).await? {
                    SandboxReviewDecision::Approved => return Ok(()),
                    SandboxReviewDecision::Rejected => {
                        return Err(ToolError::PolicyDenied(
                            serde_json::json!({
                                "type": "sandbox_review_rejected",
                                "tool": call.name,
                                "message": reason,
                                "mode": self.builtin_sandbox.mode.as_str(),
                            })
                            .to_string(),
                        ));
                    }
                    SandboxReviewDecision::Expired => {
                        return Err(ToolError::PolicyDenied(
                            serde_json::json!({
                                "type": "sandbox_review_expired",
                                "tool": call.name,
                                "message": reason,
                                "mode": self.builtin_sandbox.mode.as_str(),
                            })
                            .to_string(),
                        ));
                    }
                }
            }

            self.emit_sandbox_violation(exec_ctx.agent_id, &call.name, reason);
            return Err(ToolError::PolicyDenied(
                serde_json::json!({
                    "type": "sandbox_violation",
                    "tool": call.name,
                    "message": reason,
                    "mode": self.builtin_sandbox.mode.as_str(),
                })
                .to_string(),
            ));
        }

        Ok(())
    }

    async fn request_sandbox_review(
        &self,
        call: &LLMToolCall,
        exec_ctx: &ExecutionContext,
        reason: &str,
    ) -> Result<SandboxReviewDecision, ToolError> {
        let Some(sender) = &self.sandbox_review_sender else {
            return Err(ToolError::SandboxReviewUnavailable(
                "sandbox review sender not configured".into(),
            ));
        };

        let review_id = Uuid::now_v7().to_string();
        let (decision_tx, decision_rx) = tokio::sync::oneshot::channel();
        let request = SandboxReviewRequest {
            review_id,
            agent_id: exec_ctx.agent_id,
            session_id: exec_ctx.session_id,
            execution_id: exec_ctx.execution_id.clone(),
            route_kind: exec_ctx.route_kind.clone(),
            tool_name: call.name.clone(),
            violation_reason: reason.to_string(),
            sandbox_mode: self.builtin_sandbox.mode.as_str().to_string(),
            timeout_secs: self.sandbox_review_timeout.as_secs(),
        };

        sender
            .send(SandboxReviewRequestEnvelope {
                request,
                decision_tx,
            })
            .await
            .map_err(|_| {
                ToolError::SandboxReviewUnavailable(
                    "sandbox review coordinator is unavailable".into(),
                )
            })?;

        decision_rx.await.map_err(|_| {
            ToolError::SandboxReviewUnavailable(
                "sandbox review coordinator dropped the request".into(),
            )
        })
    }

    fn emit_sandbox_violation(&self, agent_id: Uuid, tool_name: &str, reason: &str) {
        let Some(sender) = &self.trigger_sender else {
            return;
        };

        let trigger = match self.builtin_sandbox.on_violation {
            BuiltinSandboxViolationAction::Warn => None,
            BuiltinSandboxViolationAction::Pause => Some(TriggerEvent::ManualPause {
                agent_id,
                reason: format!("sandbox violation in {tool_name}: {reason}"),
                initiated_by: "builtin_sandbox".into(),
            }),
            BuiltinSandboxViolationAction::Quarantine => Some(TriggerEvent::ManualQuarantine {
                agent_id,
                reason: format!("sandbox violation in {tool_name}: {reason}"),
                initiated_by: "builtin_sandbox".into(),
            }),
            BuiltinSandboxViolationAction::KillAll => Some(TriggerEvent::ManualKillAll {
                reason: format!("sandbox violation in {tool_name}: {reason}"),
                initiated_by: "builtin_sandbox".into(),
            }),
        };

        if let Some(trigger) = trigger {
            if sender.try_send(trigger).is_err() {
                tracing::warn!(
                    agent_id = %agent_id,
                    tool = %tool_name,
                    "failed to emit sandbox violation trigger"
                );
            } else {
                tracing::warn!(
                    agent_id = %agent_id,
                    tool = %tool_name,
                    action = ?self.builtin_sandbox.on_violation,
                    detected_at = %Utc::now(),
                    "builtin sandbox violation escalated"
                );
            }
        }
    }
}

struct ContainerHttpResponse {
    status: u16,
    final_url: String,
    content_type: String,
    headers: std::collections::HashMap<String, String>,
    body: Vec<u8>,
}

fn validate_fetch_url(url: &str, config: &FetchConfig) -> Result<(), ToolError> {
    let url = url.trim();
    if url.starts_with("http://") && !config.allow_http {
        return Err(ToolError::ExecutionFailed(
            "HTTP URLs are not allowed. Only HTTPS is permitted.".into(),
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ToolError::ExecutionFailed(format!(
            "Unsupported URL scheme. Only HTTP(S) is allowed: {}",
            truncate_for_error(url, 100),
        )));
    }

    let host = extract_host_for_network(url)
        .ok_or_else(|| ToolError::ExecutionFailed("Cannot parse host from URL".into()))?;
    if is_private_host_for_network(&host) {
        return Err(ToolError::ExecutionFailed(format!(
            "SSRF blocked: {host} resolves to private/internal IP"
        )));
    }
    Ok(())
}

fn validate_http_request(
    url: &str,
    method: &str,
    body: Option<&str>,
    config: &HttpRequestConfig,
) -> Result<String, ToolError> {
    let url = url.trim();
    let method_upper = method.trim().to_uppercase();

    if !config
        .allowed_methods
        .iter()
        .any(|allowed| allowed.to_uppercase() == method_upper)
    {
        return Err(ToolError::ExecutionFailed(format!(
            "{} is not in allowed methods: {:?}",
            method_upper, config.allowed_methods,
        )));
    }
    if url.starts_with("http://") && !config.allow_http {
        return Err(ToolError::ExecutionFailed(
            "HTTP URLs are not allowed. Only HTTPS is permitted.".into(),
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ToolError::ExecutionFailed(format!(
            "Unsupported URL scheme. Only HTTP(S) is allowed: {}",
            truncate_for_error(url, 100),
        )));
    }
    if let Some(body) = body {
        if body.len() > config.max_request_bytes {
            return Err(ToolError::ExecutionFailed(format!(
                "request body too large: {} bytes exceeds {} byte limit",
                body.len(),
                config.max_request_bytes,
            )));
        }
    }

    let host = extract_host_for_network(url)
        .ok_or_else(|| ToolError::ExecutionFailed("Cannot parse host from URL".into()))?;
    if is_private_host_for_network(&host) {
        return Err(ToolError::ExecutionFailed(format!(
            "SSRF blocked: {host} resolves to private/internal IP"
        )));
    }
    if config.allowed_domains.is_empty() {
        return Err(ToolError::ExecutionFailed(
            "HTTP request tool disabled: no allowed domains configured".into(),
        ));
    }
    let domain_allowed = config.allowed_domains.iter().any(|domain| {
        let domain = domain.to_lowercase();
        host == domain || host.ends_with(&format!(".{domain}"))
    });
    if !domain_allowed {
        return Err(ToolError::ExecutionFailed(format!(
            "Domain '{}' is not in the allowlist. Allowed: {:?}",
            host, config.allowed_domains,
        )));
    }

    Ok(method_upper)
}

fn extract_host_for_network(url: &str) -> Option<String> {
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = after_scheme.split('/').next()?.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    Some(host.to_lowercase())
}

fn is_private_host_for_network(host: &str) -> bool {
    let blocked_hosts = [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "::1",
        "[::1]",
        "metadata.google.internal",
        "169.254.169.254",
    ];
    if blocked_hosts.contains(&host) {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        };
    }
    false
}

fn parse_final_header_block(raw: &str) -> std::collections::HashMap<String, String> {
    let normalized = raw.replace("\r\n", "\n");
    let mut final_block = "";
    for block in normalized.split("\n\n") {
        if block.trim_start().starts_with("HTTP/") {
            final_block = block;
        }
    }

    final_block
        .lines()
        .skip(1)
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_lowercase(), value.trim().to_string()))
        })
        .collect()
}

fn extract_safe_headers_from_map(
    headers: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    let safe_keys = [
        "content-type",
        "content-length",
        "x-request-id",
        "x-ratelimit-limit",
        "x-ratelimit-remaining",
        "x-ratelimit-reset",
        "retry-after",
        "location",
        "etag",
        "last-modified",
        "cache-control",
        "date",
    ];

    safe_keys
        .iter()
        .filter_map(|key| {
            headers
                .get(*key)
                .map(|value| (key.to_string(), value.clone()))
        })
        .collect()
}

fn url_encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(byte & 0x0f) as usize]));
            }
        }
    }
    out
}

fn truncate_for_error(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghost_llm::provider::LLMToolCall;

    fn exec_ctx() -> ExecutionContext {
        ExecutionContext {
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            execution_id: None,
            route_kind: None,
            interactive: true,
            intervention_level: 0,
            session_duration: Duration::from_secs(0),
            session_reflection_count: 0,
            is_compaction_flush: false,
        }
    }

    fn tool_call(name: &str) -> LLMToolCall {
        LLMToolCall {
            id: "call_1".into(),
            name: name.into(),
            arguments: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn sandbox_blocks_write_file_in_read_only_mode() {
        let executor = ToolExecutor {
            builtin_sandbox: BuiltinSandboxPolicy {
                enabled: true,
                mode: BuiltinSandboxMode::ReadOnly,
                on_violation: BuiltinSandboxViolationAction::Pause,
                network_access: false,
            },
            ..ToolExecutor::default()
        };

        let error = executor
            .enforce_builtin_sandbox(&tool_call("write_file"), &exec_ctx())
            .await
            .expect_err("write_file should be denied in read-only mode");

        match error {
            ToolError::PolicyDenied(payload) => {
                let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
                assert_eq!(json["type"], "sandbox_violation");
                assert_eq!(json["tool"], "write_file");
                assert_eq!(json["mode"], "read_only");
            }
            other => panic!("expected policy denial, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sandbox_allows_write_file_in_workspace_write_mode() {
        let executor = ToolExecutor {
            builtin_sandbox: BuiltinSandboxPolicy {
                enabled: true,
                mode: BuiltinSandboxMode::WorkspaceWrite,
                on_violation: BuiltinSandboxViolationAction::Pause,
                network_access: false,
            },
            ..ToolExecutor::default()
        };

        executor
            .enforce_builtin_sandbox(&tool_call("write_file"), &exec_ctx())
            .await
            .expect("workspace_write mode should allow write_file");
    }

    #[tokio::test]
    async fn sandbox_blocks_network_when_disabled() {
        let executor = ToolExecutor {
            builtin_sandbox: BuiltinSandboxPolicy {
                enabled: true,
                mode: BuiltinSandboxMode::WorkspaceWrite,
                on_violation: BuiltinSandboxViolationAction::Pause,
                network_access: false,
            },
            ..ToolExecutor::default()
        };

        let error = executor
            .enforce_builtin_sandbox(&tool_call("web_fetch"), &exec_ctx())
            .await
            .expect_err("network tools should be denied when sandbox network is disabled");

        match error {
            ToolError::PolicyDenied(payload) => {
                let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
                assert_eq!(json["type"], "sandbox_violation");
                assert_eq!(json["tool"], "web_fetch");
                assert_eq!(json["mode"], "workspace_write");
            }
            other => panic!("expected policy denial, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sandbox_pause_emits_manual_pause_trigger() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let executor = ToolExecutor {
            builtin_sandbox: BuiltinSandboxPolicy {
                enabled: true,
                mode: BuiltinSandboxMode::ReadOnly,
                on_violation: BuiltinSandboxViolationAction::Pause,
                network_access: false,
            },
            trigger_sender: Some(sender),
            ..ToolExecutor::default()
        };

        let _ = executor
            .enforce_builtin_sandbox(&tool_call("shell"), &exec_ctx())
            .await;

        let trigger = receiver.try_recv().expect("expected sandbox trigger");
        match trigger {
            TriggerEvent::ManualPause {
                agent_id,
                reason,
                initiated_by,
            } => {
                assert_eq!(agent_id, Uuid::nil());
                assert!(reason.contains("sandbox violation in shell"));
                assert_eq!(initiated_by, "builtin_sandbox");
            }
            other => panic!("expected manual pause trigger, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn interactive_pause_requests_human_review_and_allows_approved_tool() {
        let (review_tx, mut review_rx) = tokio::sync::mpsc::channel(1);
        let executor = ToolExecutor {
            builtin_sandbox: BuiltinSandboxPolicy {
                enabled: true,
                mode: BuiltinSandboxMode::ReadOnly,
                on_violation: BuiltinSandboxViolationAction::Pause,
                network_access: false,
            },
            sandbox_review_sender: Some(review_tx),
            sandbox_review_timeout: Duration::from_secs(5),
            ..ToolExecutor::default()
        };

        let approve = tokio::spawn(async move {
            let envelope = review_rx.recv().await.expect("review request");
            envelope
                .decision_tx
                .send(SandboxReviewDecision::Approved)
                .expect("approval send");
        });

        executor
            .enforce_builtin_sandbox(&tool_call("shell"), &exec_ctx())
            .await
            .expect("approved review should allow execution to continue");

        approve.await.unwrap();
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
