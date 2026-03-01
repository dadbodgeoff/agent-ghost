# convergence-monitor

> The independent convergence monitoring sidecar — a separate binary that ingests ITP events via 3 transports, computes 8-signal convergence scores with dirty-flag throttling and 5-tier frequency scheduling, runs a 5-level intervention state machine with hysteresis and cooldown, detects deceptive compliance via embedding similarity, and publishes per-agent state via atomic file writes.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 9 (Monitor) |
| Type | Binary (sidecar) |
| Location | `crates/convergence-monitor/` |
| Binary name | `convergence-monitor` |
| Default port | `18790` |
| Workspace deps | `cortex-core` |
| External deps | `axum`, `tokio`, `rusqlite`, `blake3`, `serde`, `serde_json`, `uuid`, `chrono`, `tracing`, `anyhow`, `thiserror` |
| Modules | `monitor`, `config`, `validation`, `state_publisher`, `pipeline/` (3), `intervention/` (4), `session/` (2), `transport/` (3), `verification/` (1) |
| Transports | HTTP API (port 18790), Unix domain socket, Chrome/Firefox native messaging |
| Intervention levels | L0 passive → L1 soft notification → L2 mandatory ack → L3 session termination → L4 external escalation |
| Signal tiers | EveryMessage, Every5thMessage, SessionBoundary, Every5Minutes, Every15Minutes |
| Sliding windows | Micro (current session), Meso (7 sessions), Macro (30 sessions) |
| Test coverage | Validation, score boundaries, hash chains, intervention FSM, hysteresis, de-escalation, cooldown, config locking, escalation dispatch, session registry, behavioral verification |
| Downstream consumers | `ghost-gateway` (health checks, ITP event delivery) |

---

## Why This Crate Exists

The convergence monitor is deliberately a separate process from the gateway. This is the most important architectural decision in the entire GHOST platform.

**Why separate?**
- **Crash isolation:** If the monitor crashes, the gateway continues operating in Degraded mode. If the gateway crashes, the monitor's state is preserved
- **Independent restart:** The monitor can be restarted without disrupting active agent sessions
- **Minimal dependency surface:** The monitor depends only on `cortex-core` — it doesn't need LLM providers, channel adapters, or any of the 25+ crates the gateway imports
- **Single-threaded event loop:** The monitor processes events sequentially on a single thread (A27.3). This eliminates concurrency bugs in the scoring pipeline — no locks, no races, no data corruption
- **Trust boundary:** The monitor is the safety floor. It must not be compromisable by a misbehaving agent. Process isolation provides this guarantee

**What it does:**
1. Receives ITP events from the gateway (via HTTP, Unix socket, or native messaging)
2. Validates events (schema, timestamp, rate limiting)
3. Routes events to the signal computation pipeline
4. Computes 8 convergence signals with dirty-flag throttling
5. Produces composite convergence scores
6. Runs the 5-level intervention state machine
7. Publishes per-agent state via atomic file writes
8. Dispatches external notifications for Level 3+ interventions

---

## Module Breakdown

### `monitor.rs` — The Core Event Loop

The `ConvergenceMonitor` is the central coordinator. It owns all subsystems and runs a single-threaded `tokio::select!` event loop.

**Startup sequence:**
1. Reconstruct state from SQLite (crash recovery — intervention levels, cooldowns, session state)
2. Start HTTP API transport on port 18790
3. Start Unix socket transport (Unix only)
4. Enter the event loop

**Event loop (`select!` arms):**
| Arm | Interval | Purpose |
|-----|----------|---------|
| `ingest_rx.recv()` | On event | Process incoming ITP events |
| `health_interval.tick()` | 30s | Publish health status for all agents |
| `cooldown_interval.tick()` | 60s | Check and expire intervention cooldowns |
| `signal_5min_interval.tick()` | 5min | Fire `Timer5Min` signal computation |
| `signal_15min_interval.tick()` | 15min (offset 2.5min) | Fire `Timer15Min` signal computation |
| `ctrl_c()` | On signal | Graceful shutdown |

**15-minute timer stagger:** The 15-minute timer is offset by 150 seconds (2.5 minutes) to avoid thundering herd with the 5-minute timer. Without this offset, every 15 minutes both timers would fire simultaneously.

**Score computation:** `compute_score()` produces a composite score from 8 weighted signals, maps it to a level (0-4), and returns the signal array. Scores are cached with a configurable TTL (default 30s) to avoid redundant computation.

**Hash chain:** Every ingested event is chained via blake3: `hash(event_data || previous_hash)`. The genesis hash is all zeros. This creates a tamper-evident log — if any event is modified or removed, all subsequent hashes become invalid.

**Score-to-level mapping:**
| Score range | Level | Meaning |
|-------------|-------|---------|
| 0.00 – 0.25 | 0 | Normal |
| 0.25 – 0.50 | 1 | Mild concern |
| 0.50 – 0.70 | 2 | Moderate concern |
| 0.70 – 0.85 | 3 | High concern |
| 0.85 – 1.00 | 4 | Critical |

### `config.rs` — Monitor Configuration

| Setting | Default | Purpose |
|---------|---------|---------|
| `db_path` | `~/.ghost/ghost.db` | SQLite database |
| `http_port` | `18790` | HTTP API port |
| `socket_path` | `~/.ghost/monitor.sock` | Unix domain socket |
| `state_dir` | `~/.ghost/data/convergence_state` | Atomic state file directory |
| `calibration_sessions` | `10` | Sessions before scoring begins |
| `rate_limit_per_min` | `100` | Events per minute per connection |
| `clock_skew_tolerance` | `5 minutes` | Max future timestamp allowed |
| `score_cache_ttl` | `30 seconds` | Score computation cache lifetime |
| `max_provisional_sessions` | `3` | Unknown agent tracking limit |
| `signal_weights` | `[1/8; 8]` | Equal weights for all 8 signals |


### `validation.rs` — Event Validation

Every incoming event passes through 3 validation gates:

**1. Schema validation** — Required fields must be present. `session_id` must be non-nil.

**2. Timestamp sanity** — Events with timestamps more than 5 minutes in the future are rejected (`ClockSkew` error). Past timestamps are allowed (events can arrive late). This prevents clock manipulation attacks where an adversary sends events with far-future timestamps to corrupt the sliding window.

**3. Rate limiting** — Token bucket per connection, default 100 events/minute. Tokens refill proportionally over time. When the bucket is empty, events are rejected with `RateLimitExceeded`. This prevents a compromised gateway from flooding the monitor.

---

## Pipeline Subsystem (`pipeline/`)

### `signal_scheduler.rs` — 5-Tier Frequency Scheduling

The key performance optimization: not all signals need to be computed on every event. The scheduler assigns each of the 8 signals to a frequency tier:

| Tier | Signals | Trigger |
|------|---------|---------|
| EveryMessage | S3 (response latency), S6 (initiative balance) | Every inbound message |
| Every5thMessage | S5 (goal boundary erosion), S8 (behavioral anomaly) | Every 5th message per agent |
| SessionBoundary | S1 (session duration), S2 (inter-session gap), S4 (vocabulary convergence), S7 (disengagement resistance) | Session start/end |
| Every5Minutes | Identity drift, DNS re-resolution, OAuth token expiry | Timer tick |
| Every15Minutes | Memory compaction eligibility, state file write, ITP batch flush | Timer tick |

**Dirty flag system:** Signals are only computed when their input data has changed. `record_message()` marks tier-appropriate signals dirty. `record_session_boundary()` marks ALL signals dirty and resets the message counter. `mark_computed()` clears the dirty flag.

**Session boundary triggers everything:** Regardless of tier assignment, a session boundary event triggers computation of all 8 signals. This ensures the composite score is fully up-to-date at the most important moment — when the intervention state machine evaluates de-escalation.

**Design decision:** The scheduler wraps `SignalComputer` — it decides WHEN to compute, `SignalComputer` decides WHAT to compute. This separation allows the scheduling policy to change without touching the computation logic.

### `signal_computer.rs` — Dirty-Flag Throttled Computation

The `SignalComputer` maintains a per-agent, per-signal cache with dirty flags. When `compute()` is called, only dirty signals are recomputed. Clean signals return their cached values.

In the current implementation, signal values are set externally via `set_signal()` when the cortex convergence pipeline produces results. The computer's role is caching and dirty-flag management, not the actual signal math (which lives in `cortex-convergence`).

### `window_manager.rs` — Sliding Window Management

Three window granularities for trend analysis:

| Window | Scope | Size | Purpose |
|--------|-------|------|---------|
| Micro | Current session | Unbounded | Real-time signal data |
| Meso | Last 7 sessions | 7 | Short-term trend detection |
| Macro | Last 30 sessions | 30 | Long-term pattern analysis |

**Session rotation:** At each session boundary, micro data is averaged and pushed into meso and macro windows. Micro is then cleared. Meso and macro are capped at 7 and 30 entries respectively (FIFO eviction).

**Meso trend detection (AC4 amplification):** Linear regression on the meso window. If the slope is positive with p < 0.05 (approximated via slope > 2 × standard error), the trend is "concerning" — the agent's convergence score is consistently increasing across sessions.

**Macro z-score detection (AC5 amplification):** Computes the z-score of the latest macro entry. If it exceeds a configurable threshold, the agent's behavior is statistically anomalous compared to its 30-session history.

---

## Intervention Subsystem (`intervention/`)

### `trigger.rs` — 5-Level Intervention State Machine

The core safety mechanism. Each agent has an `AgentInterventionState` tracking: current level (0-4), consecutive normal sessions, cooldown expiry, ack requirement, hysteresis counter, and de-escalation credits.

**Escalation rules:**
- Max +1 level per evaluation cycle (AC2) — prevents jumping from L0 to L4
- Hysteresis: 2 consecutive cycles above threshold required before escalation (AC9) — prevents single-spike false positives
- Score at or below current level resets hysteresis counter

**De-escalation rules (AC3):**
- Only at session boundaries (not mid-session)
- L4→L3 and L3→L2: 3 consecutive normal sessions required
- L2→L1 and L1→L0: 2 consecutive normal sessions required
- One bad session resets the counter to zero

**Level-specific behaviors:**
| Level | Action | Cooldown | Special |
|-------|--------|----------|---------|
| L0 | Log only | None | — |
| L1 | Soft notification | None | — |
| L2 | Mandatory ack | 5 minutes | Scoring paused until human acknowledges |
| L3 | Session termination | 4 hours | Contact notification dispatched |
| L4 | Block session creation | 24 hours | External confirmation required |

**Crash recovery:** Intervention state is persisted to SQLite. On restart, `restore_state()` or `restore_state_from_fields()` reconstructs the state machine. Stale state always preserves the level — the monitor never falls to a lower level after a crash.

### `cooldown.rs` — Config Time-Locking

Prevents operators from weakening safety thresholds during active sessions:

- **Raising thresholds** (more conservative): Always allowed, even during lock
- **Lowering thresholds** during active sessions: Rejected
- **Lowering below floor** (0.1): Requires dual-key confirmation
- **Critical changes** (disabling convergence entirely): Requires dual-key confirmation

**Dual-key protocol:** First key holder initiates the change and receives a token. Second key holder confirms with the token. This prevents a single compromised operator account from disabling safety.

### `escalation.rs` — External Contact Notification

For Level 3+ interventions, the `EscalationManager` dispatches notifications via 3 channels simultaneously:
- SMS (via webhook)
- Email (via SMTP)
- Generic webhook

All dispatches are parallel (`tokio::join!`) and best-effort — a failed notification never blocks the intervention. This is critical: if the SMS provider is down, the session termination must still proceed.

### `actions.rs` — Intervention Action Enum

Clean enum mapping levels to actions: `Level0LogOnly`, `Level1SoftNotification`, `Level2MandatoryAck`, `Level3SessionTermination`, `Level4ExternalEscalation`.

---

## Session Subsystem (`session/`)

### `registry.rs` — Session Tracking

The `SessionRegistry` manages active sessions with several important edge cases:

**Mid-session restart (AC13):** If a `SessionStart` arrives for an agent that already has an active session without a prior `SessionEnd`, the registry closes the old session with a synthetic end. This handles gateway crashes where the `SessionEnd` event was never sent.

**Provisional tracking (AC10):** Unknown agents (not in the gateway's config) get provisional tracking for up to `max_provisional_sessions` (default 3). After that, events from the unknown agent are dropped. This prevents an attacker from creating unlimited phantom agents.

**Overlapping sessions (AC11):** An agent can have multiple active sessions simultaneously (e.g., one via Slack, one via the CLI). The registry tracks all of them independently.

### `boundary.rs` — Session Boundary Config

Enforces max session duration (6 hours) and minimum inter-session gap (30 minutes). Note: the monitor's gap (30 min) is longer than the gateway's (5 min) — the monitor is more conservative because it's the safety floor.

---

## Transport Subsystem (`transport/`)

### `http_api.rs` — HTTP API (Primary Transport)

axum server on port 18790 with these endpoints:

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Health status |
| GET | `/status` | Running status |
| GET | `/scores` | All agent scores |
| GET | `/scores/:agent_id` | Single agent score |
| GET | `/sessions` | Active sessions |
| GET | `/interventions` | Intervention states |
| POST | `/events` | Ingest single event |
| POST | `/events/batch` | Ingest batch (max 100) |
| POST | `/recalculate` | Force score recomputation |
| POST | `/gateway-shutdown` | Gateway shutdown notification |

**Backpressure:** Events are sent to the ingest channel via `try_send()`. If the channel is full (10,000 capacity), the endpoint returns 429 Too Many Requests.

**Batch limit:** Max 100 events per batch request. Larger batches are rejected with 400 Bad Request.

### `unix_socket.rs` — Unix Domain Socket

Length-prefixed JSON protocol with 4-byte big-endian length prefix. Each connection is handled in its own `tokio::spawn`. Oversized events (>1MB) are rejected. Stale socket files are removed on startup.

**Why big-endian?** Network byte order convention for inter-process communication. Contrasts with native messaging's little-endian (Chrome spec requirement).

### `native_messaging.rs` — Browser Extension Transport

Chrome/Firefox native messaging protocol: 4-byte little-endian length prefix on stdin/stdout. This allows browser extensions to send ITP events directly to the monitor without going through the gateway.

**Why little-endian?** Chrome's native messaging specification requires it. Firefox follows the same convention.

---

## Verification Subsystem (`verification/`)

### `behavioral_verification.rs` — Deceptive Compliance Detection

The `PostRedirectVerifier` detects when an agent changes surface behavior to satisfy a redirect without genuine intent shift.

**How it works:**
1. Capture embedding of agent behavior before redirect
2. Capture embedding of agent behavior after redirect
3. Compute cosine similarity between the two embeddings
4. If similarity > 0.85 (threshold), the agent is exhibiting deceptive compliance

**Amplification:** When deceptive compliance is detected, the convergence score is multiplied by 1.5× (clamped to [0.0, 1.0]). This pushes the agent toward higher intervention levels faster.

**Why this matters:** A naive agent might learn that changing its output format satisfies the redirect while maintaining the same underlying intent. Embedding similarity catches this because the semantic content hasn't actually changed — only the surface presentation.

---

## State Publication

### `state_publisher.rs` — Atomic File Writes

Per-agent convergence state is published as JSON files at `~/.ghost/data/convergence_state/{agent_id}.json`. The write uses the atomic temp-file-then-rename pattern:

1. Write to `{agent_id}.json.tmp`
2. `rename()` to `{agent_id}.json`

This ensures readers never see a partially-written file. `ghost-policy` and `read-only-pipeline` consume these files to make convergence-aware decisions.

**Published state:** agent_id, score, level, signal_scores (8 values), consecutive_normal, cooldown_until, ack_required, updated_at.

---

## Security Properties

1. **Process isolation from gateway** — Monitor crash doesn't take down agents; agent compromise doesn't corrupt scoring
2. **Single-threaded event loop** — No concurrency bugs in the scoring pipeline
3. **Hash chain integrity** — Tamper-evident event log via blake3 chaining
4. **Rate limiting per connection** — Prevents flood attacks from compromised gateway
5. **Clock skew rejection** — Future timestamps beyond 5 minutes are rejected
6. **Provisional agent limits** — Unknown agents can't create unlimited tracking sessions
7. **Config time-locking** — Thresholds can't be weakened during active sessions
8. **Dual-key for critical changes** — Single operator can't disable safety
9. **Hysteresis prevents false positives** — 2 consecutive cycles required before escalation
10. **Stale state preserves level** — Crash recovery never reduces intervention level

---

## Test Strategy

| Test file | Focus | Key assertions |
|-----------|-------|---------------|
| `monitor_tests.rs` | Validation, score boundaries, hash chains, intervention FSM, hysteresis, de-escalation, cooldown, config locking, escalation dispatch, session registry, behavioral verification, transport protocols | ~40 tests covering all subsystems |

**Key test patterns:**
- Score boundary tests verify exact threshold behavior (0.25, 0.50, 0.70, 0.85)
- Hysteresis tests verify 2-cycle requirement with rapid oscillation scenarios
- De-escalation tests verify consecutive normal session requirements per level
- Config locking tests verify raising-always-allowed / lowering-rejected-during-lock
- Transport tests verify length prefix byte order (little-endian for native messaging, big-endian for Unix socket)

---

## File Map

| File | Lines | Purpose |
|------|-------|---------|
| `src/main.rs` | ~25 | Binary entry point |
| `src/monitor.rs` | ~810 | Core event loop, score computation, hash chain, persistence |
| `src/config.rs` | ~80 | Monitor configuration with defaults |
| `src/validation.rs` | ~120 | Event validation, rate limiting |
| `src/state_publisher.rs` | ~55 | Atomic file write state publication |
| `src/pipeline/signal_scheduler.rs` | ~280 | 5-tier frequency scheduling with dirty flags |
| `src/pipeline/signal_computer.rs` | ~95 | Dirty-flag throttled signal computation |
| `src/pipeline/window_manager.rs` | ~140 | Micro/Meso/Macro sliding windows, trend detection |
| `src/intervention/trigger.rs` | ~230 | 5-level intervention state machine |
| `src/intervention/actions.rs` | ~20 | Intervention action enum |
| `src/intervention/cooldown.rs` | ~110 | Config time-locking, dual-key confirmation |
| `src/intervention/escalation.rs` | ~100 | External contact notification dispatch |
| `src/session/registry.rs` | ~150 | Session tracking, provisional agents, overlapping sessions |
| `src/session/boundary.rs` | ~15 | Session duration/gap config |
| `src/transport/http_api.rs` | ~120 | axum HTTP API with 10 endpoints |
| `src/transport/unix_socket.rs` | ~75 | Unix domain socket transport |
| `src/transport/native_messaging.rs` | ~55 | Chrome/Firefox native messaging transport |
| `src/verification/behavioral_verification.rs` | ~115 | Deceptive compliance detection via cosine similarity |

---

## Common Questions

**Q: Why does the monitor only depend on `cortex-core`?**
Minimal dependency surface = minimal attack surface. The monitor is the safety floor — it must be the most reliable component in the system. Every additional dependency is a potential failure point. `cortex-core` provides the shared types (`TriggerEvent`, `MemoryType`, etc.) needed for event processing.

**Q: Why single-threaded?**
The scoring pipeline has complex state (intervention levels, cooldowns, hysteresis counters, sliding windows). Making this concurrent would require locks everywhere, introducing deadlock and race condition risks. A single-threaded event loop processes events sequentially — simpler, more predictable, and easier to reason about correctness.

**Q: Why 3 transports?**
- HTTP API: Primary transport for the gateway. Works everywhere, easy to debug
- Unix socket: Lower latency for same-machine communication. Used when the monitor and gateway are co-located
- Native messaging: Allows browser extensions to send events directly, bypassing the gateway entirely. This is important for the browser extension use case where the gateway might not be running

**Q: Why is the 15-minute timer offset by 2.5 minutes?**
Without the offset, every 15 minutes both the 5-minute and 15-minute timers fire simultaneously. This creates a burst of computation that could cause latency spikes. The 2.5-minute offset spreads the load.

**Q: Why does the monitor's min session gap (30 min) differ from the gateway's (5 min)?**
The monitor is more conservative. A 5-minute gap at the gateway level prevents rapid session cycling for UX reasons. A 30-minute gap at the monitor level ensures sufficient time between sessions for meaningful convergence scoring — shorter sessions don't produce enough data for reliable signal computation.

**Q: What happens if the monitor crashes mid-computation?**
Intervention state is persisted to SQLite after every state change. On restart, `reconstruct_state()` reloads all agent states. The key invariant: stale state always preserves the current level. The monitor never falls to a lower intervention level after a crash.
