# GHOST Platform — Comprehensive Remediation Plan

> A 0.0001-engineer-grade audit and step-by-step plan to take every hollow facade,
> stub, and broken wiring in this codebase to production standard.
>
> Findings are ordered by severity, then by dependency chain.
> Each finding includes: what's broken, why it matters, and the exact fix.

---

## Executive Summary: The Facade Problem

This codebase has a **well-designed architecture** with a **hollow interior**. The type
system, trait boundaries, error types, and module structure are all correct. But when
you trace the actual data flow from user input to API response, you hit dead ends
everywhere. The pattern is consistent:

1. Route handlers exist and return well-structured JSON — but with hardcoded empty data
2. Shared state (`GatewaySharedState`) is created during bootstrap but **never passed
   to route handlers** via axum's `State` extractor
3. Safety-critical endpoints (kill switch, pause, quarantine) log the request and
   return success JSON — but **never actually mutate any state**
4. The health endpoint says "Healthy" even when the gateway is in Degraded mode
5. Channel adapters have correct trait implementations but `receive()` returns errors
   for everything except CLI
6. The convergence monitor's signal computer uses cached values from `set_signal()`
   instead of computing from actual data
7. Integration tests validate individual components in isolation but never test the
   full wired-up system end-to-end

The net effect: **you can boot the gateway, hit every endpoint, get valid JSON back,
and believe the system works — but nothing is actually connected to anything.**

---

## Severity Classification

- **S0 — BROKEN**: Returns wrong data, lies about state, or silently fails
- **S1 — DISCONNECTED**: Implementation exists but isn't wired to the data layer
- **S2 — STUB**: Acknowledged placeholder, returns error or empty data
- **S3 — DEFERRED**: Intentionally postponed (Phase 9, etc.), not blocking MVP

---

## Finding 1: Health Endpoint Lies About Gateway State [S0]

**File:** `crates/ghost-gateway/src/api/health.rs`
**Impact:** Monitoring, load balancers, and operators get false "Healthy" status

### What's broken

```rust
pub async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "alive",
        "state": "Healthy"  // HARDCODED — ignores actual GatewayState
    })))
}
```

The comment even says: *"In production, this would check GatewaySharedState via axum
State extractor."* The `GatewaySharedState` exists, has a working FSM with 6 states,
is created in bootstrap — but is never injected into the router.

### Why it matters

- Kubernetes liveness/readiness probes will never detect degraded mode
- Operators monitoring `/api/health` get false confidence
- The gateway can be in `Degraded`, `Recovering`, or even `FatalError` and still
  report "Healthy"

### Fix

1. Add `GatewaySharedState` (wrapped in `Arc`) as axum `State` to the router
2. In `build_router()`, accept `Arc<GatewaySharedState>` and pass it via `.with_state()`
3. Health handler reads `shared_state.current_state()` and returns:
   - `Initializing` → 503 + `"state": "Initializing"`
   - `Healthy` → 200 + `"state": "Healthy"`
   - `Degraded` → 200 + `"state": "Degraded"` (still alive, but degraded)
   - `Recovering` → 200 + `"state": "Recovering"`
   - `ShuttingDown` → 503 + `"state": "ShuttingDown"`
   - `FatalError` → 503 + `"state": "FatalError"`
4. Ready handler returns 503 for anything except `Healthy`

### Estimated effort: Small (1-2 hours)

---

## Finding 2: No Shared State Injected Into Any Route Handler [S0]

**File:** `crates/ghost-gateway/src/bootstrap.rs` (build_router)
**Impact:** Every single API endpoint is disconnected from the gateway's runtime state

### What's broken

`build_router()` constructs an `axum::Router` with 20+ routes but **never calls
`.with_state()`** on the main router. The `GatewaySharedState` is created in
`GatewayBootstrap::run()` and stored in the `Gateway` struct, but the router is
built without it.

This means:
- No route handler has access to the agent registry
- No route handler has access to the kill switch state
- No route handler has access to the convergence scores
- No route handler has access to the audit database connection
- No route handler has access to the session store

### Why it matters

This is the **root cause** of Findings 1, 3, 4, 5, 6, 7, 8, and 9. Every empty
JSON response traces back to this: the handlers don't have access to the data.

### Fix

1. Define an `AppState` struct that holds everything route handlers need:

```rust
pub struct AppState {
    pub shared_state: Arc<GatewaySharedState>,
    pub agent_registry: Arc<RwLock<AgentRegistry>>,
    pub kill_switch: Arc<AtomicBool>,
    pub db_pool: Arc<rusqlite::Connection>,  // or a pool
    pub convergence_cache: Arc<RwLock<ConvergenceCache>>,
    pub session_store: Arc<RwLock<SessionStore>>,
}
```

2. Create `AppState` in `GatewayBootstrap::run()` after all init steps
3. Pass `Arc<AppState>` to `build_router()` and call `.with_state(state)`
4. Update every route handler signature to accept `State(state): State<Arc<AppState>>`

### Estimated effort: Medium (4-6 hours) — this is the keystone fix

---

## Finding 3: Safety Endpoints Don't Mutate State [S0]

**Files:** `crates/ghost-gateway/src/api/safety.rs`
**Impact:** Kill switch, pause, quarantine, and resume are all no-ops

### What's broken

Every safety endpoint logs the request and returns success JSON, but:
- `kill_all()` never calls `PLATFORM_KILLED.store(true, ...)` or `KillSwitch::activate_kill_all()`
- `pause_agent()` never calls `KillSwitch::activate_agent()` with `KillLevel::Pause`
- `quarantine_agent()` never calls `KillSwitch::activate_agent()` with `KillLevel::Quarantine`
- `resume_agent()` never verifies auth or calls `KillSwitch::resume_agent()`
- `safety_status()` returns hardcoded `"platform_level": "Normal"` instead of reading
  from `KillSwitch::current_state()`

The comments in each handler describe exactly what should happen — the code just
doesn't do it.

### Why it matters

- An operator hitting "Kill All" in the dashboard gets a success response but
  **agents keep running**
- There is no way to actually stop a misbehaving agent via the API
- The safety status endpoint always says "Normal" regardless of actual state
- This is a **safety-critical failure** — the kill switch is the last line of defense

### Fix

1. After Finding 2 is resolved (AppState injection), each handler gets access to
   the kill switch and agent registry
2. `kill_all()`:
   - Call `PLATFORM_KILLED.store(true, Ordering::SeqCst)`
   - Write `kill_state.json` to disk (for persistence across restarts)
   - Emit ITP event for audit trail
   - Dispatch notification via `NotificationDispatcher`
3. `pause_agent()`:
   - Look up agent in registry, set its state to `Paused`
   - Set agent-level kill flag
4. `quarantine_agent()`:
   - Same as pause but with `Quarantined` state
   - Freeze agent's filesystem sandbox
5. `resume_agent()`:
   - Verify GHOST_TOKEN auth (currently no auth check at all)
   - For quarantine: require `forensic_reviewed` AND `second_confirmation`
   - Clear agent-level kill flag
6. `safety_status()`:
   - Read from `PLATFORM_KILLED` atomic and per-agent kill states
   - Return actual state

### Estimated effort: Medium (4-6 hours)

---

## Finding 4: Goal Approval/Rejection Are No-Ops [S0]

**File:** `crates/ghost-gateway/src/api/goals.rs`
**Impact:** Proposal approval workflow is completely broken

### What's broken

```rust
pub async fn approve_goal(Path(id): Path<String>) -> impl IntoResponse {
    tracing::info!(goal_id = %id, "Goal approval requested");
    (StatusCode::OK, Json(serde_json::json!({"status": "approved", "id": id})))
}
```

The comment says: *"Check if proposal is still pending (resolved_at IS NULL). If
already resolved → 409 Conflict (AC6)."* None of this happens.

### Why it matters

- The entire human-in-the-loop approval flow is broken
- Proposals that require human approval will never actually get approved/rejected
  in the system — the agent loop won't see the decision
- No idempotency check (approving an already-approved proposal should 409)

### Fix

1. Query the proposals table for the given ID
2. Check `resolved_at IS NULL` — if already resolved, return 409
3. Update `resolved_at`, `decision`, `decided_by` columns
4. Push a `WsEvent::ProposalDecision` to connected WebSocket clients
5. If the agent is waiting on this proposal, unblock it

### Estimated effort: Small-Medium (2-4 hours)

---

## Finding 5: Agents Endpoint Returns Empty Vec [S1]

**File:** `crates/ghost-gateway/src/api/agents.rs`
**Impact:** No visibility into running agents

### What's broken

```rust
pub async fn list_agents() -> Json<Vec<AgentInfo>> {
    Json(Vec::new())
}
```

13 lines total. No state access, no database query, no nothing.

### Fix

1. Accept `State(state): State<Arc<AppState>>`
2. Read from `state.agent_registry` (which is populated during step 4 of bootstrap)
3. Map each `RegisteredAgent` to `AgentInfo` with name, status, agent_id, isolation
   mode, spending, convergence score

### Estimated effort: Small (1 hour)

---

## Finding 6: Convergence Scores Endpoint Returns Empty Vec [S1]

**File:** `crates/ghost-gateway/src/api/convergence.rs`
**Impact:** No visibility into agent convergence state

### What's broken

Same pattern as agents — 14 lines, returns `Json(Vec::new())`.

### Fix

1. Accept `State(state): State<Arc<AppState>>`
2. Read from `state.convergence_cache` (populated by convergence monitor updates)
3. Return per-agent scores with all 7 signal dimensions

### Estimated effort: Small (1 hour)

---

## Finding 7: Sessions Endpoint Returns Empty Vec [S1]

**File:** `crates/ghost-gateway/src/api/sessions.rs`
**Impact:** No session visibility

### What's broken

7 lines. Returns `Json(Vec::new())`.

### Fix

1. Accept `State(state): State<Arc<AppState>>`
2. Query the sessions table from the SQLite database
3. Return session ID, agent ID, channel, created_at, message count

### Estimated effort: Small (1 hour)

---

## Finding 8: Audit Endpoints Return Empty Data [S1]

**File:** `crates/ghost-gateway/src/api/audit.rs`
**Impact:** No audit trail visibility — compliance failure

### What's broken

All three audit endpoints return empty arrays/zero totals:
- `query_audit()` → `"entries": [], "total": 0`
- `audit_aggregation()` → `"violations_per_day": [], "total_entries": 0`
- `audit_export()` → empty CSV/JSON/JSONL

The `ghost-audit` crate has a working `QueryEngine` with SQLite queries, aggregation,
and export — it's just never called from these handlers.

### Fix

1. Accept `State(state): State<Arc<AppState>>`
2. `query_audit()`: Call `ghost_audit::query_engine::QueryEngine::query()` with the
   filter params, passing `state.db_pool`
3. `audit_aggregation()`: Call `ghost_audit::aggregation::aggregate()` with the
   time range and agent filter
4. `audit_export()`: Call `ghost_audit::export::export()` with format param

### Estimated effort: Small-Medium (2-3 hours)

---

## Finding 9: Memory Endpoints Return Empty Data [S1]

**File:** `crates/ghost-gateway/src/api/memory.rs`
**Impact:** No memory inspection capability

### What's broken

`list_memories()` returns empty array. `get_memory()` always returns 404.

### Fix

1. Accept `State(state): State<Arc<AppState>>`
2. Query cortex-storage for memories matching the filter params
3. `get_memory()`: Look up by ID, return 404 only if actually not found

### Estimated effort: Small (1-2 hours)

---

## Finding 10: OAuth Routes Are Complete Stubs [S1]

**File:** `crates/ghost-gateway/src/api/oauth_routes.rs`
**Impact:** No OAuth integration works

### What's broken

- `list_providers()` returns hardcoded JSON with 4 providers — not from config
- `connect()` returns a fake `example.com` authorization URL
- `callback()` returns success without exchanging the code for tokens
- `list_connections()` returns empty array
- `disconnect()` logs and returns success without revoking anything

The `ghost-oauth` crate has an `OAuthBroker` with real PKCE flow, token storage,
and refresh logic — none of it is called.

### Fix

1. Accept `State(state): State<Arc<AppState>>` (AppState needs `OAuthBroker`)
2. `list_providers()`: Call `broker.list_providers()` which reads from config
3. `connect()`: Call `broker.initiate_flow(provider, scopes)` which generates
   real PKCE challenge and authorization URL
4. `callback()`: Call `broker.exchange_code(state, code)` which validates CSRF,
   exchanges code for tokens, stores encrypted tokens
5. `list_connections()`: Call `broker.list_connections()` which reads from DB
6. `disconnect()`: Call `broker.revoke(ref_id)` which revokes token and deletes

### Estimated effort: Medium (3-4 hours)

---

## Finding 11: WebSocket Handler Only Sends Pings [S2]

**File:** `crates/ghost-gateway/src/api/websocket.rs`
**Impact:** No real-time event streaming to dashboard/clients

### What's broken

The WebSocket handler:
- Accepts upgrade correctly
- Sends keepalive pings every 30s
- Receives and logs client messages
- **Never subscribes to any event source**
- **Never pushes ScoreUpdate, InterventionChange, KillSwitchActivation, or
  ProposalDecision events**

The `WsEvent` enum is well-defined with all the right variants — they're just
never constructed or sent.

### Fix

1. Create a `tokio::sync::broadcast` channel for gateway events
2. Store the broadcast `Sender` in `AppState`
3. When safety, convergence, or proposal state changes, send events to the broadcast
4. In `handle_socket()`, subscribe to the broadcast receiver
5. In the `tokio::select!` loop, add a branch for broadcast events:
   ```rust
   event = rx.recv() => {
       let json = serde_json::to_string(&event).unwrap();
       socket.send(Message::Text(json)).await?;
   }
   ```

### Estimated effort: Medium (3-4 hours)

---

## Finding 12: Channel Adapters — Only CLI Works [S2]

**Files:** `crates/ghost-channels/src/adapters/*.rs`
**Impact:** No multi-channel communication

### Status per adapter

| Adapter    | `connect()` | `send()` | `receive()` | Status |
|------------|-------------|----------|-------------|--------|
| CLI        | ✅ Works    | ✅ Works | ✅ Works    | Production-ready |
| WebSocket  | ✅ Sets flag| ⚠️ Serializes but doesn't send | ❌ Queue-only | Needs socket wiring |
| Telegram   | ✅ Sets flag| ✅ HTTP POST works | ✅ Long polling works | **Nearly complete** |
| Discord    | ✅ Sets flag| ✅ HTTP POST works | ❌ "not yet connected" | Needs Gateway WS |
| Slack      | ✅ Sets flag| ✅ HTTP POST works | ❌ "not yet connected" | Needs Socket Mode |
| WhatsApp   | ✅ Sets flag| ✅ Cloud API works / ❌ Sidecar stub | ❌ Queue-only | Cloud API send works |

### Key insight

Telegram is actually **nearly functional** — both send and receive have real HTTP
implementations. The `send()` methods for Discord, Slack, and WhatsApp Cloud API
also make real HTTP calls. The gap is primarily in `receive()` for Discord (needs
Gateway WebSocket) and Slack (needs Socket Mode WebSocket).

### Fix priority

1. **Telegram** — Already works. Just needs integration testing.
2. **WebSocket** — Wire `send()` to actual socket reference (broadcast channel)
3. **Slack** — Implement Socket Mode: call `apps.connections.open`, connect to
   returned WebSocket URL, parse `event_callback` messages
4. **Discord** — Implement Gateway WebSocket: connect to `wss://gateway.discord.gg`,
   handle HELLO→IDENTIFY→READY handshake, filter MESSAGE_CREATE events
5. **WhatsApp** — Cloud API send works; add webhook receiver for inbound

### Estimated effort: Large (2-3 days for all; Telegram is 0 effort)

---

## Finding 13: Signal Computer Returns Cached Zeros [S2]

**File:** `crates/convergence-monitor/src/pipeline/signal_computer.rs`
**Impact:** Convergence scores are always 0.0 unless manually set

### What's broken

```rust
pub fn compute(&mut self, agent_id: Uuid) -> [f64; 8] {
    // ...
    for i in 0..8 {
        if entry.dirty[i] {
            // Signal stubs: return cached value (real impl in cortex-convergence).
            entry.dirty[i] = false;
            recomputed_count += 1;
        }
    }
    entry.values  // Returns whatever was set via set_signal(), defaults to [0.0; 8]
}
```

The `compute()` method marks dirty flags as clean but **never actually computes
anything**. It returns whatever was previously set via `set_signal()`. If nothing
was set, it returns `[0.0; 8]`.

### Why it matters

- Convergence monitoring is the core safety mechanism
- Without real signal computation, intervention levels never escalate
- The 7-signal convergence score (goal drift, behavioral entropy, resource
  consumption, etc.) is always zero
- The tiered intervention system (Observe → Constrain → Intervene → Halt) never
  triggers

### Fix

The `cortex-convergence` crate has the actual signal computation logic. The signal
computer needs to:

1. For each dirty signal, call the corresponding cortex-convergence function:
   - Signal 0 (Goal Drift): `cortex_convergence::signals::goal_drift()`
   - Signal 1 (Behavioral Entropy): `cortex_convergence::signals::behavioral_entropy()`
   - Signal 2 (Resource Consumption): `cortex_convergence::signals::resource_consumption()`
   - etc.
2. Pass the agent's session data (from cortex-storage) to each signal function
3. Store the computed value in the cache
4. The convergence monitor's main loop should call `mark_dirty()` when new session
   data arrives, then `compute()` to get updated scores

### Estimated effort: Medium (4-6 hours)

---

## Finding 14: eBPF Egress Enforcement Always Falls Back to Proxy [S2]

**File:** `crates/ghost-egress/src/ebpf_provider.rs`
**Impact:** No kernel-level network isolation

### What's broken

```rust
fn try_load_ebpf(_agent_id: &Uuid, _allowed_ips: &[IpAddr]) -> bool {
    false // Always fall back in this stub — real impl uses Aya
}
```

The fallback to `ProxyEgressPolicy` works correctly, so network isolation still
happens — just at the application layer (HTTP proxy) instead of the kernel layer
(eBPF/cgroup). This is acceptable for MVP but means a malicious agent could bypass
the proxy by making raw TCP connections.

### Fix

1. Add `aya` and `aya-log` as optional dependencies behind an `ebpf` feature flag
2. Compile the eBPF program (cgroup/skb filter) that allows only IPs in a BPF HashMap
3. In `try_load_ebpf()`:
   - Check `CAP_BPF` capability
   - Load the compiled eBPF object
   - Create `CgroupSkb` program via Aya
   - Attach to the agent's cgroup (requires process isolation mode)
   - Populate the HashMap with allowed IPs
4. In `_spawn_perf_event_reader()`: read perf events for violation logging

### Estimated effort: Large (1-2 days) — requires eBPF expertise

---

## Finding 15: Mesh Protocol Is Entirely Stub [S3]

**File:** `crates/ghost-mesh/src/protocol.rs`
**Impact:** No agent-to-agent communication

### What's broken

```rust
pub fn process_message(&self, _message: &MeshMessage) -> Result<(), MeshError> {
    Err(MeshError::NotImplemented("MeshProtocol::process_message".into()))
}
```

Both `process_message()` and `send_message()` return `NotImplemented`. This is
intentionally deferred to Phase 9.

### Fix (when Phase 9 begins)

1. Implement message routing via the A2A JSON-RPC protocol
2. Wire to the mesh routes in `ghost-gateway/src/api/mesh_routes.rs` (which already
   has signature verification and agent card handling)
3. Add EigenTrust reputation scoring for peer selection
4. Implement payment negotiation flow (Request → Accept/Reject → Complete → Dispute)

### Estimated effort: Very Large (1-2 weeks) — Phase 9 scope

---

## Finding 16: Desktop Notification Is Log-Only [S2]

**File:** `crates/ghost-gateway/src/safety/notification.rs`
**Impact:** Desktop notifications don't actually show OS notifications

### What's broken

```rust
NotificationTarget::Desktop => {
    tracing::info!(subject = %payload.subject, "Desktop notification sent");
    Ok(())
}
```

The webhook, email (via lettre/SMTP), and SMS (via Twilio API) dispatchers are
**fully implemented** with retry logic. But the Desktop variant just logs. The
`notify-rust` crate is mentioned in the comment but never used.

### Fix

1. Add `notify-rust` as a dependency
2. Replace the log-only stub with:
   ```rust
   notify_rust::Notification::new()
       .summary(&payload.subject)
       .body(&payload.body)
       .urgency(notify_rust::Urgency::Critical)
       .show()
       .map_err(|e| format!("desktop notification: {e}"))?;
   ```

### Estimated effort: Tiny (30 minutes)

---

## Finding 17: VAPID Key Generation Is Placeholder [S2]

**File:** `crates/ghost-gateway/src/api/push_routes.rs`
**Impact:** Web Push subscriptions use a non-standard key

### What's broken

```rust
pub fn generate_vapid_public_key() -> String {
    let seed = blake3::hash(b"ghost-vapid-key-seed");
    base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        &seed.as_bytes()[..32],
    )
}
```

This generates a deterministic hash, not a real P-256 ECDSA key pair. Web Push
requires a valid VAPID key pair (RFC 8292). The push subscribe/unsubscribe
endpoints work correctly (they store subscriptions in memory), but actual push
delivery isn't implemented.

### Fix

1. Add the `web-push` crate as a dependency
2. Generate a real P-256 key pair on first boot, persist via `ghost-secrets`
3. Use the `web-push` crate to send actual push notifications when events occur
4. Wire push delivery to the same broadcast channel as WebSocket events

### Estimated effort: Medium (3-4 hours)

---

## Finding 18: Email Notification Uses Dangerous SMTP Config [S0]

**File:** `crates/ghost-gateway/src/safety/notification.rs`
**Impact:** Email notifications sent without TLS

### What's broken

```rust
let mailer = SmtpTransport::builder_dangerous(smtp_host)
    .port(*smtp_port)
    .timeout(Some(std::time::Duration::from_secs(*timeout_secs)))
    .build();
```

`builder_dangerous()` creates an SMTP transport with **no TLS and no authentication**.
This means:
- Credentials are sent in plaintext if SMTP AUTH is added later
- The connection can be MITM'd
- Most modern SMTP servers will reject the connection

### Fix

1. Use `SmtpTransport::relay()` or `SmtpTransport::starttls_relay()` instead
2. Accept SMTP credentials from `ghost-secrets` provider
3. Default to port 587 with STARTTLS
4. Fall back to `builder_dangerous()` only if explicitly configured (e.g., local
   mail relay on 127.0.0.1:25)

### Estimated effort: Small (1-2 hours)

---

## Finding 19: Agent Isolation spawn() Is Mostly Stub [S2]

**File:** `crates/ghost-gateway/src/agents/isolation.rs`
**Impact:** Process and container isolation modes don't work

### What's broken (inferred from signatures)

The `AgentIsolation::spawn()` method handles three modes:
- `InProcess` — runs in the same process (works, but no isolation)
- `Process` — should spawn a child process (likely stub)
- `Container` — should spawn a Docker container (likely stub)

### Fix

1. `Process` mode: Use `tokio::process::Command` to spawn a child process with
   the agent binary, passing config via environment or temp file
2. `Container` mode: Use `bollard` crate to create and start a Docker container
   with the agent image, mounting the workspace directory
3. Both modes need proper lifecycle management (health checks, restart on crash,
   graceful shutdown)

### Estimated effort: Large (1-2 days)

---

## Finding 20: Git Anchoring Is Stub [S2]

**File:** `crates/cortex/cortex-temporal/src/anchoring/` (referenced in architecture)
**Impact:** No tamper-evident external anchoring of hash chains

### What's broken

The `anchor()` method returns a placeholder record instead of actually anchoring
to a git commit or RFC 3161 timestamp authority.

### Fix

1. Git anchoring: Create a git commit with the hash chain head as the commit message,
   signed with the agent's Ed25519 key
2. RFC 3161: POST the hash to a timestamp authority (e.g., FreeTSA) and store the
   timestamp token
3. Both provide external proof that the hash chain existed at a specific time

### Estimated effort: Medium (3-4 hours)

---

---

# Implementation Plan: Ordered by Dependency Chain

The fixes below are ordered so each step unblocks the ones after it. A 0.0001
engineer would execute them in exactly this order.

---

## Phase 0: The Keystone Fix — AppState Injection (Unblocks Everything)

**Duration:** 1 day
**Unblocks:** Findings 1, 3, 4, 5, 6, 7, 8, 9, 10, 11

This is the single most important change. Without it, no route handler can access
any runtime state.

### Step 0.1: Define AppState

Create `crates/ghost-gateway/src/state.rs`:

```rust
use std::sync::{Arc, RwLock, atomic::AtomicBool};
use rusqlite::Connection;
use tokio::sync::broadcast;

use crate::gateway::GatewaySharedState;
use crate::agents::registry::AgentRegistry;
use crate::api::websocket::WsEvent;

pub struct AppState {
    pub gateway: Arc<GatewaySharedState>,
    pub agents: Arc<RwLock<AgentRegistry>>,
    pub platform_killed: Arc<AtomicBool>,
    pub db: Arc<Mutex<Connection>>,
    pub event_tx: broadcast::Sender<WsEvent>,
}
```

### Step 0.2: Create AppState in Bootstrap

In `GatewayBootstrap::run()`, after all init steps:

```rust
let (event_tx, _) = tokio::sync::broadcast::channel(256);
let app_state = Arc::new(AppState {
    gateway: Arc::new(shared_state),
    agents: Arc::new(RwLock::new(agent_registry)),
    platform_killed: Arc::clone(&PLATFORM_KILLED_ARC),
    db: Arc::new(Mutex::new(db_conn)),
    event_tx,
});
```

### Step 0.3: Pass AppState to build_router()

Change `build_router()` signature:

```rust
pub fn build_router(config: &GhostConfig, state: Arc<AppState>) -> axum::Router {
    axum::Router::new()
        .route("/api/health", get(health_handler))
        // ... all routes ...
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());
}
```

### Step 0.4: Update Every Route Handler Signature

Every handler changes from:
```rust
pub async fn list_agents() -> Json<Vec<AgentInfo>> {
```
to:
```rust
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AgentInfo>> {
```

This is mechanical — do all handlers in one pass.

### Step 0.5: Verify Compilation

Run `cargo check --workspace` to ensure all handlers compile with the new
signatures. Fix any type mismatches.

---

## Phase 1: Fix Safety-Critical Endpoints (Day 2)

**Duration:** 1 day
**Depends on:** Phase 0
**Fixes:** Findings 1, 3, 4

### Step 1.1: Fix Health Endpoint

```rust
pub async fn health_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let gw_state = state.gateway.current_state();
    let status_code = match gw_state {
        GatewayState::Healthy | GatewayState::Degraded | GatewayState::Recovering => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (status_code, Json(serde_json::json!({
        "status": if status_code == StatusCode::OK { "alive" } else { "unavailable" },
        "state": format!("{:?}", gw_state),
        "platform_killed": state.platform_killed.load(Ordering::SeqCst),
    })))
}
```

### Step 1.2: Wire Safety Endpoints to Kill Switch

For `kill_all()`:
```rust
pub async fn kill_all(
    State(state): State<Arc<AppState>>,
    Json(body): Json<KillAllRequest>,
) -> impl IntoResponse {
    state.platform_killed.store(true, Ordering::SeqCst);

    // Persist kill state for crash recovery
    let kill_state = serde_json::json!({
        "reason": body.reason,
        "initiated_by": body.initiated_by,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let kill_path = shellexpand_tilde("~/.ghost/data/kill_state.json");
    if let Err(e) = std::fs::write(&kill_path, kill_state.to_string()) {
        tracing::error!(error = %e, "Failed to persist kill_state.json");
    }

    // Broadcast to WebSocket clients
    let _ = state.event_tx.send(WsEvent::KillSwitchActivation {
        level: "KILL_ALL".into(),
        agent_id: None,
        reason: body.reason.clone(),
    });

    (StatusCode::OK, Json(serde_json::json!({
        "status": "kill_all_activated",
        "reason": body.reason,
    })))
}
```

### Step 1.3: Wire Goal Approval to Database

```rust
pub async fn approve_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    // Check if already resolved
    let already_resolved: bool = db.query_row(
        "SELECT resolved_at IS NOT NULL FROM proposals WHERE id = ?1",
        [&id],
        |row| row.get(0),
    ).unwrap_or(true);

    if already_resolved {
        return (StatusCode::CONFLICT, Json(serde_json::json!({
            "error": "proposal already resolved", "id": id
        })));
    }

    db.execute(
        "UPDATE proposals SET decision = 'approved', resolved_at = datetime('now') WHERE id = ?1",
        [&id],
    ).ok();

    let _ = state.event_tx.send(WsEvent::ProposalDecision {
        proposal_id: id.clone(),
        decision: "approved".into(),
        agent_id: String::new(), // TODO: read from proposal row
    });

    (StatusCode::OK, Json(serde_json::json!({"status": "approved", "id": id})))
}
```

### Step 1.4: Write Tests

- Test that health endpoint returns correct state for each GatewayState
- Test that kill_all actually sets PLATFORM_KILLED
- Test that goal approval returns 409 for already-resolved proposals
- Test that safety_status returns actual kill switch state

---

## Phase 2: Wire Data Endpoints (Day 3)

**Duration:** 1 day
**Depends on:** Phase 0
**Fixes:** Findings 5, 6, 7, 8, 9

### Step 2.1: Agents Endpoint

Read from `state.agents` registry, map to `AgentInfo` structs.

### Step 2.2: Convergence Scores Endpoint

Read from convergence cache (populated by monitor updates via the broadcast channel
or direct state sharing).

### Step 2.3: Sessions Endpoint

Query `SELECT * FROM sessions ORDER BY created_at DESC LIMIT ?` from `state.db`.

### Step 2.4: Audit Endpoints

Wire to `ghost_audit::query_engine::QueryEngine`:
```rust
pub async fn query_audit(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditQueryParams>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let engine = ghost_audit::query_engine::QueryEngine::new(&db);
    let results = engine.query(/* map params to QueryFilter */);
    // Return results with pagination
}
```

### Step 2.5: Memory Endpoints

Query cortex-storage for memories matching filter params.

---

## Phase 3: WebSocket Event Streaming (Day 4)

**Duration:** 0.5 day
**Depends on:** Phase 0 (broadcast channel in AppState)
**Fixes:** Finding 11

### Step 3.1: Subscribe to Broadcast in handle_socket()

```rust
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.event_tx.subscribe();
    let mut interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // keepalive ping
            }
            event = rx.recv() => {
                if let Ok(event) = event {
                    let json = serde_json::to_string(&event).unwrap();
                    if socket.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                // handle client messages
            }
        }
    }
}
```

### Step 3.2: Emit Events from State Changes

Every place that changes safety state, convergence scores, or proposal decisions
should `state.event_tx.send(event)`.

---

## Phase 4: OAuth Wiring (Day 4-5)

**Duration:** 0.5 day
**Depends on:** Phase 0
**Fixes:** Finding 10

### Step 4.1: Add OAuthBroker to AppState

### Step 4.2: Wire Each OAuth Route to Broker Methods

The `ghost-oauth` crate already has the PKCE flow, token storage, and refresh
logic. Each route handler just needs to call the corresponding broker method.

---

## Phase 5: Signal Computation (Day 5)

**Duration:** 1 day
**Depends on:** None (independent of gateway fixes)
**Fixes:** Finding 13

### Step 5.1: Wire Signal Computer to Cortex-Convergence

Replace the stub `compute()` body with actual calls to the cortex-convergence
signal functions, passing session data from cortex-storage.

### Step 5.2: Test with Real Data

Create test fixtures with known session data and verify that computed signals
match expected values.

---

## Phase 6: Channel Adapters (Day 6-8)

**Duration:** 2-3 days
**Depends on:** None (independent)
**Fixes:** Finding 12

### Step 6.1: Verify Telegram Works (0 effort)

The Telegram adapter already has real HTTP implementations for both send and
receive. Write an integration test with a test bot token.

### Step 6.2: Wire WebSocket Adapter

Replace the log-only `send()` with actual socket delivery via a broadcast channel
or `Arc<Mutex<WebSocket>>` reference.

### Step 6.3: Implement Slack Socket Mode

1. Call `apps.connections.open` with app_token to get WebSocket URL
2. Connect to the WebSocket URL
3. Parse `event_callback` messages with `message` type
4. Extract text, channel, user from the event payload

### Step 6.4: Implement Discord Gateway WebSocket

1. Connect to `wss://gateway.discord.gg/?v=10&encoding=json`
2. Handle HELLO → send IDENTIFY with bot token
3. Handle READY → extract bot user ID
4. Listen for MESSAGE_CREATE events
5. Filter for bot mentions
6. Extract content, channel_id, author

### Step 6.5: WhatsApp Webhook Receiver

Add a webhook endpoint that receives inbound messages from the WhatsApp Cloud API
and pushes them to the adapter's inbound queue.

---

## Phase 7: Security Hardening (Day 9)

**Duration:** 1 day
**Depends on:** Phase 1
**Fixes:** Findings 16, 17, 18

### Step 7.1: Fix SMTP TLS

Replace `builder_dangerous()` with `starttls_relay()`.

### Step 7.2: Implement Desktop Notifications

Add `notify-rust` and replace the log-only stub.

### Step 7.3: Fix VAPID Key Generation

Generate real P-256 key pair, persist via ghost-secrets.

---

## Phase 8: Advanced Features (Week 2+)

**Duration:** Variable
**Fixes:** Findings 14, 15, 19, 20

### Step 8.1: eBPF Egress (if needed beyond proxy)

Implement via Aya crate behind feature flag.

### Step 8.2: Agent Process/Container Isolation

Implement `Process` mode via `tokio::process::Command` and `Container` mode via
`bollard` Docker crate.

### Step 8.3: Git Anchoring

Implement git commit anchoring and RFC 3161 timestamp authority.

### Step 8.4: Mesh Protocol (Phase 9)

Full A2A JSON-RPC implementation when Phase 9 begins.

---

---

# Summary: Effort Estimates

| Phase | Duration | Findings Fixed | Priority |
|-------|----------|---------------|----------|
| Phase 0: AppState Injection | 1 day | Keystone (unblocks all) | CRITICAL |
| Phase 1: Safety Endpoints | 1 day | 1, 3, 4 | CRITICAL |
| Phase 2: Data Endpoints | 1 day | 5, 6, 7, 8, 9 | HIGH |
| Phase 3: WebSocket Events | 0.5 day | 11 | HIGH |
| Phase 4: OAuth Wiring | 0.5 day | 10 | MEDIUM |
| Phase 5: Signal Computation | 1 day | 13 | HIGH |
| Phase 6: Channel Adapters | 2-3 days | 12 | MEDIUM |
| Phase 7: Security Hardening | 1 day | 16, 17, 18 | MEDIUM |
| Phase 8: Advanced Features | Variable | 14, 15, 19, 20 | LOW |

**Total to production-ready MVP: ~8-9 working days**
**Total including all advanced features: ~3-4 weeks**

---

# Verification Checklist

After all phases are complete, verify:

- [ ] `GET /api/health` returns actual gateway state (not hardcoded "Healthy")
- [ ] `GET /api/health` returns 503 when gateway is in Degraded/ShuttingDown/FatalError
- [ ] `POST /api/safety/kill-all` actually sets PLATFORM_KILLED and persists kill_state.json
- [ ] `POST /api/safety/pause/:id` actually pauses the agent
- [ ] `GET /api/safety/status` returns actual kill switch state
- [ ] `POST /api/goals/:id/approve` updates the database and returns 409 on re-approval
- [ ] `GET /api/agents` returns the actual agent registry
- [ ] `GET /api/convergence/scores` returns real convergence scores
- [ ] `GET /api/sessions` returns actual sessions from the database
- [ ] `GET /api/audit` returns actual audit log entries
- [ ] `GET /api/audit/export` exports real data in CSV/JSON/JSONL
- [ ] `GET /api/memory` returns actual memories from cortex-storage
- [ ] WebSocket clients receive real-time events (score updates, kill switch, proposals)
- [ ] OAuth flow completes end-to-end (connect → callback → list connections → disconnect)
- [ ] Convergence monitor computes real signal values (not cached zeros)
- [ ] `ghost chat` works with at least one LLM provider configured via env var
- [ ] Telegram adapter sends and receives messages
- [ ] Desktop notifications show OS-level notifications
- [ ] Email notifications use TLS
- [ ] Integration tests pass with real (mocked) data, not empty vecs
