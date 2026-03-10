use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_stream::try_stream;
use async_trait::async_trait;
use ghost_llm::provider::{
    ChatMessage, CompletionResult, LLMError, LLMProvider, LLMResponse, MessageRole, TokenPricing,
    ToolSchema, UsageStats,
};
use ghost_llm::streaming::{StreamChunk, StreamChunkStream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const CODEX_LOGIN_WAIT: Duration = Duration::from_secs(600);
const CODEX_MODEL_PROVIDER: &str = "openai";

#[derive(Debug, Error)]
pub enum CodexError {
    #[error("codex binary not found: {0}")]
    BinaryNotFound(String),
    #[error("failed to start codex app-server: {0}")]
    Spawn(String),
    #[error("codex I/O failed: {0}")]
    Io(String),
    #[error("invalid codex response: {0}")]
    Protocol(String),
    #[error("codex server error {code}: {message}")]
    Server {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    #[error("codex authentication required: {0}")]
    Auth(String),
    #[error("failed to parse codex payload: {0}")]
    Json(String),
}

impl From<std::io::Error> for CodexError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for CodexError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<CodexError> for LLMError {
    fn from(error: CodexError) -> Self {
        match error {
            CodexError::Auth(message) => LLMError::AuthFailed(message),
            CodexError::BinaryNotFound(message) | CodexError::Spawn(message) => {
                LLMError::Unavailable(message)
            }
            CodexError::Server {
                code,
                message,
                data,
            } => {
                if code == 401 || code == 403 {
                    LLMError::AuthFailed(message)
                } else {
                    LLMError::Other(format!("codex server error {code}: {message} {data:?}"))
                }
            }
            CodexError::Io(message) | CodexError::Protocol(message) | CodexError::Json(message) => {
                LLMError::Other(message)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodexProvider {
    pub model: Option<String>,
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone)]
struct CodexExecutionOptions {
    model: Option<String>,
    api_key_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAccountStatus {
    #[serde(default)]
    pub requires_openai_auth: bool,
    #[serde(default)]
    pub account: Option<CodexAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CodexAccount {
    ApiKey,
    Chatgpt { email: String, plan_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CodexLoginStart {
    ApiKey,
    Chatgpt { auth_url: String, login_id: String },
    ChatgptAuthTokens,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexLoginCompletion {
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub login_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRateLimits {
    #[serde(default)]
    pub rate_limits: Option<Value>,
    #[serde(default)]
    pub rate_limits_by_limit_id: Option<Value>,
}

#[derive(Debug, Clone)]
struct CodexThreadContext {
    thread_id: String,
    model: String,
}

#[derive(Debug, Default)]
struct CodexTurnState {
    text: String,
    usage: UsageStats,
    terminal_text_emitted: bool,
}

#[derive(Debug)]
enum IncomingMessage {
    Response {
        id: Value,
        result: Value,
    },
    Error {
        id: Value,
        code: i64,
        message: String,
        data: Option<Value>,
    },
    Request {
        id: Value,
        method: String,
        _params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

struct CodexClient {
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    _child: Child,
    next_request_id: i64,
}

#[async_trait]
impl LLMProvider for CodexProvider {
    fn name(&self) -> &str {
        "codex"
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let options = self.execution_options();
        run_codex_turn_collect(options, messages, tools)
            .await
            .map_err(Into::into)
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn context_window(&self) -> usize {
        128_000
    }

    fn token_pricing(&self) -> TokenPricing {
        TokenPricing {
            input_per_1k: 0.0,
            output_per_1k: 0.0,
        }
    }

    async fn health_check(&self) -> Result<(), LLMError> {
        let mut client = CodexClient::spawn().await.map_err(LLMError::from)?;
        client.ensure_account(self.api_key_env.as_deref()).await?;
        Ok(())
    }
}

impl CodexProvider {
    pub fn stream_chat(&self, messages: &[ChatMessage], tools: &[ToolSchema]) -> StreamChunkStream {
        let options = self.execution_options();
        let messages = messages.to_vec();
        let tools = tools.to_vec();

        Box::pin(try_stream! {
            let mut client = CodexClient::spawn().await?;
            client.ensure_account(options.api_key_env.as_deref()).await?;
            let developer_instructions = developer_instructions_from_messages(&messages, &tools);
            let prompt = transcript_from_messages(&messages, &tools);
            let thread = client
                .start_thread(&options, developer_instructions.as_deref())
                .await?;
            let mut state = CodexTurnState::default();
            let cwd = codex_working_dir()?;
            let root = cwd.to_string_lossy().to_string();
            client
                .request_json(
                    "turn/start",
                    json!({
                        "threadId": thread.thread_id.clone(),
                        "input": [
                            {
                                "type": "text",
                                "text": prompt,
                            }
                        ],
                        "cwd": cwd,
                        "model": thread.model.clone(),
                        "personality": "pragmatic",
                        "approvalPolicy": "never",
                        "sandboxPolicy": {
                            "type": "workspaceWrite",
                            "networkAccess": false,
                            "writableRoots": [root],
                        },
                    }),
                )
                .await?;

            loop {
                match client.read_message().await? {
                    IncomingMessage::Notification { method, params } => match method.as_str() {
                        "agent/message/delta" => {
                            if let Some(delta) = read_string(params.get("delta")) {
                                state.text.push_str(&delta);
                                yield StreamChunk::TextDelta(delta);
                            }
                        }
                        "thread/tokenUsage/updated" => {
                            state.usage = parse_usage_from_notification(&params);
                        }
                        "turn/completed" => {
                            let status = params
                                .get("turn")
                                .and_then(|turn| turn.get("status"))
                                .and_then(Value::as_str)
                                .unwrap_or("completed");
                            if status == "failed" {
                                let message = params
                                    .get("turn")
                                    .and_then(|turn| turn.get("error"))
                                    .and_then(|error| error.get("message"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("Codex turn failed");
                                Err::<(), CodexError>(CodexError::Protocol(message.to_string()))?;
                            }

                            if !state.terminal_text_emitted {
                                let final_text = client
                                    .read_turn_text(&thread.thread_id)
                                    .await
                                    .unwrap_or_else(|_| state.text.clone());
                                if let Some(delta) = trailing_text_delta(&state.text, &final_text) {
                                    state.text.push_str(&delta);
                                    yield StreamChunk::TextDelta(delta);
                                }
                                state.terminal_text_emitted = true;
                            }
                            break;
                        }
                        "error" => {
                            let message = params
                                .get("error")
                                .and_then(|error| error.get("message"))
                                .and_then(Value::as_str)
                                .unwrap_or("Codex reported an error");
                            Err::<(), CodexError>(CodexError::Protocol(message.to_string()))?;
                        }
                        _ => {}
                    },
                    other => client.handle_unsolicited(other).await?,
                }
            }
            yield StreamChunk::Done(state.usage);
        })
    }

    fn execution_options(&self) -> CodexExecutionOptions {
        CodexExecutionOptions {
            model: self.model.clone(),
            api_key_env: self.api_key_env.clone(),
        }
    }
}

impl CodexClient {
    async fn spawn() -> Result<Self, CodexError> {
        let binary = codex_binary();
        let mut command = Command::new(&binary);
        command
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CodexError::BinaryNotFound(binary)
            } else {
                CodexError::Spawn(error.to_string())
            }
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CodexError::Spawn("codex app-server stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CodexError::Spawn("codex app-server stdout unavailable".into()))?;

        let mut client = Self {
            stdin,
            stdout: BufReader::new(stdout).lines(),
            _child: child,
            next_request_id: 1,
        };

        client
            .request_json(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "ghost-gateway",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "experimentalApi": false
                    }
                }),
            )
            .await?;

        Ok(client)
    }

    async fn request_json(&mut self, method: &str, params: Value) -> Result<Value, CodexError> {
        let request_id = Value::from(self.next_request_id);
        self.next_request_id += 1;
        self.write_message(json!({
            "id": request_id,
            "method": method,
            "params": params,
        }))
        .await?;

        loop {
            match self.read_message().await? {
                IncomingMessage::Response { id, result } if id == request_id => return Ok(result),
                IncomingMessage::Error {
                    id,
                    code,
                    message,
                    data,
                } if id == request_id => {
                    return Err(CodexError::Server {
                        code,
                        message,
                        data,
                    })
                }
                other => self.handle_unsolicited(other).await?,
            }
        }
    }

    async fn ensure_account(
        &mut self,
        api_key_env: Option<&str>,
    ) -> Result<CodexAccountStatus, LLMError> {
        let status = self.read_account(false).await.map_err(LLMError::from)?;
        if status.account.is_some() {
            return Ok(status);
        }

        if let Some(api_key) = api_key_from_env(api_key_env) {
            self.request_json(
                "account/login/start",
                json!({
                    "type": "apiKey",
                    "apiKey": api_key,
                }),
            )
            .await
            .map_err(LLMError::from)?;
            return self.read_account(false).await.map_err(LLMError::from);
        }

        if status.requires_openai_auth {
            return Err(LLMError::AuthFailed(
                "no Codex account is logged in. Run `ghost codex login` or configure `api_key_env` on the `codex` provider.".into(),
            ));
        }

        Err(LLMError::AuthFailed(
            "Codex is not authenticated. Run `ghost codex login` or set a provider api_key_env."
                .into(),
        ))
    }

    pub async fn read_account(
        &mut self,
        refresh_token: bool,
    ) -> Result<CodexAccountStatus, CodexError> {
        let result = self
            .request_json("account/read", json!({ "refreshToken": refresh_token }))
            .await?;
        Ok(parse_account_status(&result))
    }

    pub async fn login_chatgpt(&mut self) -> Result<CodexLoginStart, CodexError> {
        let result = self
            .request_json("account/login/start", json!({ "type": "chatgpt" }))
            .await?;
        parse_login_start(&result)
    }

    pub async fn login_api_key(&mut self, api_key: &str) -> Result<CodexLoginStart, CodexError> {
        let result = self
            .request_json(
                "account/login/start",
                json!({
                    "type": "apiKey",
                    "apiKey": api_key,
                }),
            )
            .await?;
        parse_login_start(&result)
    }

    pub async fn wait_for_login_completion(
        &mut self,
        expected_login_id: &str,
        timeout: Duration,
    ) -> Result<CodexLoginCompletion, CodexError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(CodexLoginCompletion {
                    success: false,
                    error: Some("timed out waiting for Codex login completion".into()),
                    login_id: Some(expected_login_id.to_string()),
                });
            }

            let message = tokio::time::timeout(remaining, self.read_message())
                .await
                .map_err(|_| CodexError::Protocol("timed out waiting for Codex login".into()))??;

            match message {
                IncomingMessage::Notification { method, params }
                    if method == "account/login/completed" =>
                {
                    let completion = parse_login_completion(&params)?;
                    if completion.login_id.as_deref() == Some(expected_login_id) {
                        return Ok(completion);
                    }
                }
                other => self.handle_unsolicited(other).await?,
            }
        }
    }

    pub async fn logout(&mut self) -> Result<(), CodexError> {
        self.request_json("account/logout", Value::Null).await?;
        Ok(())
    }

    pub async fn read_rate_limits(&mut self) -> Result<CodexRateLimits, CodexError> {
        let result = self
            .request_json("account/rateLimits/read", Value::Null)
            .await?;
        Ok(CodexRateLimits {
            rate_limits: result.get("rateLimits").cloned(),
            rate_limits_by_limit_id: result.get("rateLimitsByLimitId").cloned(),
        })
    }

    async fn start_thread(
        &mut self,
        options: &CodexExecutionOptions,
        developer_instructions: Option<&str>,
    ) -> Result<CodexThreadContext, CodexError> {
        let cwd = codex_working_dir()?;
        let result = self
            .request_json(
                "thread/start",
                json!({
                    "approvalPolicy": "never",
                    "cwd": cwd,
                    "developerInstructions": developer_instructions,
                    "ephemeral": true,
                    "model": options.model,
                    "modelProvider": CODEX_MODEL_PROVIDER,
                    "personality": "pragmatic",
                    "sandbox": "workspace-write",
                    "serviceName": "ghost",
                }),
            )
            .await?;

        let thread = result
            .get("thread")
            .and_then(Value::as_object)
            .ok_or_else(|| CodexError::Protocol("thread/start missing thread object".into()))?;
        let thread_id = read_string(thread.get("id"))
            .ok_or_else(|| CodexError::Protocol("thread/start missing thread id".into()))?;
        let model = read_string(result.get("model"))
            .or_else(|| read_string(thread.get("model")))
            .or_else(|| options.model.clone())
            .unwrap_or_else(|| "codex-default".into());
        Ok(CodexThreadContext { thread_id, model })
    }

    async fn start_turn_streaming(
        &mut self,
        thread: &CodexThreadContext,
        prompt: &str,
        state: &mut CodexTurnState,
        emit_stream_chunks: bool,
    ) -> Result<Vec<StreamChunk>, CodexError> {
        let cwd = codex_working_dir()?;
        let root = cwd.to_string_lossy().to_string();
        let mut trailing_chunks = Vec::new();
        self.request_json(
            "turn/start",
            json!({
                "threadId": thread.thread_id,
                "input": [
                    {
                        "type": "text",
                        "text": prompt,
                    }
                ],
                "cwd": cwd,
                "model": thread.model,
                "personality": "pragmatic",
                "approvalPolicy": "never",
                "sandboxPolicy": {
                    "type": "workspaceWrite",
                    "networkAccess": false,
                    "writableRoots": [root],
                },
            }),
        )
        .await?;

        loop {
            match self.read_message().await? {
                IncomingMessage::Notification { method, params } => match method.as_str() {
                    "agent/message/delta" => {
                        if let Some(delta) = read_string(params.get("delta")) {
                            state.text.push_str(&delta);
                            if emit_stream_chunks {
                                trailing_chunks.push(StreamChunk::TextDelta(delta));
                            }
                        }
                    }
                    "thread/tokenUsage/updated" => {
                        state.usage = parse_usage_from_notification(&params);
                    }
                    "turn/completed" => {
                        let status = params
                            .get("turn")
                            .and_then(|turn| turn.get("status"))
                            .and_then(Value::as_str)
                            .unwrap_or("completed");

                        if status == "failed" {
                            let message = params
                                .get("turn")
                                .and_then(|turn| turn.get("error"))
                                .and_then(|error| error.get("message"))
                                .and_then(Value::as_str)
                                .unwrap_or("Codex turn failed");
                            return Err(CodexError::Protocol(message.to_string()));
                        }

                        if !state.terminal_text_emitted {
                            let final_text = self
                                .read_turn_text(&thread.thread_id)
                                .await
                                .unwrap_or_else(|_| state.text.clone());
                            if let Some(delta) = trailing_text_delta(&state.text, &final_text) {
                                state.text.push_str(&delta);
                                if emit_stream_chunks {
                                    trailing_chunks.push(StreamChunk::TextDelta(delta));
                                }
                            }
                            state.terminal_text_emitted = true;
                        }

                        return Ok(trailing_chunks);
                    }
                    "error" => {
                        let message = params
                            .get("error")
                            .and_then(|error| error.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("Codex reported an error");
                        return Err(CodexError::Protocol(message.to_string()));
                    }
                    _ => {}
                },
                other => self.handle_unsolicited(other).await?,
            }
        }
    }

    async fn read_turn_text(&mut self, thread_id: &str) -> Result<String, CodexError> {
        let response = self
            .request_json(
                "thread/read",
                json!({
                    "threadId": thread_id,
                    "includeTurns": true,
                }),
            )
            .await?;
        Ok(extract_turn_text(&response))
    }

    async fn write_message(&mut self, value: Value) -> Result<(), CodexError> {
        let mut encoded = serde_json::to_vec(&value)?;
        encoded.push(b'\n');
        self.stdin.write_all(&encoded).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn send_response(&mut self, id: Value, result: Value) -> Result<(), CodexError> {
        self.write_message(json!({ "id": id, "result": result }))
            .await
    }

    async fn send_error(&mut self, id: Value, code: i64, message: &str) -> Result<(), CodexError> {
        self.write_message(json!({
            "id": id,
            "error": {
                "code": code,
                "message": message,
            }
        }))
        .await
    }

    async fn read_message(&mut self) -> Result<IncomingMessage, CodexError> {
        let line = self
            .stdout
            .next_line()
            .await?
            .ok_or_else(|| CodexError::Protocol("codex app-server closed stdout".into()))?;
        let payload: Value = serde_json::from_str(&line)?;

        if payload.get("method").is_some() && payload.get("id").is_some() {
            return Ok(IncomingMessage::Request {
                id: payload.get("id").cloned().unwrap_or(Value::Null),
                method: read_string(payload.get("method"))
                    .ok_or_else(|| CodexError::Protocol("request missing method".into()))?,
                _params: payload.get("params").cloned().unwrap_or(Value::Null),
            });
        }
        if payload.get("method").is_some() {
            return Ok(IncomingMessage::Notification {
                method: read_string(payload.get("method"))
                    .ok_or_else(|| CodexError::Protocol("notification missing method".into()))?,
                params: payload.get("params").cloned().unwrap_or(Value::Null),
            });
        }
        if payload.get("error").is_some() {
            let error = payload
                .get("error")
                .and_then(Value::as_object)
                .ok_or_else(|| CodexError::Protocol("error payload missing error object".into()))?;
            return Ok(IncomingMessage::Error {
                id: payload.get("id").cloned().unwrap_or(Value::Null),
                code: error.get("code").and_then(Value::as_i64).unwrap_or(-32000),
                message: read_string(error.get("message"))
                    .unwrap_or_else(|| "codex request failed".into()),
                data: error.get("data").cloned(),
            });
        }
        if payload.get("result").is_some() {
            return Ok(IncomingMessage::Response {
                id: payload.get("id").cloned().unwrap_or(Value::Null),
                result: payload.get("result").cloned().unwrap_or(Value::Null),
            });
        }

        Err(CodexError::Protocol(format!(
            "unrecognized codex message: {payload}"
        )))
    }

    async fn handle_unsolicited(&mut self, message: IncomingMessage) -> Result<(), CodexError> {
        match message {
            IncomingMessage::Request { id, method, .. }
                if method == "item/commandExecution/requestApproval" =>
            {
                self.send_response(id, json!({ "decision": "cancel" })).await
            }
            IncomingMessage::Request { id, method, .. }
                if method == "item/fileChange/requestApproval" =>
            {
                self.send_response(id, json!({ "decision": "cancel" })).await
            }
            IncomingMessage::Request { id, method, .. }
                if method == "item/tool/requestUserInput" =>
            {
                self.send_response(id, json!({ "answers": {} })).await
            }
            IncomingMessage::Request { id, method, .. } if method == "item/tool/call" => {
                self.send_response(
                    id,
                    json!({
                        "success": false,
                        "contentItems": [
                            {
                                "type": "inputText",
                                "text": "GHOST does not expose dynamic Codex tools through this bridge."
                            }
                        ]
                    }),
                )
                .await
            }
            IncomingMessage::Request { id, method, .. }
                if method == "account/chatgptAuthTokens/refresh" =>
            {
                self.send_error(id, -32601, "chatgptAuthTokens refresh is unsupported").await
            }
            IncomingMessage::Request { id, method, .. } => {
                self.send_error(
                    id,
                    -32601,
                    &format!("unsupported codex server request: {method}"),
                )
                .await
            }
            IncomingMessage::Notification { .. } | IncomingMessage::Response { .. } => Ok(()),
            IncomingMessage::Error {
                code,
                message,
                data,
                ..
            } => Err(CodexError::Server { code, message, data }),
        }
    }
}

pub async fn get_account_status() -> Result<CodexAccountStatus, CodexError> {
    let mut client = CodexClient::spawn().await?;
    client.read_account(false).await
}

pub async fn login_with_chatgpt(
    wait_for_completion: bool,
) -> Result<(CodexLoginStart, Option<CodexLoginCompletion>), CodexError> {
    let mut client = CodexClient::spawn().await?;
    let login = client.login_chatgpt().await?;
    let completion = match (&login, wait_for_completion) {
        (CodexLoginStart::Chatgpt { login_id, .. }, true) => Some(
            client
                .wait_for_login_completion(login_id, CODEX_LOGIN_WAIT)
                .await?,
        ),
        _ => None,
    };
    Ok((login, completion))
}

pub async fn login_with_api_key_env(
    api_key_env: &str,
) -> Result<(CodexLoginStart, CodexAccountStatus), CodexError> {
    let api_key = api_key_from_env(Some(api_key_env))
        .ok_or_else(|| CodexError::Auth(format!("environment variable {api_key_env} is empty")))?;
    let mut client = CodexClient::spawn().await?;
    let login = client.login_api_key(&api_key).await?;
    let account = client.read_account(false).await?;
    Ok((login, account))
}

pub async fn logout_account() -> Result<(), CodexError> {
    let mut client = CodexClient::spawn().await?;
    client.logout().await
}

pub async fn get_rate_limits() -> Result<CodexRateLimits, CodexError> {
    let mut client = CodexClient::spawn().await?;
    client.read_rate_limits().await
}

async fn run_codex_turn_collect(
    options: CodexExecutionOptions,
    messages: &[ChatMessage],
    tools: &[ToolSchema],
) -> Result<CompletionResult, CodexError> {
    let mut client = CodexClient::spawn().await?;
    client
        .ensure_account(options.api_key_env.as_deref())
        .await
        .map_err(|error| match error {
            LLMError::AuthFailed(message) => CodexError::Auth(message),
            other => CodexError::Protocol(other.to_string()),
        })?;

    let developer_instructions = developer_instructions_from_messages(messages, tools);
    let prompt = transcript_from_messages(messages, tools);
    let thread = client
        .start_thread(&options, developer_instructions.as_deref())
        .await?;
    let mut state = CodexTurnState::default();
    client
        .start_turn_streaming(&thread, &prompt, &mut state, false)
        .await?;

    Ok(CompletionResult {
        response: LLMResponse::Text(state.text),
        usage: state.usage,
        model: thread.model,
    })
}

fn codex_binary() -> String {
    std::env::var("GHOST_CODEX_BIN").unwrap_or_else(|_| "codex".into())
}

fn codex_working_dir() -> Result<PathBuf, CodexError> {
    let cwd = match std::env::var("GHOST_CODEX_CWD") {
        Ok(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => std::env::current_dir().map_err(|error| CodexError::Io(error.to_string()))?,
    };
    absolute_path(&cwd)
}

fn absolute_path(path: &Path) -> Result<PathBuf, CodexError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    let current_dir = std::env::current_dir().map_err(|error| CodexError::Io(error.to_string()))?;
    Ok(current_dir.join(path))
}

fn api_key_from_env(api_key_env: Option<&str>) -> Option<String> {
    api_key_env
        .and_then(crate::state::get_api_key)
        .filter(|value| !value.is_empty())
}

fn developer_instructions_from_messages(
    messages: &[ChatMessage],
    tools: &[ToolSchema],
) -> Option<String> {
    let mut sections: Vec<String> = messages
        .iter()
        .filter(|message| message.role == MessageRole::System && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_string())
        .collect();

    sections.push(
        "You are running inside GHOST through the Codex bridge. Stay inside the provided working directory and do not require outbound network access for shell actions.".into(),
    );
    if !tools.is_empty() {
        sections.push(
            "Ghost tool schemas were supplied for compatibility, but this bridge expects you to use the built-in Codex sandboxed workflow instead of emitting Ghost function calls.".into(),
        );
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

fn transcript_from_messages(messages: &[ChatMessage], _tools: &[ToolSchema]) -> String {
    let mut transcript = String::from(
        "Use the conversation transcript below as context. If code changes are needed, make them directly in the workspace and finish with a concise summary.\n",
    );

    for message in messages {
        if message.role == MessageRole::System {
            continue;
        }

        let role = match message.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::Tool => "Tool",
            MessageRole::System => continue,
        };
        transcript.push('\n');
        transcript.push_str(role);
        if let Some(tool_call_id) = message.tool_call_id.as_deref() {
            transcript.push_str(" (");
            transcript.push_str(tool_call_id);
            transcript.push(')');
        }
        transcript.push_str(":\n");
        transcript.push_str(message.content.trim());
        transcript.push('\n');

        if let Some(tool_calls) = &message.tool_calls {
            for tool_call in tool_calls {
                transcript.push_str("Tool Call ");
                transcript.push_str(&tool_call.name);
                transcript.push_str(" [");
                transcript.push_str(&tool_call.id);
                transcript.push_str("]: ");
                transcript.push_str(&tool_call.arguments.to_string());
                transcript.push('\n');
            }
        }
    }

    transcript
}

fn parse_account_status(value: &Value) -> CodexAccountStatus {
    let account = match value.get("account").and_then(Value::as_object) {
        Some(account) => match account.get("type").and_then(Value::as_str) {
            Some("apiKey") => Some(CodexAccount::ApiKey),
            Some("chatgpt") => Some(CodexAccount::Chatgpt {
                email: read_string(account.get("email")).unwrap_or_default(),
                plan_type: read_string(account.get("planType")).unwrap_or_else(|| "unknown".into()),
            }),
            _ => None,
        },
        None => None,
    };

    CodexAccountStatus {
        requires_openai_auth: value
            .get("requiresOpenaiAuth")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        account,
    }
}

fn parse_login_start(value: &Value) -> Result<CodexLoginStart, CodexError> {
    match value.get("type").and_then(Value::as_str) {
        Some("apiKey") => Ok(CodexLoginStart::ApiKey),
        Some("chatgpt") => Ok(CodexLoginStart::Chatgpt {
            auth_url: read_string(value.get("authUrl")).ok_or_else(|| {
                CodexError::Protocol("Codex login response missing authUrl".into())
            })?,
            login_id: read_string(value.get("loginId")).ok_or_else(|| {
                CodexError::Protocol("Codex login response missing loginId".into())
            })?,
        }),
        Some("chatgptAuthTokens") => Ok(CodexLoginStart::ChatgptAuthTokens),
        other => Err(CodexError::Protocol(format!(
            "unrecognized Codex login response: {other:?}"
        ))),
    }
}

fn parse_login_completion(value: &Value) -> Result<CodexLoginCompletion, CodexError> {
    Ok(CodexLoginCompletion {
        success: value
            .get("success")
            .and_then(Value::as_bool)
            .ok_or_else(|| CodexError::Protocol("login completion missing success".into()))?,
        error: read_string(value.get("error")),
        login_id: read_string(value.get("loginId")),
    })
}

fn parse_usage_from_notification(value: &Value) -> UsageStats {
    let breakdown = value
        .get("tokenUsage")
        .and_then(|usage| usage.get("last"))
        .and_then(Value::as_object);

    let input_tokens = breakdown
        .and_then(|tokens| tokens.get("inputTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let cached_input_tokens = breakdown
        .and_then(|tokens| tokens.get("cachedInputTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let output_tokens = breakdown
        .and_then(|tokens| tokens.get("outputTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let reasoning_tokens = breakdown
        .and_then(|tokens| tokens.get("reasoningOutputTokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let total_tokens = breakdown
        .and_then(|tokens| tokens.get("totalTokens"))
        .and_then(Value::as_u64)
        .unwrap_or((input_tokens + cached_input_tokens + output_tokens + reasoning_tokens) as u64)
        as usize;

    UsageStats {
        prompt_tokens: input_tokens + cached_input_tokens,
        completion_tokens: output_tokens + reasoning_tokens,
        total_tokens,
    }
}

fn extract_turn_text(value: &Value) -> String {
    let turns = value
        .get("thread")
        .and_then(|thread| thread.get("turns"))
        .and_then(Value::as_array);
    let Some(turn) = turns.and_then(|turns| turns.last()) else {
        return String::new();
    };
    let items = turn
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut agent_messages = Vec::new();
    let mut command_count = 0usize;
    let mut file_change_count = 0usize;

    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("agentMessage") => {
                if let Some(text) = read_string(item.get("text")) {
                    if !text.trim().is_empty() {
                        agent_messages.push(text);
                    }
                }
            }
            Some("commandExecution") => command_count += 1,
            Some("fileChange") => file_change_count += 1,
            _ => {}
        }
    }

    if !agent_messages.is_empty() {
        return agent_messages.join("\n");
    }

    match (command_count, file_change_count) {
        (0, 0) => String::new(),
        _ => format!(
            "Codex completed the turn with {command_count} command steps and {file_change_count} file changes."
        ),
    }
}

fn trailing_text_delta(existing: &str, final_text: &str) -> Option<String> {
    if final_text.is_empty() {
        return None;
    }
    if existing.is_empty() {
        return Some(final_text.to_string());
    }
    if let Some(delta) = final_text.strip_prefix(existing) {
        return (!delta.is_empty()).then(|| delta.to_string());
    }
    if final_text == existing {
        return None;
    }
    Some(format!("\n{final_text}"))
}

fn read_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_omits_system_messages_and_formats_roles() {
        let transcript = transcript_from_messages(
            &[
                ChatMessage {
                    role: MessageRole::System,
                    content: "internal".into(),
                    tool_calls: None,
                    tool_call_id: None,
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: "fix the failing test".into(),
                    tool_calls: None,
                    tool_call_id: None,
                },
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: "I will inspect the repo.".into(),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            &[],
        );

        assert!(transcript.contains("User:\nfix the failing test"));
        assert!(transcript.contains("Assistant:\nI will inspect the repo."));
        assert!(!transcript.contains("internal"));
    }

    #[test]
    fn extract_turn_text_prefers_agent_message_items() {
        let text = extract_turn_text(&json!({
            "thread": {
                "turns": [
                    {
                        "items": [
                            { "type": "commandExecution", "id": "1" },
                            { "type": "agentMessage", "id": "2", "text": "Done." }
                        ]
                    }
                ]
            }
        }));

        assert_eq!(text, "Done.");
    }

    #[test]
    fn trailing_delta_returns_suffix_only() {
        assert_eq!(
            trailing_text_delta("Hello", "Hello world").as_deref(),
            Some(" world")
        );
        assert!(trailing_text_delta("Hello", "Hello").is_none());
    }
}
