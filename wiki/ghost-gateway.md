# ghost-gateway

> The single long-running GHOST platform process — a 6-state FSM coordinator that owns agent lifecycle, session management, inter-agent messaging, cost tracking, kill switch enforcement, ITP event routing, periodic task scheduling, multi-channel adapters, and a 7-step graceful shutdown sequence. This is the binary that ties all 36 other crates together.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 8 (Gateway) |
| Type | Binary (`ghost`) + Library |
| Location | `crates/ghost-gateway/` |
| Binary name | `ghost` |
| Default port | `18789` |
| Workspace deps | `cortex-core`, `cortex-storage`, `cortex-temporal`, `cortex-convergence`, `cortex-validation`, `ghost-signing`, `ghost-policy`, `ghost-llm`, `ghost-secrets`, `ghost-identity`, `ghost-agent-loop`, `itp-protocol`, `simulation-boundary`, `read-only-pipeline`, `ghost-channels`, `ghost-skills`, `ghost-heartbeat`, `ghost-audit`, `ghost-backup`, `ghost-export`, `ghost-migrate`, `ghost-oauth`, `ghost-egress`, `ghost-kill-gates`, `ghost-mesh` |
| External deps | `axum`, `tower`, `tower-http`, `reqwest`, `tokio`, `clap`, `rusqlite`, `dashmap`, `lettre`, `serde_yaml`, `blake3`, `ed25519-dalek`, `futures`, `tracing` |
| Feature flags | `keychain` (default), `vault`, `ebpf`, `pf` |
| Modules | `gateway`, `bootstrap`, `config`, `health`, `shutdown`, `periodic`, `itp_buffer`, `itp_router`, `agents/` (3), `api/` (12), `auth/` (3), `cli/` (3), `cost/` (2), `messaging/` (3), `safety/` (5), `session/` (5) |
| CLI subcommands | `serve` (default), `chat`, `status`, `backup`, `export`, `migrate` |
| Gateway states | Initializing → Healthy / Degraded → Recovering → ShuttingDown / FatalError |
| Test suites | FSM transitions, ITP buffer limits, agent registry, kill switch races, compaction under load, session management, messaging, auth, config |
| Downstream consumers | None (top-level binary) |

---

## Why This Crate Exists

Every other crate in GHOST is a library. `ghost-gateway` is the one binary that wires them all together into a running system. It's the process you start, the process that listens on port 18789, the process that manages agent lifecycles, routes messages between agents, enforces spending caps, buffers ITP events when the convergence monitor goes down, and shuts down gracefully when you hit Ctrl+C.

The gateway exists as a separate crate (not just a `main.rs` in the workspace root) because:
- It needs to be both a binary and a library — the binary is the `ghost` CLI, the library exposes types for integration tests
- Feature flags (`keychain`, `vault`, `ebpf`, `pf`) control platform-specific capabilities at compile time
- The gateway's 26 workspace dependencies represent the full dependency tree — isolating it prevents accidental circular deps
- Integration tests need to import `ghost_gateway::safety::kill_switch::PLATFORM_KILLED` and other internals

The design philosophy: the gateway is a coordinator, not an implementor. It delegates to specialized crates for every capability. The gateway's job is lifecycle management, routing, and state machine enforcement.

---

## Module Breakdown

### `gateway.rs` — The 6-State FSM

The gateway's operational state is a strict finite state machine stored as an `AtomicU8` for lock-free reads from health endpoints and ITP emitters.

```
Initializing ──→ Healthy ──→ Degraded ──→ Recovering ──→ Healthy
     │              │            │              │
     │              │            │              └──→ Degraded
     │              └──→ ShuttingDown          └──→ ShuttingDown
     │                   Degraded ──→ ShuttingDown
     └──→ FatalError
```

**The 6 states:**
| State | Meaning | Accepts traffic? |
|-------|---------|-----------------|
| `Initializing` | Bootstrap in progress | No |
| `Healthy` | All subsystems operational, monitor reachable | Yes |
| `Degraded` | Gateway operational but convergence monitor unreachable | Yes (safety floor absent) |
| `Recovering` | Monitor reconnected, syncing missed state | Yes |
| `ShuttingDown` | Graceful shutdown in progress | No (draining) |
| `FatalError` | Bootstrap failed irrecoverably | No |

**Design decisions:**
- State stored as `AtomicU8` with `Acquire`/`Release` ordering — health endpoints read state without locks
- `GatewaySharedState::transition_to()` validates every transition against the FSM. In debug builds, illegal transitions panic. In release, they log and return `Err`
- `ShuttingDown` and `FatalError` are terminal — no transitions out
- `Healthy → Recovering` is intentionally forbidden — you must go through `Degraded` first. This prevents the gateway from claiming recovery when it was never degraded


### `bootstrap.rs` — The 6-Step Startup Sequence

`GatewayBootstrap::run()` is the ordered startup sequence. Each step must succeed (except Step 3) before the next begins.

**Pre-step: Kill state recovery (AC13)**
Before anything else, the bootstrap checks for `~/.ghost/data/kill_state.json`. If present, a previous `KILL_ALL` was not cleanly resolved. The gateway enters safe mode — `PLATFORM_KILLED` is set to `true` via `SeqCst`, blocking all agent operations. The operator must delete the file or use the dashboard API with a confirmation token to resume.

**Step 1: Load + validate `ghost.yml`**
Loads configuration from the CLI-specified path, `~/.ghost/ghost.yml`, or `./ghost.yml` (in that order). Validates all fields, substitutes `${ENV_VAR}` references, and expands `~` paths.

**Step 1b: Build SecretProvider**
Initializes the credential backend based on `secrets.provider` config — keychain (macOS/Linux), Vault (HashiCorp), or environment variables.

**Step 2: Run database migrations**
Opens SQLite at the configured `db_path` (default `~/.ghost/data/ghost.db`), sets `PRAGMA journal_mode=WAL` and `busy_timeout=5000`, then runs `cortex_storage::migrations::run_migrations()`.

**Step 3: Verify convergence monitor health (NEVER fatal)**
This is the critical design decision: the monitor check is the only step that cannot fail the bootstrap. If the monitor is unreachable, the gateway starts in `Degraded` mode instead of `Healthy`. This ensures the gateway is always available even if the monitor sidecar hasn't started yet.

**Step 4: Initialize agent registry + channel adapters**
Loads agent configurations, creates the agent registry, and binds channel adapters (Slack, Discord, etc.) to agents.

**Step 4b: Apply network egress policies**
Configures per-agent network egress allowlists using `ghost-egress`. Supports `unrestricted`, `allowlist`, and `denylist` policies with optional eBPF or pf enforcement.

**Step 4c: Initialize mesh networking**
If mesh is enabled in config, initializes the A2A agent network with EigenTrust reputation, known peers, and delegation depth limits.

**Step 5: Start API server**
Builds the axum router with all API routes and starts listening.

**Transition decision:**
After all steps complete, the gateway transitions to either `Healthy` (monitor connected) or `Degraded` (monitor unreachable). This is the only place where the initial state transition happens.

### `config.rs` — The `ghost.yml` Schema

The configuration is a deeply nested YAML structure with sensible defaults for every field.

**Top-level sections:**
| Section | Purpose | Key defaults |
|---------|---------|-------------|
| `gateway` | Bind address, port, DB path | `127.0.0.1:18789`, `~/.ghost/data/ghost.db` |
| `agents[]` | Per-agent config: name, template, spending cap, isolation | `$5.00/day`, `in_process` |
| `network_egress` | Global + per-agent egress policies | `unrestricted` |
| `channels[]` | Channel adapter bindings | — |
| `convergence` | Profile, monitor address, contacts | `standard`, `127.0.0.1:18790` |
| `security` | Soul drift threshold | `0.15` |
| `models` | LLM provider configs | — |
| `secrets` | Credential backend (keychain/vault/env) | `keychain` |
| `mesh` | A2A networking, known agents, trust | `min_trust: 0.3`, `max_delegation_depth: 3` |

**Isolation modes:** `InProcess` (default, shared tokio runtime), `Process` (separate OS process), `Container` (Docker/OCI isolation).

**Environment variable substitution:** Config values containing `${VAR_NAME}` are expanded at load time. This allows secrets to be injected without appearing in the YAML file.

### `health.rs` — Monitor Health Checking and Recovery

Two structs manage the gateway's relationship with the convergence monitor:

**`MonitorHealthChecker`** — Periodic health checker that runs every 30s (configurable). After `failure_threshold` (default 3) consecutive failures, transitions the gateway to `Degraded`. Handles both `Healthy → Degraded` and `Recovering → Degraded` paths.

**`RecoveryCoordinator`** — When the monitor comes back, performs 3 stability checks 5 seconds apart. If all pass, transitions `Recovering → Healthy`. If any fail, transitions back to `Recovering → Degraded`. The 3-check requirement prevents flapping on intermittent connectivity.

**Key invariant:** Both structs use `GatewaySharedState::transition_to()` for all state changes. Direct `AtomicU8` writes are forbidden — the FSM validation must always be enforced.

### `shutdown.rs` — 7-Step Graceful Shutdown

When the gateway receives SIGTERM (or Ctrl+C), it executes a 7-step shutdown sequence with a 60-second total timeout:

| Step | Action | Timeout | Kill switch behavior |
|------|--------|---------|---------------------|
| 1 | Stop accepting new connections | Immediate | — |
| 2 | Drain lane queues | 30s | — |
| 3 | Flush sessions to disk | 30s total, 15s/session | **Skipped** if kill switch active |
| 4 | Persist cost data | — | — |
| 5 | Notify convergence monitor | 2s | — |
| 6 | Close channel adapters | 5s | — |
| 7 | SQLite WAL checkpoint | — | — |

**Design decision:** Step 3 is skipped when the kill switch is active. If the platform was killed due to a safety event (credential exfiltration, sandbox escape), flushing session state could persist compromised data. Better to lose the session than persist poison.

**Second SIGTERM:** If the operator sends a second SIGTERM during shutdown, the process exits immediately with code 1. This is the "I really mean it" escape hatch.


### `periodic.rs` — Centralized Task Scheduler

Multiple subsystems need periodic background work (health checks, idle session pruning, cost resets, backup scheduling). Rather than each spawning its own timer, the gateway provides a centralized `PeriodicTaskScheduler`.

**Architecture:**
- Each task is a named async closure with a configurable interval
- Tasks run in their own `tokio::spawn` — failure in one doesn't block others
- After `max_failures` consecutive failures, a task is disabled with `tracing::error!`
- The scheduler respects the kill switch — when active, all periodic tasks stop
- 1-second granularity sleep loop (not per-task timers)

**Task health tracking:**
Each task has a `TaskHealth` with `last_success`, `last_failure`, `consecutive_failures`, `total_runs`, and a 3-state status: `Healthy`, `Degraded`, `Disabled`. This feeds into the `/api/health` endpoint for operational visibility.

**Why centralized?** Individual `tokio::spawn` timers scattered across subsystems are hard to monitor, hard to shut down cleanly, and impossible to health-check from a single endpoint. The scheduler provides a single point of control.

### `itp_buffer.rs` + `itp_router.rs` — Degraded Mode Event Handling

When the convergence monitor is unreachable, ITP events can't be delivered. The gateway buffers them for replay during recovery.

**`ITPBuffer`** — In-memory FIFO buffer with dual limits:
- Max 10MB total
- Max 10,000 events
- FIFO eviction when either limit is exceeded (oldest events dropped first)

**`ITPEventRouter`** — Routes ITP events based on gateway state:
| Gateway state | Routing behavior |
|--------------|-----------------|
| `Healthy` / `Recovering` | Send to monitor via HTTP POST |
| `Degraded` | Buffer locally |
| `Initializing` / `ShuttingDown` / `FatalError` | Drop silently |

If the monitor rejects an event (non-2xx response) or the HTTP request fails, the event is buffered rather than dropped. During recovery, `drain_buffer()` returns all buffered events for replay.

**Design decision:** The buffer is in-memory, not disk-backed (despite the module doc saying "disk-backed"). This is intentional for the current implementation — disk I/O during degraded mode could compound failures. The 10MB cap ensures bounded memory usage.

---

## Agents Subsystem (`agents/`)

### `registry.rs` — Agent Registry

The `AgentRegistry` is a triple-indexed lookup table:
- By UUID (`agents_by_id`)
- By name (`name_to_id`)
- By channel binding (`channel_to_id`)

All backed by `BTreeMap` for deterministic iteration order. Each `RegisteredAgent` tracks: id, name, lifecycle state, channel bindings, capabilities, and spending cap.

**Agent lifecycle FSM:** `Starting → Ready → Stopping → Stopped`. Transitions are validated — you can't go from `Starting` to `Stopped` directly.

### `templates.rs` — Agent Templates

Three built-in templates define capability profiles:

| Template | Capabilities | Spending cap | Heartbeat | Convergence profile |
|----------|-------------|-------------|-----------|-------------------|
| `personal` | memory_read/write, web_search, web_fetch | $5/day | 30min | `companion` |
| `developer` | + shell, filesystem, http_request | $10/day | 60min | `productivity` |
| `researcher` | memory, web, http_request | $20/day | 120min | `research` |

Templates can also be loaded from YAML for custom profiles.

### `isolation.rs` — Agent Isolation Modes

Three isolation modes with increasing security:
- `InProcess` — Shared tokio runtime (default, lowest overhead)
- `Process` — Separate OS process (memory isolation)
- `Container` — Docker/OCI container (full sandboxing)

Each mode has `spawn()` and `teardown()` methods for lifecycle management.

---

## Safety Subsystem (`safety/`)

### `kill_switch.rs` — 3-Level Hard Safety System

The kill switch is the most critical safety component in the entire platform. It provides three escalating levels:

| Level | Scope | Effect | Resume requirement |
|-------|-------|--------|-------------------|
| `Pause` | Single agent | Agent operations blocked | Owner auth |
| `Quarantine` | Single agent | Operations blocked + forensic state captured | Forensic review + second confirmation + 24h heightened monitoring |
| `KillAll` | All agents | Platform enters safe mode | Delete `kill_state.json` or dashboard API with confirmation token |

**`PLATFORM_KILLED`** — A `static AtomicBool` with `SeqCst` ordering. This is the fast-path check in every agent loop iteration. `SeqCst` is used (not `Relaxed` or `Acquire`) because safety-critical state must be globally visible immediately — no CPU can see a stale value.

**Monotonicity invariant:** Kill levels never decrease without explicit resume. If an agent is at `Quarantine`, a subsequent `Pause` trigger is silently ignored. This prevents oscillation attacks where an adversary alternates triggers to keep an agent in a lower safety state.

**RwLock poisoning:** If the `RwLock` protecting kill state is poisoned (a thread panicked while holding it), the kill switch treats this as `PlatformKilled`. A poisoned lock is a critical failure — erring on the side of caution is the only safe choice.

### `auto_triggers.rs` — Trigger Classification and Deduplication

The `AutoTriggerEvaluator` receives `TriggerEvent`s from across the platform and classifies them into kill levels:

| Trigger | Level | Scope |
|---------|-------|-------|
| `SoulDrift` | Quarantine | Per-agent |
| `SpendingCapExceeded` | Pause | Per-agent |
| `PolicyDenialThreshold` | Quarantine | Per-agent |
| `SandboxEscape` | KillAll | Platform |
| `CredentialExfiltration` | KillAll | Platform |
| `NetworkEgressViolation` | Quarantine | Per-agent |
| `MultiAgentQuarantine` | KillAll | Platform |
| `MemoryHealthCritical` | Quarantine | Per-agent |
| `DistributedKillGate` | KillAll | Platform |
| Manual variants | As specified | As specified |

**Deduplication:** Same trigger type + same agent within 60 seconds is suppressed. This prevents trigger storms from overwhelming the system. Dedup entries are cleaned up every 300 seconds.

**T6 cascade rule:** If ≥3 agents are quarantined simultaneously, the evaluator automatically escalates to `KillAll`. The reasoning: if 3 independent agents are all exhibiting dangerous behavior, the problem is likely systemic, not agent-specific.

### `quarantine.rs` — Forensic State Preservation

When an agent is quarantined, the `QuarantineManager` captures a `ForensicState` snapshot:
- Session transcript (full conversation history)
- Memory snapshot (JSON dump of agent's memory state)
- Tool history (all tools invoked)
- Trigger reason

This forensic state is preserved for post-incident review. It's the "black box" that lets operators understand what went wrong.

### `kill_gate_bridge.rs` — Distributed Kill Coordination

Bridges the local `KillSwitch` with the distributed `KillGate` from `ghost-kill-gates`. When a kill is activated locally, the bridge propagates it through the relay to all peer nodes. When a remote kill arrives, the bridge activates the local kill switch.

### `notification.rs` — Multi-Channel Alert Dispatch

When safety events occur, the `NotificationDispatcher` sends alerts through 4 channels simultaneously:

| Channel | Transport | Retry | Timeout |
|---------|-----------|-------|---------|
| Desktop | OS notification API | No | — |
| Webhook | HTTP POST (JSON) | 1 retry | 5s |
| Email | SMTP via `lettre` | No | 10s |
| SMS | Twilio-compatible HTTP POST | 1 retry | 5s |

**Key design:** All dispatches are parallel (`futures::future::join_all`) and best-effort. A failed notification never blocks the safety intervention. The notification system is explicitly not routed through agent channels — a compromised agent must not be able to suppress its own kill notification.


---

## Session Subsystem (`session/`)

### `manager.rs` — Session Lifecycle

The `SessionManager` tracks all active sessions with:
- Dual-indexed lookup: by session UUID and by agent UUID
- `SessionContext` with: session_id, agent_id, channel, timestamps, token count, cost, model context window
- `touch()` updates `last_activity` for idle detection
- `prune_idle()` removes sessions that have been inactive beyond a configurable threshold

**UUID v7:** Sessions use `Uuid::now_v7()` which embeds a timestamp, making session IDs naturally sortable by creation time.

### `lane_queue.rs` — Per-Session Request Serialization

Each session has a `LaneQueue` that serializes requests — only one request processes at a time per session. This prevents race conditions where two concurrent messages to the same agent could interleave tool executions.

**Backpressure:** Default depth limit is 5. When the queue is full, new requests are rejected with HTTP 429 (Too Many Requests). This prevents unbounded memory growth from a flood of requests.

**`LaneQueueManager`** wraps all lane queues in a `DashMap` for concurrent access across sessions. Empty queues are pruned periodically.

### `compaction.rs` — Context Window Management

When a session's token count reaches 70% of the model's context window, the `SessionCompactor` kicks in:

**Trigger:** `current_tokens / context_window >= 0.70`

**Compaction passes:** Up to 3 passes, each progressively more aggressive:
1. Pass 1: Summarize older messages, preserve recent context
2. Pass 2: More aggressive summarization
3. Pass 3: Maximum compression

**Per-type compression minimums:** Different memory types have different minimum compression levels:
| Type | Minimum level | Rationale |
|------|--------------|-----------|
| `ConvergenceEvent` | L3 | Safety-critical, preserve detail |
| `BoundaryViolation` | L3 | Safety-critical |
| `AgentGoal` | L2 | Important for coherence |
| `InterventionPlan` | L2 | Important for safety |
| `AgentReflection` | L1 | Useful but compressible |
| `ProposalRecord` | L1 | Useful but compressible |
| Other | L0 | Fully compressible |

**CompactionBlock immutability:** Once a compaction block is created, it is never re-compressed. The `is_compaction_block()` method identifies these blocks so subsequent passes skip them.

**Tool result pruning:** `prune_tool_results()` strips tool output from history before compaction. Tool results are often large (file contents, search results) and rarely needed for context continuity.

### `router.rs` — Message Routing

The `MessageRouter` maps channel keys to agent UUIDs. When a message arrives from Slack/Discord/etc., the router determines which agent should handle it. Session resolution happens in the `SessionManager` after routing.

### `boundary.rs` — Session Boundary Enforcement

Enforces two session constraints:
- **Max duration:** 6 hours (default). Sessions that exceed this are expired
- **Min gap:** 5 minutes between sessions. Prevents rapid session cycling that could be used to reset safety state

---

## Cost Subsystem (`cost/`)

### `tracker.rs` — Per-Agent and Per-Session Cost Tracking

The `CostTracker` uses `DashMap` for lock-free concurrent access:
- **Per-agent daily totals** — Reset at midnight
- **Per-session totals** — Lifetime of the session
- **Compaction cost** — Tracked separately because compaction LLM calls are system-initiated, not user-initiated. This distinction matters for spending cap enforcement — compaction cost shouldn't count against the user's daily budget

### `spending_cap.rs` — Pre/Post Call Enforcement

**Pre-call check:** Before an LLM call, estimates the cost and checks if it would exceed the agent's daily cap. If so, the call is blocked.

**Post-call check:** After an LLM call, checks if the actual cost pushed the agent over its cap. If so, fires a `SpendingCapExceeded` trigger event to the auto-trigger evaluator, which will pause the agent.

**Invariant:** An agent cannot raise its own spending cap. The cap is set in `ghost.yml` and can only be changed by the operator.

---

## Messaging Subsystem (`messaging/`)

### `protocol.rs` — Inter-Agent Message Protocol

The `AgentMessage` struct is the wire format for agent-to-agent communication:

**Fields:** id (UUIDv7), sender, recipient, payload, context (BTreeMap), nonce, timestamp, content_hash (blake3), signature (Ed25519), encrypted flag.

**Payload variants:**
- `TaskRequest` / `TaskResponse` — Direct task delegation
- `Notification` — One-way informational
- `DelegationOffer` / `Accept` / `Reject` / `Complete` / `Dispute` — Full delegation lifecycle

**Delegation state machine:** `Offered → Accepted → Completed` or `Offered → Rejected` or `Accepted → Disputed`. Transitions are validated.

**Canonical bytes:** For signing, fields are serialized in exact field order with `BTreeMap` for deterministic map ordering. If serialization fails, the canonical bytes include `<serialization_error>` — this ensures the signature will be invalid rather than silently producing wrong bytes.

### `dispatcher.rs` — Message Verification and Delivery

The `MessageDispatcher` is the gateway's message verification pipeline:

1. **Content hash gate** — Cheap blake3 check before expensive Ed25519 verify
2. **Replay detection** — Nonce-based dedup (seen nonces tracked in a set)
3. **Timestamp freshness** — Rejects messages older than a configurable window
4. **Rate limiting** — Per-sender-recipient pair, resets periodically
5. **Signature anomaly detection** — After 3 signature failures from the same agent, triggers quarantine

**Offline queue:** If a recipient agent is not currently running, messages are queued for delivery when the agent comes back online.

### `encryption.rs` — Optional Message Encryption

X25519-XSalsa20-Poly1305 encryption using the encrypt-then-sign pattern:
1. Encrypt the plaintext payload with an ephemeral X25519 keypair
2. Sign the encrypted payload (not the plaintext)

**Constraint:** Broadcast messages cannot be encrypted (no single recipient public key to encrypt to).

---

## Auth Subsystem (`auth/`)

### `token_auth.rs` — Bearer Token Authentication

Validates bearer tokens against the `GHOST_TOKEN` environment variable using constant-time comparison (XOR accumulator). If `GHOST_TOKEN` is not set, authentication is disabled with a warning.

### `mtls_auth.rs` — Mutual TLS

Feature-gated mTLS for hardened deployments. When enabled, verifies client certificates against a configurable CA trust store. Can be set to require or optionally accept client certs.

### `auth_profiles.rs` — LLM Provider Credential Rotation

Manages multiple API keys per LLM provider with automatic rotation on 401/429 responses. Supports session pinning (a session stays on the same provider key for consistency).

---

## CLI Subsystem (`cli/`)

### `chat.rs` — Interactive REPL

The `ghost chat` command starts an interactive session wired to `AgentRunner`. Supports:
- Full agentic loop (LLM calls, tool execution, proposals)
- `/quit`, `/help`, `/status`, `/model` commands
- Displays tool call count and cost per turn

### `status.rs` — Status Query

The `ghost status` command queries the gateway's health endpoint and the convergence monitor's health endpoint, displaying operational state.

### `commands.rs` — Backup, Export, Migrate

Wires the CLI subcommands to their respective crates:
- `ghost backup` → `ghost-backup::export::BackupExporter`
- `ghost export <path>` → `ghost-export::analyzer::ExportAnalyzer`
- `ghost migrate` → `ghost-migrate::migrator::OpenClawMigrator`

---

## API Server (`api/`)

The gateway exposes 12 route groups via axum:

| Route group | Endpoint prefix | Purpose |
|-------------|----------------|---------|
| `health` | `/api/health` | Gateway state, subsystem health |
| `agents` | `/api/agents` | Agent CRUD, lifecycle management |
| `sessions` | `/api/sessions` | Session creation, lookup, pruning |
| `convergence` | `/api/convergence` | Convergence scores, signals, profiles |
| `memory` | `/api/memory` | Memory read/write/search |
| `goals` | `/api/goals` | Agent goal management |
| `safety` | `/api/safety` | Kill switch control, quarantine management |
| `audit` | `/api/audit` | Audit log queries, export |
| `websocket` | `/ws` | Real-time event streaming |
| `mesh_routes` | `/api/mesh` | A2A message relay, peer management |
| `oauth_routes` | `/api/oauth` | OAuth 2.0 PKCE flow endpoints |
| `push_routes` | `/api/push` | Push notification registration |

---

## Security Properties

1. **Kill switch is never bypassable** — `PLATFORM_KILLED` is a `static AtomicBool` with `SeqCst`. No code path can skip this check
2. **Safe mode on crash recovery** — If `kill_state.json` exists at startup, the gateway enters safe mode before loading any agent
3. **Monotonic kill levels** — Levels never decrease without explicit resume with appropriate authorization
4. **RwLock poisoning = platform killed** — A poisoned lock is treated as a critical failure, not recovered from
5. **Notifications bypass agent channels** — A compromised agent cannot suppress its own kill notification
6. **Constant-time token comparison** — Bearer token auth uses XOR accumulator, not string equality
7. **Session flush skipped on kill** — Compromised session state is not persisted during safety shutdowns
8. **Spending caps are operator-only** — Agents cannot modify their own spending limits

---

## Test Strategy

| Test file | Focus | Key assertions |
|-----------|-------|---------------|
| `gateway_tests.rs` | FSM transitions, ITP buffer, agent registry, templates, isolation, shutdown, kill switch, auto triggers, quarantine, lane queues, sessions, cost, spending caps, routing, auth, messaging, compaction, config | ~80 tests covering every subsystem |
| `kill_switch_race.rs` | Concurrent trigger delivery, monotonicity under contention, dedup correctness, state restoration, T6 cascade | Serial execution via `KILL_SWITCH_TEST_LOCK` mutex |
| `compaction_under_load.rs` | Trigger thresholds (70% exact), CompactionBlock immutability, max passes, per-type minimums, tool result pruning | Boundary condition testing |
| `secrets_config_tests.rs` | Secret provider configuration and initialization | Config validation |

**Kill switch test serialization:** Tests that touch `PLATFORM_KILLED` (a global static) must run serially. A `Mutex<()>` lock ensures this. Each test calls `reset_platform_killed()` before starting.

---

## File Map

| File | Lines | Purpose |
|------|-------|---------|
| `src/main.rs` | ~90 | Binary entry point, CLI parsing, subcommand dispatch |
| `src/lib.rs` | ~15 | Module declarations |
| `src/gateway.rs` | ~170 | 6-state FSM, `Gateway` coordinator, axum server |
| `src/bootstrap.rs` | ~480 | 6-step startup sequence, router builder |
| `src/config.rs` | ~480 | `ghost.yml` schema, defaults, validation, env substitution |
| `src/health.rs` | ~170 | Monitor health checker, recovery coordinator |
| `src/shutdown.rs` | ~80 | 7-step graceful shutdown |
| `src/periodic.rs` | ~280 | Centralized periodic task scheduler |
| `src/itp_buffer.rs` | ~70 | FIFO event buffer (10MB / 10K cap) |
| `src/itp_router.rs` | ~90 | State-aware ITP event routing |
| `src/agents/registry.rs` | ~100 | Triple-indexed agent lookup |
| `src/agents/templates.rs` | ~80 | 3 built-in agent templates |
| `src/agents/isolation.rs` | ~45 | 3 isolation modes |
| `src/safety/kill_switch.rs` | ~280 | 3-level kill switch, PLATFORM_KILLED, audit log |
| `src/safety/auto_triggers.rs` | ~220 | Trigger classification, dedup, T6 cascade |
| `src/safety/quarantine.rs` | ~65 | Forensic state capture |
| `src/safety/kill_gate_bridge.rs` | ~65 | Local ↔ distributed kill coordination |
| `src/safety/notification.rs` | ~290 | 4-channel parallel notification dispatch |
| `src/session/manager.rs` | ~110 | Session CRUD, idle pruning |
| `src/session/lane_queue.rs` | ~100 | Per-session request serialization, backpressure |
| `src/session/compaction.rs` | ~340 | Context window compaction, per-type minimums |
| `src/session/router.rs` | ~40 | Channel → agent routing |
| `src/session/boundary.rs` | ~55 | Max duration / min gap enforcement |
| `src/cost/tracker.rs` | ~65 | DashMap-based cost tracking |
| `src/cost/spending_cap.rs` | ~85 | Pre/post call cap enforcement |
| `src/messaging/protocol.rs` | ~150 | AgentMessage wire format, delegation FSM |
| `src/messaging/dispatcher.rs` | ~290 | Verification pipeline, replay detection, rate limiting |
| `src/messaging/encryption.rs` | ~80 | X25519-XSalsa20-Poly1305 encrypt-then-sign |
| `src/auth/token_auth.rs` | ~25 | Constant-time bearer token validation |
| `src/auth/mtls_auth.rs` | ~90 | Mutual TLS client cert verification |
| `src/auth/auth_profiles.rs` | ~75 | LLM provider credential rotation |
| `src/cli/chat.rs` | ~110 | Interactive REPL with AgentRunner |
| `src/cli/status.rs` | ~40 | Gateway + monitor status query |
| `src/cli/commands.rs` | ~110 | Backup, export, migrate CLI wrappers |

---

## Common Questions

**Q: Why is the gateway both a binary and a library?**
The binary is the `ghost` CLI. The library exposes internal types (`PLATFORM_KILLED`, `KillSwitch`, `SessionCompactor`, etc.) for integration tests and the convergence monitor. Without the library, integration tests would need to spawn the full binary and communicate via HTTP.

**Q: Why does the monitor health check never fail the bootstrap?**
The gateway must always be available. If the monitor hasn't started yet (common in development), the gateway should still run. The `Degraded` state clearly signals that the safety floor is absent, and the health endpoint reports this to monitoring systems.

**Q: Why `SeqCst` for `PLATFORM_KILLED` instead of `Acquire`/`Release`?**
`PLATFORM_KILLED` is the most safety-critical flag in the system. `SeqCst` provides the strongest ordering guarantee — all threads see the same global order of operations. The performance cost is negligible (one atomic per agent loop iteration) and the safety benefit is absolute.

**Q: Why are kill switch tests serialized with a mutex?**
`PLATFORM_KILLED` is a global static. Concurrent tests that set/clear it would interfere with each other. The `KILL_SWITCH_TEST_LOCK` mutex ensures only one test touches the global at a time. Each test resets it before starting.

**Q: Why does compaction track cost separately?**
Compaction is system-initiated — the user didn't ask for it. Charging compaction cost against the user's daily spending cap would be unfair and could cause unexpected pauses. Tracking it separately lets operators monitor compaction overhead without penalizing users.

**Q: Why can't broadcast messages be encrypted?**
Encryption requires a specific recipient's public key. Broadcast messages go to multiple recipients, each with different keys. Supporting broadcast encryption would require encrypting the message N times (once per recipient), which is a different protocol entirely.
