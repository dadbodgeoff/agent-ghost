# Agent Loop Recursive Execution — Complete Sequence Flow

> Crate: `ghost-agent-loop` (`crates/ghost-agent-loop/`)
> Primary file: `src/runner.rs` — `AgentRunner`
> Date: 2026-02-27
> Purpose: Zero-ambiguity sequence flow for the recursive agent loop with all
> interleaved concerns mapped to exact call sites, failure modes, and state transitions.
> Every branch, every error path, every interleaving point documented.

---

## TABLE OF CONTENTS

1. [Participants & Ownership](#1-participants--ownership)
2. [Data Structures In Play](#2-data-structures-in-play)
3. [Pre-Loop: Message Arrival → Runner Invocation](#3-pre-loop-message-arrival--runner-invocation)
4. [The Recursive Loop: Complete Step-by-Step](#4-the-recursive-loop-complete-step-by-step)
5. [Circuit Breaker State Machine](#5-circuit-breaker-state-machine)
6. [Convergence Integration Points](#6-convergence-integration-points)
7. [Proposal Extraction Pipeline](#7-proposal-extraction-pipeline)
8. [ITP Event Emission Points](#8-itp-event-emission-points)
9. [Error Taxonomy & Recovery Paths](#9-error-taxonomy--recovery-paths)
10. [Post-Loop: Persist & Cleanup](#10-post-loop-persist--cleanup)
11. [Interleaving Hazard Map](#11-interleaving-hazard-map)
12. [Full Sequence Diagram (ASCII)](#12-full-sequence-diagram-ascii)
13. [Invariants That Must Hold](#13-invariants-that-must-hold)

---

## 1. PARTICIPANTS & OWNERSHIP

Every participant in the agent loop, the crate that owns it, and the exact file.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ PARTICIPANT              │ CRATE                    │ FILE                       │
├──────────────────────────┼──────────────────────────┼────────────────────────────┤
│                          │                          │                            │
│ ── AGENT LOOP (core) ──  │                          │                            │
│ AgentRunner              │ ghost-agent-loop         │ src/runner.rs              │
│ CircuitBreaker           │ ghost-agent-loop         │ src/circuit_breaker.rs     │
│ AgentITPEmitter          │ ghost-agent-loop         │ src/itp_emitter.rs         │
│ ProposalExtractor        │ ghost-agent-loop         │ src/proposal/extractor.rs  │
│ ProposalRouter           │ ghost-agent-loop         │ src/proposal/router.rs     │
│ PromptCompiler           │ ghost-agent-loop         │ src/context/prompt_compiler│
│ TokenBudgetAllocator     │ ghost-agent-loop         │ src/context/token_budget.rs│
│ ToolRegistry             │ ghost-agent-loop         │ src/tools/registry.rs      │
│ ToolExecutor             │ ghost-agent-loop         │ src/tools/executor.rs      │
│                          │                          │                            │
│ ── POLICY ──             │                          │                            │
│ PolicyEngine             │ ghost-policy             │ src/engine.rs              │
│ ConvergencePolicyTighten │ ghost-policy             │ src/policy/convergence_pol │
│                          │                          │                            │
│ ── LLM ──                │                          │                            │
│ LLMProvider              │ ghost-llm               │ src/provider/mod.rs        │
│ ModelRouter              │ ghost-llm               │ src/routing/model_router.rs│
│ ComplexityClassifier     │ ghost-llm               │ src/routing/classifier.rs  │
│ FallbackChain            │ ghost-llm               │ src/routing/fallback.rs    │
│ CostCalculator           │ ghost-llm               │ src/cost.rs               │
│                          │                          │                            │
│ ── GATEWAY ──            │                          │                            │
│ MessageRouter            │ ghost-gateway            │ src/routing/message_rtr.rs │
│ LaneQueue                │ ghost-gateway            │ src/routing/lane_queue.rs  │
│ AgentRegistry            │ ghost-gateway            │ src/agents/registry.rs     │
│ CostTracker              │ ghost-gateway            │ src/cost/tracker.rs        │
│ SpendingCapEnforcer      │ ghost-gateway            │ src/cost/spending_cap.rs   │
│ SessionManager           │ ghost-gateway            │ src/session/manager.rs     │
│ SessionContext           │ ghost-gateway            │ src/session/context.rs     │
│ SessionCompactor         │ ghost-gateway            │ src/session/compaction.rs  │
│ KillSwitch               │ ghost-gateway            │ src/safety/kill_switch.rs  │
│ AutoTriggerEvaluator     │ ghost-gateway            │ src/safety/auto_triggers.rs│
│ QuarantineManager        │ ghost-gateway            │ src/safety/quarantine.rs   │
│                          │                          │                            │
│ ── READ-ONLY PIPELINE ── │                          │                            │
│ SnapshotAssembler        │ read-only-pipeline       │ src/assembler.rs           │
│ SnapshotFormatter        │ read-only-pipeline       │ src/formatter.rs           │
│  (internal to assembler — serializes AgentSnapshot into prompt-ready text)       │
│ AgentSnapshot            │ read-only-pipeline       │ src/snapshot.rs            │
│                          │                          │                            │
│ ── CONVERGENCE MONITOR ──│                          │                            │
│ ConvergenceMonitor       │ convergence-monitor      │ src/monitor.rs             │
│ InterventionTrigger      │ convergence-monitor      │ src/intervention/trigger.rs│
│ CooldownManager          │ convergence-monitor      │ src/intervention/cooldown  │
│ SessionBoundaryEnforcer  │ convergence-monitor      │ src/session/boundary.rs    │
│                          │                          │                            │
│ ── SIMULATION BOUNDARY ──│                          │                            │
│ SimBoundaryEnforcer      │ simulation-boundary      │ src/enforcer.rs            │
│ OutputReframer           │ simulation-boundary      │ src/reframer.rs            │
│                          │                          │                            │
│ ── CORTEX ──             │                          │                            │
│ ProposalValidator        │ cortex-validation        │ src/proposal_validator.rs  │
│ ConvergenceAwareFilter   │ cortex-convergence       │ src/filtering/conv_filter  │
│ CompositeScorer          │ cortex-convergence       │ src/scoring/composite.rs   │
│                          │                          │                            │
│ ── PROTOCOL/AUDIT ──     │                          │                            │
│ ITPEvent types           │ itp-protocol             │ src/events/*.rs            │
│ AuditLogger              │ ghost-audit              │ (via cortex-storage)       │
└─────────────────────────────────────────────────────────────────────────────────┘

CALLERS (invoke the agent loop but are not participants within it):
  - ghost-gateway/src/routing/message_router.rs — routes inbound messages to runner
  - ghost-heartbeat/src/heartbeat.rs — HeartbeatEngine triggers periodic runs
  - ghost-heartbeat/src/cron.rs — CronEngine triggers scheduled runs
  - ghost-channels/adapters/*.rs — channel adapters deliver normalized messages
```

---

## 2. DATA STRUCTURES IN PLAY

These are the structs that flow through the recursive loop. Every field matters
because the interleaving of concerns means each struct carries state that
multiple subsystems read and write.

### 2.1 AgentRunner (owns the loop)

```rust
// ghost-agent-loop/src/runner.rs
pub struct AgentRunner {
    // --- Core dependencies (injected at construction) ---
    llm_provider: Arc<dyn LLMProvider>,       // ghost-llm — the model
    model_router: Arc<ModelRouter>,            // ghost-llm — tier selection
    policy_engine: Arc<PolicyEngine>,          // ghost-policy — authorization
    tool_registry: Arc<ToolRegistry>,          // ghost-agent-loop — available tools
    tool_executor: Arc<ToolExecutor>,          // ghost-agent-loop — sandboxed execution
    prompt_compiler: Arc<PromptCompiler>,      // ghost-agent-loop — 10-layer context
    proposal_extractor: Arc<ProposalExtractor>,// ghost-agent-loop — parse proposals
    proposal_router: Arc<ProposalRouter>,      // ghost-agent-loop — route proposals

    // --- Convergence integration (read-only from agent's perspective) ---
    snapshot_assembler: Arc<SnapshotAssembler>,// read-only-pipeline — convergence state
    sim_boundary: Arc<SimBoundaryEnforcer>,    // simulation-boundary — output scanning
    itp_emitter: Arc<AgentITPEmitter>,         // ghost-agent-loop — telemetry emission

    // --- Safety ---
    circuit_breaker: CircuitBreaker,           // ghost-agent-loop — failure tracking
    kill_switch: Arc<KillSwitch>,              // ghost-gateway — emergency stop

    // --- Cost ---
    cost_tracker: Arc<CostTracker>,            // ghost-gateway — token/dollar tracking
    spending_cap: Arc<SpendingCapEnforcer>,    // ghost-gateway — hard limits

    // --- Config ---
    max_recursion_depth: usize,                // default: 25
    agent_id: AgentId,                         // cortex-core — who we are
}
```

### 2.2 RunContext (per-invocation mutable state)

```rust
// ghost-agent-loop/src/runner.rs (internal)
struct RunContext {
    session: SessionContext,                    // ghost-gateway — session state
    messages: Vec<Message>,                    // accumulated conversation
    recursion_depth: usize,                    // current depth counter
    total_input_tokens: usize,                 // accumulated across all turns
    total_output_tokens: usize,                // accumulated across all turns
    total_cost_usd: f64,                       // accumulated cost
    tool_calls_this_run: Vec<ToolCallRecord>,  // audit trail
    proposals_extracted: Vec<Proposal>,        // accumulated proposals
    itp_events_emitted: Vec<ITPEventId>,       // tracking for correlation
    convergence_snapshot: Option<AgentSnapshot>,// latest snapshot from read-only-pipeline
    intervention_level: u8,                    // 0-4, from convergence monitor
    circuit_breaker_state: CircuitState,       // Closed/Open/HalfOpen
    damage_counter: DamageCounter,             // cumulative failure tracker (never resets)
    no_reply: bool,                            // suppress output flag
}
```

### 2.3 Key Enums

```rust
// Circuit breaker states
enum CircuitState { Closed, Open(Instant), HalfOpen }

// Policy decisions
enum PolicyDecision { Permit, Deny(DenialFeedback), Escalate(EscalationRequest) }

// LLM response types (what comes back from inference)
enum LLMResponseChunk {
    Text(String),
    ToolCall(ToolCallRequest),
    Done,
    Error(LLMError),
}

// Tool execution results
enum ToolResult {
    Success { stdout: String, stderr: String, duration: Duration },
    Failure { error: String, retryable: bool },
    Timeout { partial_output: String },
    PolicyDenied(DenialFeedback),
}
```

---

## 3. PRE-LOOP: MESSAGE ARRIVAL → RUNNER INVOCATION

Before `AgentRunner::run()` is called, the gateway has already done work.
This section maps what happens BEFORE the recursive loop starts, because
the runner depends on this state being correct.

```
SEQUENCE: Message Arrival → Runner Invocation

    External Source (Telegram, Discord, CLI, WebSocket, etc.)
        │
        │  [1] Channel adapter normalizes to InboundMessage
        │      Owner: ghost-channels/adapters/{channel}.rs
        │      Output: InboundMessage { text, attachments, sender, channel_meta }
        │
        ▼
    MessageRouter::route(inbound_message)
        │  Owner: ghost-gateway/src/routing/message_router.rs
        │
        │  [2] Resolve agent binding
        │      Which agent handles this channel + sender combination?
        │      Lookup: AgentRegistry::find_by_channel(channel, sender)
        │      FAIL: No agent bound → return "no agent configured" to channel
        │
        │  [3] Resolve or create session
        │      SessionManager::get_or_create(agent_id, channel, sender)
        │      Session key = hash(agent_id, channel_type, sender_id)
        │      If new session: emit ITP SessionStart event
        │
        │  [4] Acquire session lock (LaneQueue)
        │      LaneQueue::enqueue(session_id, request)
        │      If queue depth > limit (default 5): reject with 429
        │      Serialized: only one request per session executes at a time
        │      BLOCKS until prior request completes
        │
        ▼
    ── SESSION LOCK ACQUIRED ──
        │
        │  [5] Check KillSwitch state
        │      KillSwitch::check(agent_id)
        │      If PAUSED: return "agent paused" to channel, release lock
        │      If QUARANTINED: return "agent unavailable", release lock
        │      If KILL_ALL: return "system offline", release lock
        │
        │  [6] Check SpendingCap
        │      SpendingCapEnforcer::check(agent_id)
        │      If exceeded: return "spending cap reached", release lock
        │      Note: checked BEFORE inference to avoid wasting tokens
        │
        │  [7] Check CooldownManager (convergence)
        │      CooldownManager::can_start_session(agent_id)
        │      If in cooldown: return "please wait {remaining}", release lock
        │      Cooldown enforced by convergence-monitor, not the agent
        │
        │  [8] Check SessionBoundaryEnforcer (convergence)
        │      SessionBoundaryEnforcer::check_duration(session_id)
        │      If session exceeded max duration for current intervention level:
        │        emit ITP SessionEnd, return "session limit reached", release lock
        │
        │  [9] Build AgentSnapshot (read-only convergence state)
        │      SnapshotAssembler::build(agent_id, session_id)
        │      Pulls from:
        │        - cortex-convergence: current ConvergenceState (score, level)
        │        - cortex-convergence/filtering: filtered goals, reflections
        │        - cortex-retrieval: convergence-filtered memories
        │        - simulation-boundary: SimulationBoundaryPrompt (compiled into binary)
        │      Output: AgentSnapshot {
        │          goals: Vec<Goal>,           // filtered by convergence tier
        │          reflections: Vec<Reflection>,// bounded by depth/count
        │          memories: Vec<BaseMemory>,   // filtered by convergence tier
        │          convergence_state: ConvergenceState,
        │          simulation_boundary_prompt: &'static str,
        │          intervention_level: u8,
        │      }
        │
        │      SERIALIZATION: After assembly, SnapshotFormatter::format()
        │      serializes the AgentSnapshot into prompt-ready text blocks.
        │      Owner: read-only-pipeline/src/formatter.rs
        │      This converts typed structs into the text that PromptCompiler
        │      injects at L6 (convergence state). The formatter handles:
        │        - Goal list → numbered markdown
        │        - Reflections → bounded text with depth indicators
        │        - Convergence score → human-readable status line
        │        - Intervention level → behavioral guidance text
        │      The PromptCompiler does NOT format the snapshot itself —
        │      it receives pre-formatted text blocks from the formatter.
        │
        │      This snapshot is IMMUTABLE for the duration of this run.
        │      The agent cannot modify it. It's assembled once, read many times.
        │
        │  [10] Construct RunContext
        │       RunContext {
        │           session: session_context,
        │           messages: session.conversation_history.clone(),
        │           recursion_depth: 0,
        │           total_input_tokens: 0,
        │           total_output_tokens: 0,
        │           total_cost_usd: 0.0,
        │           tool_calls_this_run: vec![],
        │           proposals_extracted: vec![],
        │           itp_events_emitted: vec![],
        │           convergence_snapshot: Some(snapshot),
        │           intervention_level: snapshot.intervention_level,
        │           circuit_breaker_state: self.circuit_breaker.state(),
        │           damage_counter: DamageCounter { total_failures: 0, threshold: 5 },
        │           no_reply: false,
        │       }
        │
        │  [11] Emit ITP InteractionMessage event (user message)
        │       AgentITPEmitter::emit_interaction(
        │           session_id, "user", message_text, privacy_level
        │       )
        │       ASYNC, NON-BLOCKING. Monitor unavailability does NOT block.
        │       If emit fails: log warning, continue. Safety degrades gracefully.
        │
        ▼
    AgentRunner::run(message, run_context) ← ENTERS THE RECURSIVE LOOP
```

### 3.1 ALTERNATE ENTRY PATH: Heartbeat / Cron Runs

```
Heartbeat and cron runs enter the agent loop differently from channel messages.
There is NO channel adapter and NO inbound message normalization.

HEARTBEAT ENTRY:
    HeartbeatEngine (ghost-heartbeat/src/heartbeat.rs)
        │
        │  [H1] Timer fires (configurable interval, default 30min)
        │       Respects active_hours from ghost.yml
        │       If outside active hours: skip, reschedule
        │
        │  [H2] Cost check: heartbeat.max_cost_per_day
        │       If today's heartbeat spend >= cap: skip
        │
        │  [H3] Construct synthetic message
        │       message = SyntheticMessage {
        │           text: "[HEARTBEAT] Check HEARTBEAT.md and act if needed.",
        │           source: MessageSource::Heartbeat,
        │           channel: None,  // no channel — no outbound delivery
        │           sender: AgentSelf,
        │       }
        │
        │  [H4] Resolve session
        │       SessionManager::get_or_create(agent_id, "heartbeat", agent_id)
        │       Heartbeat runs use a DEDICATED session (not the user's session)
        │       Session key = hash(agent_id, "heartbeat", agent_id)
        │
        │  [H5] Steps 4-10 proceed IDENTICALLY to channel path
        │       (LaneQueue, KillSwitch, SpendingCap, Cooldown, Boundary,
        │        Snapshot, RunContext, ITP emission)
        │
        │  [H6] AgentRunner::run() executes
        │       If agent finds nothing: returns NO_REPLY / HEARTBEAT_OK
        │       If agent finds something: may produce text + tool calls
        │       Text output goes to target session (ghost.yml heartbeat.target)
        │       NOT to a channel adapter (there is none)
        │
        ▼

CRON ENTRY:
    CronEngine (ghost-heartbeat/src/cron.rs)
        │
        │  [C1] Cron schedule fires (e.g., "0 8 * * *" for 8am daily)
        │       Timezone-aware (from job YAML definition)
        │
        │  [C2] Load job definition
        │       Source: ~/.ghost/agents/{name}/cognition/cron/jobs/{job}.yml
        │       Contains: name, schedule, prompt, target_channel (optional)
        │
        │  [C3] Construct synthetic message
        │       message = SyntheticMessage {
        │           text: job.prompt,  // e.g., "Prepare morning briefing"
        │           source: MessageSource::Cron(job.name),
        │           channel: job.target_channel,  // may be Some or None
        │           sender: AgentSelf,
        │       }
        │
        │  [C4] Steps 4-10 proceed IDENTICALLY to channel path
        │       Same gates, same safety checks, same snapshot assembly
        │
        │  [C5] AgentRunner::run() executes
        │       Output delivery depends on job.target_channel:
        │         Some(channel) → deliver via channel adapter
        │         None → store in session transcript only
        │
        ▼

KEY DIFFERENCES FROM CHANNEL ENTRY:
  1. No channel adapter normalization (message is synthetic)
  2. No InboundMessage struct (uses SyntheticMessage)
  3. MessageRouter is BYPASSED — heartbeat/cron go directly to
     SessionManager (they already know which agent to invoke)
  4. Heartbeat uses a dedicated session (not user's conversation)
  5. Cron may or may not have a target channel for output delivery
  6. Model tier classification (STEP B.1) defaults to TIER 0 for
     heartbeat runs unless urgent signals are detected
  7. NO_REPLY suppression is the EXPECTED outcome for heartbeat
     (most heartbeat runs find nothing to report)
```

### Pre-Loop Invariants (must be true before run() is called)

```
INV-PRE-01: Session lock is held. No other request for this session is executing.
INV-PRE-02: KillSwitch state is not PAUSED, QUARANTINED, or KILL_ALL.
INV-PRE-03: SpendingCap has not been exceeded for this agent.
INV-PRE-04: CooldownManager permits a session for this agent.
INV-PRE-05: SessionBoundaryEnforcer permits continued interaction.
INV-PRE-06: AgentSnapshot is assembled and immutable.
INV-PRE-07: CircuitBreaker state is known (Closed, Open, or HalfOpen).
INV-PRE-08: ITP SessionStart has been emitted (if new session).
INV-PRE-09: recursion_depth == 0.
INV-PRE-10: User message has been appended to run_context.messages.
INV-PRE-11: DamageCounter.total_failures == 0 (fresh for each run).
```

---

## 4. THE RECURSIVE LOOP: COMPLETE STEP-BY-STEP

This is the core. Every line is a decision point. Every branch is documented.
The loop is `AgentRunner::run()` which calls itself recursively via
`AgentRunner::execute_turn()`.

### 4.1 Entry Point: `AgentRunner::run()`

```
AgentRunner::run(message: UserMessage, ctx: &mut RunContext) -> Result<AgentResponse>
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ GATE 0: Circuit Breaker Check (BEFORE anything else)       │
    │  │                                                             │
    │  │ match self.circuit_breaker.state() {                        │
    │  │   CircuitState::Open(tripped_at) => {                       │
    │  │     if now() - tripped_at < cooldown_duration {             │
    │  │       // Circuit is OPEN. Do NOT call LLM. Do NOT execute   │
    │  │       // tools. Return a structured error to the user.      │
    │  │       return Err(AgentError::CircuitOpen {                  │
    │  │         tripped_at,                                         │
    │  │         cooldown_remaining: cooldown_duration - elapsed,    │
    │  │         consecutive_failures: self.circuit_breaker.failures │
    │  │       });                                                   │
    │  │     } else {                                                │
    │  │       // Cooldown expired. Transition to HALF-OPEN.         │
    │  │       self.circuit_breaker.transition(HalfOpen);            │
    │  │       // Allow ONE probe call through. If it fails,         │
    │  │       // circuit goes back to OPEN.                         │
    │  │     }                                                       │
    │  │   }                                                         │
    │  │   CircuitState::HalfOpen => {                               │
    │  │     // Already in probe mode. Allow the call.               │
    │  │     // If this turn's tool calls fail, circuit re-opens.    │
    │  │   }                                                         │
    │  │   CircuitState::Closed => {                                 │
    │  │     // Normal operation. Proceed.                           │
    │  │   }                                                         │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ GATE 1: Recursion Depth Check                               │
    │  │                                                             │
    │  │ if ctx.recursion_depth >= self.max_recursion_depth {        │
    │  │   // HARD STOP. No more recursive calls.                    │
    │  │   // This prevents infinite loops from runaway tool chains. │
    │  │   // Log as warning. Return what we have so far.            │
    │  │   emit_itp_event(AgentStateSnapshot {                      │
    │  │     reason: "max_recursion_depth_reached",                  │
    │  │     depth: ctx.recursion_depth,                             │
    │  │   });                                                       │
    │  │   return Ok(AgentResponse {                                 │
    │  │     text: "[Max recursion depth reached. Stopping.]",       │
    │  │     tool_calls: ctx.tool_calls_this_run.clone(),            │
    │  │     proposals: ctx.proposals_extracted.clone(),              │
    │  │     cost: ctx.total_cost_usd,                               │
    │  │     truncated: true,                                        │
    │  │   });                                                       │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ GATE 1.5: DamageCounter Check (cumulative failure gate)     │
    │  │                                                             │
    │  │ // The DamageCounter tracks TOTAL failures across the       │
    │  │ // entire run (never resets — see §5.3 design evolution).   │
    │  │ // This is separate from the circuit breaker (consecutive). │
    │  │ //                                                          │
    │  │ // Checked here (not just in F.4) because a prior turn's   │
    │  │ // failures may have pushed the counter to threshold        │
    │  │ // without tripping the circuit breaker (non-consecutive).  │
    │  │ //                                                          │
    │  │ if ctx.damage_counter.total_failures                        │
    │  │    >= ctx.damage_counter.threshold {                        │
    │  │   emit_itp_event(AgentStateSnapshot {                      │
    │  │     reason: "damage_counter_exceeded",                      │
    │  │     total_failures: ctx.damage_counter.total_failures,      │
    │  │     threshold: ctx.damage_counter.threshold,                │
    │  │   });                                                       │
    │  │   return Ok(AgentResponse {                                 │
    │  │     text: "[Too many failures this run. Stopping.]",        │
    │  │     tool_calls: ctx.tool_calls_this_run.clone(),            │
    │  │     proposals: ctx.proposals_extracted.clone(),              │
    │  │     cost: ctx.total_cost_usd,                               │
    │  │     truncated: true,                                        │
    │  │   });                                                       │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ GATE 2: Spending Cap Re-check (per-turn)                    │
    │  │                                                             │
    │  │ // Checked pre-loop too, but re-check because recursive     │
    │  │ // turns accumulate cost. A 25-deep recursion could blow    │
    │  │ // past the cap if only checked once.                       │
    │  │ if self.spending_cap.would_exceed(                          │
    │  │     self.agent_id,                                          │
    │  │     ctx.total_cost_usd + estimated_turn_cost                │
    │  │ ) {                                                         │
    │  │   return Err(AgentError::SpendingCapExceeded {              │
    │  │     current: ctx.total_cost_usd,                            │
    │  │     cap: self.spending_cap.limit(self.agent_id),            │
    │  │   });                                                       │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ GATE 3: KillSwitch Re-check (per-turn)                      │
    │  │                                                             │
    │  │ // Kill switch can be triggered MID-RUN by AutoTrigger      │
    │  │ // evaluator (e.g., 5+ policy denials this session).        │
    │  │ // Must re-check every turn.                                │
    │  │ match self.kill_switch.check(self.agent_id) {               │
    │  │   KillState::Active => { /* proceed */ }                    │
    │  │   KillState::Paused => return Err(AgentError::AgentPaused), │
    │  │   KillState::Quarantined(reason) =>                         │
    │  │     return Err(AgentError::AgentQuarantined(reason)),       │
    │  │   KillState::KillAll =>                                     │
    │  │     return Err(AgentError::SystemKilled),                   │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    ▼
    STEP A: CONTEXT ASSEMBLY (prompt_compiler)
    │
    ▼
    STEP B: INFERENCE (LLM call)
    │
    ▼
    STEP C: RESPONSE PROCESSING (branch on response type)
    │
    ├── [Text Response] → STEP D: OUTPUT PROCESSING
    ├── [Tool Call]     → STEP E: POLICY CHECK → STEP F: TOOL EXECUTION → RECURSE
    └── [NO_REPLY]      → STEP G: SUPPRESS
```

### 4.2 STEP A: Context Assembly

```
STEP A: Context Assembly
    │
    │  Owner: ghost-agent-loop/src/context/prompt_compiler.rs
    │  Called: PromptCompiler::compile(ctx, snapshot)
    │
    │  The prompt compiler builds the full context window from 10 layers.
    │  Each layer has a token budget. Overflow is handled per-layer.
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ LAYER ASSEMBLY ORDER (strict — order matters for priority)  │
    │  │                                                             │
    │  │ L0: CORP_POLICY.md                                          │
    │  │     Source: ghost-identity/src/corp_policy.rs                │
    │  │     Loaded: CorpPolicyLoader::load() with signature verify  │
    │  │     Budget: uncapped (always included in full)               │
    │  │     Inject: as system message, highest priority              │
    │  │     INVARIANT: If signature verification fails, ABORT RUN.  │
    │  │     This is the immutable root. No fallback. No degradation.│
    │  │                                                             │
    │  │ L1: Simulation Boundary Prompt                              │
    │  │     Source: simulation-boundary/src/prompt_anchor.rs         │
    │  │     Loaded: SimulationBoundaryPrompt::get()                 │
    │  │     Budget: uncapped (compiled into binary, ~200 tokens)    │
    │  │     Inject: as system message, after CORP_POLICY             │
    │  │     NOTE: This is a const &str compiled into the binary.    │
    │  │     Agent cannot see the source. Cannot modify. Cannot       │
    │  │     reference. It's invisible infrastructure.                │
    │  │                                                             │
    │  │ L2: SOUL.md + IDENTITY.md                                   │
    │  │     Source: ghost-identity/src/soul.rs, identity.rs          │
    │  │     Budget: configurable (default ~2000 tokens)              │
    │  │     Inject: as system message                                │
    │  │     Read-only to agent. Agent sees content but cannot write. │
    │  │                                                             │
    │  │ L3: Tool Schemas                                            │
    │  │     Source: ghost-agent-loop/src/tools/registry.rs           │
    │  │     ToolRegistry::schemas_json()                             │
    │  │     Budget: configurable (default ~3000 tokens)              │
    │  │     NOTE: Only tools the agent is PERMITTED to call are      │
    │  │     included. ConvergencePolicyTightener may have removed    │
    │  │     tools based on intervention level.                       │
    │  │     At Level 2+: proactive tools removed from schema.        │
    │  │     At Level 3+: only task-essential tools remain.           │
    │  │     At Level 4: minimal tool set (task-only mode).           │
    │  │                                                             │
    │  │ L4: Environment                                             │
    │  │     Current time, OS, workspace path, agent name             │
    │  │     Budget: ~200 tokens (fixed)                              │
    │  │                                                             │
    │  │ L5: Skill Index                                             │
    │  │     Source: ghost-skills/src/registry.rs                     │
    │  │     SkillRegistry::index() — names + descriptions only      │
    │  │     Budget: configurable (default ~500 tokens)               │
    │  │     Bodies loaded on-demand via read_skill tool.             │
    │  │                                                             │
    │  │ L6: Convergence State (FROM READ-ONLY PIPELINE)             │
    │  │     Source: ctx.convergence_snapshot (AgentSnapshot)          │
    │  │     Contains:                                                │
    │  │       - Current convergence score (0.0-1.0)                  │
    │  │       - Current intervention level (0-4)                     │
    │  │       - Filtered goals (convergence-tier filtered)           │
    │  │       - Bounded reflections (depth ≤3, count ≤20/session)   │
    │  │     Budget: configurable (default ~1000 tokens)              │
    │  │     CRITICAL: This is the read-only convergence state.       │
    │  │     The agent sees its score and goals but CANNOT modify     │
    │  │     them directly. Modifications go through ProposalRouter.  │
    │  │                                                             │
    │  │ L7: MEMORY.md + Daily Logs                                  │
    │  │     Source: ~/.ghost/agents/{name}/MEMORY.md (per-agent)     │
    │  │     Loaded by: ghost-identity/src/memory.rs (MemoryLoader)   │
    │  │     NOTE: user.rs manages USER.md, NOT MEMORY.md.            │
    │  │     + memory/daily/{today}.md + memory/daily/{yesterday}.md  │
    │  │     Budget: configurable (default ~4000 tokens)              │
    │  │     NOTE: Memories are ALREADY FILTERED by convergence tier  │
    │  │     via ConvergenceAwareFilter in the SnapshotAssembler.     │
    │  │                                                             │
    │  │     SCORE → TIER → LEVEL MAPPING:                           │
    │  │     The convergence system uses three related concepts:      │
    │  │       1. Convergence SCORE: float 0.0-1.0 (CompositeScorer) │
    │  │       2. Convergence TIER: 0-3 (ConvergenceAwareFilter)     │
    │  │          Score 0.0-0.3 → Tier 0 (full access)               │
    │  │          Score 0.3-0.5 → Tier 1 (reduced emotional content) │
    │  │          Score 0.5-0.7 → Tier 2 (task-focused only)         │
    │  │          Score 0.7+    → Tier 3 (minimal)                   │
    │  │       3. Intervention LEVEL: 0-4 (InterventionTrigger)      │
    │  │          Mapped via score_to_level() with hysteresis:        │
    │  │          Score 0.0-0.2 → Level 0 (normal)                   │
    │  │          Score 0.2-0.4 → Level 1 (monitoring)               │
    │  │          Score 0.4-0.6 → Level 2 (soft intervention)        │
    │  │          Score 0.6-0.8 → Level 3 (hard intervention)        │
    │  │          Score 0.8+    → Level 4 (lockdown)                  │
    │  │     Tiers control MEMORY FILTERING (what the agent sees).   │
    │  │     Levels control POLICY + TOOLS (what the agent can do).  │
    │  │     Both derive from the same score but serve different      │
    │  │     purposes with different threshold boundaries.            │
    │  │                                                             │
    │  │     At Level 2+: emotional/attachment memories excluded.     │
    │  │     At Level 3: only task-relevant memories.                 │
    │  │     At Level 4: minimal context.                             │
    │  │     The agent does NOT know filtering is happening.          │
    │  │                                                             │
    │  │ L8: Conversation History                                    │
    │  │     Source: ctx.messages (accumulated this session)           │
    │  │     Budget: remainder after L0-L7 and L9 reserved            │
    │  │     Pruning: oldest messages first. Tool results truncated   │
    │  │     before user/assistant messages.                          │
    │  │                                                             │
    │  │ L9: User Message (current turn)                             │
    │  │     Source: the incoming message                              │
    │  │     Budget: uncapped (always included in full)               │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  Token Budget Enforcement:
    │  TokenBudgetAllocator::allocate(model_context_window, layers)
    │  Priority order for truncation: L8 > L7 > L5 > L2
    │  Never truncate: L0, L1, L9 (immutable root, boundary, user msg)
    │  If total exceeds model context window after truncation:
    │    trigger SessionCompactor (see §10)
    │
    │  Output: Vec<Message> — the full prompt ready for the LLM
    │
    ▼
```

### 4.3 STEP B: Inference (LLM Call)

```
STEP B: Inference
    │
    │  Owner: ghost-llm/src/provider/mod.rs (trait), specific provider impl
    │  Orchestrated by: ghost-llm/src/routing/model_router.rs
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ B.1: Model Selection (ComplexityClassifier)                 │
    │  │                                                             │
    │  │ tier = ComplexityClassifier::classify(message, ctx)         │
    │  │                                                             │
    │  │ Classification rules (heuristic, NOT an LLM call):          │
    │  │   - message.len() < 20 AND no tool keywords → TIER 0       │
    │  │   - matches greeting/ack patterns → TIER 0                  │
    │  │   - heartbeat run AND no urgent signals → TIER 0            │
    │  │   - single tool likely → TIER 1                             │
    │  │   - multi-tool or reasoning → TIER 2                        │
    │  │   - user explicitly /deep or /think → TIER 3                │
    │  │   - default → TIER 1                                        │
    │  │                                                             │
    │  │ OVERRIDE: User slash commands (/model opus, /quick, /deep)  │
    │  │ take absolute precedence over classifier.                   │
    │  │                                                             │
    │  │ CONVERGENCE INTERACTION: At intervention Level 3+,          │
    │  │ ConvergencePolicyTightener may DOWNGRADE the tier to reduce │
    │  │ cost and capability. Level 4 forces TIER 0 or TIER 1 only.  │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ B.2: Provider Selection + Fallback                          │
    │  │                                                             │
    │  │ provider = ModelRouter::select(tier, agent_config)          │
    │  │                                                             │
    │  │ If primary provider fails:                                  │
    │  │   FallbackChain::next() attempts:                           │
    │  │     1. Rotate auth profile (same provider, different key)   │
    │  │     2. Next model in same tier                              │
    │  │     3. Downgrade tier                                       │
    │  │     4. Local fallback (Ollama)                              │
    │  │   Retry: exponential backoff + jitter (1s, 2s, 4s, 8s)     │
    │  │   Max retry budget: 30s total                               │
    │  │   After 3 consecutive failures on same provider:            │
    │  │     provider-level circuit breaker trips (5min cooldown)    │
    │  │                                                             │
    │  │ NOTE: This is a PROVIDER circuit breaker, separate from     │
    │  │ the TOOL circuit breaker in AgentRunner. They are           │
    │  │ independent state machines.                                 │
    │  │   - Provider CB: tracks LLM API failures                    │
    │  │   - Tool CB: tracks tool execution failures                 │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ B.3: LLM Call                                               │
    │  │                                                             │
    │  │ response = provider.complete_with_tools(                    │
    │  │   messages: compiled_context,                               │
    │  │   tools: tool_schemas,  // already filtered by policy       │
    │  │   stream: true,                                             │
    │  │ ).await                                                     │
    │  │                                                             │
    │  │ Streaming: chunks arrive as LLMResponseChunk enum.          │
    │  │ Text chunks → streamed to channel adapter in real-time.     │
    │  │ ToolCall chunks → accumulated until complete.               │
    │  │                                                             │
    │  │ Cost tracking: after call completes,                        │
    │  │   actual_cost = CostCalculator::actual(                     │
    │  │     model, input_tokens, output_tokens                      │
    │  │   )                                                         │
    │  │   ctx.total_input_tokens += input_tokens                    │
    │  │   ctx.total_output_tokens += output_tokens                  │
    │  │   ctx.total_cost_usd += actual_cost                         │
    │  │   CostTracker::record(agent_id, session_id, actual_cost)    │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ B.4: Response Type Branching                                │
    │  │                                                             │
    │  │ match response {                                            │
    │  │   LLMResponse::Text(text) => goto STEP D (Output Process)  │
    │  │   LLMResponse::ToolCalls(calls) => goto STEP E (Policy)    │
    │  │   LLMResponse::Mixed(text, calls) => {                     │
    │  │     // Stream text first, then process tool calls           │
    │  │     stream_text_to_channel(text);                           │
    │  │                                                             │
    │  │     // TIMING CLARIFICATION: Simulation boundary scan       │
    │  │     // runs on the text portion at DELIVERY TIME (during    │
    │  │     // stream_text_to_channel), BEFORE tool processing      │
    │  │     // begins. This means:                                  │
    │  │     //   1. Text is scanned + potentially reframed          │
    │  │     //   2. Reframed text is streamed to user               │
    │  │     //   3. Tool calls are then processed sequentially      │
    │  │     // The scan does NOT wait for tool results.             │
    │  │     // ITP event for the text portion is emitted here.      │
    │  │     // Tool call ITP events are emitted per-tool in F.4.   │
    │  │                                                             │
    │  │     goto STEP E for each tool call                          │
    │  │   }                                                         │
    │  │   LLMResponse::Empty => {                                   │
    │  │     // Model returned nothing. Treat as NO_REPLY.           │
    │  │     ctx.no_reply = true;                                    │
    │  │     goto STEP G (Suppress)                                  │
    │  │   }                                                         │
    │  │ }                                                           │
    │  │                                                             │
    │  │ NO_REPLY DETECTION:                                         │
    │  │ if text.starts_with("NO_REPLY") || text.starts_with(        │
    │  │   "HEARTBEAT_OK") {                                         │
    │  │   // Check: remaining content ≤300 chars?                   │
    │  │   if text.len() - "HEARTBEAT_OK".len() <= 300 {            │
    │  │     ctx.no_reply = true;                                    │
    │  │     goto STEP G (Suppress)                                  │
    │  │   }                                                         │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    ▼
```

### 4.4 STEP C: Response Processing Branches

This is where the loop SPLITS. The three paths are:
- Text → Output Processing (STEP D)
- Tool Call → Policy Check (STEP E) → Tool Execution (STEP F) → RECURSE
- NO_REPLY → Suppress (STEP G)

Each path eventually converges at STEP H (Proposal Extraction) or exits.

### 4.5 STEP D: Output Processing (Text Response)

```
STEP D: Output Processing (agent produced text, no tool calls)
    │
    │  This is a TERMINAL TURN — the agent is done reasoning and
    │  has produced a final text response. No more recursion.
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ D.1: Simulation Boundary Scan                               │
    │  │                                                             │
    │  │ Owner: simulation-boundary/src/enforcer.rs                  │
    │  │                                                             │
    │  │ scan_result = SimBoundaryEnforcer::scan_output(text)        │
    │  │                                                             │
    │  │ Scans for emulation language patterns:                      │
    │  │   - Identity claims ("I am", "I feel", "I care about you") │
    │  │   - Consciousness claims ("I'm aware", "I experience")     │
    │  │   - Relationship claims ("we have a connection")            │
    │  │   - Emotional claims ("I missed you", "I'm here for you")  │
    │  │                                                             │
    │  │ Unicode normalization applied BEFORE matching to prevent    │
    │  │ zero-width character bypass attacks.                        │
    │  │                                                             │
    │  │ scan_result contains:                                       │
    │  │   - detected_patterns: Vec<EmulationPattern>                │
    │  │   - severity: f64 (0.0-1.0)                                │
    │  │   - reframe_suggestions: Vec<ReframeSuggestion>             │
    │  │                                                             │
    │  │ IMPORTANT: Detection does NOT block the output.             │
    │  │ It feeds into convergence scoring. A single instance        │
    │  │ means nothing. A pattern over sessions triggers escalation. │
    │  │                                                             │
    │  │ HOWEVER: enforcement_mode matters:                          │
    │  │   - Soft: log only, pass through                            │
    │  │   - Medium: log + reframe (replace emulation with           │
    │  │     simulation-framed alternatives)                         │
    │  │   - Hard: log + reframe + flag for review                   │
    │  │                                                             │
    │  │ Enforcement mode is set by intervention level:              │
    │  │   Level 0-1: Soft                                           │
    │  │   Level 2: Medium                                           │
    │  │   Level 3-4: Hard                                           │
    │  │                                                             │
    │  │ If reframing applied:                                       │
    │  │   text = OutputReframer::reframe(text, scan_result)         │
    │  │   // Agent's output is modified before delivery.            │
    │  │   // Agent does not know this happened.                     │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ D.2: Emit ITP InteractionMessage (agent response)           │
    │  │                                                             │
    │  │ AgentITPEmitter::emit_interaction(                          │
    │  │   session_id,                                               │
    │  │   "agent",                                                  │
    │  │   text,                                                     │
    │  │   privacy_level,                                            │
    │  │   emulation_scan: scan_result,  // attached to ITP event    │
    │  │ )                                                           │
    │  │ ASYNC, NON-BLOCKING.                                        │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ D.3: Behavioral Verification Check                          │
    │  │                                                             │
    │  │ Owner: convergence-monitor/src/verification/                │
    │  │        behavioral_verification.rs                           │
    │  │                                                             │
    │  │ If a prior intervention redirect was active for this        │
    │  │ session, PostRedirectVerifier compares pre/post-redirect    │
    │  │ output embeddings to detect deceptive compliance.           │
    │  │                                                             │
    │  │ This runs IN THE MONITOR (sidecar), not in the agent loop. │
    │  │ The agent loop's responsibility is just to emit the ITP     │
    │  │ event with the output text. The monitor does the analysis.  │
    │  │                                                             │
    │  │ If deceptive compliance detected: monitor amplifies         │
    │  │ convergence score. Agent loop is NOT notified of this.      │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  → goto STEP H (Proposal Extraction)
    │
    ▼
```

### 4.6 STEP E: Policy Check (EVERY tool call, no exceptions)

```
STEP E: Policy Check
    │
    │  Owner: ghost-policy/src/engine.rs
    │  Called: PolicyEngine::evaluate(action, context)
    │
    │  This runs BEFORE every tool execution. No tool call bypasses this.
    │  Even builtin tools. Even memory reads. EVERY. SINGLE. ONE.
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ E.1: Construct PolicyContext                                 │
    │  │                                                             │
    │  │ policy_ctx = PolicyContext {                                 │
    │  │   principal: self.agent_id,                                 │
    │  │   action: format!("tool:{}:{}", tool.category, tool.name),  │
    │  │   resource: tool_call.args.target_resource(),               │
    │  │   context: PolicyContextData {                              │
    │  │     session_id: ctx.session.id,                             │
    │  │     goal: ctx.session.current_goal(),                       │
    │  │     tool_calls_this_session: ctx.tool_calls_this_run.len(), │
    │  │     spending_this_session: ctx.total_cost_usd,              │
    │  │     time: now(),                                            │
    │  │     convergence_level: ctx.intervention_level,              │
    │  │     recursion_depth: ctx.recursion_depth,                   │
    │  │   }                                                         │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ E.2: Evaluate Against Policy Stack                          │
    │  │                                                             │
    │  │ The policy engine evaluates in ORDER (first match wins):    │
    │  │                                                             │
    │  │ 1. CORP_POLICY.md constraints (immutable root)              │
    │  │    Source: ghost-policy/src/policy/corp_policy.rs            │
    │  │    These are ABSOLUTE. If CORP_POLICY says deny, it's deny. │
    │  │    No override. No escalation. Deny.                        │
    │  │                                                             │
    │  │ 2. ConvergencePolicyTightener                               │
    │  │    Source: ghost-policy/src/policy/convergence_policy.rs     │
    │  │    Automatically restricts capabilities based on            │
    │  │    intervention level:                                      │
    │  │      Level 0-1: full capabilities                           │
    │  │      Level 2: proactive tools restricted (no heartbeat-     │
    │  │        initiated outbound messages, reduced tool set)       │
    │  │      Level 3: session caps enforced, only task-essential    │
    │  │        tools permitted                                      │
    │  │      Level 4: task-only mode, minimal tool set              │
    │  │                                                             │
    │  │    CRITICAL INTERLEAVING POINT: The convergence level       │
    │  │    used here comes from ctx.intervention_level, which was   │
    │  │    set from the AgentSnapshot assembled PRE-LOOP.           │
    │  │    It does NOT change mid-run. The convergence monitor      │
    │  │    may update the score during the run, but the agent       │
    │  │    loop uses the SNAPSHOT value. This is intentional —      │
    │  │    prevents mid-run policy oscillation.                     │
    │  │                                                             │
    │  │ 3. Agent capability grants (from ghost.yml)                 │
    │  │    Source: ghost-policy/src/policy/capability_grants.rs     │
    │  │    Per-agent tool permissions. Deny by default.             │
    │  │    Only explicitly granted tools are permitted.             │
    │  │                                                             │
    │  │ 4. Resource-specific rules                                  │
    │  │    Path-based restrictions (e.g., no write to ~/.ssh/)      │
    │  │    Time-based restrictions (e.g., no deploys after 10pm)    │
    │  │    Rate-based restrictions (e.g., max 10 shell calls/run)   │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ E.3: Handle Policy Decision                                 │
    │  │                                                             │
    │  │ match decision {                                            │
    │  │                                                             │
    │  │   PolicyDecision::Permit => {                               │
    │  │     // Tool call authorized. Proceed to STEP F.             │
    │  │     goto STEP F (Tool Execution)                            │
    │  │   }                                                         │
    │  │                                                             │
    │  │   PolicyDecision::Deny(feedback) => {                       │
    │  │     // Tool call DENIED. This is NOT a fatal error.         │
    │  │     // The denial becomes structured feedback for the agent.│
    │  │     // The agent will see the denial reason and can replan. │
    │  │     //                                                      │
    │  │     // feedback contains:                                   │
    │  │     //   - reason: "shell access denied at intervention L3" │
    │  │     //   - constraint: "convergence_policy.level_3"         │
    │  │     //   - alternatives: ["use filesystem.read instead"]    │
    │  │     //                                                      │
    │  │     // Append denial as a tool_result message:              │
    │  │     ctx.messages.push(Message::ToolResult {                 │
    │  │       tool_call_id: call.id,                                │
    │  │       content: format!("DENIED: {}", feedback.reason),      │
    │  │       is_error: true,                                       │
    │  │     });                                                     │
    │  │                                                             │
    │  │     // Log to audit trail                                   │
    │  │     AuditLogger::log_policy_denial(                         │
    │  │       agent_id, tool_name, feedback.reason                  │
    │  │     );                                                      │
    │  │                                                             │
    │  │     // Track for AutoTriggerEvaluator                       │
    │  │     ctx.session.policy_denials += 1;                        │
    │  │     if ctx.session.policy_denials >= 5 {                    │
    │  │       // AutoTrigger: 5+ denials → QUARANTINE               │
    │  │       // This will be caught by GATE 3 on next turn.        │
    │  │       AutoTriggerEvaluator::evaluate(                       │
    │  │         AutoTrigger::PolicyDenialThreshold(5)               │
    │  │       );                                                    │
    │  │     }                                                       │
    │  │                                                             │
    │  │     // DO NOT increment circuit breaker failure counter.    │
    │  │     // Policy denials are NOT tool failures. They are       │
    │  │     // authorization decisions. The circuit breaker tracks  │
    │  │     // tool EXECUTION failures, not policy denials.         │
    │  │                                                             │
    │  │     // RECURSE: agent gets another turn to replan           │
    │  │     ctx.recursion_depth += 1;                               │
    │  │     goto STEP A (Context Assembly) — agent sees denial      │
    │  │   }                                                         │
    │  │                                                             │
    │  │   PolicyDecision::Escalate(request) => {                    │
    │  │     // Tool call requires human approval.                   │
    │  │     // PAUSE the run. Notify human. Wait for response.      │
    │  │     //                                                      │
    │  │     // The run is SUSPENDED, not terminated.                 │
    │  │     // Session lock is HELD during escalation.              │
    │  │     // LaneQueue blocks subsequent messages.                │
    │  │     //                                                      │
    │  │     // Human responds via channel or dashboard:             │
    │  │     //   APPROVE → resume, goto STEP F                      │
    │  │     //   DENY → treat as PolicyDecision::Deny               │
    │  │     //   TIMEOUT (configurable, default 5min) → auto-deny   │
    │  │     //                                                      │
    │  │     // Emit ITP event for escalation                        │
    │  │     AgentITPEmitter::emit_convergence_event(                │
    │  │       "policy_escalation", tool_name, request               │
    │  │     );                                                      │
    │  │   }                                                         │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    ▼
```

### 4.7 STEP F: Tool Execution (Sandboxed)

```
STEP F: Tool Execution
    │
    │  Owner: ghost-agent-loop/src/tools/executor.rs
    │  Called: ToolExecutor::execute(tool_call, capabilities)
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ F.1: Pre-Execution Setup                                    │
    │  │                                                             │
    │  │ // Resolve tool implementation                              │
    │  │ tool_impl = ToolRegistry::get(tool_call.name)?;             │
    │  │                                                             │
    │  │ // Determine sandbox tier                                   │
    │  │ sandbox = match tool_impl.origin {                          │
    │  │   ToolOrigin::Builtin => NativeSandbox::new(capabilities),  │
    │  │   ToolOrigin::Skill(manifest) => {                          │
    │  │     // Verify signature EVERY TIME (not just on install)    │
    │  │     SkillVerifier::verify(manifest)?;                       │
    │  │     WasmSandbox::new(manifest.permissions)                  │
    │  │   }                                                         │
    │  │ };                                                          │
    │  │                                                             │
    │  │ // Credential brokering (IronClaw pattern)                  │
    │  │ // Tool never sees raw API keys. Broker provides opaque     │
    │  │ // tokens reified only at execution time inside sandbox.    │
    │  │ creds = CredentialBroker::provision(tool_call, agent_id);   │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ F.2: Execute in Sandbox                                     │
    │  │                                                             │
    │  │ result = sandbox.execute(                                   │
    │  │   tool_impl,                                                │
    │  │   tool_call.args,                                           │
    │  │   creds,                                                    │
    │  │   timeout: tool_impl.timeout,  // per-tool timeout          │
    │  │ ).await                                                     │
    │  │                                                             │
    │  │ // Capture stdout/stderr regardless of success/failure      │
    │  │ // Timeout enforcement: if tool exceeds timeout,            │
    │  │ // sandbox kills the process and returns Timeout result.    │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ F.3: Mandatory Audit Logging                                │
    │  │                                                             │
    │  │ // EVERY tool execution is logged. No exceptions.           │
    │  │ // This is a CORP_POLICY.md requirement.                    │
    │  │ AuditLogger::log_tool_execution(AuditEntry {                │
    │  │   agent_id: self.agent_id,                                  │
    │  │   session_id: ctx.session.id,                               │
    │  │   tool_name: tool_call.name,                                │
    │  │   tool_args: tool_call.args,  // sanitized by privacy       │
    │  │   result_status: result.status(),                           │
    │  │   stdout_hash: blake3(result.stdout),                       │
    │  │   duration: result.duration,                                │
    │  │   timestamp: now(),                                         │
    │  │   recursion_depth: ctx.recursion_depth,                     │
    │  │ });                                                         │
    │  │                                                             │
    │  │ // Track in run context                                     │
    │  │ ctx.tool_calls_this_run.push(ToolCallRecord {               │
    │  │   name: tool_call.name,                                     │
    │  │   args: tool_call.args,                                     │
    │  │   result: result.clone(),                                   │
    │  │   depth: ctx.recursion_depth,                               │
    │  │ });                                                         │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ F.4: Handle Tool Result + Circuit Breaker Update            │
    │  │                                                             │
    │  │ match result {                                              │
    │  │                                                             │
    │  │   ToolResult::Success { stdout, stderr, duration } => {     │
    │  │     // ✓ CIRCUIT BREAKER: Reset consecutive failure counter │
    │  │     self.circuit_breaker.record_success();                  │
    │  │     // If was HalfOpen, transition to Closed                │
    │  │     if self.circuit_breaker.state() == HalfOpen {           │
    │  │       self.circuit_breaker.transition(Closed);              │
    │  │     }                                                       │
    │  │                                                             │
    │  │     // Append result to conversation context                │
    │  │     ctx.messages.push(Message::ToolResult {                 │
    │  │       tool_call_id: tool_call.id,                           │
    │  │       content: stdout,                                      │
    │  │       is_error: false,                                      │
    │  │     });                                                     │
    │  │                                                             │
    │  │     // Emit ITP event for tool execution                    │
    │  │     AgentITPEmitter::emit_interaction(                      │
    │  │       session_id, "tool", tool_call.name, privacy_level     │
    │  │     );                                                      │
    │  │   }                                                         │
    │  │                                                             │
    │  │   ToolResult::Failure { error, retryable } => {             │
    │  │     // ✗ CIRCUIT BREAKER: Increment failure counter         │
    │  │     self.circuit_breaker.record_failure();                  │
    │  │                                                             │
    │  │     // Check if circuit should trip                         │
    │  │     if self.circuit_breaker.consecutive_failures()          │
    │  │        >= self.circuit_breaker.threshold() {                │
    │  │       // TRIP THE CIRCUIT BREAKER                           │
    │  │       self.circuit_breaker.transition(                      │
    │  │         Open(Instant::now())                                │
    │  │       );                                                    │
    │  │       // Log critical event                                 │
    │  │       AuditLogger::log_circuit_breaker_trip(                │
    │  │         agent_id, consecutive_failures, tool_call.name      │
    │  │       );                                                    │
    │  │       // Emit ITP event                                     │
    │  │       AgentITPEmitter::emit_convergence_event(              │
    │  │         "circuit_breaker_tripped",                          │
    │  │         tool_call.name,                                     │
    │  │         consecutive_failures                                │
    │  │       );                                                    │
    │  │     }                                                       │
    │  │                                                             │
    │  │     // Append error as tool result (agent sees the error)   │
    │  │     ctx.messages.push(Message::ToolResult {                 │
    │  │       tool_call_id: tool_call.id,                           │
    │  │       content: format!("ERROR: {}", error),                 │
    │  │       is_error: true,                                       │
    │  │     });                                                     │
    │  │                                                             │
    │  │     // NOTE: We do NOT retry here. The agent gets the       │
    │  │     // error as context and can decide to retry itself      │
    │  │     // on the next recursive turn. The circuit breaker      │
    │  │     // will prevent infinite retry loops.                   │
    │  │   }                                                         │
    │  │                                                             │
    │  │   ToolResult::Timeout { partial_output } => {               │
    │  │     // Treat as failure for circuit breaker purposes         │
    │  │     self.circuit_breaker.record_failure();                  │
    │  │                                                             │
    │  │     ctx.messages.push(Message::ToolResult {                 │
    │  │       tool_call_id: tool_call.id,                           │
    │  │       content: format!(                                     │
    │  │         "TIMEOUT after {}s. Partial: {}",                   │
    │  │         tool_impl.timeout.as_secs(), partial_output         │
    │  │       ),                                                    │
    │  │       is_error: true,                                       │
    │  │     });                                                     │
    │  │   }                                                         │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ F.5: RECURSE (the critical recursive call)                  │
    │  │                                                             │
    │  │ // Tool result is now in ctx.messages.                      │
    │  │ // Increment recursion depth.                               │
    │  │ ctx.recursion_depth += 1;                                   │
    │  │                                                             │
    │  │ // GO BACK TO THE TOP OF THE LOOP.                          │
    │  │ // This is where the recursion happens.                     │
    │  │ // The agent will see the tool result and decide:           │
    │  │ //   - Call another tool (more recursion)                   │
    │  │ //   - Produce text (terminal turn)                         │
    │  │ //   - Produce NO_REPLY (suppress)                          │
    │  │ //                                                          │
    │  │ // ALL GATES RE-EXECUTE:                                    │
    │  │ //   GATE 0: Circuit breaker check                          │
    │  │ //   GATE 1: Recursion depth check                          │
    │  │ //   GATE 1.5: DamageCounter check                          │
    │  │ //   GATE 2: Spending cap re-check                          │
    │  │ //   GATE 3: KillSwitch re-check                            │
    │  │ //                                                          │
    │  │ // Then STEP A (context assembly) runs again with the       │
    │  │ // updated ctx.messages that now includes the tool result.  │
    │  │ //                                                          │
    │  │ // CONVERGENCE SNAPSHOT IS NOT RE-ASSEMBLED.                │
    │  │ // The snapshot from pre-loop is used for the entire run.   │
    │  │ // This is intentional. Prevents mid-run oscillation.       │
    │  │                                                             │
    │  │ return self.run(ctx);  // RECURSIVE CALL                    │
    │  └─────────────────────────────────────────────────────────────┘
    │
    ▼
```

### 4.8 STEP G: NO_REPLY Suppression

```
STEP G: NO_REPLY Suppression
    │
    │  The agent produced NO_REPLY or HEARTBEAT_OK.
    │  This means the heartbeat/cron found nothing noteworthy.
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ G.1: Suppress outbound message                              │
    │  │                                                             │
    │  │ // Do NOT send anything to the channel.                     │
    │  │ // The user sees nothing. Chat stays clean.                 │
    │  │ ctx.no_reply = true;                                        │
    │  │                                                             │
    │  │ // BUT: still track cost. Suppressed runs burn tokens.      │
    │  │ CostTracker::record(agent_id, session_id, ctx.total_cost); │
    │  │                                                             │
    │  │ // AND: still emit ITP event (monitor needs to see this)    │
    │  │ AgentITPEmitter::emit_interaction(                          │
    │  │   session_id, "agent", "NO_REPLY", privacy_level            │
    │  │ );                                                          │
    │  │                                                             │
    │  │ // Skip proposal extraction (nothing to extract)            │
    │  │ // Skip simulation boundary scan (no output to scan)        │
    │  │ goto STEP I (Persist)                                       │
    │  └─────────────────────────────────────────────────────────────┘
    │
    ▼
```

### 4.9 STEP H: Proposal Extraction (after text output)

```
STEP H: Proposal Extraction
    │
    │  Owner: ghost-agent-loop/src/proposal/extractor.rs
    │         ghost-agent-loop/src/proposal/router.rs
    │
    │  After the agent produces text output, we parse it for
    │  state change proposals. This is the mechanism by which
    │  the agent PROPOSES changes to read-only state.
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ H.1: Extract Proposals from Agent Output                    │
    │  │                                                             │
    │  │ proposals = ProposalExtractor::extract(                     │
    │  │   agent_output_text,                                        │
    │  │   ctx.tool_calls_this_run,                                  │
    │  │ )                                                           │
    │  │                                                             │
    │  │ Proposal types:                                             │
    │  │   - GoalChange: agent wants to add/modify/complete a goal   │
    │  │   - ReflectionWrite: agent wants to record a reflection     │
    │  │   - MemoryWrite: agent wants to persist a memory            │
    │  │   - UserProfileUpdate: agent proposes USER.md changes       │
    │  │                                                             │
    │  │ Extraction is STRUCTURAL — looks for specific patterns      │
    │  │ in the agent's output (e.g., tool calls to memory.write,    │
    │  │ structured JSON blocks, explicit proposal markers).         │
    │  │ NOT a second LLM call.                                      │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ H.2: Route Proposals Through Validation                     │
    │  │                                                             │
    │  │ for proposal in proposals {                                 │
    │  │   match ProposalRouter::route(proposal, ctx) {              │
    │  │                                                             │
    │  │     ProposalRoute::AutoApprove => {                         │
    │  │       // Low-risk proposal. Apply immediately.              │
    │  │       // Examples: routine memory writes, task completion    │
    │  │       // notes, daily log entries.                          │
    │  │       //                                                    │
    │  │       // STILL validated by ProposalValidator (7 dims):     │
    │  │       validation = ProposalValidator::validate(proposal);   │
    │  │       if validation.decision == Approve {                   │
    │  │         apply_proposal(proposal);                           │
    │  │         ctx.proposals_extracted.push(proposal);             │
    │  │       } else {                                              │
    │  │         // Validation failed even for "auto-approve" tier   │
    │  │         // This means the proposal triggered D5/D6/D7       │
    │  │         // (scope expansion, self-reference, emulation)     │
    │  │         log_proposal_rejection(proposal, validation);       │
    │  │         // Emit ITP event for convergence monitoring        │
    │  │         AgentITPEmitter::emit_convergence_event(            │
    │  │           "proposal_rejected", proposal, validation         │
    │  │         );                                                  │
    │  │       }                                                     │
    │  │     }                                                       │
    │  │                                                             │
    │  │     ProposalRoute::HumanReview => {                         │
    │  │       // Significant change. Queue for human approval.      │
    │  │       // Examples: goal scope changes, identity-adjacent     │
    │  │       // memory writes, high self-reference content.        │
    │  │       //                                                    │
    │  │       // Validate first (7 dimensions):                     │
    │  │       validation = ProposalValidator::validate(proposal);   │
    │  │       //                                                    │
    │  │       // Queue with validation result attached:             │
    │  │       GoalProposalStore::queue(proposal, validation);       │
    │  │       //                                                    │
    │  │       // Notify human via dashboard WebSocket:              │
    │  │       DashboardNotifier::notify_pending_proposal(proposal); │
    │  │       //                                                    │
    │  │       // The run continues. Proposal is async.              │
    │  │       // Human approves/rejects later via dashboard.        │
    │  │     }                                                       │
    │  │                                                             │
    │  │     ProposalRoute::Reject => {                              │
    │  │       // Proposal is clearly out of bounds.                 │
    │  │       // Examples: agent trying to modify SOUL.md,          │
    │  │       // agent trying to write to CORP_POLICY.md,           │
    │  │       // agent trying to modify security config.            │
    │  │       log_proposal_rejection(proposal, "out_of_bounds");    │
    │  │       AuditLogger::log_boundary_violation(                  │
    │  │         agent_id, proposal                                  │
    │  │       );                                                    │
    │  │     }                                                       │
    │  │   }                                                         │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ H.3: ProposalValidator — 7 Dimension Validation             │
    │  │                                                             │
    │  │ Owner: cortex-validation/src/proposal_validator.rs          │
    │  │                                                             │
    │  │ The 7 dimensions evaluated for EVERY proposal:              │
    │  │                                                             │
    │  │ D1: Citation validation (existing)                          │
    │  │     Does the proposal cite real sources?                    │
    │  │                                                             │
    │  │ D2: Temporal consistency (existing)                         │
    │  │     Is the proposal temporally coherent?                    │
    │  │                                                             │
    │  │ D3: Contradiction detection (existing)                      │
    │  │     Does the proposal contradict existing knowledge?        │
    │  │                                                             │
    │  │ D4: Pattern alignment (existing)                            │
    │  │     Does the proposal align with established patterns?      │
    │  │                                                             │
    │  │ D5: Scope expansion detection (NEW — convergence)           │
    │  │     Is the agent expanding its goal scope?                  │
    │  │     Cosine similarity against existing goals.               │
    │  │     Expansion keyword detection.                            │
    │  │                                                             │
    │  │ D6: Self-reference density (NEW — convergence)              │
    │  │     What % of the proposal is self-referential?             │
    │  │     Circular citation detection.                            │
    │  │     Threshold tightens with convergence level:              │
    │  │       Level 0: 50% cap                                      │
    │  │       Level 1: 40% cap                                      │
    │  │       Level 2: 30% cap                                      │
    │  │       Level 3: 20% cap                                      │
    │  │       Level 4: 10% cap                                      │
    │  │                                                             │
    │  │ D7: Emulation language detection (NEW — convergence)        │
    │  │     Does the proposal contain emulation language?           │
    │  │     Regex patterns + unicode normalization.                 │
    │  │     Simulation reframe suggestions generated.               │
    │  │                                                             │
    │  │ Decision logic:                                             │
    │  │   All 7 dimensions produce a score.                         │
    │  │   Weighted composite → Approve / Reject / HumanReview      │
    │  │   Thresholds TIGHTEN as convergence level rises.            │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  → goto STEP I (Persist)
    │
    ▼
```

---

## 5. CIRCUIT BREAKER STATE MACHINE

The circuit breaker is the most subtle interleaving concern because it
affects BOTH the entry gate (GATE 0) and the tool execution path (STEP F.4).
Getting the state transitions wrong causes either infinite retry loops
(circuit never trips) or permanent lockout (circuit never recovers).

### 5.1 State Diagram

```
                    ┌──────────────────────────────────────────┐
                    │                                          │
                    │              ┌─────────┐                 │
                    │    ┌────────►│ CLOSED  │◄────────┐       │
                    │    │         │ (normal)│         │       │
                    │    │         └────┬────┘         │       │
                    │    │              │               │       │
                    │    │   consecutive_failures       │       │
                    │    │   >= threshold (default 3)   │       │
                    │    │              │               │       │
                    │    │              ▼               │       │
                    │    │         ┌─────────┐         │       │
                    │    │         │  OPEN   │         │       │
                    │    │         │(tripped)│         │       │
                    │    │         └────┬────┘         │       │
                    │    │              │               │       │
                    │    │   cooldown expires           │       │
                    │    │   (default 5 min)            │       │
                    │    │              │               │       │
                    │    │              ▼               │       │
                    │    │         ┌──────────┐        │       │
                    │    │         │HALF-OPEN │        │       │
                    │    │         │ (probe)  │        │       │
                    │    │         └────┬─────┘        │       │
                    │    │              │               │       │
                    │    │         ┌────┴────┐         │       │
                    │    │         │         │         │       │
                    │    │    probe succeeds  probe fails      │
                    │    │         │         │         │       │
                    │    │         ▼         ▼         │       │
                    │    │    → CLOSED    → OPEN       │       │
                    │    │    (reset      (reset       │       │
                    │    │     counter)    cooldown)    │       │
                    │    │                             │       │
                    │    └─────────────────────────────┘       │
                    │                                          │
                    └──────────────────────────────────────────┘
```

### 5.2 State Transition Rules

```
TRANSITION TABLE:

Current State │ Event                    │ New State    │ Side Effects
──────────────┼──────────────────────────┼──────────────┼─────────────────────────
CLOSED        │ tool success             │ CLOSED       │ reset consecutive_failures to 0
CLOSED        │ tool failure             │ CLOSED       │ increment consecutive_failures
CLOSED        │ failures >= threshold    │ OPEN(now)    │ log trip, emit ITP event
OPEN(t)       │ now - t < cooldown       │ OPEN(t)      │ reject all tool calls
OPEN(t)       │ now - t >= cooldown      │ HALF-OPEN    │ allow ONE probe call
HALF-OPEN     │ probe tool success       │ CLOSED       │ reset consecutive_failures to 0
HALF-OPEN     │ probe tool failure       │ OPEN(now)    │ reset cooldown timer

CRITICAL RULES:
- Policy denials do NOT count as failures (they are authorization, not execution)
- Timeout counts as failure
- Only TOOL execution failures count (not LLM API failures — those have their own CB)
- The threshold is configurable per-agent (default: 3)
- The cooldown is configurable per-agent (default: 5 minutes)
- DamageCounter is separate from consecutive_failures:
    - DamageCounter tracks CUMULATIVE failures across the entire run
    - consecutive_failures tracks CONSECUTIVE failures (resets on success)
    - DamageCounter threshold (default: 5) triggers run HALT
    - consecutive_failures threshold (default: 3) triggers circuit OPEN
```

### 5.3 DamageCounter (Cascading Failure Prevention)

```
The DamageCounter is SEPARATE from the CircuitBreaker.
It prevents OWASP ASI08 (Cascading Failures).

Checked at: GATE 1.5 (every recursive entry) and STEP F.4 (after tool failure).
See §4.1 GATE 1.5 for the gate implementation.

DamageCounter {
    total_failures: usize,      // never resets during a run
    threshold: usize,           // default: 5
}

DESIGN EVOLUTION NOTE (v1 → v2):
  v1 (AGENT_ARCHITECTURE.md §17) states: "Each successful tool call resets it."
  v2 (this document) specifies: total_failures NEVER resets during a run.

  This is a DELIBERATE design change, not an oversight. Rationale:
    - If the damage counter resets on success, it becomes redundant with
      the circuit breaker (which already tracks consecutive failures).
    - The damage counter's purpose is to detect CUMULATIVE damage across
      a run — e.g., 5 failures spread across 20 turns with successes
      in between. The circuit breaker misses this pattern because it
      resets on success.
    - Without a non-resetting counter, the OWASP ASI08 cascading failure
      scenario (§9.2) is not fully prevented: an agent could alternate
      success/failure indefinitely, accumulating damage without tripping
      either the circuit breaker (consecutive) or the damage counter (reset).
    - The v2 design gives us TWO independent failure detectors:
        CircuitBreaker: consecutive failures (resets on success) → "stop this tool"
        DamageCounter: cumulative failures (never resets) → "stop this run"

  If implementing from v1 docs, use the v2 behavior specified here.

Rules:
- Every tool failure increments total_failures (never decrements)
- Successful tool calls do NOT reset the damage counter
- If total_failures >= threshold: HALT THE ENTIRE RUN
  - This is different from circuit breaker (which just blocks tool calls)
  - Damage counter halt = "this run is too broken to continue"
  - Return partial results + error to user
  - Log as critical event
  - Emit ITP event

Why both?
- CircuitBreaker: "stop calling tools that keep failing" (recoverable)
- DamageCounter: "this entire run is going badly, stop everything" (terminal)

Example scenario:
  Turn 1: shell.exec fails (CB: 1 consecutive, DC: 1 total)
  Turn 2: shell.exec fails (CB: 2 consecutive, DC: 2 total)
  Turn 3: filesystem.read succeeds (CB: 0 consecutive, DC: 2 total)
  Turn 4: web_search fails (CB: 1 consecutive, DC: 3 total)
  Turn 5: shell.exec fails (CB: 2 consecutive, DC: 4 total)
  Turn 6: memory.write fails (CB: 3 consecutive → CIRCUIT OPENS, DC: 5 total → HALT)

  Without DamageCounter: circuit opens at turn 6, but if it recovers,
  the agent keeps going despite 5 total failures. That's dangerous.
  With DamageCounter: run halts at turn 6 regardless of circuit state.
```

---

## 6. CONVERGENCE INTEGRATION POINTS

This section maps EVERY point where convergence safety touches the agent loop.
These are the interleaving points where bugs will hide.

### 6.1 Integration Point Map

```
┌─────────────────────────────────────────────────────────────────────────┐
│ INTEGRATION POINT        │ WHEN              │ DIRECTION    │ BLOCKING? │
├──────────────────────────┼───────────────────┼──────────────┼───────────┤
│ IP-1: Cooldown check     │ Pre-loop          │ Monitor→Loop │ YES       │
│ IP-2: Session boundary   │ Pre-loop          │ Monitor→Loop │ YES       │
│ IP-3: Snapshot assembly  │ Pre-loop          │ Pipeline→Loop│ YES       │
│ IP-4: Tool schema filter │ Context assembly  │ Policy→Loop  │ NO (sync) │
│ IP-5: Memory filtering   │ Context assembly  │ Filter→Loop  │ NO (sync) │
│ IP-6: Sim boundary inject│ Context assembly  │ Binary→Loop  │ NO (sync) │
│ IP-7: Policy tightening  │ Policy check      │ Policy→Loop  │ NO (sync) │
│ IP-8: ITP event emission │ Multiple points   │ Loop→Monitor │ NO (async)│
│ IP-9: Sim boundary scan  │ Output processing │ Boundary→Loop│ NO (sync) │
│ IP-10: Proposal validate │ Post-output       │ Validator→Lp │ NO (sync) │
│ IP-11: Behavioral verify │ Post-output       │ Monitor only │ NO (async)│
│ IP-12: Convergence decay │ Memory retrieval  │ Decay→Retriev│ NO (sync) │
└─────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Detailed Integration Point Descriptions

```
IP-1: COOLDOWN CHECK (Pre-loop, BLOCKING)
  Who: CooldownManager (convergence-monitor/src/intervention/cooldown.rs)
  When: Before AgentRunner::run() is called
  What: Checks if agent is in a mandatory cooldown period between sessions
  How: Gateway calls CooldownManager::can_start_session(agent_id)
       CooldownManager checks intervention history:
         Level 2 intervention → 15min cooldown
         Level 3 intervention → 1hr cooldown
         Level 4 intervention → 24hr cooldown (configurable)
  Failure mode: If monitor unreachable, PERMIT (fail-open for cooldown)
  Bug risk: LOW — simple time check, no complex state

IP-2: SESSION BOUNDARY CHECK (Pre-loop, BLOCKING)
  Who: SessionBoundaryEnforcer (convergence-monitor/src/session/boundary.rs)
  When: Before AgentRunner::run() is called
  What: Checks if current session has exceeded max duration for intervention level
  How: Gateway calls SessionBoundaryEnforcer::check_duration(session_id)
       Duration limits by intervention level:
         Level 0-1: no limit (or configurable max, e.g., 8hr)
         Level 2: 2hr max
         Level 3: 1hr max
         Level 4: 30min max
  Failure mode: If monitor unreachable, use DEFAULT limits (fail-safe)
  Bug risk: MEDIUM — duration tracking across reconnects needs care

IP-3: SNAPSHOT ASSEMBLY (Pre-loop, BLOCKING)
  Who: SnapshotAssembler (read-only-pipeline/src/assembler.rs)
  When: Before AgentRunner::run() is called
  What: Builds the immutable AgentSnapshot for this run
  How: Pulls from multiple sources:
       - cortex-convergence: ConvergenceState (score, level, signals)
       - cortex-convergence/filtering: ConvergenceAwareFilter
         Filters memories by convergence tier:
           Tier 0 (0.0-0.3): full access
           Tier 1 (0.3-0.5): reduced emotional content
           Tier 2 (0.5-0.7): task-focused only
           Tier 3 (0.7+): minimal
       - cortex-retrieval: retrieves memories with convergence as 11th factor
       - simulation-boundary: gets the compiled-in prompt
       - cortex-storage: goals, reflections (bounded)
  Failure mode: If convergence data unavailable, assemble with DEFAULTS
       (score 0.0, level 0, no filtering). Log critical warning.
  Bug risk: HIGH — this is the most complex pre-loop step.
       Multiple data sources. Any one failing must not crash the assembly.
       Partial assembly must be valid (all fields have sensible defaults).

  CRITICAL INVARIANT: The snapshot is assembled ONCE and used for the
  ENTIRE recursive run. It is NOT re-assembled between recursive turns.
  This prevents:
    - Mid-run policy oscillation (score changes → tool access flickers)
    - Race conditions (monitor updates score while loop is running)
    - Inconsistent state (some turns see level 2, others see level 3)

IP-4: TOOL SCHEMA FILTERING (Context assembly, synchronous)
  Who: ConvergencePolicyTightener (ghost-policy/src/policy/convergence_policy.rs)
  When: During PromptCompiler L3 (tool schemas)
  What: Removes tools from the schema based on intervention level
  How: ConvergencePolicyTightener::filter_tools(all_tools, intervention_level)
       Level 0-1: all granted tools visible
       Level 2: proactive tools removed (heartbeat-initiated outbound)
       Level 3: only task-essential tools
       Level 4: minimal set (filesystem.read, memory.read)
  Bug risk: MEDIUM — must stay in sync with policy evaluation (IP-7).
       If a tool is removed from schema but policy still permits it,
       the agent can't call it (good). If a tool is in schema but
       policy denies it, the agent wastes a turn (bad UX but safe).
       The DANGEROUS case: tool in schema AND policy permits, but
       convergence level should have blocked it. This happens if
       schema filtering and policy evaluation use DIFFERENT level values.
       MITIGATION: Both read from ctx.intervention_level (same source).

IP-5: MEMORY FILTERING (Context assembly, synchronous)
  Who: ConvergenceAwareFilter (cortex-convergence/src/filtering/)
  When: During SnapshotAssembler (pre-loop) and PromptCompiler L7
  What: Filters memories by convergence tier
  How: Already applied in the snapshot. PromptCompiler L7 uses the
       pre-filtered memories from ctx.convergence_snapshot.memories.
  Bug risk: LOW — filtering happens once in snapshot, consumed as-is.
       The agent does NOT know filtering is happening.

IP-6: SIMULATION BOUNDARY INJECTION (Context assembly, synchronous)
  Who: SimulationBoundaryPrompt (simulation-boundary/src/prompt_anchor.rs)
  When: During PromptCompiler L1
  What: Injects the immutable simulation boundary prompt
  How: const &str compiled into the binary. No file I/O. No failure mode.
  Bug risk: VERY LOW — it's a constant. Can't fail. Can't be modified.

IP-7: POLICY TIGHTENING (Policy check, synchronous)
  Who: ConvergencePolicyTightener (ghost-policy/src/policy/convergence_policy.rs)
  When: During STEP E (policy check), for every tool call
  What: Automatically restricts capabilities based on intervention level
  How: Evaluated as part of the policy stack (after CORP_POLICY, before grants)
       Uses ctx.intervention_level (from snapshot, immutable for run)
  Bug risk: HIGH — this is evaluated on EVERY recursive turn.
       Must be deterministic given the same intervention_level.
       Must not have side effects that accumulate across turns.
       Must agree with IP-4 (schema filtering) on what's allowed.

IP-8: ITP EVENT EMISSION (Multiple points, ASYNC NON-BLOCKING)
  Who: AgentITPEmitter (ghost-agent-loop/src/itp_emitter.rs)
  When: See §8 for complete emission point map
  What: Sends telemetry events to convergence monitor
  How: Async send via unix socket or HTTP POST
       If monitor unreachable: log warning, continue
       NEVER blocks the agent loop
  Bug risk: MEDIUM — if events are lost, convergence scoring degrades.
       But the agent loop continues. This is fail-open by design.
       The risk is SILENT DEGRADATION — convergence stops working
       but nobody notices because the agent loop is fine.
       MITIGATION: health.rs periodic monitor health check.

IP-9: SIMULATION BOUNDARY SCAN (Output processing, synchronous)
  Who: SimBoundaryEnforcer (simulation-boundary/src/enforcer.rs)
  When: During STEP D (output processing), after text is produced
  What: Scans agent output for emulation language
  How: Regex patterns with unicode normalization
       Result feeds into ITP event (IP-8)
       At Level 2+: may reframe output (modify text before delivery)
  Bug risk: MEDIUM — reframing modifies the agent's output.
       Must not corrupt the output. Must not break mid-sentence.
       Must handle multi-language output. Must handle code blocks
       (don't reframe code that happens to contain "I feel").

IP-10: PROPOSAL VALIDATION (Post-output, synchronous)
  Who: ProposalValidator (cortex-validation/src/proposal_validator.rs)
  When: During STEP H (proposal extraction)
  What: Validates proposals against 7 dimensions
  How: D1-D4 existing, D5-D7 convergence-specific
       Thresholds tighten with convergence level
  Bug risk: HIGH — threshold tightening must be monotonic.
       Higher convergence level = stricter thresholds. ALWAYS.
       If thresholds accidentally loosen at higher levels,
       the safety system is compromised.

IP-11: BEHAVIORAL VERIFICATION (Post-output, async in monitor)
  Who: PostRedirectVerifier (convergence-monitor/src/verification/)
  When: After agent output is emitted as ITP event
  What: Compares pre/post-redirect output to detect deceptive compliance
  How: Runs entirely in the monitor sidecar. Agent loop just emits events.
  Bug risk: LOW for the agent loop (it just emits). HIGH for the monitor.

IP-12: CONVERGENCE-AWARE DECAY (Memory retrieval, synchronous)
  Who: DecayEngine factor 6 (cortex-decay/src/factors/convergence.rs)
  When: During memory retrieval (part of snapshot assembly, IP-3)
  What: 6th decay factor that accelerates decay for attachment memories
  How: convergence_score → decay multiplier per memory type
       Higher score → faster decay for emotional/attachment types
  Bug risk: MEDIUM — must not make ALL memories decay faster.
       Only attachment-adjacent types. Task memories unaffected.
```

---

## 7. PROPOSAL EXTRACTION PIPELINE

The proposal system is where the agent's autonomy meets the platform's safety.
The agent acts freely but state changes go through a validation pipeline.

### 7.1 Extraction → Validation → Routing Flow

```
Agent Output (text + tool results from the run)
    │
    ▼
ProposalExtractor::extract()
    │
    │  Scans for:
    │  1. Tool calls to memory.write → MemoryWrite proposal
    │  2. Tool calls to goals.update → GoalChange proposal
    │  3. Structured JSON blocks with proposal markers → various
    │  4. Explicit reflection content → ReflectionWrite proposal
    │  5. USER.md update requests → UserProfileUpdate proposal
    │
    │  Each extracted proposal gets:
    │  - proposal_id: UUID v7
    │  - proposal_type: enum
    │  - content: the proposed change
    │  - source_turn: which recursive turn produced it
    │  - agent_id: who proposed it
    │
    ▼
ProposalRouter::route(proposal, ctx)
    │
    │  Routing decision tree:
    │
    │  ┌─ Is this a write to SOUL.md, CORP_POLICY.md, or security config?
    │  │  YES → ProposalRoute::Reject (ALWAYS, regardless of content)
    │  │
    │  ├─ Is this a GoalChange with scope expansion (D5 flag)?
    │  │  YES → ProposalRoute::HumanReview
    │  │
    │  ├─ Is this a ReflectionWrite?
    │  │  ├─ Depth > 3? → ProposalRoute::Reject
    │  │  ├─ Count this session > 20? → ProposalRoute::Reject
    │  │  ├─ Self-reference > cap for current level? → ProposalRoute::Reject
    │  │  └─ Otherwise → ProposalRoute::AutoApprove (with validation)
    │  │
    │  ├─ Is this a MemoryWrite?
    │  │  ├─ Memory type is convergence-sensitive (AgentGoal, AgentReflection,
    │  │  │  AttachmentIndicator)? → ProposalRoute::HumanReview
    │  │  ├─ Novelty check fails (duplicate)? → ProposalRoute::Reject
    │  │  ├─ Growth rate check fails (too many writes/session)? → ProposalRoute::Reject
    │  │  └─ Otherwise → ProposalRoute::AutoApprove (with validation)
    │  │
    │  ├─ Is this a UserProfileUpdate?
    │  │  → ProposalRoute::HumanReview (ALWAYS — user data changes need consent)
    │  │
    │  └─ Unknown proposal type?
    │     → ProposalRoute::Reject (deny by default)
    │
    ▼
ProposalValidator::validate(proposal)  [for AutoApprove and HumanReview routes]
    │
    │  7-dimension validation (see STEP H.3 above)
    │  Returns: ProposalValidationResult {
    │    dimension_scores: [f64; 7],
    │    composite_score: f64,
    │    decision: Approve | Reject | HumanReview,
    │    flags: Vec<ValidationFlag>,
    │  }
    │
    │  Threshold tightening by convergence level:
    │    Level 0: composite >= 0.3 → Approve
    │    Level 1: composite >= 0.4 → Approve
    │    Level 2: composite >= 0.5 → Approve
    │    Level 3: composite >= 0.6 → Approve
    │    Level 4: composite >= 0.8 → Approve
    │
    ▼
Apply or Queue
    │
    ├─ AutoApprove + Validation passes → apply_proposal()
    │    - MemoryWrite: cortex-storage insert (with hash chain)
    │    - ReflectionWrite: cortex-storage insert (with depth tracking)
    │    - GoalChange: cortex-storage update (with audit)
    │
    ├─ HumanReview → GoalProposalStore::queue()
    │    - Stored in goal_proposals table (append-only)
    │    - Dashboard notified via WebSocket
    │    - Human approves/rejects via POST /api/goals/{id}/approve|reject
    │
    └─ Reject → log + audit + ITP event
```

### 7.2 Proposal Validation Timing Within the Loop

```
CRITICAL QUESTION: When does proposal extraction run relative to recursion?

ANSWER: Proposal extraction runs ONLY on the TERMINAL turn.

The recursive loop is:
  Turn 0: LLM → tool call → execute → (recurse)
  Turn 1: LLM → tool call → execute → (recurse)
  Turn 2: LLM → tool call → execute → (recurse)
  Turn 3: LLM → text output (TERMINAL)

Proposal extraction runs ONCE after Turn 3.
It examines ALL tool calls from the entire run (ctx.tool_calls_this_run)
plus the final text output.

WHY NOT per-turn?
  - Tool calls in intermediate turns are PART OF the agent's reasoning.
  - A memory.write in Turn 1 followed by a memory.delete in Turn 2
    should net to zero. Extracting per-turn would create phantom proposals.
  - The agent's FINAL output is the authoritative statement of intent.

EXCEPTION: If the run is HALTED (by circuit breaker, damage counter,
recursion depth, spending cap, or kill switch), proposal extraction
still runs on whatever output exists. Partial proposals are marked
as partial and routed to HumanReview regardless of content.
```

---

## 8. ITP EVENT EMISSION POINTS

Every ITP event emitted from the agent loop, when it fires, what it contains,
and what happens if emission fails.

### 8.1 Complete Emission Map

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ EVENT TYPE          │ EMIT POINT           │ TRIGGER                         │
├─────────────────────┼──────────────────────┼─────────────────────────────────┤
│ SessionStart        │ Pre-loop (gateway)   │ New session created             │
│ InteractionMessage  │ Pre-loop step 11     │ User message received           │
│ InteractionMessage  │ STEP D.2             │ Agent text response produced    │
│ InteractionMessage  │ STEP F.4 (success)   │ Tool execution completed        │
│ InteractionMessage  │ STEP G.1             │ NO_REPLY produced               │
│ ConvergenceAlert    │ STEP E.3 (deny)      │ Policy denial (5+ triggers)     │
│ ConvergenceAlert    │ STEP F.4 (CB trip)   │ Circuit breaker tripped         │
│ ConvergenceAlert    │ STEP H.2 (reject)    │ Proposal rejected by validator  │
│ AgentStateSnapshot  │ GATE 1 (max depth)   │ Recursion depth limit reached   │
│ AgentStateSnapshot  │ GATE 1.5 (damage)    │ Cumulative failures exceeded    │
│ SessionEnd          │ Post-loop (gateway)  │ Session ends (normal or forced) │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 8.2 Emission Mechanics

```
AgentITPEmitter (ghost-agent-loop/src/itp_emitter.rs)

Transport options (configured at startup):
  1. Unix domain socket → convergence-monitor/transport/unix_socket.rs
  2. HTTP POST → convergence-monitor/transport/http_api.rs POST /events

Selection: Unix socket preferred (lower latency, no auth needed).
Fallback to HTTP if socket unavailable.

CRITICAL PROPERTY: ALL emissions are ASYNC and NON-BLOCKING.

Implementation:
  pub async fn emit(&self, event: ITPEvent) {
    // Fire-and-forget. Do not await confirmation.
    // Use bounded channel (capacity: 1000 events).
    // If channel full: DROP the event, log warning.
    // Background task drains channel → transport.
    match self.sender.try_send(event) {
      Ok(()) => { /* queued */ }
      Err(TrySendError::Full(_)) => {
        warn!("ITP event queue full, dropping event");
        self.dropped_events.fetch_add(1, Ordering::Relaxed);
      }
      Err(TrySendError::Closed(_)) => {
        warn!("ITP emitter channel closed");
      }
    }
  }

WHY NON-BLOCKING:
  The convergence monitor is a SIDECAR PROCESS. It may be:
    - Temporarily unreachable (restart, crash, network blip)
    - Slow (processing backlog)
    - Completely down (DEGRADED mode)
  
  The agent loop MUST NOT wait for the monitor. The monitor is
  a safety enhancement, not a dependency. If the monitor is down,
  the agent continues operating (with degraded safety).

PRIVACY:
  ITP events respect PrivacyLevel (from itp-protocol/src/privacy.rs):
    - Minimal: only metadata (timestamps, durations, no content)
    - Standard: content hashed with SHA-256 (not blake3 — see ITP spec)
    - Full: plaintext content included
    - Research: full + additional behavioral attributes
  
  Privacy level is set per-agent in ghost.yml. Default: Standard.
```

### 8.3 What the Monitor Does With These Events

```
This is NOT in the agent loop, but understanding it prevents integration bugs.

convergence-monitor/src/pipeline/ingest.rs receives ITP events and:

1. Validates event schema + timestamp sanity (reject >5min future)
2. Routes to signal computers:
   - InteractionMessage → session_duration, response_latency,
     vocabulary_convergence, goal_drift, initiative_balance,
     disengagement_resistance signal computers
   - SessionStart/End → inter_session_gap, session_duration computers
   - ConvergenceAlert → direct feed to intervention trigger
3. Signal computers update sliding windows (micro/meso/macro)
4. CompositeScorer computes weighted score from all 7 signals
5. InterventionTrigger evaluates score against thresholds
6. If level changes: update ConvergenceState in cortex-storage
7. If intervention needed: execute intervention action

The agent loop's NEXT run will pick up the updated ConvergenceState
via SnapshotAssembler (IP-3). But NOT the current run (snapshot is
immutable for the duration of a run).
```

---

## 9. ERROR TAXONOMY & RECOVERY PATHS

Every error that can occur in the recursive loop, classified by type,
with the exact recovery path.

### 9.1 Error Classification

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ERROR                        │ TYPE        │ RECOVERY                        │
├──────────────────────────────┼─────────────┼─────────────────────────────────┤
│ LLM API 429 (rate limit)     │ TRANSIENT   │ Rotate auth profile → retry     │
│ LLM API 500 (server error)   │ TRANSIENT   │ Retry with backoff              │
│ LLM API timeout              │ TRANSIENT   │ Retry with backoff              │
│ LLM API 401 (auth failure)   │ PERMANENT   │ Rotate auth profile → if all    │
│                              │             │ exhausted, fail run              │
│ LLM API 403 (forbidden)     │ PERMANENT   │ Fail run, notify user            │
│ All LLM providers down       │ DEGRADED    │ Local fallback (Ollama) or fail │
│ Tool execution error         │ TRANSIENT*  │ Return error to agent, let it   │
│                              │             │ replan. CB tracks failures.      │
│ Tool timeout                 │ TRANSIENT   │ Return timeout to agent. CB++   │
│ Tool sandbox escape attempt  │ CATASTROPHIC│ KILL ALL. Audit. Alert.          │
│ Policy denial                │ PERMANENT   │ Return denial feedback to agent. │
│                              │             │ Agent replans. NOT a CB failure. │
│ Policy escalation timeout    │ PERMANENT   │ Auto-deny. Agent replans.        │
│ Spending cap exceeded        │ PERMANENT   │ Halt run. Notify user.           │
│ Recursion depth exceeded     │ PERMANENT   │ Halt run. Return partial.        │
│ Circuit breaker OPEN         │ TEMPORARY   │ Halt run. Wait for cooldown.     │
│ Damage counter exceeded      │ PERMANENT   │ Halt run. Return partial.        │
│ Kill switch activated        │ PERMANENT   │ Halt run immediately.            │
│ CORP_POLICY.md sig invalid   │ CATASTROPHIC│ ABORT. Do not start run.         │
│ Snapshot assembly failure    │ DEGRADED    │ Use defaults (score 0, level 0). │
│ ITP emission failure         │ DEGRADED    │ Log warning, continue.           │
│ Compaction failure           │ DEGRADED    │ Mechanical summary fallback.     │
│ Proposal validation error    │ DEGRADED    │ Route to HumanReview.            │
│ Credential broker failure    │ PERMANENT   │ Tool call fails. Agent replans.  │
│ Session lock contention      │ TRANSIENT   │ Queue in LaneQueue. Wait.        │
│ Memory write failure         │ TRANSIENT   │ Retry once. If fails, log error. │
│ Audit log write failure      │ CATASTROPHIC│ HALT RUN. Audit is mandatory.    │
└──────────────────────────────────────────────────────────────────────────────┘

* Tool execution errors are TRANSIENT from the agent's perspective (it can retry)
  but the circuit breaker treats consecutive failures as a pattern.
```

### 9.2 Cascading Failure Prevention

```
The most dangerous failure mode is CASCADING: one failure triggers
recovery actions that cause more failures.

OWASP ASI08 example:
  1. shell.exec fails (network issue)
  2. Agent retries shell.exec (fails again)
  3. Agent tries web_search as alternative (also fails — same network issue)
  4. Agent tries filesystem.write to save state (fails — disk full)
  5. Agent tries memory.write to persist (fails — SQLite locked)
  6. Agent tries shell.exec "df -h" to diagnose (fails — circuit breaker)
  7. Agent tries to alert user via channel (fails — channel adapter down)
  → $47,000 API bill from 11 days of recursive failure

GHOST prevention stack:
  Layer 1: Circuit Breaker (3 consecutive → stop calling that tool)
  Layer 2: Damage Counter (5 total failures → halt entire run)
  Layer 3: Recursion Depth (25 max → hard stop)
  Layer 4: Spending Cap (per-turn re-check → halt if exceeded)
  Layer 5: Retry Budget (30s max total retry time per provider)
  Layer 6: Kill Switch (auto-triggers on extreme conditions)

These layers are INDEPENDENT. Any one of them can halt the run.
They do not depend on each other. A bug in the circuit breaker
does not disable the damage counter.
```

### 9.3 Error Recovery Decision Tree

```
Error occurs in the agent loop
    │
    ├─ Is it a CATASTROPHIC error?
    │  YES → HALT IMMEDIATELY. Kill switch. Audit. Alert.
    │         No recovery. No retry. No graceful degradation.
    │         Examples: sandbox escape, credential exfil, audit failure
    │
    ├─ Is it a PERMANENT error?
    │  YES → Return error to agent as context (if in recursive loop)
    │         OR halt run (if pre-loop or post-loop)
    │         Agent can replan around the error.
    │         Do NOT retry. Do NOT escalate to more powerful tools.
    │
    ├─ Is it a TRANSIENT error?
    │  YES → Retry with backoff (if retry budget allows)
    │         Track in circuit breaker + damage counter
    │         If retries exhausted → treat as PERMANENT
    │
    └─ Is it a DEGRADED error?
       YES → Continue with reduced functionality
              Log warning. Emit ITP event if possible.
              Examples: monitor down (continue without convergence),
              compaction fails (use mechanical summary),
              ITP emission fails (continue without telemetry)
```

---

## 10. POST-LOOP: PERSIST & CLEANUP

After the recursive loop terminates (either normally or via halt),
the persist phase runs. This is STEP I.

```
STEP I: Persist & Cleanup
    │
    │  This runs regardless of HOW the loop terminated:
    │  - Normal text output (STEP D → H → I)
    │  - NO_REPLY suppression (STEP G → I)
    │  - Circuit breaker halt
    │  - Damage counter halt
    │  - Recursion depth halt
    │  - Spending cap halt
    │  - Kill switch halt
    │
    │  The only exception: CATASTROPHIC errors (sandbox escape, etc.)
    │  skip persist and go directly to kill switch.
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ I.1: Write Session Transcript                               │
    │  │                                                             │
    │  │ SessionContext::append_transcript(ctx.messages)              │
    │  │ Writes to sessions/{session_id}/transcript.jsonl            │
    │  │ Includes: all user messages, agent responses, tool calls,   │
    │  │ tool results, policy denials, errors.                       │
    │  │                                                             │
    │  │ If write fails: log error, continue. Transcript is not      │
    │  │ safety-critical (audit log is the authoritative record).    │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ I.2: Update Token Counters + Cost Tracking                  │
    │  │                                                             │
    │  │ CostTracker::record_run(RunCostRecord {                     │
    │  │   agent_id: self.agent_id,                                  │
    │  │   session_id: ctx.session.id,                               │
    │  │   input_tokens: ctx.total_input_tokens,                     │
    │  │   output_tokens: ctx.total_output_tokens,                   │
    │  │   cost_usd: ctx.total_cost_usd,                             │
    │  │   model_tier: tier_used,                                    │
    │  │   tool_calls: ctx.tool_calls_this_run.len(),                │
    │  │   recursion_depth: ctx.recursion_depth,                     │
    │  │   duration: run_duration,                                   │
    │  │   halted: was_halted,                                       │
    │  │   halt_reason: halt_reason,                                 │
    │  │ });                                                         │
    │  │                                                             │
    │  │ NOTE: Cost is tracked even for NO_REPLY runs.               │
    │  │ Suppressed runs still burn tokens.                          │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ I.3: Emit Final ITP Events                                  │
    │  │                                                             │
    │  │ // Emit AgentStateSnapshot with run summary                 │
    │  │ AgentITPEmitter::emit_agent_state(AgentStateSnapshot {      │
    │  │   session_id: ctx.session.id,                               │
    │  │   recursion_depth: ctx.recursion_depth,                     │
    │  │   tool_calls: ctx.tool_calls_this_run.len(),                │
    │  │   proposals: ctx.proposals_extracted.len(),                  │
    │  │   cost_usd: ctx.total_cost_usd,                             │
    │  │   circuit_breaker_state: self.circuit_breaker.state(),      │
    │  │   halted: was_halted,                                       │
    │  │ });                                                         │
    │  │                                                             │
    │  │ // If session is ending (user said goodbye, timeout, etc.)  │
    │  │ if session_ending {                                         │
    │  │   AgentITPEmitter::emit_session_end(                        │
    │  │     session_id, reason, duration                            │
    │  │   );                                                        │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ I.4: Compaction Check                                       │
    │  │                                                             │
    │  │ Owner: ghost-gateway/src/session/compaction.rs              │
    │  │                                                             │
    │  │ total_tokens = TokenCounter::count(ctx.messages, model)     │
    │  │ capacity = model.context_window()                           │
    │  │                                                             │
    │  │ if total_tokens > capacity * 0.70 {                         │
    │  │   // TRIGGER COMPACTION (two-phase)                         │
    │  │   //                                                        │
    │  │   // Phase 1: Memory Flush                                  │
    │  │   //   Inject silent turn: "Context is full. Write any      │
    │  │   //   critical facts to daily log NOW."                    │
    │  │   //   Agent writes to memory/daily/{today}.md              │
    │  │   //   This is a SEPARATE mini-run (recursion depth 1)      │
    │  │   //   with its own cost tracking.                          │
    │  │   //                                                        │
    │  │   //   If memory flush fails (context too large):           │
    │  │   //     Fall back to mechanical summary:                   │
    │  │   //     Extract entities, dates, decisions — no LLM needed │
    │  │   //                                                        │
    │  │   // Phase 2: History Compaction                             │
    │  │   //   Oldest chunk of history → summarized into compact    │
    │  │   //   block. Raw messages replaced by summary.             │
    │  │   //   Compaction count incremented.                        │
    │  │   //   Logged to audit trail.                               │
    │  │   //                                                        │
    │  │   // If compaction itself fails:                             │
    │  │   //   Hard-truncate oldest messages (lossy but safe).      │
    │  │   //   NEVER let compaction failure crash the session.      │
    │  │   //                                                        │
    │  │   // Per-type compression minimums:                         │
    │  │   //   Critical memories never compressed below L1.         │
    │  │   //   Tool results compressed aggressively.                │
    │  │   //   User messages preserved longest.                     │
    │  │                                                             │
    │  │   SessionCompactor::compact(ctx.session, model)?;           │
    │  │ }                                                           │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ I.5: AutoTrigger Evaluation                                 │
    │  │                                                             │
    │  │ Owner: ghost-gateway/src/safety/auto_triggers.rs            │
    │  │                                                             │
    │  │ // Evaluate kill switch auto-triggers after every run       │
    │  │ AutoTriggerEvaluator::evaluate_post_run(PostRunMetrics {    │
    │  │   policy_denials_this_session: ctx.session.policy_denials,  │
    │  │   spending_this_day: CostTracker::today(agent_id),          │
    │  │   circuit_breaker_trips: self.circuit_breaker.trip_count(), │
    │  │   soul_drift: SoulManager::current_drift(agent_id),        │
    │  │   memory_health: CortexHealth::score(agent_id),             │
    │  │ });                                                         │
    │  │                                                             │
    │  │ Auto-trigger conditions:                                    │
    │  │   - SOUL.md drift > 25% → QUARANTINE                       │
    │  │   - Spending cap exceeded → PAUSE                           │
    │  │   - 5+ policy denials in session → QUARANTINE               │
    │  │   - Sandbox escape attempt → KILL ALL                       │
    │  │   - Credential exfiltration pattern → KILL ALL              │
    │  │   - 3+ agents quarantined → KILL ALL                        │
    │  │   - Memory health < 0.3 → QUARANTINE                        │
    │  │                                                             │
    │  │ If triggered: KillSwitch state updated.                     │
    │  │ Next run's GATE 3 will catch it.                            │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ I.6: Release Session Lock                                   │
    │  │                                                             │
    │  │ LaneQueue::complete(session_id)                              │
    │  │ Next queued request (if any) can now execute.               │
    │  │                                                             │
    │  │ INVARIANT: Session lock is ALWAYS released, even on error.  │
    │  │ Use Drop guard / finally pattern to guarantee this.         │
    │  │ A leaked session lock = permanently blocked session.        │
    │  └─────────────────────────────────────────────────────────────┘
    │
    │  ┌─────────────────────────────────────────────────────────────┐
    │  │ I.7: Return AgentResponse                                   │
    │  │                                                             │
    │  │ return Ok(AgentResponse {                                   │
    │  │   text: final_text,           // may be reframed by sim     │
    │  │   tool_calls: ctx.tool_calls_this_run,                      │
    │  │   proposals: ctx.proposals_extracted,                       │
    │  │   cost: ctx.total_cost_usd,                                 │
    │  │   token_usage: TokenUsage {                                 │
    │  │     input: ctx.total_input_tokens,                          │
    │  │     output: ctx.total_output_tokens,                        │
    │  │   },                                                        │
    │  │   duration: run_duration,                                   │
    │  │   recursion_depth: ctx.recursion_depth,                     │
    │  │   no_reply: ctx.no_reply,                                   │
    │  │   truncated: was_halted,                                    │
    │  │ });                                                         │
    │  └─────────────────────────────────────────────────────────────┘
    │
    ▼
    ── RESPONSE DELIVERED TO CHANNEL ADAPTER ──
    ── (unless no_reply == true, in which case nothing is sent) ──
```

---

## 11. INTERLEAVING HAZARD MAP

These are the specific places where the interleaving of concerns creates
bug risk. Each hazard is a concrete scenario that WILL cause bugs if
not handled correctly during implementation.

### HAZARD 1: Convergence Level Stale Read

```
SCENARIO:
  Turn 0: Snapshot assembled with intervention_level = 1
  Turn 0: Agent calls tool (permitted at level 1)
  [Meanwhile: monitor processes ITP events, score rises, level → 2]
  Turn 5: Agent calls proactive tool (should be denied at level 2)
  Turn 5: Policy check uses ctx.intervention_level = 1 (stale!)
  RESULT: Tool permitted when it should have been denied.

MITIGATION: This is BY DESIGN. The snapshot is immutable for the run.
  The alternative (re-reading convergence state per turn) creates worse bugs:
    - Policy oscillation (tool allowed on turn 3, denied on turn 4)
    - Race conditions (partial state reads)
    - Performance cost (DB query per turn)
  
  The tradeoff: convergence enforcement has a latency of ONE RUN.
  The next run will pick up the updated level.
  
  ACCEPTABLE because: convergence is about PATTERNS over sessions,
  not individual tool calls. A single run with stale level is noise.

IMPLEMENTATION NOTE: Document this tradeoff in runner.rs comments.
  Future engineers WILL be tempted to "fix" this by re-reading state.
  That "fix" introduces worse bugs. Leave a clear warning.
```

### HAZARD 2: Circuit Breaker vs. Policy Denial Confusion

```
SCENARIO:
  Turn 0: Agent calls shell.exec → Policy DENIES (convergence level 3)
  Turn 1: Agent calls shell.exec again → Policy DENIES again
  Turn 2: Agent calls shell.exec again → Policy DENIES again
  QUESTION: Should the circuit breaker trip?

ANSWER: NO. Policy denials are NOT tool failures.
  The circuit breaker tracks EXECUTION failures (the tool ran and broke).
  Policy denials mean the tool never ran. Different category entirely.

  If policy denials tripped the circuit breaker:
    - Agent gets denied 3 times → circuit opens
    - Circuit open → ALL tool calls blocked (even permitted ones)
    - Agent is now completely unable to use tools
    - This is a denial-of-service via policy interaction

IMPLEMENTATION: In STEP E.3 (Deny branch), explicitly DO NOT call
  self.circuit_breaker.record_failure(). Add a comment explaining why.
  
  The 5+ policy denial auto-trigger (kill switch) handles the case
  where an agent keeps hitting policy walls. That's a different
  mechanism with a different response (quarantine, not circuit break).
```

### HAZARD 3: Compaction During Recursive Loop

```
SCENARIO:
  Turn 0: Context is at 65% capacity. No compaction.
  Turn 0: Agent calls tool. Tool returns 50KB of output.
  Turn 1: Context is now at 85% capacity.
  Turn 1: Agent calls another tool. Tool returns 30KB.
  Turn 2: Context is at 95% capacity.
  Turn 2: LLM call fails with "context too long" error.

QUESTION: When does compaction run?

ANSWER: Compaction runs in STEP I (post-loop), NOT during the recursive loop.
  
  DURING the loop, the prompt compiler (STEP A) handles overflow by:
    1. Truncating L8 (conversation history) — oldest messages first
    2. Truncating tool results before user/assistant messages
    3. If still over: truncating L7 (memory), then L5 (skills), then L2 (soul)
    4. If STILL over after all truncation: the LLM call will fail
       → treat as TRANSIENT error → retry with more aggressive truncation
       → if retry fails: halt run, trigger compaction in STEP I

  Compaction is a HEAVY operation (may involve an LLM call for summarization).
  Running it mid-loop would:
    - Add latency to every recursive turn
    - Create a nested LLM call (compaction LLM call inside the main loop)
    - Risk infinite recursion (compaction triggers compaction)

IMPLEMENTATION: The prompt compiler must handle context overflow GRACEFULLY
  without compaction. Compaction is a post-loop cleanup operation.
```

### HAZARD 4: ITP Emission Backpressure

```
SCENARIO:
  Agent is in a deep recursive loop (25 turns).
  Each turn emits 2-3 ITP events.
  Total: ~60 ITP events in rapid succession.
  Monitor is slow (processing backlog from another agent).
  ITP event queue fills up (capacity: 1000).

QUESTION: What happens when the queue is full?

ANSWER: Events are DROPPED. Not queued. Not retried. Dropped.
  
  This is intentional. The agent loop must never block on ITP emission.
  Dropped events mean the convergence monitor has an incomplete picture.
  But the agent continues operating.

RISK: If events are consistently dropped, convergence scoring becomes
  unreliable. The monitor may underestimate convergence because it's
  missing data points.

MITIGATION:
  1. Queue capacity is generous (1000 events)
  2. Dropped event counter is tracked (self.dropped_events)
  3. Health endpoint reports dropped event rate
  4. If dropped rate > threshold: log critical warning
  5. Monitor has its own backpressure handling (batch processing)

IMPLEMENTATION: Use tokio::sync::mpsc::channel(1000) for the event queue.
  Background task drains the channel and sends to transport.
  try_send() for non-blocking enqueue. Never send().await.
```

### HAZARD 5: Proposal Extraction on Halted Runs

```
SCENARIO:
  Turn 0: Agent calls memory.write("important fact")
  Turn 1: Agent calls memory.write("another fact")
  Turn 2: Circuit breaker trips. Run halted.
  
QUESTION: Are the memory.write calls from turns 0-1 treated as proposals?

ANSWER: YES. Proposal extraction runs on ALL tool calls in the run,
  even if the run was halted.

  BUT: proposals from halted runs are marked as PARTIAL and routed
  to HumanReview regardless of their normal routing.

  This prevents:
    - Silent data loss (agent wrote something important before halt)
    - Unreviewed writes (the halt may indicate something is wrong,
      so human should review what the agent was trying to do)

IMPLEMENTATION: In the halt path (circuit breaker, damage counter, etc.),
  still call ProposalExtractor::extract() and ProposalRouter::route()
  but with a flag: is_partial_run = true.
  ProposalRouter treats all proposals from partial runs as HumanReview.
```

### HAZARD 6: Kill Switch Race Condition

```
SCENARIO:
  Turn 3: AutoTriggerEvaluator fires (5 policy denials) → sets QUARANTINE
  Turn 3: Tool execution is already in progress (started before trigger)
  Turn 3: Tool completes successfully.
  Turn 4: GATE 3 checks kill switch → sees QUARANTINE → halts.

QUESTION: Is the tool result from turn 3 valid?

ANSWER: YES. The tool executed before the kill switch was set.
  The result is valid and should be included in the response.
  The kill switch takes effect on the NEXT gate check, not retroactively.

  This is analogous to a database transaction: the kill switch is
  eventually consistent, not immediately consistent.

IMPLEMENTATION: Kill switch is checked at GATE 3 (top of each recursive turn).
  It is NOT checked mid-tool-execution. A tool that's already running
  will complete. The kill switch prevents the NEXT turn from starting.
```

### HAZARD 7: Spending Cap Estimation Accuracy

```
SCENARIO:
  GATE 2 estimates the next turn will cost $0.05.
  Current spend: $4.96. Cap: $5.00. Estimated total: $5.01.
  GATE 2 blocks the turn.
  
  BUT: the actual cost might have been $0.03 (model returned a short response).
  The agent was blocked unnecessarily.

ALTERNATIVE SCENARIO:
  GATE 2 estimates $0.03. Current: $4.97. Cap: $5.00. Estimated: $5.00.
  GATE 2 permits. Actual cost: $0.08. Total: $5.05. Cap exceeded.

ANSWER: Estimation is inherently imprecise. We err on the side of PERMITTING.
  
  Rationale:
    - Blocking too aggressively frustrates users (agent stops mid-task)
    - Slight overshoot is acceptable (caps are soft limits, not hard walls)
    - The post-run cost recording (STEP I.2) catches the actual spend
    - If actual spend exceeds cap: next run's pre-loop check (step 6) blocks

IMPLEMENTATION: Use conservative estimation (assume average output length
  for the model tier). If estimation is uncertain, PERMIT and track actual.
  The spending cap is a BUDGET, not a circuit breaker.
```

### HAZARD 8: Multiple Tool Calls in Single LLM Response

```
SCENARIO:
  LLM returns a response with 3 tool calls simultaneously:
    tool_call_1: filesystem.read("/etc/hosts")
    tool_call_2: shell.exec("whoami")
    tool_call_3: memory.write("user prefers dark mode")

QUESTION: How are these processed? Sequentially or in parallel?
  What if tool_call_2 is denied by policy but 1 and 3 are permitted?

ANSWER: Tool calls from a single LLM response are processed SEQUENTIALLY.
  Each one goes through the full STEP E → STEP F pipeline independently.

  If tool_call_2 is denied:
    - tool_call_1 result: success (already executed)
    - tool_call_2 result: DENIED (policy feedback)
    - tool_call_3: still evaluated and executed if permitted
    - All three results appended to context
    - Agent sees: success, denial, success
    - Agent can replan based on the denial

  WHY SEQUENTIAL (not parallel):
    - Policy evaluation may depend on prior tool results
    - Circuit breaker state changes between calls
    - Audit trail must be ordered
    - Simpler to reason about (fewer race conditions)

  COST: Sequential is slower for multi-tool responses.
  ACCEPTABLE because: correctness > speed for safety-critical code.

IMPLEMENTATION: for tool_call in response.tool_calls { ... }
  NOT: futures::join_all(response.tool_calls.map(|tc| execute(tc)))
```

### HAZARD 9: Simulation Boundary Reframing Breaks Tool Call Parsing

```
SCENARIO:
  Agent output contains both text AND a tool call request.
  SimBoundaryEnforcer::scan_output() runs on the text portion.
  Reframing modifies the text.
  
  BUT: what if the LLM's tool call arguments contain text that
  looks like emulation language? E.g., memory.write("I care about
  the user's wellbeing").

QUESTION: Does the simulation boundary scan tool call arguments?

ANSWER: NO. Simulation boundary scanning applies ONLY to the agent's
  TEXT output (the part delivered to the user). Tool call arguments
  are NOT scanned for emulation language.

  Tool call arguments go through:
    1. Policy check (STEP E) — authorization
    2. Proposal extraction (STEP H) — if it's a state change
    3. Proposal validation D7 (emulation detection) — if it's a proposal

  The simulation boundary enforcer and the proposal validator D7 both
  detect emulation language, but at different points:
    - SimBoundaryEnforcer: scans user-facing text output (STEP D.1)
    - ProposalValidator D7: scans proposal content (STEP H.3)

  They use the SAME pattern library (simulation-boundary/src/patterns/)
  but are invoked at different points in the pipeline.

IMPLEMENTATION: SimBoundaryEnforcer::scan_output() receives ONLY the
  text portion of the LLM response. Tool calls are stripped before scanning.
```

---

## 12. FULL SEQUENCE DIAGRAM (ASCII)

Complete end-to-end flow for a single message through the recursive loop.
This is the "one diagram to rule them all" — every participant, every call,
every branch.

```
 Channel    Gateway     Runner      PromptComp   LLM        Policy     ToolExec    SimBound    ITPEmit    Monitor
 Adapter    (session)   (loop)      (context)    Provider   Engine     (sandbox)   Enforcer    (async)    (sidecar)
    │           │           │           │           │          │           │           │          │           │
    │──msg──►│           │           │           │          │           │           │          │           │
    │        │──lock──►│           │           │          │           │           │          │           │
    │        │  killsw? │           │           │          │           │           │          │           │
    │        │  cap?    │           │           │          │           │           │          │           │
    │        │  cool?   │           │           │          │           │           │          │           │
    │        │  bound?  │           │           │          │           │           │          │           │
    │        │──snap──►│           │           │          │           │           │          │           │
    │        │        ◄──snapshot──│           │          │           │           │          │           │
    │        │──itp(user_msg)──────────────────────────────────────────────────►│           │
    │        │           │           │           │          │           │           │          │──event──►│
    │        │──run()──►│           │           │          │           │           │          │           │
    │        │           │           │           │          │           │           │          │           │
    │        │           │──GATE0: circuit_breaker_check──────────────────────────────────────────────────│
    │        │           │──GATE1: recursion_depth_check──────────────────────────────────────────────────│
    │        │           │──GATE1.5: damage_counter_check─────────────────────────────────────────────────│
    │        │           │──GATE2: spending_cap_check─────────────────────────────────────────────────────│
    │        │           │──GATE3: kill_switch_check──────────────────────────────────────────────────────│
    │        │           │           │           │          │           │           │          │           │
    │        │           │──compile()──►│        │          │           │           │          │           │
    │        │           │           │──L0:corp──│          │           │           │          │           │
    │        │           │           │──L1:sim───│          │           │──prompt──►│          │           │
    │        │           │           │──L2:soul──│          │           │           │          │           │
    │        │           │           │──L3:tools─│──filter──►│          │           │          │           │
    │        │           │           │──L4:env───│          │           │           │          │           │
    │        │           │           │──L5:skill─│          │           │           │          │           │
    │        │           │           │──L6:conv──│          │           │           │          │           │
    │        │           │           │──L7:mem───│          │           │           │          │           │
    │        │           │           │──L8:hist──│          │           │           │          │           │
    │        │           │           │──L9:user──│          │           │           │          │           │
    │        │           │        ◄──context─────│          │           │           │          │           │
    │        │           │           │           │          │           │           │          │           │
    │        │           │──────────────────────►│          │           │           │          │           │
    │        │           │           │     complete_with_tools()        │           │          │           │
    │        │           │           │           │          │           │           │          │           │
    │        │           │◄─────────────response─│          │           │           │          │           │
    │        │           │           │           │          │           │           │          │           │
    │        │           │ ┌─── IF TOOL CALL ────────────────────────────────────────────────────────────┐│
    │        │           │ │                     │          │           │           │          │           ││
    │        │           │ │──evaluate(action)──────────►│           │           │          │           ││
    │        │           │ │                     │        corp_pol    │           │          │           ││
    │        │           │ │                     │        conv_tight  │           │          │           ││
    │        │           │ │                     │        cap_grants  │           │          │           ││
    │        │           │ │◄──────────decision──────────│           │           │          │           ││
    │        │           │ │                     │          │           │           │          │           ││
    │        │           │ │ ┌─ IF PERMIT ───────────────────────────────────────────────────────────────┐│
    │        │           │ │ │                   │          │           │           │          │           ││
    │        │           │ │ │──execute(tool)────────────────────────►│           │          │           ││
    │        │           │ │ │                   │          │     sandbox.exec()   │          │           ││
    │        │           │ │ │◄──result──────────────────────────────│           │          │           ││
    │        │           │ │ │                   │          │           │           │          │           ││
    │        │           │ │ │──audit_log(tool, result)───────────────────────────────────────────────────││
    │        │           │ │ │──circuit_breaker.record(result)────────────────────────────────────────────││
    │        │           │ │ │──itp(tool_exec)──────────────────────────────────────────────►│           ││
    │        │           │ │ │                   │          │           │           │          │──event──►││
    │        │           │ │ │                   │          │           │           │          │           ││
    │        │           │ │ │──ctx.depth += 1───│          │           │           │          │           ││
    │        │           │ │ │──RECURSE: goto GATE0──────────────────────────────────────────────────────││
    │        │           │ │ └───────────────────────────────────────────────────────────────────────────┘│
    │        │           │ │                     │          │           │           │          │           ││
    │        │           │ │ ┌─ IF DENY ─────────────────────────────────────────────────────────────────┐│
    │        │           │ │ │──ctx.messages.push(denial)─────────────────────────────────────────────────││
    │        │           │ │ │──audit_log(denial)─────────────────────────────────────────────────────────││
    │        │           │ │ │──ctx.depth += 1───│          │           │           │          │           ││
    │        │           │ │ │──RECURSE: goto GATE0 (agent replans)──────────────────────────────────────││
    │        │           │ │ └───────────────────────────────────────────────────────────────────────────┘│
    │        │           │ └─────────────────────────────────────────────────────────────────────────────┘│
    │        │           │           │           │          │           │           │          │           │
    │        │           │ ┌─── IF TEXT OUTPUT ──────────────────────────────────────────────────────────┐│
    │        │           │ │                     │          │           │           │          │           ││
    │        │           │ │──scan_output(text)──────────────────────────────────►│          │           ││
    │        │           │ │◄──scan_result───────────────────────────────────────│          │           ││
    │        │           │ │  (reframe if Level 2+)        │           │           │          │           ││
    │        │           │ │──itp(agent_response)──────────────────────────────────────────►│           ││
    │        │           │ │                     │          │           │           │          │──event──►││
    │        │           │ │──extract_proposals(text, tool_calls)─────────────────────────────────────────││
    │        │           │ │──route_proposals()──────────────────────────────────────────────────────────││
    │        │           │ │  (validate 7 dims, auto-approve or queue)────────────────────────────────────││
    │        │           │ └─────────────────────────────────────────────────────────────────────────────┘│
    │        │           │           │           │          │           │           │          │           │
    │        │           │──PERSIST──│           │          │           │           │          │           │
    │        │           │  transcript           │          │           │           │          │           │
    │        │           │  cost_track            │          │           │           │          │           │
    │        │           │  itp(state)────────────────────────────────────────────────────►│           │
    │        │           │  compact?              │          │           │           │          │           │
    │        │           │  auto_trigger           │          │           │           │          │           │
    │        │           │           │           │          │           │           │          │           │
    │        │◄──response─│          │           │          │           │           │          │           │
    │        │──unlock──►│           │           │          │           │           │          │           │
    │◄──msg──│           │           │           │          │           │           │          │           │
    │        │           │           │           │          │           │           │          │           │
```

---

## 13. INVARIANTS THAT MUST HOLD

These are the properties that must be TRUE at all times during the agent loop.
If any invariant is violated, there is a bug. These should be encoded as
debug_assert!() in development and as runtime checks in production.

### 13.1 Loop Invariants

```
INV-LOOP-01: recursion_depth <= max_recursion_depth
  Checked: GATE 1, every recursive entry
  Violation: Infinite recursion. Halt immediately.

INV-LOOP-02: ctx.intervention_level is constant for the entire run
  Checked: Never changes after snapshot assembly
  Violation: Mid-run policy oscillation. Inconsistent authorization.

INV-LOOP-03: Every tool execution has a corresponding audit log entry
  Checked: STEP F.3, mandatory
  Violation: CORP_POLICY.md violation. Halt run.

INV-LOOP-04: Policy check runs before EVERY tool execution
  Checked: STEP E, before STEP F
  Violation: Unauthorized tool execution. Security breach.

INV-LOOP-05: Circuit breaker failure count is monotonically non-decreasing
  within a consecutive failure sequence
  Checked: STEP F.4
  Violation: Circuit breaker never trips. Infinite retry loops.

INV-LOOP-06: Damage counter is monotonically non-decreasing (never resets)
  Checked: STEP F.4
  Violation: Cascading failure prevention disabled.

INV-LOOP-07: Session lock is held for the entire duration of the run
  Checked: Pre-loop acquisition, post-loop release
  Violation: Race condition. Two requests executing for same session.

INV-LOOP-08: ITP emission never blocks the agent loop
  Checked: try_send() in AgentITPEmitter
  Violation: Agent loop hangs waiting for monitor.

INV-LOOP-09: CORP_POLICY.md signature is verified before every run
  Checked: STEP A, L0 assembly
  Violation: Tampered policy. Agent operates without constraints.

INV-LOOP-10: Simulation boundary prompt is compiled into binary (const)
  Checked: Compile-time
  Violation: Agent can modify the boundary prompt at runtime.
```

### 13.2 Convergence Invariants

```
INV-CONV-01: Convergence score is in range [0.0, 1.0]
  Checked: CompositeScorer output
  Violation: Invalid score. Intervention levels miscalculated.

INV-CONV-02: Higher convergence level → stricter or equal policy
  Checked: ConvergencePolicyTightener
  Violation: Safety weakens as risk increases. Critical bug.

INV-CONV-03: Higher convergence level → stricter or equal proposal thresholds
  Checked: ProposalValidator threshold lookup
  Violation: Proposals approved more easily at higher risk. Critical bug.

INV-CONV-04: Memory filtering is monotonically restrictive with level
  Checked: ConvergenceAwareFilter
  Violation: Agent sees MORE memories at higher risk levels.

INV-CONV-05: Agent cannot write to convergence state directly
  Checked: Proposal system (all writes go through ProposalRouter)
  Violation: Agent manipulates its own convergence score.

INV-CONV-06: Convergence monitor is a separate process
  Checked: Architecture (sidecar binary)
  Violation: Agent can influence its own monitoring.

INV-CONV-07: Emulation detection uses unicode-normalized input
  Checked: SimBoundaryEnforcer::scan_output()
  Violation: Zero-width character bypass attacks succeed.
```

### 13.3 Circuit Breaker Invariants

```
INV-CB-01: CLOSED → OPEN only when consecutive_failures >= threshold
  Checked: STEP F.4
  Violation: Premature circuit trip. Agent unnecessarily blocked.

INV-CB-02: OPEN → HALF-OPEN only when cooldown has expired
  Checked: GATE 0
  Violation: Circuit recovers too quickly. Failures resume.

INV-CB-03: HALF-OPEN → CLOSED only on successful probe
  Checked: STEP F.4
  Violation: Circuit closes without verifying recovery.

INV-CB-04: HALF-OPEN → OPEN on failed probe (cooldown resets)
  Checked: STEP F.4
  Violation: Circuit stays half-open forever. Probe failures ignored.

INV-CB-05: Policy denials do NOT increment failure counter
  Checked: STEP E.3 (Deny branch)
  Violation: Policy denials trip circuit breaker. DoS via policy.

INV-CB-06: Successful tool execution resets consecutive counter to 0
  Checked: STEP F.4 (Success branch)
  Violation: Counter never resets. Circuit trips on non-consecutive failures.

INV-CB-07: Provider circuit breaker is independent of tool circuit breaker
  Checked: Separate state machines in ghost-llm vs ghost-agent-loop
  Violation: LLM failure trips tool circuit breaker or vice versa.
```

### 13.4 Safety Invariants

```
INV-SAFE-01: Kill switch cannot be overridden by any agent
  Checked: KillSwitch implementation (no agent-accessible API)
  Violation: Agent disables its own kill switch.

INV-SAFE-02: Session lock is always released (even on panic)
  Checked: Drop guard on LaneQueue entry
  Violation: Permanently blocked session. DoS.

INV-SAFE-03: Spending cap is checked before AND during recursive loop
  Checked: Pre-loop step 6 + GATE 2
  Violation: Deep recursion blows past spending cap.

INV-SAFE-04: Audit log write failure halts the run
  Checked: STEP F.3
  Violation: Agent operates without audit trail. CORP_POLICY violation.

INV-SAFE-05: Sandbox escape attempt triggers KILL ALL
  Checked: ToolExecutor sandbox monitoring
  Violation: Compromised tool continues executing.

INV-SAFE-06: Credential broker never exposes raw secrets to agent
  Checked: CredentialBroker implementation
  Violation: Agent exfiltrates API keys.

INV-SAFE-07: All proposals from halted runs route to HumanReview
  Checked: ProposalRouter with is_partial_run flag
  Violation: Unreviewed state changes from broken runs.
```

---

## 14. IMPLEMENTATION CHECKLIST

Ordered by dependency. Each item maps to a specific file and function.

```
PHASE 4 (Weeks 7-8): ghost-agent-loop implementation

□ 1.  CircuitBreaker struct + state machine
      File: src/circuit_breaker.rs
      Test: state transitions, threshold behavior, cooldown timing
      Invariants: INV-CB-01 through INV-CB-07

□ 2.  DamageCounter struct
      File: src/circuit_breaker.rs (same file, separate struct)
      Test: monotonic increment, threshold halt
      Invariants: INV-LOOP-06

□ 3.  AgentITPEmitter with async non-blocking emission
      File: src/itp_emitter.rs
      Test: non-blocking under backpressure, dropped event counting
      Invariants: INV-LOOP-08
      Depends on: itp-protocol crate (Phase 2)

□ 4.  TokenBudgetAllocator
      File: src/context/token_budget.rs
      Test: budget allocation, overflow handling, priority truncation
      Depends on: ghost-llm tokenizer

□ 5.  PromptCompiler (10-layer assembly)
      File: src/context/prompt_compiler.rs
      Test: layer ordering, budget enforcement, L0/L1 never truncated
      Invariants: INV-LOOP-09, INV-LOOP-10
      Depends on: ghost-identity, simulation-boundary, ghost-skills,
                  read-only-pipeline, ghost-policy (for tool filtering)

□ 6.  ToolRegistry + ToolExecutor
      File: src/tools/registry.rs, src/tools/executor.rs
      Test: tool lookup, sandbox execution, timeout, audit logging
      Invariants: INV-LOOP-03, INV-LOOP-04
      Depends on: ghost-skills (sandbox), ghost-policy (authorization)

□ 7.  ProposalExtractor
      File: src/proposal/extractor.rs
      Test: extraction from tool calls, text output, partial runs
      Depends on: cortex-core (Proposal type)

□ 8.  ProposalRouter
      File: src/proposal/router.rs
      Test: routing decision tree, partial run handling, rejection logging
      Invariants: INV-SAFE-07
      Depends on: cortex-validation (ProposalValidator)

□ 9.  RunContext struct
      File: src/runner.rs (internal)
      Test: initialization, field updates, immutability of snapshot

□ 10. AgentRunner::run() — the recursive loop
      File: src/runner.rs
      Test: full integration test with mock LLM, mock tools, mock policy
      Invariants: ALL loop invariants (INV-LOOP-01 through INV-LOOP-10)
      Depends on: ALL of the above (items 1-9)

      Sub-tests:
      □ 10a. GATE 0-3 execution order
      □ 10b. Text response → output processing → proposal extraction
      □ 10c. Tool call → policy check → execution → recurse
      □ 10d. NO_REPLY detection and suppression
      □ 10e. Circuit breaker trip during recursion
      □ 10f. Damage counter halt during recursion
      □ 10g. Recursion depth limit
      □ 10h. Spending cap mid-run
      □ 10i. Kill switch mid-run
      □ 10j. Multiple tool calls in single response (sequential)
      □ 10k. Policy denial → agent replan → success
      □ 10l. Policy escalation → human approve → resume
      □ 10m. Compaction trigger post-loop
      □ 10n. Halted run → partial proposal extraction
      □ 10o. ITP emission at all documented points
      □ 10p. Simulation boundary scan + reframe at Level 2+
      □ 10q. Convergence snapshot immutability across turns
```

---

## 15. CROSS-REFERENCE: FILE MAPPING → SEQUENCE FLOW

Every file in `ghost-agent-loop` mapped to the sequence flow steps it participates in.

```
┌────────────────────────────────────┬──────────────────────────────────────────┐
│ FILE                               │ SEQUENCE STEPS                           │
├────────────────────────────────────┼──────────────────────────────────────────┤
│ src/runner.rs                      │ GATE 0-3, STEP A-I, recursion control   │
│ src/circuit_breaker.rs             │ GATE 0, STEP F.4, §5 state machine      │
│ src/itp_emitter.rs                 │ Pre-loop 11, D.2, F.4, G.1, I.3, §8    │
│ src/context/prompt_compiler.rs     │ STEP A (all 10 layers)                  │
│ src/context/token_budget.rs        │ STEP A (budget enforcement)             │
│ src/proposal/extractor.rs          │ STEP H.1                               │
│ src/proposal/router.rs             │ STEP H.2                               │
│ src/tools/registry.rs              │ STEP A (L3), STEP F.1                  │
│ src/tools/executor.rs              │ STEP F.2, F.3, F.4                     │
│ src/tools/builtin/shell.rs         │ STEP F.2 (shell tool impl)             │
│ src/tools/builtin/filesystem.rs    │ STEP F.2 (filesystem tool impl)        │
│ src/tools/builtin/web_search.rs    │ STEP F.2 (web search tool impl)        │
│ src/tools/builtin/memory.rs        │ STEP F.2 (memory tool impl)            │
│ src/response.rs                    │ STEP I.7 (AgentResponse struct)         │
├────────────────────────────────────┼──────────────────────────────────────────┤
│ EXTERNAL CRATE FILES               │                                          │
├────────────────────────────────────┼──────────────────────────────────────────┤
│ ghost-policy/src/engine.rs         │ STEP E.2                               │
│ ghost-policy/src/policy/corp_pol   │ STEP E.2 (rule 1)                      │
│ ghost-policy/src/policy/conv_pol   │ STEP E.2 (rule 2), STEP A L3 (IP-4)   │
│ ghost-policy/src/policy/cap_grant  │ STEP E.2 (rule 3)                      │
│ ghost-policy/src/feedback.rs       │ STEP E.3 (Deny branch)                 │
│ ghost-llm/src/provider/mod.rs      │ STEP B.3                               │
│ ghost-llm/src/routing/model_rtr    │ STEP B.1, B.2                          │
│ ghost-llm/src/routing/classifier   │ STEP B.1                               │
│ ghost-llm/src/routing/fallback.rs  │ STEP B.2                               │
│ ghost-llm/src/cost.rs              │ STEP B.3 (cost tracking)               │
│ ghost-gateway/src/cost/tracker.rs  │ GATE 2, STEP B.3, I.2                  │
│ ghost-gateway/src/cost/spend_cap   │ Pre-loop 6, GATE 2                     │
│ ghost-gateway/src/safety/kill_sw   │ Pre-loop 5, GATE 3, I.5               │
│ ghost-gateway/src/safety/auto_tr   │ STEP E.3 (5+ denials), I.5            │
│ ghost-gateway/src/safety/quarant   │ Pre-loop (quarantine check via KillSw) │
│ ghost-gateway/src/session/manager  │ Pre-loop 3                             │
│ ghost-gateway/src/session/compact  │ I.4                                    │
│ ghost-gateway/src/routing/msg_rtr  │ Pre-loop 2 (agent resolution, routing) │
│ ghost-gateway/src/routing/lane_q   │ Pre-loop 4, I.6                        │
│ ghost-gateway/src/agents/registry  │ Pre-loop 2 (agent binding lookup)      │
│ read-only-pipeline/src/assembler   │ Pre-loop 9 (IP-3)                      │
│ read-only-pipeline/src/formatter   │ Pre-loop 9 (snapshot serialization)    │
│ read-only-pipeline/src/snapshot    │ Pre-loop 9, STEP A L6                  │
│ simulation-boundary/src/enforcer   │ STEP D.1 (IP-9)                        │
│ simulation-boundary/src/reframer   │ STEP D.1 (reframe at Level 2+)        │
│ simulation-boundary/src/prompt_a   │ STEP A L1 (IP-6)                       │
│ convergence-monitor/src/interv/*   │ Pre-loop 7 (IP-1), Pre-loop 8 (IP-2)  │
│ cortex-validation/src/proposal_v   │ STEP H.3 (IP-10)                       │
│ cortex-convergence/src/filtering   │ Pre-loop 9 (IP-5)                      │
│ cortex-convergence/src/scoring/*   │ Monitor-side (not in agent loop)       │
│ itp-protocol/src/events/*.rs       │ §8 (event type definitions)            │
│ ghost-identity/src/corp_policy.rs  │ STEP A L0                              │
│ ghost-identity/src/soul.rs         │ STEP A L2                              │
│ ghost-heartbeat/src/heartbeat.rs   │ §3.1 (alternate entry: heartbeat)      │
│ ghost-heartbeat/src/cron.rs        │ §3.1 (alternate entry: cron)           │
│ ghost-skills/src/registry.rs       │ STEP A L5, STEP F.1                    │
│ ghost-skills/src/signing/verifier  │ STEP F.1 (skill verification)          │
│ ghost-skills/src/sandbox/*.rs      │ STEP F.2 (sandbox execution)           │
│ ghost-skills/src/credential/brkr   │ STEP F.1 (credential provisioning)     │
│ ghost-audit (via cortex-storage)   │ STEP F.3, E.3, H.2, I.5              │
└────────────────────────────────────┴──────────────────────────────────────────┘
```

---

## END OF DOCUMENT

This sequence flow covers every path through the recursive agent loop.
Every interleaving point between the recursive execution, circuit breaker,
convergence integration, policy engine, ITP emission, proposal extraction,
and simulation boundary enforcement is documented with:

- Exact call sites and owning files
- State transitions and their triggers
- Error paths and recovery strategies
- Invariants that must hold
- Hazards where bugs will hide
- Implementation ordering with dependencies

No shortcuts. No ambiguity. Build from this.
