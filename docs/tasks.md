# GHOST Platform — Remaining Implementation Tasks

> 6 confirmed gaps between the current codebase and a functional autonomous agent.
> Ordered by dependency chain: each task unblocks the ones below it.
> Estimated effort is per-task, not cumulative.

---

## Task 1: LLM Provider HTTP Implementations

**Status:** Stubbed — all 5 providers return `LLMResponse::Empty`
**Crate:** `ghost-llm`
**Files:** `crates/ghost-llm/src/provider.rs`
**Effort:** Large
**Blocks:** Task 3 (agentic loop), Task 5 (heartbeat dispatch)

### Context

The `LLMProvider` trait, `ChatMessage`, `LLMToolCall`, `LLMResponse`, `UsageStats`, `CompletionResult`, and `TokenPricing` types are all defined and correct. The `FallbackChain` (auth rotation, circuit breaker, exponential backoff) is fully implemented and tested — it just wraps providers that return nothing. `reqwest` is already a workspace dependency.

### What to implement

**AnthropicProvider:**
- POST to `https://api.anthropic.com/v1/messages`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`
- Map `ChatMessage[]` → Anthropic's `messages` array format (role mapping: `System` → system param, `Tool` → tool_result content block)
- Map `ToolSchema[]` → Anthropic's `tools` array with `input_schema`
- Parse response: `content[]` blocks → `LLMResponse::Text`, `LLMResponse::ToolCalls`, or `LLMResponse::Mixed`
- Extract `usage.input_tokens` / `usage.output_tokens` → `UsageStats`
- Map HTTP 401 → `LLMError::AuthFailed`, 429 → `LLMError::RateLimited` (parse `retry-after` header), 529 → `LLMError::Unavailable`
- Use `self.api_key.read()` for the key (already RwLock-wrapped for FallbackChain rotation)

**OpenAIProvider:**
- POST to `https://api.openai.com/v1/chat/completions`
- Headers: `Authorization: Bearer {key}`, `Content-Type: application/json`
- Map `ChatMessage[]` → OpenAI messages format (role: system/user/assistant/tool, tool_calls array, tool_call_id)
- Map `ToolSchema[]` → OpenAI `tools` array with `function.parameters`
- Parse response: `choices[0].message.content` + `choices[0].message.tool_calls` → `LLMResponse` variants
- Extract `usage.prompt_tokens` / `usage.completion_tokens` → `UsageStats`
- Same error mapping as Anthropic (401/429/5xx)

**GeminiProvider:**
- POST to `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={key}`
- Map `ChatMessage[]` → Gemini `contents[]` with `parts[]` (role: user/model, no system role — prepend system as first user turn or use `systemInstruction`)
- Map `ToolSchema[]` → Gemini `tools[].functionDeclarations`
- Parse response: `candidates[0].content.parts[]` → text parts + functionCall parts → `LLMResponse`
- Extract `usageMetadata.promptTokenCount` / `candidatesTokenCount` → `UsageStats`

**OllamaProvider:**
- POST to `{self.base_url}/api/chat`
- OpenAI-compatible message format (Ollama supports it natively)
- Tool calling: Ollama supports OpenAI-format tool calling for compatible models
- No auth needed (local)
- Timeout should be longer (local inference is slower) — use 120s default

**OpenAICompatProvider:**
- Same as OpenAI but POST to `{self.base_url}/v1/chat/completions`
- Already has `base_url` and `context_window_size` fields

### Streaming (follow-up, not blocking)

The `StreamChunk` enum and `StreamingResponse` type exist. SSE parsing can be added after the synchronous path works. For Anthropic this means parsing `event: content_block_delta` / `event: message_stop` lines. For OpenAI/Ollama it means parsing `data: {"choices":[{"delta":...}]}` lines.

### Testing approach

- Unit tests with recorded HTTP responses (use `serde_json::from_str` on fixture JSON, no live API calls in CI)
- One `#[ignore]` integration test per provider that makes a real API call (run manually with env vars)
- Test error mapping: 401 → AuthFailed, 429 → RateLimited, timeout → Timeout
- Test that `FallbackChain` rotates providers correctly when first returns error

---

## Task 2: Wire Tool Dispatch to Builtin Implementations

**Status:** `ToolExecutor::dispatch()` returns hardcoded JSON string
**Crate:** `ghost-agent-loop`
**Files:** `crates/ghost-agent-loop/src/tools/executor.rs`, `crates/ghost-agent-loop/src/tools/builtin/*.rs`
**Effort:** Medium
**Blocks:** Task 3 (agentic loop needs working tools)

### Context

The builtin tools already have real implementations:
- `filesystem.rs` — `FilesystemTool` with `read_file`, `write_file`, `list_dir` (path traversal protection works)
- `shell.rs` — `execute_shell` with capability-scoped prefix checking and timeout
- `memory.rs` — `read_memories` with substring matching against snapshot
- `web_search.rs` — stub (returns empty), needs HTTP call to search API

The `ToolRegistry` stores `RegisteredTool` entries with name, schema, capability, and timeout. The `ToolExecutor` already does timeout enforcement and plan validation.

### What to implement

1. Add a `ToolDispatcher` enum or trait object that the executor holds:

```rust
// In executor.rs
pub struct ToolExecutor {
    default_timeout: Duration,
    plan_validator: PlanValidator,
    filesystem: Option<FilesystemTool>,
    shell_config: ShellToolConfig,
    // snapshot reference set per-run for memory tool
}
```

2. Replace the stub `dispatch()` body with a match on `call.name`:

```rust
async fn dispatch(&self, call: &LLMToolCall, tool: &RegisteredTool) -> Result<String, ToolError> {
    match call.name.as_str() {
        "read_file" => { /* extract path from call.arguments, call self.filesystem.read_file() */ }
        "write_file" => { /* extract path+content, call self.filesystem.write_file() */ }
        "list_dir" => { /* extract path, call self.filesystem.list_dir() */ }
        "shell" => { /* extract command, call execute_shell() */ }
        "memory_read" => { /* extract query+limit, call read_memories() */ }
        "web_search" => { /* extract query, call search() */ }
        _ => Err(ToolError::NotFound(call.name.clone()))
    }
}
```

3. Register the builtin tools in `ToolRegistry` at startup with proper `ToolSchema` JSON schemas (parameter names, types, descriptions matching what the LLM expects).

4. For `web_search`: implement the actual HTTP call in `web_search.rs`. Support SearXNG (self-hosted, JSON API) as the default backend. The config already has `api_url` — just do a GET to `{api_url}?q={query}&format=json` and parse the results.

### Testing approach

- Unit test each dispatch arm with mock `LLMToolCall` arguments
- Test path traversal rejection in filesystem tool (already has tests, verify they pass through dispatch)
- Test shell capability scoping through dispatch
- Integration test: register tools → dispatch a call → verify output

---

## Task 3: Agentic Loop (Recursive Tool-Call Cycle)

**Status:** `AgentRunner` has `pre_loop` (setup) and `check_gates` but no execution loop
**Crate:** `ghost-agent-loop`
**Files:** `crates/ghost-agent-loop/src/runner.rs`
**Effort:** Large
**Blocks:** Task 5 (heartbeat needs to trigger turns), Task 6 (channels need to deliver responses)
**Depends on:** Task 1 (LLM providers), Task 2 (tool dispatch)

### Context

`pre_loop` does 11 steps of setup and returns a `RunContext`. `check_gates` validates all 6 gates. `PromptCompiler::compile()` produces the 10-layer prompt. `ToolExecutor::execute()` runs tools with timeout. `ProposalExtractor` and `ProposalRouter` exist in `proposal/`. The `OutputInspector` scans for credential exfiltration. The `FlushExecutor` trait exists for session compaction callbacks.

### What to implement

Add `pub async fn run_turn(&mut self, ctx: &mut RunContext, ...) -> Result<RunResult, RunError>`:

```
loop {
    1. check_gates(ctx) — bail on any gate failure
    2. compile prompt via PromptCompiler::compile()
    3. call LLM via FallbackChain::complete(messages, tool_schemas)
    4. update ctx.total_tokens and ctx.total_cost from UsageStats
    5. record circuit breaker success/failure
    6. match response:
       LLMResponse::Text(text) =>
         a. OutputInspector::scan() — if KillAll, trigger kill switch and return
         b. if Warning, use redacted_text
         c. extract proposals via ProposalExtractor
         d. route proposals via ProposalRouter
         e. return RunResult with final text
       LLMResponse::ToolCalls(calls) =>
         a. ToolExecutor::validate_plan(calls) — if denied, record denial, return error text to LLM
         b. for each call: check policy, execute, collect ToolResult
         c. track damage_counter for destructive tools (write_file, shell)
         d. append tool results as Tool-role messages to conversation
         e. increment ctx.recursion_depth
         f. continue loop (next iteration re-prompts LLM with tool results)
       LLMResponse::Mixed { text, tool_calls } =>
         a. process text (inspect, extract proposals)
         b. process tool_calls (same as ToolCalls branch)
         c. continue loop
       LLMResponse::Empty =>
         a. return RunResult with empty response (NO_REPLY)
    7. emit ITP events for each action
}
```

Key invariants to maintain:
- `RunContext.snapshot` is immutable for the entire run (INV-PRE-06)
- Gate checks happen EVERY iteration, not just the first
- `recursion_depth` increments per tool-call round-trip, checked by GATE 1
- `total_cost` accumulates across iterations, checked by GATE 2
- Kill switch is checked every iteration (GATE 3)
- Tool results are appended to the message history as `MessageRole::Tool` with `tool_call_id`

### Testing approach

- Unit test with a mock `LLMProvider` that returns `ToolCalls` on first call, `Text` on second
- Test gate enforcement: mock provider that always returns tool calls → verify recursion depth limit triggers
- Test kill switch mid-loop: set kill switch after first iteration → verify clean exit
- Test credential exfiltration: mock provider returns text with `sk-...` pattern → verify KillAll
- Test spending cap: mock provider with high token counts → verify cap triggers

---

## Task 4: Gateway Server Startup (Mount Axum Routes)

**Status:** `Gateway::run()` only waits for ctrl+c; `step5_start_api` is a no-op
**Crate:** `ghost-gateway`
**Files:** `crates/ghost-gateway/src/bootstrap.rs`, `crates/ghost-gateway/src/gateway.rs`
**Effort:** Medium
**Depends on:** None (can be done in parallel with Tasks 1-3)

### Context

All the axum route handlers exist:
- `api/health.rs` — `health_handler`, `ready_handler`
- `api/agents.rs` — `list_agents`
- `api/audit.rs` — `query_audit`, `audit_aggregation`, `audit_export`
- `api/convergence.rs`, `api/goals.rs`, `api/sessions.rs`, `api/safety.rs` — various endpoints
- `api/websocket.rs` — `ws_handler` with keepalive loop
- `api/mesh_routes.rs` — `mesh_router()` returns a complete axum Router with Ed25519 auth
- `api/oauth_routes.rs`, `api/push_routes.rs` — additional endpoints

The `GhostConfig` struct exists in `config.rs`. The `GatewaySharedState` FSM is implemented.

### What to implement

1. In `bootstrap.rs` `step5_start_api`, build the axum Router:

```rust
fn step5_start_api(config: &GhostConfig, shared_state: &GatewaySharedState) -> Result<Router, BootstrapError> {
    let app = Router::new()
        .route("/api/health", get(api::health::health_handler))
        .route("/api/ready", get(api::health::ready_handler))
        .route("/api/agents", get(api::agents::list_agents))
        .route("/api/audit", get(api::audit::query_audit))
        .route("/api/audit/aggregation", get(api::audit::audit_aggregation))
        .route("/api/audit/export", get(api::audit::audit_export))
        .route("/api/ws", get(api::websocket::ws_handler))
        // ... remaining routes
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());
    Ok(app)
}
```

2. In `Gateway::run()`, bind and serve:

```rust
pub async fn run(self, router: Router, bind_addr: &str) -> Result<(), GatewayError> {
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %bind_addr, "Gateway listening");
    self.shared_state.transition_to(GatewayState::Healthy)?;

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;

    self.shared_state.transition_to(GatewayState::ShuttingDown)?;
    Ok(())
}
```

3. Wire shared state (DB connections, agent registry, config) into axum via `Extension` or `State` extractor so route handlers can access backing data instead of returning empty results.

4. Default bind address: `127.0.0.1:18789` (loopback only, matching OpenClaw convention). Configurable via `ghost.yml` or `--bind` CLI flag.

### Testing approach

- Integration test: start gateway on random port, hit `/api/health`, verify 200 + JSON
- Test graceful shutdown: start gateway, send SIGINT, verify clean exit
- Test WebSocket: connect to `/api/ws`, verify ping received within 30s

---

## Task 5: Heartbeat Agent Turn Dispatch

**Status:** `HeartbeatEngine::should_fire()` works but nothing calls the agent loop
**Crate:** `ghost-heartbeat` + `ghost-gateway`
**Files:** `crates/ghost-heartbeat/src/heartbeat.rs`, `crates/ghost-gateway/src/periodic.rs`
**Effort:** Small
**Depends on:** Task 3 (agentic loop must exist to dispatch into)

### Context

`HeartbeatEngine` has:
- Convergence-aware tiered frequency (Stable→120s, Active→30s, Escalated→15s, Critical→5s)
- Dedicated session key via `heartbeat_session_key(agent_id)`
- Cost ceiling tracking
- Active hours / timezone support
- Kill switch and agent pause checks
- The synthetic message: `"[HEARTBEAT] Check HEARTBEAT.md and act if needed."`

The gateway's `periodic.rs` module exists but needs the dispatch wiring.

### What to implement

1. Add `pub async fn fire(&mut self, runner: &mut AgentRunner) -> Result<(), HeartbeatError>` to `HeartbeatEngine`:

```rust
pub async fn fire(&mut self, runner: &mut AgentRunner) -> Result<(), HeartbeatError> {
    let ctx = runner.pre_loop(
        self.agent_id,
        self.session_key,
        "heartbeat",
        HEARTBEAT_MESSAGE,
    ).await?;

    let result = runner.run_turn(&mut ctx, ...).await;

    match &result {
        Ok(run_result) => self.record_beat_with_score(run_result.cost, run_result.convergence_score),
        Err(_) => self.record_beat(0.0),
    }

    result.map(|_| ())
}
```

2. In `ghost-gateway/src/periodic.rs`, spawn a tokio task per agent that runs the heartbeat loop:

```rust
async fn heartbeat_loop(mut engine: HeartbeatEngine, runner: Arc<Mutex<AgentRunner>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5)); // check frequency
    loop {
        interval.tick().await;
        if engine.should_fire(current_convergence_level) {
            let mut runner = runner.lock().await;
            if let Err(e) = engine.fire(&mut runner).await {
                tracing::warn!(agent_id = %engine.agent_id, error = %e, "heartbeat failed");
            }
        }
    }
}
```

3. The `CronEngine` in `crates/ghost-heartbeat/src/cron.rs` needs the same dispatch pattern — cron jobs should also trigger agent turns.

### Testing approach

- Unit test: mock AgentRunner, verify `fire()` calls `pre_loop` with heartbeat session key and synthetic message
- Test cost ceiling: fire repeatedly until ceiling → verify `should_fire()` returns false
- Test convergence tier escalation: set high convergence → verify shorter intervals

---

## Task 6: Channel Adapter Implementations

**Status:** 5 of 6 adapters are stubs; CLI adapter has basic working send/receive
**Crate:** `ghost-channels`
**Files:** `crates/ghost-channels/src/adapters/*.rs`
**Effort:** Large (per adapter), but can be incremental
**Depends on:** Task 3 (need an agentic loop to feed messages into), Task 4 (gateway serves WebSocket)

### Context

The `ChannelAdapter` trait is defined with `connect`, `disconnect`, `send`, `receive`, `supports_streaming`, `supports_editing`, `channel_type`. The `InboundMessage` and `OutboundMessage` types exist in `types.rs`. The `streaming.rs` module has chunk-based delivery via `StreamingFormatter` (throttled flush). The CLI adapter already has a working `receive()` (reads from `std::io::stdin`) and `send()` (prints to stdout via `println!`). The gateway's `cli/chat.rs` has a working REPL with `/quit`, `/help`, `/status`, `/model` commands but doesn't yet dispatch user messages to the agent loop.

### Priority order (implement one at a time)

**Phase A — CLI adapter → agent loop wiring (highest priority, easiest):**
- The CLI adapter's `receive()` and `send()` already work (sync stdin/stdout). What's missing is wiring `cli/chat.rs` to call `AgentRunner::pre_loop()` + `run_turn()` instead of printing `"[Chat requires a running gateway]"`.
- Add streaming support: use `StreamingFormatter` to print chunks as they arrive instead of waiting for full response.
- This gives you an interactive `ghost chat` command immediately.

**Phase B — WebSocket adapter:**
- The gateway already has `api/websocket.rs` with a working handler.
- The adapter needs to accept WebSocket connections and bridge them to the agent loop.
- Inbound: parse JSON messages from WebSocket → `InboundMessage`.
- Outbound: serialize `OutboundMessage` → JSON → WebSocket text frame.
- This enables web UI clients and the companion app pattern.

**Phase C — Telegram adapter:**
- Use Telegram Bot API (HTTPS, no special libraries needed in Rust).
- Long polling: `GET https://api.telegram.org/bot{token}/getUpdates?offset={last_update_id+1}`
- Send: `POST https://api.telegram.org/bot{token}/sendMessage` with `chat_id` + `text`
- Parse `Update` → extract `message.text`, `message.chat.id`, `message.from.id` → `InboundMessage`
- Support `reply_to_message_id` for threaded conversations.

**Phase D — Discord adapter:**
- Discord Gateway WebSocket for receiving events + REST API for sending.
- Connect to `wss://gateway.discord.gg/?v=10&encoding=json`
- Handle HELLO → send IDENTIFY with bot token → receive READY
- Listen for MESSAGE_CREATE events → `InboundMessage`
- Send via `POST https://discord.com/api/v10/channels/{id}/messages`
- Mention-based activation: only respond when bot is @mentioned.

**Phase E — Slack adapter:**
- Socket Mode (WebSocket) for receiving events + Web API for sending.
- Connect via `apps.connections.open` → WebSocket URL
- Listen for `event_callback` with `message` type → `InboundMessage`
- Send via `chat.postMessage` REST endpoint.

**Phase F — WhatsApp adapter:**
- Most complex. OpenClaw uses Baileys (Node.js WhatsApp Web protocol).
- For Rust: either shell out to a Baileys sidecar (the approach already sketched in `whatsapp.rs` with restart logic) or use the WhatsApp Cloud API (official, simpler, requires Meta business account).
- Cloud API approach: webhook receiver for inbound + REST POST for outbound.
- Sidecar approach: spawn Node.js process, communicate via stdin/stdout JSON-RPC.

### Testing approach

- CLI: integration test that pipes stdin/stdout and verifies round-trip
- WebSocket: connect with `tokio-tungstenite`, send message, verify response
- Telegram/Discord/Slack: mock HTTP servers that simulate the platform APIs
- WhatsApp: test sidecar restart logic (already partially tested)

---

## Dependency Graph

```
Task 1 (LLM Providers)  ──┐
                           ├──→ Task 3 (Agentic Loop) ──→ Task 5 (Heartbeat Dispatch)
Task 2 (Tool Dispatch)  ──┘                           ──→ Task 6 (Channel Adapters)

Task 4 (Gateway Server) ── independent, do in parallel
```

**Critical path:** Task 1 → Task 3 → working autonomous agent with CLI.
**Parallel track:** Task 4 (gateway) + Task 2 (tools) can happen simultaneously.
**After critical path:** Task 5 (heartbeat) and Task 6 (channels beyond CLI) are incremental.

---

## Out of Scope (documented but not blocking autonomy)

These are real gaps but don't block a functional agent:

- **WASM sandbox execution** — `wasmtime` instantiation in `ghost-skills/src/sandbox/wasm_sandbox.rs`. Needed for untrusted skill execution but native tools work without it.
- **Vector/embedding search** — `cortex-retrieval` needs an embedding pipeline + SQLite-vec storage. Current substring matching in the memory tool works for MVP.
- **Browser automation** — No browser tool exists. Would be a new module in `ghost-agent-loop/src/tools/builtin/browser.rs` using headless Chrome via CDP or Playwright equivalent.
- **SSE streaming from LLM providers** — Synchronous completion works first; streaming is a UX improvement.
- **Audit endpoint backing data** — Route handlers exist but return empty results. Wire to `ghost-audit::AuditQueryEngine` + `cortex-storage` SQLite tables.
