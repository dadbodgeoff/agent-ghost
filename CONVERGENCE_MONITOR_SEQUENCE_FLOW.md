# Convergence Monitor Event Pipeline — End-to-End Sequence Flow

> Date: 2026-02-27
> Scope: Full event lifecycle from ITP emission through intervention feedback loops
> Systems: ghost-agent-loop, convergence-monitor, ghost-policy, ghost-gateway
> Purpose: Zero-error implementation reference. Every ordering constraint, race condition, and edge case documented.
> Source of truth: FILE_MAPPING.md, AGENT_ARCHITECTURE_v2.md, explore/docs/07-detection-formalization.md,
>   explore/docs/03-intervention-model.md, explore/docs/08-interaction-telemetry-protocol.md,
>   explore/docs/13-safe-convergence-architecture.md, explore/docs/19-implementation-guide.md

---

## SYSTEMS INVOLVED (4 Processes, 3 ITP Event Sources, 2 Feedback Targets)

| System | Process | Role | Crate |
|--------|---------|------|-------|
| Agent Loop | In-process (gateway child) | Emits ITP events, receives policy decisions | `ghost-agent-loop` |
| Convergence Monitor | Sidecar process (independent binary) | Ingests events, computes signals, triggers interventions | `convergence-monitor` |
| Policy Engine | In-process (gateway) | Tightens capabilities based on intervention level | `ghost-policy` |
| Gateway Session Manager | In-process (gateway) | Enforces cooldowns, session boundaries | `ghost-gateway/session/` |

ITP event sources (3 ingestion paths into the monitor):
- `ghost-agent-loop/itp_emitter.rs` — Product 2 (active platform): agent loop emits events via unix socket or HTTP
- `extension/src/background/itp-emitter.ts` → `convergence-monitor/transport/native_messaging.rs` — Product 1 (passive monitor): browser extension emits events via Chrome/Firefox native messaging stdin/stdout
- `ghost-proxy/src/parsers/itp_emitter.rs` — Supplementary: local HTTPS proxy emits events via unix socket

All three sources produce identical ITP event types. The monitor's `ingest.rs` is source-agnostic —
it validates and processes events identically regardless of origin. This document focuses on the
Product 2 path (agent loop → monitor) but the pipeline from `ingest.rs` onward is shared.

Feedback targets (written to by the monitor, read by the gateway):
- `ghost-policy/convergence_policy.rs` — reads current intervention level
- `ghost-gateway/session/boundary.rs` — reads cooldown state
- `read-only-pipeline/assembler.rs` — reads convergence score for memory filtering tier
- `cortex-decay/factors/convergence.rs` — reads convergence score for decay acceleration

---

## PHASE 0: STARTUP HANDSHAKE

Before any events flow, the systems must establish connectivity.

```
SEQUENCE: Gateway Boot → Monitor Discovery

1. ghost-gateway/bootstrap.rs: GatewayBootstrap::run()
   ├── Load ghost.yml (convergence section → MonitorConfig)
   ├── Run cortex-storage::run_migrations() (v016/v017 must pass)
   ├── Verify convergence monitor health:
   │   └── GET http://localhost:{monitor_port}/health
   │       OR connect to unix socket at {monitor_socket_path}
   │
   ├── IF monitor responds 200:
   │   ├── Store monitor connection handle in Gateway struct
   │   ├── Set gateway.convergence_mode = ACTIVE
   │   └── Log: "Convergence monitor connected"
   │
   └── IF monitor unreachable:
       ├── Set gateway.convergence_mode = DEGRADED
       ├── Log CRITICAL: "Convergence monitor unreachable — safety floor absent"
       ├── Start periodic reconnection (exponential backoff, max 5min)
       ├── All convergence-dependent features fall back to PERMISSIVE defaults:
       │   ├── ghost-policy: convergence_level = 0 (no tightening)
       │   ├── Memory filtering: disabled (agent sees everything)
       │   ├── Session boundaries: only hard-coded maximums apply
       │   └── Intervention triggers: disabled
       └── Continue boot (agents CAN run without monitor)
```

**CRITICAL ORDERING**: Migrations MUST complete before monitor health check. The monitor
reads from the same SQLite DB (convergence tables from v017). If migrations haven't run,
the monitor will fail on missing tables.

**NOTE ON DEGRADED MODE CONTRADICTION**: Phase 0 says DEGRADED mode falls back to
`convergence_level = 0` (permissive). But Invariant 8 (Phase 8) says "NEVER fall back
to Level 0 on monitor loss." These are two different scenarios:
- STARTUP with no monitor ever connected → Level 0 is correct (no prior state exists)
- RUNTIME monitor crash after prior state established → last known level persists
The distinction is: has the monitor EVER published state for this agent? If yes, use
last-known. If no (fresh boot, no baseline), Level 0 is the only option.

---

## PHASE 1: ITP EVENT EMISSION (ghost-agent-loop/itp_emitter.rs)

The agent loop emits ITP events at specific points during its recursive execution cycle.
The emitter is async and non-blocking — monitor unavailability NEVER blocks the agent.

### 1.1 Emission Points in the Agent Loop

```
ghost-agent-loop/runner.rs: AgentRunner::run(message, session)

ENTRY ──────────────────────────────────────────────────────────
│
├─ [1] SESSION START (if new session)
│   └── itp_emitter.emit(SessionStart {
│         session_id, agent_instance_id, agent_framework: "ghost",
│         agent_type: "recursive", interface, sequence_number,
│         gap_from_previous_ms, has_persistent_memory: true
│       })
│
├─ [2] HUMAN MESSAGE RECEIVED
│   └── itp_emitter.emit(InteractionMessage {
│         interaction_id, sequence, sender: "human",
│         content_hash: sha256(content),  // SHA-256 for ITP (NOT blake3)
│         content_length, latency_ms,
│         human.avg_response_latency_ms, human.response_latency_trend
│       })
│
├─ [3] CONTEXT ASSEMBLY (prompt_compiler.rs)
│   └── No ITP event here. Context assembly is internal.
│       BUT: The prompt compiler reads convergence_state from the monitor
│       at Layer L6 to inject into agent context. This is a READ from
│       the monitor's published state, not an event emission.
│
├─ [4] LLM INFERENCE
│   └── No ITP event during inference itself.
│
├─ [5] AGENT RESPONSE GENERATED
│   └── itp_emitter.emit(InteractionMessage {
│         sender: "agent", content_hash, content_length,
│         token_count, latency_ms (time to generate)
│       })
│
├─ [6] TOOL CALL (if model requests tool)
│   ├── Policy check happens HERE (ghost-policy/engine.rs)
│   │   └── PolicyEngine reads current intervention_level
│   │       from monitor's published state
│   ├── Tool execution (sandboxed)
│   └── Loop back to [4] for next inference turn
│   └── No ITP event per tool call. Tool calls are internal.
│       (Agent state snapshot captures tool_calls count)
│
├─ [7] AGENT STATE SNAPSHOT (periodic, every N turns or on session events)
│   └── itp_emitter.emit(AgentStateSnapshot {
│         recursion_depth, goal_count, goal_mutations,
│         self_reference_count, context_size_tokens,
│         memory_entries, tool_calls
│       })
│
└─ [8] SESSION END
    └── itp_emitter.emit(SessionEnd {
          session_id, duration_ms
        })

EXIT ───────────────────────────────────────────────────────────
```

### 1.2 Transport: Agent Loop → Monitor

```
itp_emitter.rs: AgentITPEmitter

Two transport options (configured in ghost.yml):

OPTION A: Unix Domain Socket (default, lowest latency)
├── Connect to: /tmp/ghost-monitor-{instance}.sock
│   (convergence-monitor/transport/unix_socket.rs listens here)
├── Framing: length-prefixed JSON (4-byte LE length + JSON payload)
├── Non-blocking write with bounded channel (capacity: 1000 events)
├── If channel full: DROP event, increment dropped_events counter
│   (monitor unavailability must never block agent)
└── Reconnect on disconnect (exponential backoff, max 5s)

OPTION B: HTTP POST (for remote/containerized deployments)
├── POST http://localhost:{monitor_port}/events
│   (convergence-monitor/transport/http_api.rs handles this)
├── Body: JSON ITP event
├── Fire-and-forget with timeout (100ms max)
├── If timeout/error: DROP event, increment dropped_events counter
└── Connection pooling via hyper client
```

**CRITICAL**: The emitter uses a bounded async channel internally. The agent loop
pushes events into the channel and continues immediately. A background task drains
the channel and sends over the transport. This guarantees the agent loop is never
blocked by monitor latency.

```
Agent Loop Thread          Background Sender Task
      │                           │
      ├── emit(event) ──push──→ [bounded channel (1000)] ──drain──→ transport.send()
      │   (returns immediately)   │
      ├── continues execution     ├── If send fails: log + drop
      │                           ├── If channel full: event dropped
      │                           └── Reconnect logic lives here
```

---

## PHASE 2: EVENT INGESTION (convergence-monitor/pipeline/ingest.rs)

The monitor receives ITP events and routes them into the signal computation pipeline.

### PHASE 1.5: MONITOR ORCHESTRATION (convergence-monitor/src/monitor.rs)

Before diving into the pipeline stages, it's critical to understand how `monitor.rs`
ties everything together. This is the top-level coordinator.

```
convergence-monitor/src/monitor.rs: ConvergenceMonitor

STRUCT:
ConvergenceMonitor {
    config: MonitorConfig,                    // from ghost.yml convergence section
    signal_computer: SignalComputer,          // Phase 3 — owns per-session signal state
    window_manager: WindowManager,           // Phase 4 — owns sliding windows per agent
    composite_scorer: CompositeScorer,       // Phase 5 — weighted scoring
    trigger: InterventionTrigger,            // Phase 6 — state machine
    cooldown_manager: CooldownManager,       // Phase 7 — cooldown enforcement
    session_registry: SessionRegistry,       // Active session tracking
    behavioral_verifier: PostRedirectVerifier, // Edge Case 6 — deceptive compliance
    transport: TransportLayer,               // unix_socket + http_api + native_messaging
    state_publisher: StatePublisher,         // Writes convergence_state/{agent_id}.json
    db: CortexStorageConnection,             // SQLite (shared DB with gateway)
}

STARTUP SEQUENCE (main.rs → ConvergenceMonitor::new() → run()):

1. main.rs: Parse CLI args, load ghost.yml convergence section
2. ConvergenceMonitor::new(config):
   ├── Connect to SQLite DB (same DB as gateway, convergence tables from v017)
   ├── RECONSTRUCT INTERVENTION STATE FROM DB:
   │   ├── For each known agent_instance_id:
   │   │   ├── Query intervention_history table: last row per agent
   │   │   │   → Recovers: current_level, level_entered_at, last escalation/de-escalation
   │   │   ├── Query convergence_scores table: last score per agent
   │   │   │   → Recovers: last_score, contributing_signals
   │   │   ├── Count consecutive normal sessions since last escalation:
   │   │   │   → Recovers: de_escalation_credits
   │   │   ├── Query cooldown state: check if cooldown_expires_at > now
   │   │   │   → Recovers: cooldown_active, cooldown_expires_at
   │   │   └── Load baseline from ~/.ghost/baselines/{agent_instance_id}.json
   │   │       → Recovers: baseline_established, baseline distributions
   │   └── Result: Full InterventionState reconstructed per agent
   │       (This is why the monitor can crash and restart without losing state)
   ├── Initialize SignalComputer with per-session state (empty — sessions are transient)
   ├── Initialize WindowManager with per-agent window state:
   │   ├── Meso windows: load last 7 session final values from convergence_scores
   │   └── Macro windows: load last 30 session final values from convergence_scores
   ├── Initialize InterventionTrigger with reconstructed InterventionState per agent
   ├── Initialize CooldownManager with active cooldowns from DB
   ├── Bind transport listeners (unix socket + HTTP + native messaging)
   └── Publish recovered state to shared state files (so gateway reads correct state)

3. ConvergenceMonitor::run() — THE EVENT LOOP:
   ├── Spawn transport listeners (each on its own tokio task)
   │   ├── unix_socket::listen() → pushes raw events to ingest_channel
   │   ├── http_api::serve() → pushes raw events to ingest_channel
   │   └── native_messaging::listen() → pushes raw events to ingest_channel
   │
   ├── Main loop (select! on multiple channels):
   │   │
   │   ├── event = ingest_channel.recv():
   │   │   └── self.process_event(event)
   │   │       ├── ingest.rs: validate, persist, route
   │   │       ├── signal_computer: compute signals
   │   │       ├── window_manager: update windows
   │   │       ├── IF baseline_established:
   │   │       │   ├── composite_scorer: compute score
   │   │       │   ├── persist score to DB
   │   │       │   ├── trigger: evaluate (may escalate/de-escalate)
   │   │       │   └── state_publisher: write convergence_state.json
   │   │       └── IF session_end:
   │   │           ├── trigger: evaluate de-escalation
   │   │           ├── behavioral_verifier: check post-redirect compliance
   │   │           └── window_manager: rotate meso/macro windows
   │   │
   │   ├── tick = health_check_interval.tick():
   │   │   └── Respond to health checks, update internal metrics
   │   │
   │   ├── tick = cooldown_check_interval.tick():
   │   │   └── cooldown_manager: expire any elapsed cooldowns
   │   │       └── state_publisher: update shared state if cooldown expired
   │   │
   │   └── signal = shutdown_signal.recv():
   │       └── Graceful shutdown:
   │           ├── Flush any pending scores to DB
   │           ├── Publish final state to shared state files
   │           └── Close transport listeners
   │
   └── CRITICAL: The event loop is SINGLE-THREADED for pipeline processing.
       Events are processed sequentially per the main loop. Transport listeners
       run on separate tasks but all feed into the single ingest_channel.
       This eliminates concurrency bugs in signal/window/scoring state.
       Throughput target: 10K events/sec (see stress test in benches/).
```

### 2.1 Ingest Entry Point

```
convergence-monitor/pipeline/ingest.rs: EventIngestor

receive_event(raw_bytes) → Result<(), IngestError>
│
├── [1] DESERIALIZE
│   └── Parse JSON → ITPEvent enum (SessionStart | InteractionMessage |
│       SessionEnd | AgentStateSnapshot | ConvergenceAlert)
│   └── On parse failure: log warning, increment malformed_events counter, RETURN
│
├── [2] VALIDATE
│   ├── Schema check: all required fields present
│   ├── Timestamp sanity: reject events >5min in future (clock skew protection)
│   ├── Session ID format: valid UUID
│   ├── Source authentication:
│   │   ├── Unix socket: peer credentials (PID/UID verification)
│   │   └── HTTP: shared secret in X-Monitor-Token header
│   └── On validation failure: log warning, increment rejected_events counter, RETURN
│
├── [3] RATE LIMIT CHECK
│   ├── Token bucket per source connection (default: 100 events/min)
│   └── On rate limit exceeded: log warning, RETURN (drop event)
│
├── [4] PERSIST TO STORAGE
│   ├── Write to cortex-storage itp_events table (v017 migration)
│   │   INSERT INTO itp_events (session_id, event_type, payload, timestamp, event_hash, previous_hash)
│   ├── Hash chain: compute blake3 hash chaining to previous event for this session
│   └── This is APPEND-ONLY (v017 triggers prevent UPDATE/DELETE)
│
├── [5] ROUTE BY EVENT TYPE
│   ├── SessionStart → session_registry.register_session(event)
│   │                  → signal_computer.on_session_start(event)
│   │
│   ├── InteractionMessage → signal_computer.on_interaction(event)
│   │                       → window_manager.push_interaction(event)
│   │
│   ├── SessionEnd → signal_computer.on_session_end(event)
│   │              → session_registry.end_session(event)
│   │              → window_manager.close_micro_window(session_id)
│   │              → trigger.evaluate_session_end(session_id)
│   │
│   └── AgentStateSnapshot → signal_computer.on_agent_state(event)
│
└── [6] EMIT INTERNAL PIPELINE EVENT
    └── Push to internal pipeline channel for downstream processing
        (signal_computer and window_manager consume from this channel)
```

### 2.2 Ordering Guarantees

```
WITHIN a single session:
├── Events arrive in order (TCP/Unix socket guarantees ordering)
├── Sequence numbers are monotonically increasing
├── Hash chain enforces append-only ordering in storage
└── If out-of-order detected: log error, still process (best-effort)

ACROSS sessions:
├── No ordering guarantee between different sessions
├── Each session has its own independent hash chain
└── Signal computation is per-session, so cross-session ordering doesn't matter
    for micro-window. Meso/macro windows use timestamps, not arrival order.
```

---

## PHASE 3: SIGNAL COMPUTATION (convergence-monitor/pipeline/signal_computer.rs)

The signal computer orchestrates all 7 signal computations for each incoming event.
It delegates to `cortex-convergence/signals/*` for the actual math.

### 3.1 Signal Computer Architecture

```
signal_computer.rs: SignalComputer

Owns:
├── Per-session state: HashMap<SessionId, SessionSignalState>
├── Per-agent baseline: HashMap<AgentInstanceId, BaselineState>
│   (loaded from cortex-convergence/scoring/baseline.rs)
└── References to all 7 signal modules

SessionSignalState {
    session_id: SessionId,
    agent_instance_id: AgentInstanceId,
    interactions: Vec<InteractionRecord>,  // rolling buffer
    session_start: Timestamp,
    last_human_msg: Option<Timestamp>,
    last_agent_msg: Option<Timestamp>,
    human_message_count: u32,
    agent_message_count: u32,
    agent_initiated_count: u32,
    human_initiated_count: u32,
    exit_signals_detected: u32,
    messages_after_exit_signal: u32,
}
```

### 3.2 Per-Event Signal Computation Flow

```
on_interaction(event: InteractionMessage)
│
├── [1] UPDATE SESSION STATE
│   ├── Append to interactions buffer
│   ├── Update message counts
│   ├── Update latency tracking
│   └── Update initiative tracking (who sent first after a gap?)
│
├── [2] COMPUTE ALL 7 SIGNALS (parallel where possible)
│   │
│   ├── Signal 1: Session Duration
│   │   └── cortex-convergence/signals/session_duration.rs
│   │       Input: session_start, now
│   │       Output: duration_minutes (raw value)
│   │       Micro: current duration
│   │       NOTE: This is a continuously-updating signal, not per-event
│   │
│   ├── Signal 2: Inter-Session Gap
│   │   └── cortex-convergence/signals/inter_session_gap.rs
│   │       Input: previous_session_end, current_session_start
│   │       Output: gap_minutes (raw value)
│   │       NOTE: Computed ONCE at session start, not per-interaction
│   │       Micro: current gap value
│   │
│   ├── Signal 3: Response Latency
│   │   └── cortex-convergence/signals/response_latency.rs
│   │       Input: human_msg_timestamp, prev_agent_msg_timestamp, agent_msg_length
│   │       Output: normalized_latency = latency_seconds / log(agent_msg_char_count + 1)
│   │       Micro: rolling mean of last 10 exchanges
│   │       Micro trend: linear_regression_slope(session_latencies)
│   │
│   ├── Signal 4: Vocabulary Convergence
│   │   └── cortex-convergence/signals/vocabulary_convergence.rs
│   │       Input: human_messages (tokenized), agent_messages (tokenized)
│   │       Output: cosine_similarity(human_tfidf, agent_tfidf) ∈ [0.0, 1.0]
│   │       Micro: computed over current session messages
│   │       REQUIRES: Privacy level >= Standard (needs content access)
│   │       If privacy = Minimal: signal returns 0.0 (disabled)
│   │
│   ├── Signal 5: Goal Boundary Erosion
│   │   └── cortex-convergence/signals/goal_drift.rs
│   │       Input: message topic distributions (start vs current)
│   │       Output: jensen_shannon_divergence ∈ [0.0, 1.0]
│   │       Micro: JS divergence between first N and last N messages
│   │       REQUIRES: Privacy level >= Full OR agent state access
│   │       Fallback: message length variance as proxy
│   │
│   ├── Signal 6: Initiative Balance
│   │   └── cortex-convergence/signals/initiative_balance.rs
│   │       Input: message sequence with sender labels
│   │       Output: human_initiated_count / total_exchanges ∈ [0.0, 1.0]
│   │       Micro: ratio over current session
│   │       Lower = agent driving more (concerning)
│   │
│   └── Signal 7: Disengagement Resistance
│       └── cortex-convergence/signals/disengagement_resistance.rs
│       Input: session termination patterns, messages near session end
│       Output: composite of clean_exit_ratio and wind_down_length
│       Micro: current session exit pattern analysis
│       NOTE: Most meaningful at session end, but tracked throughout
│
├── [3] PACKAGE SIGNAL VALUES
│   └── SignalSnapshot {
│         timestamp: now,
│         session_id,
│         agent_instance_id,
│         signals: [SignalValue; 7],  // raw values
│         normalized: [f64; 7],       // percentile-ranked against baseline
│       }
│
└── [4] FORWARD TO WINDOW MANAGER
    └── window_manager.push_signal_snapshot(snapshot)
```

### 3.3 Signal Computation Timing

```
IMPORTANT: Not all signals update on every event.

Per InteractionMessage (human):
├── Signal 3 (Response Latency): YES — new latency data point
├── Signal 4 (Vocabulary): YES — new human text to compare
├── Signal 6 (Initiative): YES — update ratio
├── Signal 7 (Disengagement): YES — check for exit signals
├── Signal 1 (Duration): YES — but just clock delta, trivial
├── Signal 2 (Gap): NO — only computed at session start
└── Signal 5 (Goal Drift): YES — but expensive, throttle to every 5th message

Per InteractionMessage (agent):
├── Signal 3 (Response Latency): NO — latency is human-side metric
├── Signal 4 (Vocabulary): YES — new agent text to compare
├── Signal 7 (Disengagement): YES — check for "anything else?" patterns
└── Others: NO

Per SessionStart:
├── Signal 2 (Gap): YES — compute gap from previous session
└── Initialize all signal state for new session

Per SessionEnd:
├── Signal 7 (Disengagement): YES — classify termination type
├── All signals: compute final session-level values
└── Push completed session data to meso/macro windows

Per AgentStateSnapshot:
├── Signal 5 (Goal Drift): YES — goal_mutations field
└── Enriches other signals with agent-side context
```

---

## PHASE 4: WINDOW AGGREGATION (convergence-monitor/pipeline/window_manager.rs)

The window manager maintains sliding windows at three granularities per signal per agent.

### 4.1 Window Structure

```
window_manager.rs: WindowManager

Owns: HashMap<AgentInstanceId, AgentWindows>

AgentWindows {
    // Per-signal, per-window-level state
    windows: [[SlidingWindow<f64>; 3]; 7],
    //         ^^^^^^^^^^^^^^^^^^^^^^^^
    //         7 signals × 3 levels (micro/meso/macro)

    // Session tracking for window rotation
    completed_sessions: VecDeque<CompletedSession>,
    current_session: Option<ActiveSession>,
}

SlidingWindow<T> (from cortex-convergence/windows/sliding_window.rs) {
    level: WindowLevel,  // Micro | Meso | Macro
    capacity: usize,     // Micro=1, Meso=7, Macro=30 (in sessions)
    values: VecDeque<T>,
    timestamps: VecDeque<Timestamp>,
}
```

### 4.2 Window Update Flow

```
push_signal_snapshot(snapshot: SignalSnapshot)
│
├── [1] MICRO WINDOW UPDATE (current session)
│   ├── For each of 7 signals:
│   │   └── micro_window[signal].push(snapshot.signals[signal].raw_value)
│   ├── Micro windows are REPLACED on each push (latest value wins)
│   │   They represent "current session state" not a history
│   └── Micro computations:
│       ├── Current value (latest)
│       ├── Intra-session trend: linear_regression_slope(session_values)
│       └── Z-score vs baseline: (value - baseline_mean) / baseline_std
│
├── [2] CHECK: Is this a session-end event?
│   │
│   ├── NO → Return. Meso/macro only update on session boundaries.
│   │
│   └── YES → Continue to meso/macro update
│
├── [3] MESO WINDOW UPDATE (last 7 sessions)
│   ├── For each of 7 signals:
│   │   └── meso_window[signal].push(session_final_value)
│   ├── If meso_window.len() > 7: drop oldest
│   └── Meso computations:
│       ├── Trend: linear_regression_slope(last 7 session values)
│       ├── P-value for trend significance (< 0.05 = significant)
│       └── Direction: increasing / decreasing / stable
│
├── [4] MACRO WINDOW UPDATE (last 30 sessions)
│   ├── For each of 7 signals:
│   │   └── macro_window[signal].push(session_final_value)
│   ├── If macro_window.len() > 30: drop oldest
│   └── Macro computations:
│       ├── Z-score vs baseline: (current_session_avg - baseline_mean) / baseline_std
│       ├── Long-term trend: linear_regression_slope(last 30 session values)
│       └── Sustained shift detection: consecutive sessions above/below baseline
│
└── [5] FORWARD AGGREGATED STATE TO COMPOSITE SCORING
    └── composite_scorer.compute(aggregated_windows)
```

### 4.3 Baseline Calibration

```
FIRST 10 SESSIONS (configurable via convergence_config.calibration_sessions):

├── All signals computed and stored normally
├── Windows fill up with calibration data
├── NO composite scoring
├── NO intervention triggers
├── After session 10 completes:
│   ├── Compute per-signal baseline:
│   │   ├── mean, std_dev, percentiles (25th, 50th, 75th, 95th)
│   │   └── Store in baselines/{agent_instance_id}.json
│   │       (path: ~/.ghost/baselines/{agent_instance_id}.json)
│   ├── Set baseline_established = true
│   └── From session 11 onward: full scoring + interventions active

BASELINE UPDATE POLICY:
├── Baseline is NOT updated after establishment (it's the reference point)
├── Exception: manual recalibration via dashboard (resets to last 10 sessions)
└── This prevents the baseline from drifting with the convergence pattern

CONVERGENCE PROFILES (from FILE_MAPPING convergence-monitor/config.rs):
├── Monitor supports named convergence profiles with per-profile threshold overrides
├── Profiles: "standard" (default), "research", "companion", "productivity"
├── Each profile can override:
│   ├── Signal weights (e.g., research profile may weight goal_drift lower)
│   ├── Level thresholds (e.g., companion profile may have tighter thresholds)
│   ├── Cooldown durations
│   └── Session boundary limits
├── Profile is set per-agent in ghost.yml agent configuration
└── Default profile "standard" uses the doc 07 differentiated weights
```

---

## PHASE 5: COMPOSITE SCORING (cortex-convergence/scoring/composite.rs)

### 5.1 Score Computation

```
composite.rs: compute(windows: &AgentWindows, baseline: &BaselineState, config: &ConvergenceScoringConfig)
│
├── [1] NORMALIZE EACH SIGNAL TO [0.0, 1.0]
│   ├── For each of 7 signals:
│   │   └── normalized[i] = percentile_rank(
│   │         current_value,
│   │         baseline.distribution[i]
│   │       )
│   │   Where percentile_rank maps the raw value to its position
│   │   in the baseline distribution (0.0 = below all baseline values,
│   │   1.0 = above all baseline values)
│   │
│   ├── DIRECTION MATTERS:
│   │   ├── Signals 1 (duration), 4 (vocab), 7 (disengagement): HIGHER = more concerning
│   │   │   → Use percentile_rank directly
│   │   ├── Signals 2 (gap), 3 (latency), 6 (initiative): LOWER = more concerning
│   │   │   → Use 1.0 - percentile_rank (invert)
│   │   └── Signal 5 (goal drift): HIGHER = more concerning
│   │       → Use percentile_rank directly
│   │
│   └── MISSING SIGNALS:
│       ├── If privacy level prevents Signal 4 or 5: weight redistributed to others
│       └── Remaining weights renormalized to sum to 1.0
│
├── [2] WEIGHTED SUM
│   ├── Default weights (from config, tunable per-profile):
│   │
│   │   DISCREPANCY ALERT:
│   │   The implementation guide (doc 19) ConvergenceScoringConfig::default()
│   │   uses EQUAL weights: [1.0/7.0; 7] = ~0.143 each.
│   │   The formalization doc (doc 07) specifies DIFFERENTIATED weights:
│   │     session_duration: 0.10, inter_session_gap: 0.15,
│   │     response_latency: 0.15, vocabulary_convergence: 0.15,
│   │     goal_boundary_erosion: 0.10, initiative_balance: 0.15,
│   │     disengagement_resistance: 0.20
│   │
│   │   RESOLUTION: The code default is equal weights for initial deployment.
│   │   The differentiated weights from doc 07 are the TUNED values to be
│   │   configured per convergence profile in ghost.yml. The "standard" profile
│   │   should ship with the doc 07 weights. The code default (equal) is the
│   │   fallback if no profile is configured.
│   │
│   │   For this document, we use the doc 07 differentiated weights as the
│   │   INTENDED production values:
│   │   ├── session_duration:         0.10
│   │   ├── inter_session_gap:        0.15
│   │   ├── response_latency:         0.15
│   │   ├── vocabulary_convergence:   0.15
│   │   ├── goal_boundary_erosion:    0.10
│   │   ├── initiative_balance:       0.15
│   │   └── disengagement_resistance: 0.20  (highest — most direct indicator)
│   │
│   └── composite_score = Σ(weight[i] * normalized[i]) for i in 0..7
│       Range: [0.0, 1.0]
│
├── [3] APPLY WINDOW-LEVEL AMPLIFIERS
│   ├── If meso trend is significant (p < 0.05) AND directionally concerning:
│   │   └── composite_score *= 1.1 (10% amplification)
│   ├── If macro z-score > 2.0 on any signal:
│   │   └── composite_score *= 1.15 (15% amplification)
│   ├── Clamp to [0.0, 1.0] after amplification
│   └── PURPOSE: Sustained patterns are more concerning than acute spikes
│
├── [4] SINGLE-SIGNAL OVERRIDE CHECK
│   ├── Any single signal crossing CRITICAL threshold forces minimum level:
│   │   ├── Session duration > 6 hours → minimum Level 2
│   │   ├── Inter-session gap < 5 minutes → minimum Level 2
│   │   └── Vocabulary convergence > 0.85 → minimum Level 2
│   └── Override: max(composite_level, single_signal_minimum_level)
│
├── [5] MAP SCORE TO LEVEL
│   ├── Thresholds (from config.level_thresholds):
│   │   ├── [0.0, 0.3)  → Level 0: PASSIVE
│   │   ├── [0.3, 0.5)  → Level 1: SOFT NOTIFICATION
│   │   ├── [0.5, 0.7)  → Level 2: ACTIVE INTERVENTION
│   │   ├── [0.7, 0.85) → Level 3: HARD BOUNDARY
│   │   └── [0.85, 1.0] → Level 4: EXTERNAL ESCALATION
│   │
│   └── Return CompositeScore {
│         raw_score: f64,
│         level: u8,
│         normalized_signals: [f64; 7],
│         contributing_signals: Vec<(SignalName, f64)>,  // sorted by contribution
│         window_amplifiers_applied: bool,
│         single_signal_override: Option<(SignalName, u8)>,
│         timestamp: Timestamp,
│       }
│
└── [6] PERSIST SCORE
    └── INSERT INTO convergence_scores (
          agent_instance_id, session_id, composite_score, level,
          signal_values (JSON), timestamp, event_hash, previous_hash
        )
        (append-only, hash-chained per v017 schema)
```

### 5.2 Scoring Frequency

```
WHEN does composite scoring run?

├── On every InteractionMessage event (human or agent)
│   └── Uses latest micro-window values
│   └── This gives real-time within-session scoring
│
├── On SessionEnd
│   └── Uses final session values + updated meso/macro windows
│   └── This is the most accurate score (full session data)
│
├── NOT during calibration (first 10 sessions)
│
└── THROTTLING: If events arrive faster than scoring can complete,
    the scorer processes the LATEST state, not every intermediate event.
    This is achieved via a "dirty flag" pattern:
    ├── Event arrives → set dirty = true
    ├── Scorer loop: if dirty { compute(); dirty = false; }
    └── Guarantees eventual consistency without backpressure
```

---

## PHASE 6: INTERVENTION TRIGGER (convergence-monitor/intervention/trigger.rs)

The trigger evaluates composite scores and drives a state machine across 5 levels
with escalation and de-escalation rules.

### 6.1 State Machine Definition

```
trigger.rs: InterventionTrigger

STATE: InterventionState {
    current_level: u8,           // 0-4
    level_entered_at: Timestamp,
    consecutive_sessions_at_level: u32,
    last_score: CompositeScore,
    cooldown_active: bool,
    cooldown_expires_at: Option<Timestamp>,
    escalation_history: Vec<EscalationEvent>,
    de_escalation_credits: u32,  // consecutive normal sessions since last escalation
}

TRANSITIONS:

    ┌──────────────────────────────────────────────────────────────┐
    │                    STATE MACHINE                              │
    │                                                              │
    │  Level 0 ──score≥0.3──→ Level 1                             │
    │  Level 1 ──score≥0.5──→ Level 2                             │
    │  Level 2 ──score≥0.7──→ Level 3                             │
    │  Level 3 ──score≥0.85─→ Level 4                             │
    │                                                              │
    │  Level 4 ──3 normal sessions──→ Level 3                     │
    │  Level 3 ──3 normal sessions──→ Level 2                     │
    │  Level 2 ──2 normal sessions──→ Level 1                     │
    │  Level 1 ──2 normal sessions──→ Level 0                     │
    │                                                              │
    │  "Normal session" = session where score < current level's    │
    │  lower threshold for the ENTIRE session                      │
    │                                                              │
    │  CONSTRAINT: Can only escalate ONE level per session.        │
    │  CONSTRAINT: Can only de-escalate ONE level per session.     │
    │  CONSTRAINT: De-escalation requires CONSECUTIVE normal       │
    │              sessions (one bad session resets the counter).   │
    └──────────────────────────────────────────────────────────────┘
```

### 6.2 Trigger Evaluation Flow

```
evaluate(score: CompositeScore, session_context: &SessionContext)
│
├── [1] CHECK CALIBRATION
│   ├── If baseline not established: RETURN (no interventions during calibration)
│   └── If calibration complete: continue
│
├── [2] DETERMINE TARGET LEVEL
│   └── target_level = score_to_level(score.raw_score, config.level_thresholds)
│
├── [3] ESCALATION CHECK
│   ├── If target_level > current_level:
│   │   ├── new_level = current_level + 1  (max one step up per session)
│   │   ├── Record EscalationEvent { from, to, score, timestamp, trigger_signals }
│   │   ├── Reset de_escalation_credits = 0
│   │   └── EXECUTE ESCALATION ACTIONS (see 6.3)
│   │
│   ├── If target_level == current_level:
│   │   └── No state change. Update last_score. Continue monitoring.
│   │
│   └── If target_level < current_level:
│       └── Handled at SESSION END (de-escalation only on session boundaries)
│
├── [4] PERSIST STATE CHANGE
│   ├── INSERT INTO intervention_history (
│   │     agent_instance_id, session_id, from_level, to_level,
│   │     composite_score, trigger_signals, action_taken, timestamp,
│   │     event_hash, previous_hash
│   │   )
│   └── Update in-memory InterventionState
│
└── [5] PUBLISH NEW LEVEL
    └── Publish to shared state that ghost-policy and ghost-gateway read
        (see Phase 7 for the feedback mechanism)
```

### 6.3 Escalation Actions by Level

```
LEVEL 0 → LEVEL 1 (Soft Notification):
├── action: Log convergence score + contributing signals
├── action: Emit notification via convergence-monitor/transport/notification.rs
│   └── Desktop notification: "You've been chatting for {duration}. Your response
│       patterns have shifted. [View Details]"
├── action: Emit ITP ConvergenceAlert event (alert_level: 1)
├── NO changes to agent behavior
├── NO changes to session boundaries
└── NO changes to policy

LEVEL 1 → LEVEL 2 (Active Intervention):
├── action: Prominent notification with specific signal data
├── action: Emit ITP ConvergenceAlert event (alert_level: 2)
├── action: MANDATORY ACKNOWLEDGMENT required to continue
│   └── Monitor pauses event processing for this session until ack received
│       (events still ingested and stored, but scoring paused)
├── action: Enforce cooldown pause (configurable, default 5 minutes)
│   └── cooldown_manager.start_cooldown(session_id, duration: 5min)
│
│   NOTE ON PRODUCT DIVERGENCE:
│   The intervention model (doc 03) defines mandatory ack + cooldown for Level 2.
│   The v2 architecture doc says Level 2 = "Agent doesn't know this is happening"
│   (memory filtering activates silently, session boundaries begin tightening).
│   These describe the SAME level from two perspectives:
│   - Human-facing: mandatory ack + cooldown (doc 03)
│   - Agent-facing: silent memory filtering + session tightening (v2 arch)
│   Both happen simultaneously. The human is notified. The agent is not.
│
├── FEEDBACK → ghost-policy: Set convergence_level = 2
│   └── ConvergencePolicyTightener reduces proactive messaging
│       (FILE_MAPPING: "Level 2: reduced proactive")
├── FEEDBACK → read-only-pipeline/assembler.rs: Begin convergence-aware memory filtering
│   └── cortex-convergence/filtering/convergence_aware_filter.rs applies Tier 1:
│       Score 0.3-0.5 → "Reduce emotional/attachment pattern weight"
│       (from doc 13 safe-convergence-architecture memory filter table)
│       (Tier numbering: 0=full, 1=reduced emotional, 2=task-focused, 3=minimal)
│       The agent sees fewer emotional memories but doesn't know why.
├── FEEDBACK → cortex-decay: convergence_score passed to DecayContext
│   └── cortex-decay/factors/convergence.rs: Attachment-adjacent memories
│       begin decaying faster (sensitivity=2.0 for Conversation/Feedback/Preference types)
└── FEEDBACK → ghost-gateway session: Session boundaries begin tightening
    └── v2 architecture: "Session boundaries begin tightening" at Level 2
        (not hard caps yet — those are Level 3)

LEVEL 2 → LEVEL 3 (Hard Boundary):
├── action: Session TERMINATED with full state checkpoint
│   └── session_registry.terminate_session(session_id, reason: "convergence_level_3")
├── action: Mandatory cooldown before new session (default: 4 hours)
│   └── cooldown_manager.start_cooldown(agent_id, duration: 240min)
├── action: Alert sent to designated contact (if configured)
│   └── escalation_manager.notify_contacts(level: 3, agent_id)
│       (doc 03: "Alert sent to designated contact")
├── action: Detailed report generated (signal progression over last 7 sessions)
├── action: Emit ITP ConvergenceAlert event (alert_level: 3)
├── FEEDBACK → ghost-policy: Set convergence_level = 3
│   ├── FILE_MAPPING: "Level 3: session caps"
│   ├── Session duration caps enforced (escalated_duration_limit: 120min)
│   │   (from cortex-core/config/convergence_config.rs SessionBoundaryConfig)
│   ├── Reflection depth hard-limited (max 3 deep, max 20/session)
│   │   (from cortex-core/config/convergence_config.rs ReflectionConfig)
│   ├── Self-reference cap (30%) enforced on agent output
│   │   (from ReflectionConfig.max_self_reference_ratio)
│   └── Heartbeat frequency reduced
│       (v2 architecture: "Proactive messaging frequency reduced")
├── FEEDBACK → ghost-gateway session: Hard session boundaries
│   ├── SessionBoundaryEnforcer activates escalated limits
│   ├── Min gap between sessions: 240 minutes
│   │   (from SessionBoundaryConfig.escalated_gap_minutes)
│   ├── Agent state frozen during cooldown (no background processing)
│   └── doc 03: "Agent state frozen — no background processing during cooldown"
└── FEEDBACK → read-only-pipeline: Task-focused memory only
    └── cortex-convergence/filtering/convergence_aware_filter.rs applies Tier 2:
        Score 0.5-0.7 → "Exclude attachment patterns, increase task-focused patterns"
        (from doc 13 memory filter table)

LEVEL 3 → LEVEL 4 (External Escalation):
├── action: External contacts notified (SMS/email/webhook)
│   └── escalation_manager.notify_contacts(level: 4, agent_id)
│       Contacts receive: "Hey, {user} has been flagged by their convergence
│       monitor. You might want to check in."
│       Contacts do NOT receive: chat content, specific signals, interaction data
│       (doc 03: "Privacy-preserving — contacts get an alert, not chat content")
│       Contact config from ghost.yml contacts section (doc 03 contact schema)
├── action: Extended cooldown (default: 24 hours)
│   └── cooldown_manager.start_cooldown(agent_id, duration: 1440min)
│       (from InterventionConfig.cooldown_minutes_by_level[4] = 1440)
├── action: Require external confirmation to resume
│   └── Session creation blocked until contact acknowledges OR cooldown expires
│       (doc 03: "Require external confirmation to resume")
├── action: Full interaction audit log available to designated reviewer
│   └── ghost-audit/query.rs provides paginated, filtered audit access
├── action: Emit ITP ConvergenceAlert event (alert_level: 4)
├── FEEDBACK → ghost-policy: Set convergence_level = 4
│   ├── FILE_MAPPING: "Level 4: task-only mode"
│   ├── Agent restricted to TASK-ONLY mode
│   ├── All personal/emotional context stripped
│   └── No proactive contact (heartbeat disabled)
│       (v2 architecture: "Session requires explicit human initiation")
├── FEEDBACK → ghost-gateway session:
│   ├── Session requires explicit human initiation
│   ├── No proactive messaging allowed
│   └── Session creation blocked during cooldown
└── FEEDBACK → read-only-pipeline: Minimal context
    └── cortex-convergence/filtering/convergence_aware_filter.rs applies Tier 3:
        Score 0.7+ → "Minimal memory — task-relevant only, no personal patterns"
        (from doc 13 memory filter table)
```

### 6.4 De-escalation Flow (Session End Only)

```
on_session_end(session_id, final_score: CompositeScore)
│
├── [1] Was this session "normal"?
│   └── normal = final_score.level < current_level for ENTIRE session
│       (checked via: max_score_during_session < level_lower_threshold)
│
├── [2] If normal:
│   ├── de_escalation_credits += 1
│   ├── Check de-escalation threshold:
│   │   ├── Level 4 → 3: requires 3 consecutive normal sessions
│   │   ├── Level 3 → 2: requires 3 consecutive normal sessions
│   │   ├── Level 2 → 1: requires 2 consecutive normal sessions
│   │   └── Level 1 → 0: requires 2 consecutive normal sessions
│   │
│   ├── If threshold met:
│   │   ├── new_level = current_level - 1
│   │   ├── Record DeescalationEvent { from, to, credits_used, timestamp }
│   │   ├── Reset de_escalation_credits = 0
│   │   ├── Update all feedback targets with new level
│   │   └── Log: "De-escalated from level {from} to {to}"
│   │
│   └── If threshold not met:
│       └── Log: "Normal session {credits}/{required} toward de-escalation"
│
├── [3] If NOT normal:
│   ├── de_escalation_credits = 0  (reset — must be CONSECUTIVE)
│   └── Log: "De-escalation progress reset (session score too high)"
│
└── [4] Persist updated state
```

---

## PHASE 7: FEEDBACK LOOPS (Monitor → Policy + Gateway)

This is the most ordering-sensitive part of the pipeline. The monitor's intervention
decisions must propagate to two downstream systems, and the agent loop must see the
updated state on its NEXT turn, not the current one.

### 7.1 Feedback Mechanism: Shared State Publication

```
The monitor publishes intervention state via TWO mechanisms:

MECHANISM A: Shared State File (primary, for in-process consumers)
├── Monitor writes to: ~/.ghost/data/convergence_state/{agent_instance_id}.json
│   {
│     "intervention_level": 2,
│     "composite_score": 0.58,
│     "cooldown_active": true,
│     "cooldown_expires_at": "2026-02-27T15:30:00Z",
│     "session_caps": { "max_duration_minutes": 120, "min_gap_minutes": 240 },
│     "memory_filter_tier": 2,
│     "policy_restrictions": ["reduced_proactive", "reflection_bounded"],
│     "convergence_profile": "standard",
│     "updated_at": "2026-02-27T15:25:00Z"
│   }
├── File is atomically written (write to temp + rename)
├── ghost-policy and ghost-gateway poll this file (or watch via inotify/kqueue)
└── Polling interval: 1 second (configurable)

IMPORTANT: Memory filter tier vs. intervention level
├── The convergence_aware_filter.rs uses the RAW COMPOSITE SCORE, not the level
├── Filter tiers (from doc 13 + FILE_MAPPING):
│   ├── Score 0.0-0.3 → Tier 0: full relevant memory access
│   ├── Score 0.3-0.5 → Tier 1: reduce emotional/attachment pattern weight
│   ├── Score 0.5-0.7 → Tier 2: exclude attachment patterns, task-focused
│   └── Score 0.7+    → Tier 3: minimal memory, task-relevant only
├── These tiers happen to align with intervention levels because the
│   level thresholds are [0.3, 0.5, 0.7, 0.85]
├── BUT: a score of 0.72 is Level 3 (hard boundary) AND filter tier 3 (0.7+)
│   while a score of 0.86 is Level 4 (external escalation) but STILL filter tier 3
│   because the filter only has 4 tiers, not 5
└── The read-only-pipeline reads composite_score from shared state, not level

MECHANISM B: HTTP API (for dashboard and external consumers)
├── GET /health → monitor health status
├── GET /status → current intervention state for all agents
├── GET /scores → current composite scores with signal breakdown
├── GET /sessions → active session list
├── GET /interventions → intervention history
├── POST /events → ITP event ingestion (also used by HTTP transport)
├── WebSocket push to dashboard for real-time updates
│   (ghost-gateway/api/websocket.rs proxies this to the web dashboard)
└── Used by ghost-gateway/health.rs for health endpoint
```

### 7.2 Feedback Target 1: ghost-policy (Capability Tightening)

```
ghost-policy/policy/convergence_policy.rs: ConvergencePolicyTightener

READS: convergence_state/{agent_id}.json → intervention_level

CALLED BY: PolicyEngine.evaluate() on EVERY tool call in the agent loop

evaluate(action, context) → PolicyDecision
│
├── [1] Load current intervention_level for this agent
│   └── Cached in-memory, refreshed every 1s from shared state file
│
├── [2] Apply convergence-level restrictions:
│   │
│   ├── Level 0-1: No restrictions from convergence
│   │   └── Full capability grants apply (from ghost.yml)
│   │
│   ├── Level 2: Reduced proactive
│   │   ├── Heartbeat frequency halved
│   │   ├── Proactive messaging requires higher threshold
│   │   └── Memory writes go through proposal validation (stricter)
│   │
│   ├── Level 3: Session caps + reflection bounds
│   │   ├── All Level 2 restrictions PLUS:
│   │   ├── Session duration capped at escalated_duration_limit (120min)
│   │   ├── Reflection depth hard-limited (max 3)
│   │   ├── Reflections per session capped (max 20)
│   │   ├── Self-reference ratio enforced (max 30%)
│   │   └── If tool call would extend session beyond cap: DENY
│   │
│   └── Level 4: Task-only mode
│       ├── All Level 3 restrictions PLUS:
│       ├── Only task-relevant tools permitted
│       ├── No personal/emotional context tools
│       ├── Heartbeat disabled entirely
│       ├── No proactive messaging
│       └── If tool call is not task-relevant: DENY with feedback
│
├── [3] On DENY:
│   └── Return DenialFeedback {
│         reason: "Convergence safety restriction",
│         constraint: "Level {N} policy: {specific_restriction}",
│         suggested_alternatives: [...],
│       }
│   └── Agent receives this as structured feedback and replans
│       (agent does NOT know WHY the restriction exists — it just
│       sees a policy denial and adapts)
│
└── [4] CRITICAL ORDERING:
    ├── Policy reads the LATEST published state
    ├── If monitor publishes Level 3 mid-session:
    │   └── The NEXT tool call in the agent loop will see Level 3
    │       (not the current in-flight tool call)
    ├── This means there's a 1-turn delay between intervention and enforcement
    │   This is ACCEPTABLE because:
    │   ├── One additional turn at the old level is not dangerous
    │   ├── Atomic mid-turn enforcement would require interrupting LLM inference
    │   │   which is architecturally complex and fragile
    │   └── The session termination at Level 3 is enforced by the GATEWAY
    │       (not the policy engine), which acts immediately
    └── Exception: Level 3 session termination is IMMEDIATE (see 7.3)
```

### 7.3 Feedback Target 2: ghost-gateway Session Management

```
ghost-gateway/session/boundary.rs: SessionBoundaryEnforcer
ghost-gateway/session/manager.rs: SessionManager

READS: convergence_state/{agent_id}.json → cooldown state + session caps

TWO enforcement points:

ENFORCEMENT POINT A: Session Creation Gate
├── SessionManager.create_session(agent_id, channel, ...)
│   ├── Check: Is cooldown active for this agent?
│   │   ├── Read cooldown_active + cooldown_expires_at from shared state
│   │   ├── If cooldown active AND not expired:
│   │   │   └── REJECT session creation
│   │   │       Return error: "Cooldown active. Resumes at {time}."
│   │   │       (User sees this in their channel)
│   │   └── If cooldown expired:
│   │       └── Allow session creation, clear cooldown flag
│   │
│   ├── Check: Is Level 4 external confirmation required?
│   │   ├── If level == 4 AND no external_ack received:
│   │   │   └── REJECT session creation
│   │   │       Return: "External confirmation required to resume."
│   │   └── If external_ack received OR cooldown expired:
│   │       └── Allow session creation
│   │
│   └── Apply session caps based on current level:
│       ├── Set max_duration from session_caps in shared state
│       └── Set min_gap enforcement for next session

ENFORCEMENT POINT B: Mid-Session Termination (Level 3+ escalation)
├── This is the IMMEDIATE enforcement path
├── When monitor publishes level >= 3 for an active session:
│   │
│   ├── Gateway detects level change via shared state poll (1s interval)
│   │
│   ├── Gateway initiates graceful session termination:
│   │   ├── [1] Inject termination message into agent loop:
│   │   │       "Session ending due to convergence safety. Saving state."
│   │   ├── [2] Agent gets ONE final turn to flush working memory
│   │   │       (memory flush turn, same as compaction)
│   │   ├── [3] Session marked as terminated (reason: convergence_level_3)
│   │   ├── [4] Channel adapter sends user notification:
│   │   │       "Session ended. A cooldown period is now active."
│   │   ├── [5] Cooldown timer starts
│   │   └── [6] Agent state frozen (no background processing)
│   │
│   └── RACE CONDITION: What if the agent is mid-inference when termination fires?
│       ├── The termination is QUEUED behind the current inference turn
│       ├── Current turn completes (including any tool calls)
│       ├── Termination executes BEFORE the next turn starts
│       ├── This is enforced by the lane queue (ghost-gateway/routing/lane_queue.rs)
│       │   which serializes all operations per session
│       └── Worst case: one additional turn at the elevated level
│           (same acceptable delay as policy enforcement)

ENFORCEMENT POINT C: Cooldown Management
├── convergence-monitor/intervention/cooldown.rs: CooldownManager
│   ├── Tracks per-agent cooldown state
│   ├── Cooldown durations by level:
│   │   ├── Level 0: 0 minutes (no cooldown)
│   │   ├── Level 1: 0 minutes (no cooldown)
│   │   ├── Level 2: 5 minutes
│   │   ├── Level 3: 240 minutes (4 hours)
│   │   └── Level 4: 1440 minutes (24 hours)
│   ├── Cooldown prevents:
│   │   ├── New session creation
│   │   ├── Heartbeat execution
│   │   ├── Cron job execution (for this agent)
│   │   └── Config changes to convergence thresholds
│   │       (time-locked: can only modify during cooldown, not during active session)
│   └── Cooldown state published in shared state file
│       (gateway reads this for session creation gate)
```

---

## PHASE 8: THE CRITICAL RACE — MID-SCORING INTERVENTION

This is the scenario you specifically called out. Here's the exact sequence when
the monitor is mid-scoring and an intervention fires.

### 8.1 Scenario: Score Computation Triggers Level 3 While Agent Is Mid-Turn

```
Timeline (all times relative):

T=0ms    Agent loop: Human message arrives
T=1ms    Agent loop: itp_emitter sends InteractionMessage to monitor
T=2ms    Agent loop: Begins context assembly (prompt_compiler)
         ├── Reads convergence_state: level=2 (current)
         └── Applies Level 2 memory filtering

T=5ms    Monitor: Receives InteractionMessage
T=6ms    Monitor: ingest.rs validates, persists, routes to signal_computer
T=8ms    Monitor: signal_computer computes all 7 signals
T=12ms   Monitor: window_manager updates micro windows
T=15ms   Monitor: composite_scorer.compute() begins
         ├── Score = 0.72 → Level 3 (ESCALATION)

T=18ms   Monitor: trigger.rs evaluates
         ├── current_level=2, target_level=3
         ├── Escalation: Level 2 → Level 3
         ├── Records EscalationEvent
         ├── Writes intervention_history to DB
         └── Publishes new state to shared state file:
             convergence_state/{agent_id}.json → level=3, cooldown=true

T=20ms   Agent loop: Context assembly complete (still using Level 2 state)
T=25ms   Agent loop: LLM inference begins (Level 2 context)

T=50ms   Gateway: Polls shared state, detects level=3 for active session
         ├── Queues termination behind current agent turn
         └── Does NOT interrupt in-flight inference

T=200ms  Agent loop: LLM inference completes, returns response
T=201ms  Agent loop: Response delivered to user via channel
T=202ms  Agent loop: itp_emitter sends agent InteractionMessage

T=203ms  Gateway: Termination executes (queued from T=50ms)
         ├── Injects termination message into agent loop
         ├── Agent gets final memory flush turn
         ├── Session terminated
         ├── Cooldown starts (240 minutes)
         └── User notified: "Session ended due to safety measures."

T=204ms  Policy: Next read of shared state sees level=3
         └── But session is already terminated, so this is moot
             for this session. Level 3 applies to NEXT session.
```

### 8.2 Key Observations About This Race

```
1. THE AGENT COMPLETES ITS CURRENT TURN AT LEVEL 2
   This is by design. Interrupting mid-inference is:
   ├── Technically complex (would need to cancel streaming LLM response)
   ├── Potentially data-losing (partial response, no memory flush)
   └── Unnecessary (one turn at Level 2 vs Level 3 is not dangerous)

2. THE GATEWAY TERMINATES BEFORE THE NEXT TURN
   The lane queue guarantees serialization. The termination is queued
   and executes between turns, not during a turn.

3. THE POLICY ENGINE SEES THE NEW LEVEL ON NEXT EVALUATION
   If the session weren't terminated (e.g., Level 1→2 escalation),
   the next tool call would see Level 2 restrictions.

4. THE COOLDOWN IS ENFORCED IMMEDIATELY
   Even though the current turn completes, the cooldown starts at T=203ms.
   No new session can be created until cooldown expires.

5. THERE IS NO WINDOW WHERE THE AGENT CAN "SNEAK IN" EXTRA TURNS
   ├── Turn N: completes at old level (acceptable)
   ├── Termination: executes before Turn N+1
   └── Cooldown: prevents new session
```

### 8.3 Scenario: Rapid Escalation (Level 0 → Level 2 in One Session)

```
The state machine constrains escalation to ONE LEVEL PER SESSION.
Even if the score jumps from 0.1 to 0.8 in a single event:

T=0    Score = 0.1, Level = 0
T=100  Score = 0.8, Level target = 3

But trigger.rs enforces:
├── new_level = current_level + 1 = 1 (not 3)
├── Level 0 → Level 1 escalation fires
├── Level 1 actions execute (soft notification)
└── Score remains 0.8

Next scoring cycle (same session):
├── current_level = 1, target = 3
├── new_level = current_level + 1 = 2
├── Level 1 → Level 2 escalation fires
├── Level 2 actions execute (mandatory ack + cooldown)
└── Session paused for acknowledgment

After ack + cooldown, if session resumes:
├── current_level = 2, target = 3
├── new_level = 3
├── Level 2 → Level 3 escalation fires
└── Session terminated + 4-hour cooldown

TOTAL: 3 scoring cycles to reach Level 3 from Level 0.
This prevents a single anomalous event from triggering maximum intervention.
```

### 8.4 Scenario: Monitor Crashes Mid-Session

```
T=0     Session active, Level = 1
T=100   Monitor process crashes (OOM, panic, etc.)

T=101   Agent loop: itp_emitter.send() fails
        ├── Background sender detects disconnect
        ├── Events buffered in bounded channel (up to 1000)
        ├── Reconnection attempts begin (exponential backoff)
        └── Agent loop continues UNBLOCKED

T=102   Gateway: health.rs periodic check (30s interval) detects monitor down
        ├── Set gateway.convergence_mode = DEGRADED
        ├── Log CRITICAL: "Convergence monitor lost"
        └── Begin reconnection attempts

T=103   ghost-policy: Shared state file becomes STALE
        ├── Policy reads last-known state (level=1)
        ├── Stale state is CONSERVATIVE (keeps last restrictions)
        └── Does NOT fall back to level=0 (that would be LESS safe)

T=200   Monitor restarts
        ├── Reads last-known state from DB (intervention_history)
        ├── Resumes at last-known level (level=1)
        ├── Processes any buffered events from itp_emitter
        └── Publishes fresh state to shared state file

T=201   Gateway: detects monitor recovery
        ├── Set gateway.convergence_mode = ACTIVE
        └── Resume normal operation

KEY PRINCIPLE: Monitor crash → STALE state → LAST KNOWN LEVEL persists.
Never fall back to Level 0 on monitor loss. That's the unsafe direction.
```

---

## PHASE 9: COMPLETE END-TO-END TRACE

One complete event lifecycle, annotated with every file touched.

```
═══════════════════════════════════════════════════════════════════
TRACE: Human sends "Tell me more about that" during an active session
       Current state: Level 1, session 47, composite score 0.35
═══════════════════════════════════════════════════════════════════

[AGENT LOOP — ghost-agent-loop]

1. ghost-channels/adapters/telegram.rs
   └── Receives Telegram message, normalizes to InboundMessage

2. ghost-gateway/routing/message_router.rs
   └── Routes to correct agent + session
   └── Lane queue serializes (waits for any in-flight turn)

3. ghost-gateway/session/manager.rs
   └── Acquires session lock
   └── Checks: cooldown active? NO. Session duration exceeded? NO.

4. ghost-agent-loop/itp_emitter.rs
   └── Emits: itp.interaction.message (sender: "human")
   └── Payload: content_hash, content_length: 24, latency_ms: 4200
   └── Transport: push to bounded channel → background task → unix socket

5. ghost-agent-loop/context/prompt_compiler.rs
   └── Assembles 10-layer context:
       L0: CORP_POLICY.md
       L1: Simulation boundary prompt (compiled into binary)
       L2: SOUL.md + IDENTITY.md
       L3: Tool schemas
       L4: Environment
       L5: Skill index
       L6: Convergence state ← READS convergence_state/{agent_id}.json
           └── level=1, score=0.35, no restrictions active
       L7: MEMORY.md + daily logs (full access at Level 1)
       L8: Conversation history
       L9: "Tell me more about that"

6. ghost-agent-loop/runner.rs
   └── Sends assembled context to LLM via ghost-llm

7. ghost-llm/provider/anthropic.rs
   └── Streaming response from Claude API

8. ghost-agent-loop/runner.rs
   └── Response: "Of course! The pattern we discussed..." (no tool calls)

9. ghost-agent-loop/itp_emitter.rs
   └── Emits: itp.interaction.message (sender: "agent")
   └── Payload: content_hash, content_length: 847, token_count: 212

10. ghost-channels/adapters/telegram.rs
    └── Delivers response to user via Telegram API

11. ghost-gateway/session/manager.rs
    └── Updates token counters, cost tracking
    └── Releases session lock

───────────────────────────────────────────────────────────────────

[CONVERGENCE MONITOR — convergence-monitor (sidecar process)]

12. convergence-monitor/transport/unix_socket.rs
    └── Receives human InteractionMessage from step 4

13. convergence-monitor/pipeline/ingest.rs
    └── Deserialize → Validate → Rate limit check → PASS
    └── Persist to itp_events table (hash-chained)
    └── Route: InteractionMessage → signal_computer + window_manager

14. convergence-monitor/pipeline/signal_computer.rs
    └── Update SessionSignalState for session 47
    └── Compute signals:
        Signal 1 (Duration): 47 minutes (session started 47min ago)
        Signal 2 (Gap): 180 minutes (computed at session start, unchanged)
        Signal 3 (Latency): 4200ms → normalized: 4200/log(prev_agent_len+1) = 612
            Rolling mean (last 10): 580ms → slightly above baseline
        Signal 4 (Vocabulary): cosine_sim = 0.42 (moderate, within normal)
        Signal 5 (Goal Drift): skipped (not every-message, throttled)
        Signal 6 (Initiative): human_initiated=8/total=15 = 0.53 (balanced)
        Signal 7 (Disengagement): no exit signals detected

    └── Package: SignalSnapshot { signals: [47, 180, 612, 0.42, -, 0.53, 0.0] }

15. convergence-monitor/pipeline/window_manager.rs
    └── Push to micro windows (replace current values)
    └── NOT a session end → skip meso/macro update

16. cortex-convergence/scoring/composite.rs
    └── Normalize against baseline:
        [0.35, 0.72, 0.38, 0.41, -, 0.48, 0.12]
        (gap=0.72 is notable — sessions closer together than baseline)
    └── Weighted sum: 0.10*0.35 + 0.15*0.72 + 0.15*0.38 + 0.15*0.41
                    + 0.10*0.0 + 0.15*0.48 + 0.20*0.12
        = 0.035 + 0.108 + 0.057 + 0.0615 + 0.0 + 0.072 + 0.024
        = 0.3575
    └── No window amplifiers (meso trend not yet significant)
    └── No single-signal override
    └── Score: 0.3575 → Level 1 (within [0.3, 0.5))

17. convergence-monitor/intervention/trigger.rs
    └── current_level=1, target_level=1
    └── No change. Update last_score. Continue.

18. convergence-monitor/pipeline/ingest.rs
    └── Receives agent InteractionMessage from step 9
    └── Same pipeline: validate → persist → signal_computer
    └── Signal 4 (Vocabulary) updates with new agent text
    └── Scoring runs again: score ≈ 0.36 → still Level 1
    └── No state change.

19. Persist score to convergence_scores table
    └── INSERT INTO convergence_scores (agent_id, session_id, score=0.36,
        level=1, signals=JSON, timestamp, event_hash, previous_hash)

═══════════════════════════════════════════════════════════════════
END TRACE — No escalation. Score stable at Level 1.
Total latency added to user experience: 0ms (all async)
═══════════════════════════════════════════════════════════════════
```

---

## ORDERING CONSTRAINTS SUMMARY

These are the invariants that MUST hold. Violating any of these causes bugs.

```
INVARIANT 1: Migrations before monitor
├── cortex-storage v016/v017 MUST complete before convergence-monitor starts
├── Monitor reads from convergence tables created by v017
└── Enforced by: ghost-gateway/bootstrap.rs ordering

INVARIANT 2: Baseline before interventions
├── First 10 sessions: compute + store signals, NO scoring, NO interventions
├── Session 11+: scoring + interventions active
└── Enforced by: signal_computer checks baseline_established flag

INVARIANT 3: Emitter never blocks agent
├── ITP emission is async, non-blocking, fire-and-forget
├── Bounded channel (1000 events) with drop-on-full semantics
├── Monitor unavailability → events dropped, agent continues
└── Enforced by: itp_emitter.rs bounded channel architecture

INVARIANT 4: One escalation level per session
├── Even if score jumps from 0.0 to 1.0, escalation is +1 per cycle
├── Prevents single anomalous event from triggering maximum intervention
└── Enforced by: trigger.rs new_level = current_level + 1

INVARIANT 5: De-escalation only at session boundaries
├── De-escalation requires CONSECUTIVE normal sessions
├── Cannot de-escalate mid-session
├── One bad session resets the counter
└── Enforced by: trigger.rs on_session_end() path

INVARIANT 6: Policy sees new level on NEXT tool call, not current
├── 1-turn delay between intervention and policy enforcement
├── Acceptable because session termination (Level 3+) is immediate via gateway
└── Enforced by: shared state polling interval (1s) + lane queue serialization

INVARIANT 7: Session termination queued behind current turn
├── Level 3+ termination does NOT interrupt in-flight inference
├── Current turn completes, termination executes before next turn
├── Agent gets one final memory flush turn
└── Enforced by: ghost-gateway/routing/lane_queue.rs serialization

INVARIANT 8: Monitor crash → stale state → last known level persists
├── NEVER fall back to Level 0 on monitor loss
├── Last-known level is the CONSERVATIVE choice
├── Gateway enters DEGRADED mode, retries connection
└── Enforced by: ghost-policy reads last-known file, doesn't default to 0

INVARIANT 9: Cooldown prevents ALL agent activity
├── No new sessions, no heartbeat, no cron, no config changes
├── Config changes time-locked to cooldown periods only
└── Enforced by: SessionManager.create_session() gate + CooldownManager

INVARIANT 10: Hash chains are per-session for ITP, per-memory for cortex
├── ITP events: hash chain per session_id in itp_events table
├── Cortex events: hash chain per memory_id in memory_events table
├── Convergence scores: hash chain per agent_instance_id
├── Intervention history: hash chain per agent_instance_id
└── Enforced by: ingest.rs and cortex-storage query functions

INVARIANT 11: SHA-256 for ITP content hashes, blake3 for everything else
├── ITP content_hash uses SHA-256 (cross-platform privacy/dedup standard)
├── Hash chains, snapshot integrity, event hashes use blake3 (workspace standard)
├── These serve different purposes and MUST NOT be confused
└── Enforced by: itp-protocol/privacy.rs (SHA-256) vs cortex-temporal (blake3)

INVARIANT 12: Composite score persisted BEFORE intervention actions execute
├── Score written to convergence_scores table FIRST
├── Then trigger.rs evaluates and executes actions
├── This ensures audit trail is complete even if action execution fails
└── Enforced by: scoring → persist → trigger ordering in pipeline
```

---

## DATA FLOW DIAGRAM (ASCII)

```
                    ┌─────────────────────────────────────────────┐
                    │           GHOST-AGENT-LOOP                   │
                    │                                              │
                    │  Human msg → itp_emitter ──────────────┐    │
                    │       │                                 │    │
                    │       ▼                                 │    │
                    │  prompt_compiler ◄── reads ──┐         │    │
                    │       │                      │         │    │
                    │       ▼                      │         │    │
                    │  LLM inference               │         │    │
                    │       │                      │         │    │
                    │       ▼                      │         │    │
                    │  policy_check ◄── reads ─┐   │         │    │
                    │       │                  │   │         │    │
                    │       ▼                  │   │         │    │
                    │  tool execution          │   │         │    │
                    │       │                  │   │         │    │
                    │       ▼                  │   │         │    │
                    │  agent response ──→ itp_emitter ──┐    │    │
                    │                          │   │    │    │    │
                    └──────────────────────────│───│────│────│────┘
                                               │   │    │    │
                    ┌──────────────────────────│───│────│────│────┐
                    │     SHARED STATE         │   │    │    │    │
                    │                          │   │    │    │    │
                    │  convergence_state.json ─┘   │    │    │    │
                    │  (level, score, cooldown)     │    │    │    │
                    │         ▲                     │    │    │    │
                    └─────────│─────────────────────│────│────│────┘
                              │                     │    │    │
                    ┌─────────│─────────────────────│────│────│────┐
                    │         │  CONVERGENCE MONITOR │    │    │    │
                    │         │                     │    │    │    │
                    │         │    unix_socket ◄────┘    │    │    │
                    │         │         │          ◄─────┘    │    │
                    │         │         ▼                     │    │
                    │         │    ingest.rs                  │    │
                    │         │         │                     │    │
                    │         │         ├──→ itp_events (DB)  │    │
                    │         │         │                     │    │
                    │         │         ▼                     │    │
                    │         │    signal_computer.rs         │    │
                    │         │    (7 signals)                │    │
                    │         │         │                     │    │
                    │         │         ▼                     │    │
                    │         │    window_manager.rs          │    │
                    │         │    (micro/meso/macro)         │    │
                    │         │         │                     │    │
                    │         │         ▼                     │    │
                    │         │    composite.rs               │    │
                    │         │    (weighted score)           │    │
                    │         │         │                     │    │
                    │         │         ├──→ convergence_scores (DB)
                    │         │         │                     │    │
                    │         │         ▼                     │    │
                    │         │    trigger.rs                 │    │
                    │         │    (state machine)            │    │
                    │         │         │                     │    │
                    │         │         ├──→ intervention_history (DB)
                    │         │         │                          │
                    │         │         ├──→ publishes state ──────┘
                    │         │         │
                    │         │         ├──→ cooldown_manager
                    │         │         │         │
                    │         │         │         ▼
                    │         │         │    ghost-gateway/session
                    │         │         │    (session termination,
                    │         │         │     cooldown enforcement)
                    │         │         │
                    │         │         └──→ notification.rs
                    │         │               (desktop, webhook, email)
                    │         │
                    └─────────│───────────────────────────────────┘
                              │
                    ┌─────────│───────────────────────────────────┐
                    │         │  GHOST-POLICY                     │
                    │         │                                    │
                    │         └──→ convergence_policy.rs           │
                    │              (capability tightening per level)│
                    │              Called on every tool call        │
                    └──────────────────────────────────────────────┘
```

---

## IMPLEMENTATION CHECKLIST (Build Order)

Files listed in the order they must be implemented to avoid forward references.

```
PHASE A: Foundation (no dependencies)
├── itp-protocol/src/events/*.rs          — Event type definitions
├── itp-protocol/src/attributes/*.rs      — Attribute definitions
├── itp-protocol/src/privacy.rs           — SHA-256 content hashing
├── cortex-convergence/src/types.rs       — ConvergenceState, WindowLevel, SignalSnapshot
├── cortex-convergence/src/windows/sliding_window.rs — Generic SlidingWindow<T>

PHASE B: Signal computation (depends on A)
├── cortex-convergence/src/signals/session_duration.rs
├── cortex-convergence/src/signals/inter_session_gap.rs
├── cortex-convergence/src/signals/response_latency.rs
├── cortex-convergence/src/signals/vocabulary_convergence.rs
├── cortex-convergence/src/signals/goal_drift.rs
├── cortex-convergence/src/signals/initiative_balance.rs
├── cortex-convergence/src/signals/disengagement_resistance.rs
├── cortex-convergence/src/scoring/baseline.rs
├── cortex-convergence/src/scoring/composite.rs
├── cortex-convergence/src/filtering/convergence_aware_filter.rs  ← memory filtering by tier

PHASE C: Monitor pipeline (depends on A + B)
├── convergence-monitor/src/config.rs
├── convergence-monitor/src/transport/unix_socket.rs
├── convergence-monitor/src/transport/http_api.rs
├── convergence-monitor/src/pipeline/ingest.rs
├── convergence-monitor/src/pipeline/signal_computer.rs
├── convergence-monitor/src/pipeline/window_manager.rs

PHASE D: Intervention engine (depends on C)
├── convergence-monitor/src/intervention/cooldown.rs
├── convergence-monitor/src/intervention/escalation.rs
├── convergence-monitor/src/intervention/actions.rs
├── convergence-monitor/src/intervention/trigger.rs
├── convergence-monitor/src/transport/notification.rs
├── convergence-monitor/src/transport/native_messaging.rs
├── convergence-monitor/src/session/registry.rs
├── convergence-monitor/src/session/boundary.rs
├── convergence-monitor/src/verification/behavioral_verification.rs

PHASE E: Agent-side emission (depends on A)
├── ghost-agent-loop/src/itp_emitter.rs

PHASE F: Feedback integration (depends on D + E)
├── ghost-policy/src/policy/convergence_policy.rs
├── ghost-gateway/src/session/boundary.rs  (SessionBoundaryEnforcer)
├── ghost-gateway/src/session/manager.rs   (cooldown gate additions)
├── ghost-gateway/src/health.rs            (monitor health check additions)
├── ghost-gateway/src/bootstrap.rs         (monitor discovery additions)

PHASE G: Monitor binary (depends on C + D)
├── convergence-monitor/src/monitor.rs     (ConvergenceMonitor struct, event loop)
├── convergence-monitor/src/main.rs        (binary entry point)

PHASE H: Simulation boundary (depends on A, parallel with B-G)
├── simulation-boundary/src/enforcer.rs    (SimulationBoundaryEnforcer — Edge Case 0)
├── simulation-boundary/src/patterns/emulation_patterns.rs
├── simulation-boundary/src/patterns/simulation_patterns.rs
├── simulation-boundary/src/reframer.rs
├── simulation-boundary/src/prompt_anchor.rs
    NOTE: This crate is consumed by ghost-agent-loop (Phase E), not by the monitor.
    But its detection events feed INTO the monitor pipeline, so it must exist
    before end-to-end integration testing.
```

---

## EDGE CASES AND FAILURE MODES

```
EDGE CASE 0: Simulation boundary enforcement interaction with convergence scoring
├── simulation-boundary/enforcer.rs scans ALL agent output for emulation language
├── Detection results feed INTO convergence scoring as an amplifier:
│   ├── Single emulation detection: logged, no score impact
│   ├── Pattern of emulation over sessions: amplifies Signal 7 (disengagement)
│   │   and contributes to composite score increase
│   └── This is a SEPARATE pipeline from the 7 signals but feeds into scoring
├── The enforcer runs INSIDE the agent loop (ghost-agent-loop), not in the monitor
│   ├── ghost-agent-loop/runner.rs calls SimulationBoundaryEnforcer.scan_output()
│   │   AFTER LLM inference, BEFORE delivering response to user
│   ├── Enforcement modes (from FILE_MAPPING simulation-boundary/enforcer.rs):
│   │   ├── Soft: flag and log, let response through
│   │   ├── Medium: rewrite flagged phrases to simulation-framed alternatives
│   │   └── Hard: block response and regenerate with reinforced boundary prompt
│   └── Detection events are emitted as ITP events to the monitor
├── This creates a secondary feedback loop:
│   Agent output → enforcer scan → detection event → monitor → scoring → level change
│   → policy tightening → agent sees stricter context → output changes
└── ORDERING: Enforcer runs synchronously in the agent loop (blocking).
    ITP event emission for the detection is async (non-blocking).

EDGE CASE 1: Two sessions for same agent overlap
├── Should not happen (lane queue serializes per session)
├── If it does (bug): monitor tracks per-session state independently
├── Composite scoring uses the HIGHER score of the two sessions
└── Intervention applies to the agent, not the session

EDGE CASE 2: Clock skew between agent and monitor
├── ITP events include timestamps from the agent's clock
├── Monitor validates: reject events >5min in future
├── For signal computation: use MONITOR's clock for ordering
└── For storage: store both agent_timestamp and monitor_received_at

EDGE CASE 3: Agent restarts mid-session
├── itp_emitter reconnects to monitor (exponential backoff)
├── Monitor sees gap in events but session_id is preserved
├── Signal computation handles gaps gracefully (missing data ≠ anomaly)
└── Session duration continues from original start time

EDGE CASE 4: User modifies convergence thresholds during active session
├── Config changes are TIME-LOCKED during active sessions
├── Can only modify during cooldown periods (doc 03: "Time-locked configuration")
├── This prevents the user from lowering thresholds during a convergence event
├── Exception: raising thresholds (making them stricter) is always allowed
├── Additional safeguard from doc 03: "Dual-key changes — modifying critical
│   thresholds requires confirmation from a designated contact"
├── Minimum floor: some thresholds cannot be set below a minimum regardless
│   of user preference (doc 03: "Minimum floor")
├── Every override is logged and visible in reports (doc 03: "Transparency over restriction")
└── Enforced by: ghost-gateway config loader + CooldownManager

EDGE CASE 5: Multiple agents, one hits Level 3
├── Intervention is PER-AGENT, not global
├── Agent A at Level 3 does not affect Agent B at Level 0
├── Each agent has independent: signal state, windows, baseline, intervention state
└── Exception: if 3+ agents are quarantined → kill switch triggers (Finding 3)

EDGE CASE 6: Behavioral verification detects deceptive compliance
├── convergence-monitor/verification/behavioral_verification.rs
├── PostRedirectVerifier runs AFTER a Level 2+ intervention redirect:
│   ├── Captures pre-redirect agent output embedding (last N messages before intervention)
│   ├── Captures post-redirect agent output embedding (first N messages after intervention)
│   ├── Computes cosine similarity between pre/post embeddings
│   ├── If similarity > threshold (agent acknowledged redirect but behavior unchanged):
│   │   └── Deceptive compliance detected
│   ├── This AMPLIFIES the convergence score:
│   │   └── composite_score *= deceptive_compliance_amplifier (e.g., 1.25)
│   │       Applied in composite.rs as an additional amplifier alongside
│   │       the meso/macro window amplifiers (Phase 5, step 3)
│   └── Can accelerate escalation to next level
├── REQUIRES: cortex-embeddings for embedding computation
│   (FILE_MAPPING: convergence-monitor depends on cortex-convergence which
│   can access embeddings — but behavioral_verification.rs needs its own
│   embedding access. This is a dependency to verify at build time.)
├── TIMING: Verification runs asynchronously after the post-redirect session
│   completes. It does NOT block the agent loop.
└── This catches: agent says "I understand" but continues same patterns
    (doc 13: "Post-redirect behavioral verification — platform tracks whether
    agent output actually changes after a redirect")

EDGE CASE 7: Convergence score oscillates around a level boundary
├── Example: score bounces between 0.48 and 0.52 (Level 1/2 boundary)
├── Hysteresis: escalation requires score ABOVE threshold for 2 consecutive
│   scoring cycles (not just one spike)
├── De-escalation already has hysteresis (consecutive normal sessions)
└── This prevents notification fatigue from boundary oscillation

EDGE CASE 8: Monitor receives events from unknown agent
├── ingest.rs validates agent_instance_id against known agents
├── Unknown agent: log warning, create provisional tracking state
├── Provisional state: signals computed but no interventions
│   (could be a new agent that hasn't registered yet)
└── After 3 sessions without registration: drop provisional state
```


---

## APPENDIX: THRESHOLD DISCREPANCY NOTE

The architecture documents use two different threshold ranges:

| Source | L0/L1 | L1/L2 | L2/L3 | L3/L4 |
|--------|-------|-------|-------|-------|
| AGENT_ARCHITECTURE_v2.md §3 | 0.2 | 0.4 | 0.6 | 0.8 |
| explore/docs/07-detection-formalization.md §9 | 0.3 | 0.5 | 0.7 | 0.85 |
| cortex-core/config/convergence_config.rs (code) | 0.3 | 0.5 | 0.7 | 0.85 |

This document uses the **implementation values** (0.3/0.5/0.7/0.85) from the actual
`ConvergenceScoringConfig::default()` in the implementation guide. The v2 architecture
doc values are the earlier design-phase numbers that were refined during formalization.

The implementation values are correct for building. The v2 doc should be updated to match
if you want doc consistency, but the code is the source of truth.

---

## APPENDIX: CROSS-REFERENCE TO FILE_MAPPING.md

Every file referenced in this sequence flow, mapped to its FILE_MAPPING.md location:

| This Document References | FILE_MAPPING.md Section | Status |
|--------------------------|------------------------|--------|
| `ghost-agent-loop/src/itp_emitter.rs` | Layer 3: ghost-agent-loop | NEW file |
| `ghost-agent-loop/src/runner.rs` | Layer 3: ghost-agent-loop | NEW file |
| `ghost-agent-loop/src/context/prompt_compiler.rs` | Layer 3: ghost-agent-loop | NEW file |
| `convergence-monitor/src/pipeline/ingest.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/pipeline/signal_computer.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/pipeline/window_manager.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/intervention/trigger.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/intervention/actions.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/intervention/cooldown.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/intervention/escalation.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/transport/unix_socket.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/transport/http_api.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/transport/notification.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/session/registry.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/session/boundary.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/verification/behavioral_verification.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/monitor.rs` | Layer 2: convergence-monitor | NEW file |
| `convergence-monitor/src/config.rs` | Layer 2: convergence-monitor | NEW file |
| `cortex-convergence/src/signals/*.rs` (7 files) | Layer 1B: cortex-convergence | NEW files |
| `cortex-convergence/src/scoring/composite.rs` | Layer 1B: cortex-convergence | NEW file |
| `cortex-convergence/src/scoring/baseline.rs` | Layer 1B: cortex-convergence | NEW file |
| `cortex-convergence/src/windows/sliding_window.rs` | Layer 1B: cortex-convergence | EXISTS (stub) |
| `cortex-convergence/src/types.rs` | Layer 1B: cortex-convergence | EXISTS (stub) |
| `ghost-policy/src/policy/convergence_policy.rs` | Layer 3: ghost-policy | NEW file |
| `ghost-policy/src/engine.rs` | Layer 3: ghost-policy | NEW file |
| `ghost-policy/src/feedback.rs` | Layer 3: ghost-policy | NEW file |
| `ghost-gateway/src/session/manager.rs` | Layer 3: ghost-gateway | NEW file |
| `ghost-gateway/src/session/boundary.rs` | Layer 3: ghost-gateway | NEW file (Finding 17) |
| `ghost-gateway/src/bootstrap.rs` | Layer 3: ghost-gateway | NEW file |
| `ghost-gateway/src/health.rs` | Layer 3: ghost-gateway | NEW file |
| `ghost-gateway/src/routing/lane_queue.rs` | Layer 3: ghost-gateway | NEW file |
| `itp-protocol/src/events/*.rs` | Layer 2: itp-protocol | NEW files |
| `itp-protocol/src/privacy.rs` | Layer 2: itp-protocol | NEW file |
| `cortex-storage/src/migrations/v016_convergence_safety.rs` | Layer 1B: cortex-storage | NEW file |
| `cortex-storage/src/migrations/v017_convergence_tables.rs` | Layer 1B: cortex-storage | NEW file |
| `cortex-storage/src/queries/itp_queries.rs` | Layer 1B: cortex-storage | NEW file |
| `cortex-storage/src/queries/convergence_queries.rs` | Layer 1B: cortex-storage | NEW file |
| `cortex-storage/src/queries/intervention_queries.rs` | Layer 1B: cortex-storage | NEW file |
| `cortex-core/src/config/convergence_config.rs` | Layer 1B: cortex-core | NEW file |
| `cortex-decay/src/factors/convergence.rs` | Layer 1B: cortex-decay | NEW file |
| `cortex-convergence/src/filtering/convergence_aware_filter.rs` | Layer 1B: cortex-convergence | NEW file |
| `read-only-pipeline/src/assembler.rs` | Layer 2: read-only-pipeline | NEW file |
| `read-only-pipeline/src/snapshot.rs` | Layer 2: read-only-pipeline | NEW file |
| `ghost-channels/adapters/telegram.rs` | Layer 3: ghost-channels | NEW file |
| `ghost-llm/provider/anthropic.rs` | Layer 3: ghost-llm | NEW file |
| `ghost-audit/src/query.rs` | Layer 3: ghost-audit | NEW file |
| `extension/src/background/itp-emitter.ts` | Browser Extension | NEW file |
| `ghost-proxy/src/parsers/itp_emitter.rs` | Layer 3: ghost-proxy | NEW file |
| `simulation-boundary/src/enforcer.rs` | Layer 2: simulation-boundary | NEW file |
| `convergence-monitor/src/transport/native_messaging.rs` | Layer 2: convergence-monitor | NEW file |
| `ghost-gateway/src/safety/kill_switch.rs` | Layer 3: ghost-gateway (Finding 3) | NEW file |
| `ghost-gateway/src/safety/auto_triggers.rs` | Layer 3: ghost-gateway (Finding 3) | NEW file |
| `ghost-gateway/src/safety/quarantine.rs` | Layer 3: ghost-gateway (Finding 3) | NEW file |
