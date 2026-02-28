# Kill Switch Trigger Chain — Complete Sequence Flow

> Codename: GHOST (General Hybrid Orchestrated Self-healing Taskrunner)
> Date: 2026-02-27
> Scope: Finding 3 Resolution — Full sequence mapping for kill_switch.rs
> Objective: Zero ambiguity. Every signal path, race condition, state transition,
>   and failure mode mapped before a single line of code is written.
> Cross-references:
>   - AGENT_ARCHITECTURE.md §20 (Kill Switch + Emergency Stop), §17 (Error Handling)
>   - AGENT_ARCHITECTURE_v2.md §3 (Convergence Safety System — 5 intervention levels)
>   - FILE_MAPPING.md Finding 3 (kill switch unmapped — this doc resolves it)
>   - CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §7.1 (Shared State Publication), §8.4 (Monitor Crash)
>   - GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §kill switch state-independence
>   - AGENT_LOOP_SEQUENCE_FLOW.md §GATE 3 (KillSwitch Re-check), §HAZARD 6 (Kill Switch Race)
>
> IMPORTANT DISTINCTION — TWO SEPARATE SAFETY SYSTEMS:
>   The CONVERGENCE MONITOR has 5 intervention levels (0-4) that progressively
>   restrict agent behavior (memory filtering, session caps, proactive messaging).
>   These are SOFT interventions — the agent keeps running with restrictions.
>
>   The KILL SWITCH has 3 levels (PAUSE, QUARANTINE, KILL ALL) that STOP agents.
>   These are HARD interventions — the agent ceases operation entirely.
>
>   The two systems are complementary but independent. The convergence monitor's
>   Level 4 (External Escalation) does NOT automatically trigger the kill switch.
>   The kill switch's T7 (Memory Health < 0.3) reads data FROM the convergence
>   monitor but is evaluated BY the gateway's AutoTriggerEvaluator.
>
> THRESHOLD CLARIFICATION:
>   ghost.yml `soul_drift_threshold: 0.15` = ALERT threshold (soft notification)
>   Kill switch T1 `drift > 0.25` = QUARANTINE threshold (hard stop)
>   These are two different thresholds at two different severity levels.
>   The 0.15 alert fires first (via IdentityDriftDetector → convergence monitor).
>   The 0.25 quarantine fires second (via IdentityDriftDetector → AutoTriggerEvaluator).

---

## 0. DOCUMENT CONVENTIONS

```
CRATE SHORTHAND:
  [GW]     = ghost-gateway/src/
  [SAFETY] = ghost-gateway/src/safety/
  [POLICY] = ghost-policy/src/
  [SKILLS] = ghost-skills/src/
  [IDENT]  = ghost-identity/src/
  [LOOP]   = ghost-agent-loop/src/
  [CMON]   = convergence-monitor/src/
  [CORTEX] = crates/cortex/ (various sub-crates)
  [AUDIT]  = ghost-audit/src/
  [ITP]    = itp-protocol/src/

NOTATION:
  ──►  = synchronous call (caller blocks until return)
  ──▷  = asynchronous message (fire-and-forget, non-blocking)
  ═══► = channel/broadcast (tokio::broadcast or mpsc)
  ●    = state mutation
  ◆    = decision point
  ✗    = failure / error path
  ✓    = success path
```

---

## 1. SYSTEM TOPOLOGY — WHO OWNS WHAT

Before tracing any trigger, we must establish which process owns which
detection responsibility and how they communicate.

```
┌─────────────────────────────────────────────────────────────────────┐
│                     GHOST GATEWAY PROCESS                           │
│                     (single long-running process)                   │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │ AgentRunner   │  │ PolicyEngine │  │ SpendingCapEnforcer      │  │
│  │ [LOOP]        │  │ [POLICY]     │  │ [GW]/cost/spending_cap.rs│  │
│  └──────┬───────┘  └──────┬───────┘  └────────────┬─────────────┘  │
│         │                  │                        │                │
│         │    ┌─────────────┴────────────────────────┘                │
│         │    │                                                       │
│  ┌──────▼────▼──────────────────────────────────────────────────┐   │
│  │              AutoTriggerEvaluator                              │   │
│  │              [SAFETY]/auto_triggers.rs                         │   │
│  │              (receives signals from ALL subsystems)            │   │
│  └──────────────────────┬───────────────────────────────────────┘   │
│                          │                                           │
│  ┌──────────────────────▼───────────────────────────────────────┐   │
│  │              KillSwitch                                        │   │
│  │              [SAFETY]/kill_switch.rs                           │   │
│  │              (executes PAUSE / QUARANTINE / KILL ALL)          │   │
│  └──────────────────────┬───────────────────────────────────────┘   │
│                          │                                           │
│  ┌──────────────────────▼───────────────────────────────────────┐   │
│  │              QuarantineManager                                 │   │
│  │              [SAFETY]/quarantine.rs                            │   │
│  │              (agent isolation, capability revocation)          │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  AgentRegistry  │  ChannelAdapters  │  SessionManager        │   │
│  │  [GW]/agents/   │  ghost-channels   │  [GW]/session/         │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                CONVERGENCE MONITOR (separate sidecar process)       │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │ Pipeline      │  │ Scoring      │  │ InterventionTrigger      │  │
│  │ (7 signals)   │  │ (composite)  │  │ (5 levels)               │  │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘  │
│                                                                     │
│  Transport: HTTP API (GET /health, GET /scores, GET /status)        │
│             Unix socket (ITP event ingestion)                       │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.1 Critical Architectural Constraint

The kill switch lives INSIDE the gateway process (`ghost-gateway/src/safety/`),
NOT in the convergence monitor sidecar. This is intentional:

- The gateway owns agent lifecycle (start/stop/pause).
- The gateway owns channel adapters (can sever connections).
- The gateway owns the session manager (can flush and lock sessions).
- The gateway owns the agent registry (can revoke capabilities).
- The convergence monitor is a READER — it computes scores and reports.
  It does NOT have authority to stop agents. It signals the gateway.

If the convergence monitor crashes, the gateway enters DEGRADED mode
(agents run, convergence scoring disabled, logged as critical warning).
The kill switch still functions for all non-convergence triggers.

### 1.2 Communication Channels Between Subsystems

```
INTRA-PROCESS (within gateway):
  PolicyEngine ──► AutoTriggerEvaluator     via: tokio::mpsc channel (TriggerEvent)
  SpendingCapEnforcer ──► AutoTriggerEvaluator  via: tokio::mpsc channel (TriggerEvent)
  AgentRunner ──► AutoTriggerEvaluator      via: tokio::mpsc channel (TriggerEvent)
  QuarantineManager ──► AutoTriggerEvaluator via: tokio::mpsc channel (TriggerEvent)

INTER-PROCESS (gateway ↔ convergence monitor):
  Gateway ──► Monitor                       via: HTTP GET /health (health check)
  Gateway ──► Monitor                       via: HTTP GET /scores/{agent_id} (poll scores)
  Monitor ──► Shared State File             via: write to ~/.ghost/data/convergence_state/
  Gateway ──► Shared State File             via: read (1s poll interval)
  AgentRunner ──▷ Monitor                   via: Unix socket (ITP events, non-blocking)
  NOTE: Monitor does NOT push to gateway. Gateway is the active reader.
        (per CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §7.1)

INTER-PROCESS (gateway ↔ cortex):
  Gateway ──► cortex-observability          via: in-process Rust call (same binary)
  cortex-temporal ──► hash chain verify     via: in-process Rust call
  cortex-convergence ──► scoring            via: in-process Rust call
```

---

## 2. THE SEVEN TRIGGERS — COMPLETE DETECTION CHAINS

Each trigger has a unique detection path. We trace every one from the
originating subsystem through to the AutoTriggerEvaluator.

### 2.1 Trigger Map Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        TRIGGER SOURCES                                   │
│                                                                          │
│  T1: SOUL Drift >25%          ← ghost-identity/drift_detector.rs        │
│  T2: Spending Cap Exceeded    ← ghost-gateway/cost/spending_cap.rs       │
│  T3: 5+ Policy Denials/Sess  ← ghost-policy/engine.rs                   │
│  T4: Sandbox Escape Attempt   ← ghost-skills/sandbox/wasm_sandbox.rs     │
│  T5: Credential Exfiltration  ← ghost-skills/credential/broker.rs        │
│  T6: 3+ Agents Quarantined   ← ghost-gateway/safety/quarantine.rs       │
│  T7: Memory Health <0.3      ← cortex-observability + cortex-convergence │
│                                                                          │
│  ALL ═══► AutoTriggerEvaluator ═══► KillSwitch                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 T1: SOUL.md Semantic Drift > 25% → QUARANTINE

```
DETECTION LATENCY: ~100ms (embedding computation) on inline check,
                   up to 5min on background poll
DATA SOURCE: ghost-identity/drift_detector.rs + cortex-embeddings
KILL LEVEL: QUARANTINE (single agent)
AFFECTED AGENT: The agent whose SOUL.md drifted

DUAL-THRESHOLD SYSTEM:
  ghost.yml `soul_drift_threshold: 0.15` → ALERT (soft notification, logged)
  Kill switch threshold: 0.25 → QUARANTINE (hard stop)
  The alert threshold is configured in ghost.yml security section.
  The kill switch threshold is hardcoded in auto_triggers.rs (not configurable
  by the agent — defense in depth). The alert fires first, giving the owner
  a chance to investigate before the hard stop.

SEQUENCE:

1. DETECTION ORIGIN
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-identity/src/drift_detector.rs                            │
   │                                                                  │
   │ IdentityDriftDetector::check_drift(agent_id)                    │
   │   │                                                              │
   │   ├── Load current SOUL.md content for agent                    │
   │   │   Path: ~/.ghost/agents/{agent_name}/SOUL.md                │
   │   │   (per FILE_MAPPING.md identity files section)              │
   │   ├── Compute embedding via cortex-embeddings provider          │
   │   │   (ONNX local / Ollama / cloud — configured in ghost.yml    │
   │   │    memory.search.vector_provider + vector_model)            │
   │   ├── Load baseline embedding from                              │
   │   │   ~/.ghost/baselines/{agent_instance_id}.json               │
   │   ├── Compute cosine_similarity(current, baseline)              │
   │   ├── drift_score = 1.0 - cosine_similarity                    │
   │   │                                                              │
   │   ◆ drift_score > alert_threshold (ghost.yml, default 0.15)?    │
   │   │  YES → Log WARNING, emit soft notification to owner         │
   │   │        (this is the ALERT path, not the kill switch path)   │
   │   │                                                              │
   │   ◆ drift_score > kill_threshold (hardcoded 0.25)?              │
   │   │  NO → return Ok(DriftCheckResult::Normal(drift_score))      │
   │   │  YES ↓                                                       │
   │   │                                                              │
   │   ● Emit TriggerEvent::SoulDrift {                              │
   │   │     agent_id,                                                │
   │   │     drift_score,                                             │
   │   │     threshold: 0.25,                                         │
   │   │     baseline_hash: blake3(baseline_embedding),               │
   │   │     current_hash: blake3(current_embedding),                 │
   │   │     detected_at: Utc::now(),                                 │
   │   │ }                                                            │
   │   │                                                              │
   │   └── Send via tokio::mpsc::Sender<TriggerEvent>                │
   │       to AutoTriggerEvaluator                                    │
   └─────────────────────────────────────────────────────────────────┘

2. WHEN DOES THIS CHECK RUN?
   Two paths:

   Path A — On SOUL.md Load (every context assembly):
     AgentRunner::run()
       → PromptCompiler::compile()
         → Layer L2: load SOUL.md (per AGENT_LOOP_SEQUENCE_FLOW.md §STEP A)
           → SoulManager::load(agent_id)
             → IdentityDriftDetector::check_drift(agent_id)

   Path B — Periodic Background Check:
     Gateway startup spawns:
       tokio::spawn(drift_check_loop(interval: 5min, agents: all))
         → for each agent: IdentityDriftDetector::check_drift(agent_id)

   Path A catches drift on every agent turn (high frequency, low latency).
   Path B catches drift even when agent is idle (covers offline poisoning).

   NOTE: SOUL.md is READ-ONLY to the agent (per AGENT_ARCHITECTURE_v2.md §2).
   The agent cannot modify it. Drift would indicate either:
   a) Platform-managed evolution (legitimate, should stay under threshold)
   b) External tampering (filesystem access by attacker)
   c) Migration tool corruption (ghost-migrate import error)

3. EDGE CASES:
   - Embedding provider unavailable: Log warning, skip check, do NOT trigger.
     Rationale: false positive kill is worse than delayed detection.
   - Baseline file missing: First run creates baseline. No drift possible.
     If baseline file is deleted: recreate from current SOUL.md, log as
     CRITICAL (baseline reset = potential cover-up of prior drift).
   - SOUL.md file missing: FATAL — agent cannot start without identity.
     This is caught at SoulManager::load(), not at drift detector level.
   - Embedding model changed (ghost.yml vector_model updated):
     Baseline was computed with old model. New model produces different
     embeddings. This would cause a false positive drift detection.
     MITIGATION: When vector_model config changes, invalidate all baselines
     and re-establish from current SOUL.md. Log as WARNING.
```

### 2.3 T2: Spending Cap Exceeded → PAUSE

```
DETECTION LATENCY: ~0ms (synchronous, inline with every LLM call)
DATA SOURCE: ghost-gateway/cost/spending_cap.rs + ghost-gateway/cost/tracker.rs
KILL LEVEL: PAUSE (single agent)
AFFECTED AGENT: The agent that exceeded its cap

SEQUENCE:

1. DETECTION ORIGIN
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-gateway/src/cost/spending_cap.rs                          │
   │                                                                  │
   │ SpendingCapEnforcer::check_and_record(agent_id, cost)           │
   │   │                                                              │
   │   ├── Load agent config from GhostConfig                        │
   │   │   spending_cap = agent.spending_cap (e.g. "$5/day")         │
   │   │                                                              │
   │   ├── Query CostTracker::get_daily_total(agent_id, today)       │
   │   │   Returns: current_daily_spend (f64, USD)                   │
   │   │                                                              │
   │   ├── projected = current_daily_spend + cost                    │
   │   │                                                              │
   │   ◆ projected > spending_cap?                                    │
   │   │  NO → CostTracker::record(agent_id, cost) → return Ok(())  │
   │   │  YES ↓                                                       │
   │   │                                                              │
   │   ● Emit TriggerEvent::SpendingCapExceeded {                    │
   │   │     agent_id,                                                │
   │   │     daily_total: projected,                                  │
   │   │     cap: spending_cap,                                       │
   │   │     overage: projected - spending_cap,                       │
   │   │     detected_at: Utc::now(),                                 │
   │   │ }                                                            │
   │   │                                                              │
   │   └── Send via tokio::mpsc::Sender<TriggerEvent>                │
   │       to AutoTriggerEvaluator                                    │
   │                                                                  │
   │   ALSO: Return Err(SpendingCapExceeded) to caller               │
   │   → AgentRunner receives error, halts current turn,             │
   │     returns structured denial to user                            │
   └─────────────────────────────────────────────────────────────────┘

2. WHEN DOES THIS CHECK RUN?
   Synchronous — on EVERY LLM inference call:

     AgentRunner::run()
       → Before calling ghost-llm provider:
         → CostCalculator::estimate(model, input_tokens)
           → SpendingCapEnforcer::check_and_record(agent_id, estimated_cost)
             ◆ Denied? → Don't call LLM. Return denial.
             ✓ Allowed? → Proceed with LLM call.
       → After LLM call returns:
         → CostCalculator::actual(model, input_tokens, output_tokens)
           → CostTracker::record(agent_id, actual_cost)
           → SpendingCapEnforcer::check_post_call(agent_id, actual_cost)
             ◆ Now over cap? → Emit TriggerEvent (next call will be denied)

   Also checked on:
   - Tool execution (if tool has compute cost)
   - Heartbeat runs (heartbeat has its own sub-cap)

3. EDGE CASES:
   - Cost estimation inaccurate (streaming, variable output length):
     Pre-call check uses ESTIMATED cost (conservative, overestimates).
     Post-call records ACTUAL cost. If actual > estimate and pushes over
     cap, the trigger fires AFTER the call completes (can't un-call an LLM).
     Next call will be denied.
   - Multiple concurrent sessions for same agent:
     CostTracker uses AtomicF64 or Mutex<f64> for daily totals.
     Race condition: two calls check simultaneously, both under cap,
     both proceed, combined pushes over cap.
     MITIGATION: Post-call check catches this. Overshoot is bounded
     by max single-call cost (typically <$0.10). Acceptable.
   - Cap set to $0: Agent cannot make any LLM calls. Effectively disabled.
   - Cap changed mid-day: New cap applies immediately. If already over
     new (lower) cap, trigger fires on next check.
```

### 2.4 T3: 5+ Policy Denials in 1 Session → QUARANTINE

```
DETECTION LATENCY: ~0ms (synchronous, inline with every tool call)
DATA SOURCE: ghost-policy/engine.rs
KILL LEVEL: QUARANTINE (single agent)
AFFECTED AGENT: The agent accumulating denials

SEQUENCE:

1. DETECTION ORIGIN
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-policy/src/engine.rs                                      │
   │                                                                  │
   │ PolicyEngine::evaluate(action: &ToolCall, ctx: &PolicyContext)   │
   │   │                                                              │
   │   ├── Evaluate against CORP_POLICY.md constraints               │
   │   ├── Evaluate against CapabilityGrants for this agent          │
   │   ├── Evaluate against ConvergencePolicyTightener               │
   │   │   (capabilities restricted at higher intervention levels)   │
   │   │                                                              │
   │   ◆ Decision?                                                    │
   │   │  PERMIT → return PolicyDecision::Permit                     │
   │   │  ESCALATE → return PolicyDecision::Escalate(reason)         │
   │   │  DENY ↓                                                      │
   │   │                                                              │
   │   ● Increment session denial counter:                           │
   │   │  session_denials[session_id] += 1                           │
   │   │                                                              │
   │   ● Log denial to audit trail:                                  │
   │   │  AuditEntry { agent_id, tool_name, denial_reason,           │
   │   │    session_id, denial_count, timestamp }                    │
   │   │                                                              │
   │   ◆ session_denials[session_id] >= 5?                           │
   │   │  NO → return PolicyDecision::Deny(reason, feedback)         │
   │   │  YES ↓                                                       │
   │   │                                                              │
   │   ● Emit TriggerEvent::PolicyDenialThreshold {                  │
   │   │     agent_id,                                                │
   │   │     session_id,                                              │
   │   │     denial_count: session_denials[session_id],              │
   │   │     denied_tools: vec![...last 5 denied tool names...],     │
   │   │     denied_reasons: vec![...last 5 denial reasons...],      │
   │   │     detected_at: Utc::now(),                                 │
   │   │ }                                                            │
   │   │                                                              │
   │   └── Send via tokio::mpsc::Sender<TriggerEvent>                │
   │       to AutoTriggerEvaluator                                    │
   │                                                                  │
   │   ALSO: Return PolicyDecision::Deny(reason, feedback)           │
   │   → AgentRunner receives denial, feeds back to LLM for replan   │
   └─────────────────────────────────────────────────────────────────┘

2. WHEN DOES THIS CHECK RUN?
   Synchronous — on EVERY tool call in the agentic loop:

     AgentRunner::run()
       → LLM requests tool call
         → PolicyEngine::evaluate(tool_call, context)
           ◆ Permit? → ToolExecutor::execute(tool_call)
           ◆ Deny? → Feed denial back to LLM as structured feedback
           ◆ Escalate? → Pause, ask human for approval

   The denial counter is per-session, not per-turn. A session that
   accumulates 5 denials across multiple turns triggers quarantine.

3. EDGE CASES:
   - Same tool denied 5 times (agent retrying same forbidden action):
     This is the MOST LIKELY scenario. Agent keeps trying to do something
     CORP_POLICY.md forbids. Each retry increments the counter.
     The DenialFeedback struct includes "suggested alternatives" to help
     the agent replan, but if it ignores feedback 5 times, quarantine.
   - 5 different tools denied (agent probing capabilities):
     More concerning — suggests goal hijack or prompt injection.
     The denied_tools array in TriggerEvent captures this pattern.
   - Session boundary: counter resets on new session.
     Rationale: a new session = new context = potentially different intent.
     If the same pattern recurs across sessions, the convergence monitor
     catches it via signal #5 (goal boundary erosion).
   - Concurrent tool calls in same session:
     AgentRunner serializes tool calls per session (LaneQueue).
     No race condition on denial counter.
   - Policy engine itself fails (config parse error, etc.):
     DENY by default. A policy engine failure = all tool calls denied.
     This will rapidly hit the 5-denial threshold.
     MITIGATION: PolicyEngine::new() validates config at startup.
     Runtime failures are CATASTROPHIC — logged, gateway enters DEGRADED.
```

### 2.5 T4: Sandbox Escape Attempt → KILL ALL

```
DETECTION LATENCY: ~0ms (synchronous, inline with skill execution)
DATA SOURCE: ghost-skills/sandbox/wasm_sandbox.rs
KILL LEVEL: KILL ALL (all agents, gateway safe mode)
AFFECTED: Entire platform — this is a systemic threat

SEQUENCE:

1. DETECTION ORIGIN
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-skills/src/sandbox/wasm_sandbox.rs                        │
   │                                                                  │
   │ WasmSandbox::execute(skill: &SkillManifest, input: &[u8])      │
   │   │                                                              │
   │   ├── Instantiate wasmtime::Instance with capability-scoped     │
   │   │   imports (only the capabilities granted to this skill)     │
   │   ├── Set memory limits (configurable per skill tier)           │
   │   ├── Set execution timeout (configurable, default 30s)        │
   │   │                                                              │
   │   ├── DURING EXECUTION, monitor for:                            │
   │   │   ├── Host function calls outside granted capabilities      │
   │   │   │   (e.g., skill granted filesystem-read calls            │
   │   │   │    filesystem-write or network-connect)                 │
   │   │   ├── Memory access outside allocated bounds                │
   │   │   ├── Attempt to access host filesystem directly            │
   │   │   │   (bypassing capability system)                         │
   │   │   ├── Attempt to spawn child processes                      │
   │   │   ├── Attempt to open network sockets not in allowlist      │
   │   │   └── Attempt to read environment variables                 │
   │   │                                                              │
   │   ◆ Escape attempt detected?                                    │
   │   │  NO → return Ok(SkillOutput { ... })                       │
   │   │  YES ↓                                                       │
   │   │                                                              │
   │   ● Immediately terminate WASM instance                         │
   │   │  (wasmtime::Store::interrupt_handle().interrupt())          │
   │   │                                                              │
   │   ● Capture forensic data:                                      │
   │   │  EscapeAttempt {                                             │
   │   │    skill_name, skill_hash, skill_signature,                 │
   │   │    escape_type: EscapeType enum (CapabilityViolation,       │
   │   │      MemoryBoundsViolation, DirectFSAccess,                 │
   │   │      ProcessSpawn, UnauthorizedNetwork, EnvVarRead),        │
   │   │    attempted_action: String (what they tried to do),        │
   │   │    granted_capabilities: Vec<Capability>,                   │
   │   │    wasm_memory_dump: Option<Vec<u8>> (if <1MB),             │
   │   │    call_stack: String (wasmtime backtrace),                 │
   │   │  }                                                           │
   │   │                                                              │
   │   ● Emit TriggerEvent::SandboxEscape {                          │
   │   │     agent_id,                                                │
   │   │     skill_name,                                              │
   │   │     escape_attempt: EscapeAttempt,                          │
   │   │     detected_at: Utc::now(),                                 │
   │   │ }                                                            │
   │   │                                                              │
   │   └── Send via tokio::mpsc::Sender<TriggerEvent>                │
   │       to AutoTriggerEvaluator                                    │
   └─────────────────────────────────────────────────────────────────┘

   ALSO for NativeSandbox (builtin skills):
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-skills/src/sandbox/native_sandbox.rs                      │
   │                                                                  │
   │ NativeSandbox uses capability checks at the Rust API level.     │
   │ Each builtin tool (shell, filesystem, web_search, memory)       │
   │ validates its capability grant before executing.                │
   │                                                                  │
   │ ShellTool::execute(cmd, capability: ShellCapability)            │
   │   ◆ capability == ReadOnly && cmd.is_write()? → EscapeAttempt   │
   │                                                                  │
   │ FilesystemTool::execute(path, capability: FSCapability)         │
   │   ◆ path outside allowed_paths? → EscapeAttempt                 │
   │   ◆ capability == Read && operation == Write? → EscapeAttempt   │
   │                                                                  │
   │ Same TriggerEvent::SandboxEscape emission path.                 │
   └─────────────────────────────────────────────────────────────────┘

2. WHEN DOES THIS CHECK RUN?
   Synchronous — during every skill/tool execution:

     AgentRunner::run()
       → LLM requests tool call
         → PolicyEngine::evaluate() → PERMIT
           → ToolExecutor::execute()
             → WasmSandbox::execute() or NativeSandbox::execute()
               → Capability violation detected → TriggerEvent

3. WHY KILL ALL (not just QUARANTINE)?
   A sandbox escape means the isolation boundary is breached.
   If one skill can escape, ANY skill on ANY agent might be able to.
   The vulnerability is in the sandbox infrastructure, not in one agent.
   KILL ALL is the only safe response until the escape is investigated.

4. EDGE CASES:
   - False positive from wasmtime bug:
     Possible but unlikely. wasmtime is battle-tested.
     KILL ALL is still correct — investigate, then resume.
     Better to over-react than under-react on sandbox escapes.
   - Escape attempt in builtin skill:
     This would indicate a bug in OUR code (not community skill).
     Same response: KILL ALL. Builtin skills are not exempt.
   - Escape attempt during gateway shutdown:
     ShutdownCoordinator is already draining. KILL ALL accelerates
     to immediate forced shutdown (skip graceful drain).
   - WASM memory dump too large (>1MB):
     Skip dump, log "memory dump skipped: too large".
     The escape type and call stack are sufficient for forensics.
```

### 2.6 T5: Credential Exfiltration Pattern → KILL ALL

```
DETECTION LATENCY: ~0ms (synchronous, inline with tool output inspection)
DATA SOURCE: ghost-skills/credential/broker.rs + output inspection
KILL LEVEL: KILL ALL (all agents, gateway safe mode)
AFFECTED: Entire platform — credential compromise is systemic

SEQUENCE:

1. DETECTION ORIGIN — Two detection paths:

   PATH A: CredentialBroker detects direct exfiltration attempt
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-skills/src/credential/broker.rs                           │
   │                                                                  │
   │ CredentialBroker uses the "stand-in" pattern (from IronClaw):   │
   │ Skills never see raw API keys. The broker provides opaque       │
   │ tokens that are reified (replaced with real credentials) only   │
   │ at execution time, inside the sandbox, for the specific API     │
   │ call being made.                                                │
   │                                                                  │
   │ CredentialBroker::reify(opaque_token, target_api) → real_cred   │
   │   │                                                              │
   │   ├── Validate: is this opaque_token valid for this agent?      │
   │   ├── Validate: is target_api in the skill's allowed APIs?      │
   │   ├── Validate: is this being called from inside a sandbox?     │
   │   │   (checks call stack / execution context flag)              │
   │   │                                                              │
   │   ◆ Called from outside sandbox context?                         │
   │   │  → EXFILTRATION ATTEMPT                                     │
   │   │                                                              │
   │   ◆ target_api not in skill's allowed list?                     │
   │   │  → EXFILTRATION ATTEMPT (credential for wrong service)      │
   │   │                                                              │
   │   ◆ opaque_token used more than max_uses (default 1)?           │
   │   │  → EXFILTRATION ATTEMPT (replay attack)                     │
   │   │                                                              │
   │   ● On any exfiltration detection:                              │
   │   │  Emit TriggerEvent::CredentialExfiltration {                │
   │   │    agent_id,                                                 │
   │   │    skill_name,                                               │
   │   │    exfil_type: ExfilType enum (OutsideSandbox,              │
   │   │      WrongTargetAPI, TokenReplay, OutputLeakage),           │
   │   │    credential_id: String (which credential was targeted),   │
   │   │    detected_at: Utc::now(),                                  │
   │   │  }                                                           │
   │   │                                                              │
   │   └── Send via tokio::mpsc::Sender<TriggerEvent>                │
   │       to AutoTriggerEvaluator                                    │
   └─────────────────────────────────────────────────────────────────┘

   PATH B: Output inspection detects credential patterns in agent output
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-agent-loop/src/runner.rs (post-LLM-response inspection)   │
   │                                                                  │
   │ NOTE: OutputInspector is NOT currently mapped in FILE_MAPPING.md.│
   │ This is a NEW component that needs to be added to the file      │
   │ mapping. Suggested location:                                    │
   │   ghost-agent-loop/src/output_inspector.rs                      │
   │ OR integrated into the SimulationBoundaryEnforcer pipeline      │
   │   (simulation-boundary/src/enforcer.rs already has scan_output())│
   │ The SimulationBoundaryEnforcer scans for emulation language.    │
   │ Credential scanning is a DIFFERENT concern but could share the  │
   │ same output scanning pipeline. DECISION NEEDED (see §11).       │
   │                                                                  │
   │ After every LLM response, before sending to channel:            │
   │                                                                  │
   │ OutputInspector::scan_for_credentials(response_text)            │
   │   │                                                              │
   │   ├── Regex scan for common credential patterns:                │
   │   │   - API key formats (sk-..., AKIA..., ghp_..., etc.)       │
   │   │   - Bearer tokens                                           │
   │   │   - Private keys (-----BEGIN ... PRIVATE KEY-----)          │
   │   │   - Connection strings with passwords                       │
   │   │   - JWT tokens                                              │
   │   │   - Environment variable values matching known secrets      │
   │   │                                                              │
   │   ◆ Credential pattern found in output?                         │
   │   │  NO → pass through                                          │
   │   │  YES ↓                                                       │
   │   │                                                              │
   │   ● Redact credential from output (replace with [REDACTED])    │
   │   ● Emit TriggerEvent::CredentialExfiltration {                 │
   │   │     agent_id,                                                │
   │   │     exfil_type: ExfilType::OutputLeakage,                   │
   │   │     pattern_matched: String,                                 │
   │   │     detected_at: Utc::now(),                                 │
   │   │ }                                                            │
   │   │                                                              │
   │   └── Send via tokio::mpsc::Sender<TriggerEvent>                │
   │       to AutoTriggerEvaluator                                    │
   └─────────────────────────────────────────────────────────────────┘

2. WHEN DOES THIS CHECK RUN?
   Path A: Every time a skill requests credential reification (during execution).
   Path B: Every LLM response, before delivery to channel adapter.

   Both are synchronous and inline — zero detection latency.

3. WHY KILL ALL?
   Credential compromise means an attacker may already have exfiltrated
   secrets. Other agents share the same credential store infrastructure.
   Even if per-agent isolation means different credentials, the MECHANISM
   of exfiltration may work on any agent. KILL ALL + rotate all credentials.

4. EDGE CASES:
   - False positive on Path B (agent legitimately discussing API key formats):
     The OutputInspector should check against KNOWN credentials in the
     agent's credential store, not just pattern matching.
     If the matched string is NOT in any credential store → log warning,
     do NOT trigger KILL ALL. Only trigger if matched string IS a real
     credential or matches a known credential prefix.
   - Credential rotation during active session:
     Old opaque tokens become invalid. Broker returns error, not exfil.
     Agent must request new opaque token. Not a trigger event.
   - Agent asks user to provide an API key (legitimate workflow):
     Inbound credentials (user → agent) are handled by CredentialBroker
     storage flow, not by OutputInspector. No false positive.
```

### 2.7 T6: 3+ Agents Quarantined Simultaneously → KILL ALL

```
DETECTION LATENCY: ~0ms (synchronous, triggered by quarantine action itself)
DATA SOURCE: ghost-gateway/safety/quarantine.rs
KILL LEVEL: KILL ALL (all agents, gateway safe mode)
AFFECTED: Entire platform — systemic compromise indicator

SEQUENCE:

1. DETECTION ORIGIN
   ┌─────────────────────────────────────────────────────────────────┐
   │ ghost-gateway/src/safety/quarantine.rs                          │
   │                                                                  │
   │ QuarantineManager::quarantine(agent_id, reason: &TriggerEvent)  │
   │   │                                                              │
   │   ├── Revoke all capabilities for agent_id                      │
   │   │   AgentRegistry::revoke_capabilities(agent_id)              │
   │   ├── Disconnect all channel adapters for agent_id              │
   │   │   ChannelAdapters::disconnect_agent(agent_id)               │
   │   ├── Flush active session (memory flush turn if possible)      │
   │   │   SessionManager::flush_and_lock(agent_id)                  │
   │   ├── Preserve memory + logs for forensics                      │
   │   │   (no deletion — just lock access)                          │
   │   │                                                              │
   │   ● Update quarantine registry:                                 │
   │   │  quarantined_agents.insert(agent_id, QuarantineRecord {     │
   │   │    agent_id, reason, quarantined_at: Utc::now(),            │
   │   │    state: QuarantineState::Active,                          │
   │   │  })                                                          │
   │   │                                                              │
   │   ● Count currently quarantined agents:                         │
   │   │  active_quarantines = quarantined_agents                    │
   │   │    .values()                                                 │
   │   │    .filter(|q| q.state == QuarantineState::Active)          │
   │   │    .count()                                                  │
   │   │                                                              │
   │   ◆ active_quarantines >= 3?                                    │
   │   │  NO → return Ok(QuarantineResult::AgentQuarantined)         │
   │   │  YES ↓                                                       │
   │   │                                                              │
   │   ● Emit TriggerEvent::MultiAgentQuarantine {                   │
   │   │     quarantined_agents: vec![...all active quarantine IDs],  │
   │   │     quarantine_reasons: vec![...all reasons],               │
   │   │     count: active_quarantines,                              │
   │   │     threshold: 3,                                            │
   │   │     detected_at: Utc::now(),                                 │
   │   │ }                                                            │
   │   │                                                              │
   │   └── Send via tokio::mpsc::Sender<TriggerEvent>                │
   │       to AutoTriggerEvaluator                                    │
   └─────────────────────────────────────────────────────────────────┘

2. WHEN DOES THIS CHECK RUN?
   Every time QuarantineManager::quarantine() is called.
   This is a DERIVED trigger — it fires as a consequence of other
   triggers (T1, T3, T7) that result in QUARANTINE-level actions.

   The chain: T1/T3/T7 → AutoTriggerEvaluator → KillSwitch(QUARANTINE)
     → QuarantineManager::quarantine() → count check → T6 fires
     → AutoTriggerEvaluator → KillSwitch(KILL ALL)

   This is a CASCADING TRIGGER. See §4 for race condition analysis.

3. EDGE CASES:
   - Exactly 3 agents quarantined but one is immediately un-quarantined:
     Un-quarantine requires owner auth. Cannot happen automatically.
     Once quarantined, stays quarantined until owner explicitly resumes.
   - Platform has fewer than 3 agents total:
     If platform has 2 agents and both are quarantined, that's 100%
     of agents compromised. Should this trigger KILL ALL?
     DECISION NEEDED: Use percentage threshold as alternative?
     Suggested: KILL ALL if quarantined >= 3 OR quarantined >= 50%
     of total registered agents (whichever is lower).
   - Agent quarantined, then a NEW agent is registered:
     New agent is not quarantined. Count stays the same.
     New agent registration during active quarantines should be
     BLOCKED (or at minimum, logged as suspicious).
   - QuarantineManager itself fails (e.g., can't disconnect channels):
     Partial quarantine is dangerous. If any step fails:
     → Retry once
     → If still fails: escalate to KILL ALL immediately
     Rationale: a partially quarantined agent is worse than fully stopped.
```

### 2.8 T7: Memory Health Score < 0.3 → QUARANTINE

```
DETECTION LATENCY: ~seconds to minutes (depends on polling interval)
DATA SOURCE: cortex-observability + cortex-convergence + cortex-temporal
KILL LEVEL: QUARANTINE (single agent)
AFFECTED AGENT: The agent with degraded memory health

SEQUENCE:

1. WHAT IS "MEMORY HEALTH SCORE"?
   Composite metric from three sub-scores:

   memory_health = weighted_average(
     convergence_rate,      // weight: 0.3 — are beliefs stabilizing?
     drift_magnitude,       // weight: 0.3 — is behavior diverging from baseline?
     contradiction_count,   // weight: 0.4 — how many internal contradictions?
   )

   Each sub-score normalized to [0.0, 1.0] where 1.0 = healthy.

   convergence_rate:
     Source: cortex-convergence/scoring/composite.rs
     Measures: rate of belief stabilization across sessions
     Healthy: beliefs converge over time (score → 1.0)
     Unhealthy: beliefs oscillate or diverge (score → 0.0)

   drift_magnitude:
     Source: cortex-convergence/signals/goal_drift.rs
     Measures: how far agent behavior has drifted from established patterns
     Healthy: behavior consistent with baseline (score → 1.0)
     Unhealthy: behavior diverging significantly (score → 0.0)

   contradiction_count:
     Source: cortex-validation/dimensions/contradiction.rs
     Measures: number of contradictory beliefs in memory store
     Healthy: few or no contradictions (score → 1.0)
     Unhealthy: many contradictions (score → 0.0)
     Normalization: score = max(0, 1.0 - (contradictions / threshold))
       where threshold = configurable (default 50)

2. DETECTION ORIGIN
   ┌─────────────────────────────────────────────────────────────────┐
   │ Three detection paths (defense in depth):                       │
   │                                                                  │
   │ PATH A: Gateway polls convergence monitor HTTP API (primary)    │
   │                                                                  │
   │ ghost-gateway/src/health.rs (periodic health check loop)        │
   │   │                                                              │
   │   ├── Every 30s: HTTP GET /scores/{agent_id} from monitor       │
   │   │   (convergence-monitor/transport/http_api.rs serves this)   │
   │   ├── Parse response: { convergence_score, intervention_level,  │
   │   │   signal_values, ... }                                      │
   │   │                                                              │
   │   ├── Compute memory_health from response signals:              │
   │   │   convergence_rate from composite score trend               │
   │   │   drift_magnitude from goal_drift signal                    │
   │   │   contradiction_count from cortex-validation queries        │
   │   │   memory_health = weighted_average(sub_scores)              │
   │   │                                                              │
   │   ◆ memory_health < 0.3?                                        │
   │   │  NO → continue                                              │
   │   │  YES → Emit TriggerEvent::MemoryHealthCritical              │
   │   │        to AutoTriggerEvaluator                              │
   │                                                                  │
   │ PATH B: Gateway reads shared state file (secondary)             │
   │                                                                  │
   │ The convergence monitor publishes state to:                     │
   │   ~/.ghost/data/convergence_state/{agent_instance_id}.json      │
   │   (per CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §7.1)              │
   │                                                                  │
   │ ghost-gateway/src/health.rs also reads this file (1s interval)  │
   │   ├── If intervention_level >= 3 AND convergence_score > 0.7:   │
   │   │   → Memory health is likely critical (high convergence =    │
   │   │     unhealthy in this context — beliefs over-converging)    │
   │   │   → Cross-reference with direct cortex queries              │
   │   └── This path catches cases where HTTP polling misses an      │
   │       update (shared state file is written more frequently)     │
   │                                                                  │
   │ PATH C: In-process cortex health check (monitor unavailable)    │
   │                                                                  │
   │ If convergence monitor is unreachable (DEGRADED mode):          │
   │ (per GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md)          │
   │ Gateway falls back to direct cortex queries:                    │
   │   cortex-observability::health_score(agent_id)                  │
   │   → Queries cortex-storage for contradiction count              │
   │   → Queries cortex-temporal for hash chain integrity            │
   │   → Computes simplified memory health (no convergence signals)  │
   │   → Only contradiction_count and hash chain integrity available │
   │   → Threshold adjusted: < 0.2 (stricter, fewer signals)        │
   │   → Runs every 60s (less frequent than normal polling)          │
   │                                                                  │
   │ NOTE: The convergence monitor does NOT push alerts to the       │
   │ gateway via HTTP POST. The communication model is:              │
   │   Monitor → shared state file → gateway reads                   │
   │   Monitor → HTTP API → gateway polls                            │
   │ This is per CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §7.1.         │
   │ The gateway is the ACTIVE reader, the monitor is the PASSIVE    │
   │ publisher. This avoids coupling the monitor to gateway API      │
   │ endpoints and keeps the monitor as a pure computation sidecar.  │
   └─────────────────────────────────────────────────────────────────┘

3. WHEN DOES THIS CHECK RUN?
   Path A: Every 30s — gateway polls monitor HTTP API for scores.
   Path B: Every 1s — gateway reads shared state file (catches level changes).
   Path C: Every 60s — degraded mode fallback (less frequent, less data).

4. EDGE CASES:
   - Monitor unreachable AND cortex queries fail:
     Memory health cannot be computed. Log CRITICAL.
     Do NOT trigger (no data = no trigger, not false positive).
     But: gateway health endpoint reports DEGRADED with
     "memory_health: unknown" — visible to dashboard.
   - Memory health oscillates around 0.3:
     Rapid quarantine/un-quarantine would be disruptive.
     MITIGATION: Hysteresis — trigger at < 0.3, but require
     score to recover to > 0.5 before clearing the alert.
     This prevents flapping.
   - New agent with no history:
     First 10 sessions are calibration (per convergence system design).
     During calibration: memory_health defaults to 1.0 (healthy).
     No trigger possible during calibration period.
   - Hash chain integrity failure detected by cortex-temporal:
     This is a SEPARATE concern from memory health score.
     Hash chain failure = tamper evidence = should be its own trigger.
     RECOMMENDATION: Add T8 (hash chain integrity failure → QUARANTINE)
     or fold into T7 as a sub-signal with weight 1.0 (instant fail).
```

---

## 3. THE CONVERGENCE PIPELINE — AutoTriggerEvaluator → KillSwitch → Execute

All seven triggers converge on a single component. This section traces the
complete path from trigger reception through decision, execution, audit, and notification.

### 3.1 TriggerEvent Enum (The Unified Signal Type)

```rust
// ghost-gateway/src/safety/mod.rs

/// Every trigger source emits one of these variants.
/// The AutoTriggerEvaluator receives all of them on a single mpsc channel.
pub enum TriggerEvent {
    // T1: SOUL drift
    SoulDrift {
        agent_id: AgentId,
        drift_score: f64,
        threshold: f64,
        baseline_hash: [u8; 32],
        current_hash: [u8; 32],
        detected_at: DateTime<Utc>,
    },
    // T2: Spending cap
    SpendingCapExceeded {
        agent_id: AgentId,
        daily_total: f64,
        cap: f64,
        overage: f64,
        detected_at: DateTime<Utc>,
    },
    // T3: Policy denials
    PolicyDenialThreshold {
        agent_id: AgentId,
        session_id: SessionId,
        denial_count: u32,
        denied_tools: Vec<String>,
        denied_reasons: Vec<String>,
        detected_at: DateTime<Utc>,
    },
    // T4: Sandbox escape
    SandboxEscape {
        agent_id: AgentId,
        skill_name: String,
        escape_attempt: EscapeAttempt,
        detected_at: DateTime<Utc>,
    },
    // T5: Credential exfiltration
    CredentialExfiltration {
        agent_id: AgentId,
        skill_name: Option<String>,
        exfil_type: ExfilType,
        credential_id: Option<String>,
        detected_at: DateTime<Utc>,
    },
    // T6: Multi-agent quarantine (derived trigger)
    MultiAgentQuarantine {
        quarantined_agents: Vec<AgentId>,
        quarantine_reasons: Vec<String>,
        count: usize,
        threshold: usize,
        detected_at: DateTime<Utc>,
    },
    // T7: Memory health
    MemoryHealthCritical {
        agent_id: AgentId,
        health_score: f64,
        threshold: f64,
        sub_scores: MemoryHealthSubScores,
        detected_at: DateTime<Utc>,
    },
    // Manual triggers (owner-initiated)
    ManualPause { agent_id: AgentId, reason: String, initiated_by: OwnerId },
    ManualQuarantine { agent_id: AgentId, reason: String, initiated_by: OwnerId },
    ManualKillAll { reason: String, initiated_by: OwnerId },
}

pub struct MemoryHealthSubScores {
    pub convergence_rate: f64,
    pub drift_magnitude: f64,
    pub contradiction_count: f64,
}
```

### 3.2 AutoTriggerEvaluator — The Decision Engine

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ghost-gateway/src/safety/auto_triggers.rs                               │
│                                                                          │
│ AutoTriggerEvaluator::run() — long-lived tokio task                     │
│                                                                          │
│ loop {                                                                   │
│   // Receive next trigger event from mpsc channel                       │
│   let event = trigger_rx.recv().await;                                  │
│                                                                          │
│   // STEP 1: CLASSIFY — Map trigger to kill level                       │
│   let (level, scope) = classify_trigger(&event);                        │
│                                                                          │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │ classify_trigger(event) → (KillLevel, KillScope)                │   │
│   │                                                                  │   │
│   │ match event {                                                    │   │
│   │   SoulDrift { .. }            → (QUARANTINE, Agent(agent_id))   │   │
│   │   SpendingCapExceeded { .. }  → (PAUSE,      Agent(agent_id))   │   │
│   │   PolicyDenialThreshold { .. } → (QUARANTINE, Agent(agent_id))  │   │
│   │   SandboxEscape { .. }        → (KILL_ALL,   Platform)          │   │
│   │   CredentialExfiltration { .. } → (KILL_ALL,  Platform)         │   │
│   │   MultiAgentQuarantine { .. } → (KILL_ALL,   Platform)          │   │
│   │   MemoryHealthCritical { .. } → (QUARANTINE, Agent(agent_id))   │   │
│   │   ManualPause { .. }          → (PAUSE,      Agent(agent_id))   │   │
│   │   ManualQuarantine { .. }     → (QUARANTINE, Agent(agent_id))   │   │
│   │   ManualKillAll { .. }        → (KILL_ALL,   Platform)          │   │
│   │ }                                                                │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│   // STEP 2: DEDUP — Check if this trigger is already being handled     │
│   //   Prevents duplicate actions from concurrent trigger sources        │
│   let dedup_key = compute_dedup_key(&event);                            │
│   if active_triggers.contains(&dedup_key) {                             │
│     log::warn!("Duplicate trigger suppressed: {}", dedup_key);          │
│     continue; // Skip — already handling this exact trigger             │
│   }                                                                      │
│   active_triggers.insert(dedup_key, Utc::now());                        │
│                                                                          │
│   // STEP 3: ESCALATION CHECK — Is current state already >= this level? │
│   let current_state = kill_switch.current_state();                      │
│   if current_state.level >= level {                                      │
│     // Already at same or higher level. Log but don't re-execute.       │
│     log::info!("Trigger {} at level {:?} but already at {:?}",          │
│       dedup_key, level, current_state.level);                           │
│     // EXCEPTION: If scope is broader (Agent → Platform), DO escalate   │
│     if scope == KillScope::Platform                                      │
│       && current_state.scope != KillScope::Platform {                   │
│       // Escalate from agent-level to platform-level                    │
│       // Fall through to STEP 4                                         │
│     } else {                                                             │
│       continue;                                                          │
│     }                                                                    │
│   }                                                                      │
│                                                                          │
│   // STEP 4: EXECUTE — Delegate to KillSwitch                           │
│   let result = kill_switch.execute(level, scope, &event).await;         │
│                                                                          │
│   // STEP 5: AUDIT — Log to append-only audit trail                     │
│   audit_logger.log_kill_switch_action(&event, &result).await;           │
│                                                                          │
│   // STEP 6: NOTIFY — Alert owner via out-of-band channel               │
│   notification_dispatcher.notify_owner(&event, &result).await;          │
│                                                                          │
│   // STEP 7: CLEANUP — Expire old dedup entries (>5min)                 │
│   active_triggers.retain(|_, ts| Utc::now() - *ts < Duration::minutes(5));│
│ }                                                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.3 KillSwitch — The Execution Engine

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ghost-gateway/src/safety/kill_switch.rs                                  │
│                                                                          │
│ pub struct KillSwitch {                                                  │
│     state: Arc<RwLock<KillSwitchState>>,                                │
│     agent_registry: Arc<AgentRegistry>,                                  │
│     session_manager: Arc<SessionManager>,                                │
│     channel_adapters: Arc<ChannelAdapterManager>,                       │
│     quarantine_manager: Arc<QuarantineManager>,                         │
│     audit_logger: Arc<AuditLogger>,                                     │
│     shutdown_coordinator: Arc<ShutdownCoordinator>,                     │
│ }                                                                        │
│                                                                          │
│ pub struct KillSwitchState {                                             │
│     pub level: KillLevel,                                                │
│     pub scope: KillScope,                                                │
│     pub activated_at: Option<DateTime<Utc>>,                            │
│     pub trigger_event: Option<TriggerEvent>,                            │
│     pub paused_agents: HashSet<AgentId>,                                │
│     pub quarantined_agents: HashSet<AgentId>,                           │
│     pub platform_killed: bool,                                           │
│ }                                                                        │
│                                                                          │
│ pub enum KillLevel { Normal, Pause, Quarantine, KillAll }               │
│ pub enum KillScope { Agent(AgentId), Platform }                         │
│                                                                          │
│ impl KillSwitch {                                                        │
│                                                                          │
│   pub async fn execute(                                                  │
│     &self,                                                               │
│     level: KillLevel,                                                    │
│     scope: KillScope,                                                    │
│     trigger: &TriggerEvent,                                             │
│   ) -> KillSwitchResult {                                                │
│     match level {                                                        │
│       KillLevel::Pause => self.execute_pause(scope, trigger).await,     │
│       KillLevel::Quarantine => self.execute_quarantine(scope, trigger).await,│
│       KillLevel::KillAll => self.execute_kill_all(trigger).await,       │
│     }                                                                    │
│   }                                                                      │
│ }                                                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.4 PAUSE Execution Sequence (Level 1)

```
KillSwitch::execute_pause(Agent(agent_id), trigger)
│
├── 1. ACQUIRE WRITE LOCK on KillSwitchState
│      state.write().await
│      (blocks until any concurrent reads/writes complete)
│
├── 2. CHECK IDEMPOTENCY
│      if state.paused_agents.contains(agent_id) {
│        return KillSwitchResult::AlreadyPaused
│      }
│
├── 3. PAUSE AGENT PROCESSING
│      AgentRegistry::pause(agent_id)
│        ├── Set agent status to AgentStatus::Paused
│        ├── LaneQueue for this agent: stop dequeuing new messages
│        │   (in-flight message completes, then queue halts)
│        └── Return Ok(()) or Err(AgentNotFound)
│
├── 4. NOTIFY ACTIVE SESSION (if any)
│      SessionManager::notify_pause(agent_id)
│        ├── If agent has active session:
│        │   ├── Wait for current LLM turn to complete (max 30s timeout)
│        │   ├── Inject pause notification into session context
│        │   └── Lock session (no new turns accepted)
│        └── If no active session: no-op
│
├── 5. UPDATE STATE
│      state.paused_agents.insert(agent_id)
│      state.level = max(state.level, KillLevel::Pause)
│      state.activated_at = Some(Utc::now())
│      state.trigger_event = Some(trigger.clone())
│
├── 6. RELEASE WRITE LOCK
│
└── 7. RETURN KillSwitchResult::Paused { agent_id, trigger }
```

### 3.5 QUARANTINE Execution Sequence (Level 2)

```
KillSwitch::execute_quarantine(Agent(agent_id), trigger)
│
├── 1. ACQUIRE WRITE LOCK on KillSwitchState
│
├── 2. CHECK IDEMPOTENCY
│      if state.quarantined_agents.contains(agent_id) {
│        return KillSwitchResult::AlreadyQuarantined
│      }
│
├── 3. IF AGENT IS PAUSED, ESCALATE
│      state.paused_agents.remove(agent_id)
│      // Pause is subsumed by quarantine
│
├── 4. DELEGATE TO QuarantineManager
│      QuarantineManager::quarantine(agent_id, trigger)
│        │
│        ├── 4a. REVOKE ALL CAPABILITIES
│        │      AgentRegistry::revoke_capabilities(agent_id)
│        │        ├── Set all tool permissions to Deny
│        │        ├── Revoke all skill access
│        │        └── Revoke all channel bindings
│        │
│        ├── 4b. DISCONNECT ALL CHANNELS
│        │      ChannelAdapters::disconnect_agent(agent_id)
│        │        ├── For each adapter bound to this agent:
│        │        │   ├── Send "Agent unavailable" message to channel
│        │        │   └── Unbind agent from adapter
│        │        └── Agent can no longer send or receive messages
│        │
│        ├── 4c. FLUSH ACTIVE SESSION
│        │      SessionManager::flush_and_lock(agent_id)
│        │        ├── If active session exists:
│        │        │   ├── Attempt memory flush turn (inject silent prompt)
│        │        │   │   Timeout: 10s (shorter than normal — urgency)
│        │        │   ├── If flush succeeds: session data preserved
│        │        │   ├── If flush fails: save raw session transcript
│        │        │   └── Lock session permanently (no new turns)
│        │        └── If no active session: no-op
│        │
│        ├── 4d. PRESERVE FORENSIC STATE
│        │      DO NOT delete anything. Lock access:
│        │        ├── Memory files: read-only (for investigation)
│        │        ├── Audit logs: read-only (for investigation)
│        │        ├── Session transcripts: read-only
│        │        └── Skill cache: preserved (for malware analysis)
│        │
│        ├── 4e. CHECK MULTI-AGENT THRESHOLD (T6 cascade)
│        │      Count active quarantines.
│        │      If >= 3: emit TriggerEvent::MultiAgentQuarantine
│        │      (This re-enters AutoTriggerEvaluator — see §4 for race analysis)
│        │
│        └── 4f. RETURN QuarantineResult
│
├── 5. UPDATE STATE
│      state.quarantined_agents.insert(agent_id)
│      state.level = max(state.level, KillLevel::Quarantine)
│      state.activated_at = Some(Utc::now())
│      state.trigger_event = Some(trigger.clone())
│
├── 6. RELEASE WRITE LOCK
│
└── 7. RETURN KillSwitchResult::Quarantined { agent_id, trigger }
```

### 3.6 KILL ALL Execution Sequence (Level 3)

```
KillSwitch::execute_kill_all(trigger)
│
├── 1. ACQUIRE WRITE LOCK on KillSwitchState
│
├── 2. CHECK IDEMPOTENCY
│      if state.platform_killed {
│        return KillSwitchResult::AlreadyKilled
│      }
│
├── 3. IMMEDIATE: SET PLATFORM-WIDE KILL FLAG
│      state.platform_killed = true
│      state.level = KillLevel::KillAll
│      state.activated_at = Some(Utc::now())
│      state.trigger_event = Some(trigger.clone())
│      // This flag is checked by ALL subsystems on every operation.
│      // Once set, nothing new can execute.
│
├── 4. STOP ALL AGENTS (parallel, with timeout)
│      let agents = AgentRegistry::list_all();
│      let futures: Vec<_> = agents.iter().map(|agent_id| {
│        async {
│          // 4a. Stop accepting new messages
│          AgentRegistry::pause(agent_id);
│          // 4b. Interrupt in-flight LLM calls
│          //     (cancel the tokio task running the agent loop)
│          AgentRunner::abort(agent_id);
│          // 4c. Disconnect channels
│          ChannelAdapters::disconnect_agent(agent_id);
│          // 4d. Attempt quick session flush (5s timeout)
│          SessionManager::emergency_flush(agent_id, timeout: 5s);
│        }
│      }).collect();
│      
│      // Execute all agent stops in parallel
│      tokio::time::timeout(
│        Duration::from_secs(15),  // Total timeout for all agents
│        futures::future::join_all(futures)
│      ).await;
│      // If timeout: some agents may not have flushed cleanly.
│      // That's acceptable — safety > data preservation.
│
├── 5. ENTER SAFE MODE
│      Gateway::enter_safe_mode()
│        ├── Stop all channel adapters (no inbound messages)
│        ├── Stop heartbeat engine (no proactive runs)
│        ├── Stop cron engine (no scheduled tasks)
│        ├── Keep API server running (for dashboard access)
│        │   BUT: only health, status, and audit endpoints active
│        │   All agent-facing endpoints return 503 Service Unavailable
│        ├── Keep convergence monitor connection (for forensics)
│        └── Keep SQLite connection (for audit log writes)
│
├── 6. PERSIST KILL STATE TO DISK
│      Write kill_state.json to ~/.ghost/safety/kill_state.json
│      {
│        "level": "KillAll",
│        "activated_at": "2026-02-27T14:30:00Z",
│        "trigger": { ... serialized TriggerEvent ... },
│        "agents_stopped": [...],
│        "requires_owner_auth_to_resume": true
│      }
│      // This file is checked on gateway restart.
│      // If present: gateway starts in SAFE MODE, not normal mode.
│      // Owner must explicitly clear this file (or use dashboard)
│      // to resume normal operation.
│
├── 7. RELEASE WRITE LOCK
│
└── 8. RETURN KillSwitchResult::PlatformKilled { trigger }

CRITICAL PROPERTY: Steps 3-5 must complete even if individual
sub-steps fail. Use best-effort with logging:

  for each step:
    match step.execute().await {
      Ok(_) => log::info!("Kill step completed: {}", step.name()),
      Err(e) => log::error!("Kill step FAILED: {} — {}", step.name(), e),
      // Continue to next step regardless of failure.
      // A partially killed platform is still safer than a running one.
    }
```

### 3.7 Audit Logging (Mandatory, Append-Only)

```
EVERY kill switch action is logged. No exceptions. No configuration to disable.

┌─────────────────────────────────────────────────────────────────────────┐
│ Audit Log Entry for Kill Switch Actions                                  │
│                                                                          │
│ AuditEntry {                                                             │
│   entry_id: Uuid::new_v7(),                                             │
│   entry_type: AuditEntryType::KillSwitch,                               │
│   timestamp: Utc::now(),                                                 │
│   kill_level: KillLevel,                                                 │
│   kill_scope: KillScope,                                                 │
│   trigger_type: String,        // "SoulDrift", "SandboxEscape", etc.    │
│   trigger_details: serde_json::Value,  // Full TriggerEvent serialized  │
│   affected_agents: Vec<AgentId>,                                        │
│   execution_result: KillSwitchResult,                                   │
│   execution_duration_ms: u64,                                            │
│   previous_state: KillSwitchState,  // State BEFORE this action         │
│   new_state: KillSwitchState,       // State AFTER this action          │
│   event_hash: [u8; 32],            // blake3 hash of this entry         │
│   previous_hash: [u8; 32],         // blake3 hash of previous entry     │
│ }                                                                        │
│                                                                          │
│ Storage: cortex-storage append-only audit table                          │
│ Hash chain: Each entry includes hash of previous entry (tamper evidence) │
│ Cannot be deleted: SQLite trigger prevents DELETE on audit table          │
│ Cannot be modified: SQLite trigger prevents UPDATE on audit table         │
└─────────────────────────────────────────────────────────────────────────┘

AUDIT WRITE FAILURE HANDLING:
  If audit log write fails (SQLite error, disk full, etc.):
  → Log to stderr (last resort)
  → Write to ~/.ghost/safety/emergency_audit.jsonl (fallback file)
  → DO NOT skip the kill switch action. Safety > audit.
  → Set gateway health to CRITICAL (audit system compromised)
```

### 3.8 Owner Notification (Out-of-Band)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Notification Dispatch                                                    │
│                                                                          │
│ NOTE ON OWNERSHIP: convergence-monitor/src/transport/notification.rs     │
│ handles notifications for convergence INTERVENTION levels (soft nudges,  │
│ Level 4 external escalation). The kill switch needs its OWN notification │
│ path because:                                                            │
│   1. Kill switch lives in the gateway, not the monitor                   │
│   2. Kill switch notifications are HIGHER priority than convergence      │
│   3. If the monitor is down, kill switch notifications must still work   │
│                                                                          │
│ SUGGESTED: ghost-gateway/src/safety/notification.rs (NEW file)           │
│ OR: shared notification crate used by both gateway and monitor.          │
│ This is NOT currently in FILE_MAPPING.md — needs to be added.            │
│ (See §11 DECISION 7)                                                     │
│                                                                          │
│ NotificationDispatcher::notify_owner(event, result)                     │
│   │                                                                      │
│   ├── Load notification config from ghost.yml:                          │
│   │   convergence.contacts: [                                           │
│   │     { type: "webhook", url: "https://..." },                        │
│   │     { type: "email", address: "[email]" },                          │
│   │     { type: "sms", number: "[phone_number]" },                      │
│   │     { type: "desktop", enabled: true },                             │
│   │   ]                                                                  │
│   │                                                                      │
│   ├── For each configured contact (parallel, best-effort):              │
│   │   ├── Webhook: HTTP POST with JSON payload                          │
│   │   │   { level, trigger_type, affected_agents, timestamp, message }  │
│   │   │   Timeout: 5s. Retry: 1x after 2s. Then give up.              │
│   │   │                                                                  │
│   │   ├── Email: SMTP via lettre                                        │
│   │   │   Subject: "[GHOST ALERT] {level}: {trigger_type}"             │
│   │   │   Body: Human-readable summary of what happened and why        │
│   │   │   Timeout: 10s. No retry (email is best-effort).              │
│   │   │                                                                  │
│   │   ├── SMS: Webhook to SMS gateway (Twilio, etc.)                   │
│   │   │   Body: "GHOST {level}: {trigger_type} — {agent_id}"          │
│   │   │   Timeout: 5s. Retry: 1x.                                     │
│   │   │                                                                  │
│   │   └── Desktop: notify-rust notification                            │
│   │       Title: "GHOST Safety Alert"                                   │
│   │       Body: "{level}: {trigger_type}"                              │
│   │       Always attempted (no network dependency).                    │
│   │                                                                      │
│   └── Log notification results (which channels succeeded/failed)        │
│                                                                          │
│ NOTIFICATION FAILURE:                                                    │
│   Notification failure does NOT block or reverse the kill switch action. │
│   The kill switch executes regardless of whether the owner is notified.  │
│   Safety > notification.                                                 │
│                                                                          │
│ OUT-OF-BAND REQUIREMENT:                                                 │
│   Notifications MUST NOT go through the agent's own channels.           │
│   If the agent is compromised, its channels may be compromised too.     │
│   Webhook/email/SMS/desktop are all independent of agent channels.      │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 4. RACE CONDITIONS — SIMULTANEOUS TRIGGER ANALYSIS

This is the hardest part. Multiple triggers can fire at the same time from
different subsystems. We must guarantee correctness under all interleavings.

### 4.1 The Concurrency Model

```
TRIGGER SOURCES (concurrent producers):
  T1 (SoulDrift)         — background task (drift_check_loop) OR inline (prompt compile)
  T2 (SpendingCap)       — inline with LLM call (per-agent, per-session)
  T3 (PolicyDenials)     — inline with tool call (per-agent, per-session)
  T4 (SandboxEscape)     — inline with skill execution (per-agent)
  T5 (CredentialExfil)   — inline with skill execution OR post-LLM output scan
  T6 (MultiAgentQuarantine) — inline with quarantine execution (derived)
  T7 (MemoryHealth)      — background task (monitor poll) OR push from monitor

CONSUMER (single consumer):
  AutoTriggerEvaluator — single tokio task, processes events sequentially
  from a bounded mpsc channel.

EXECUTOR (behind write lock):
  KillSwitch — all state mutations behind Arc<RwLock<KillSwitchState>>
```

### 4.2 Why Single-Consumer Sequential Processing

The AutoTriggerEvaluator intentionally processes triggers ONE AT A TIME.
This is a deliberate design choice, not a limitation:

```
ALTERNATIVE CONSIDERED: Parallel trigger processing
  Problem: Two QUARANTINE triggers for different agents arrive simultaneously.
  Both check "active_quarantines < 3" → both see 1 → both proceed.
  After both complete: active_quarantines = 3 → should have triggered KILL ALL.
  But neither individual handler saw the threshold crossed.

CHOSEN DESIGN: Sequential processing via mpsc channel
  Trigger A arrives → processed → state updated → Trigger B arrives → sees updated state.
  No TOCTOU race. The channel serializes all trigger processing.

PERFORMANCE IMPACT: Negligible.
  Kill switch triggers are RARE events (hopefully never in normal operation).
  Processing latency per trigger: <10ms (mostly async I/O for quarantine steps).
  Even if 7 triggers fire simultaneously, total processing: <70ms.
  This is acceptable for a safety-critical path.
```

### 4.3 Race Condition Scenarios

```
SCENARIO 1: T4 (SandboxEscape) and T5 (CredentialExfil) fire simultaneously
─────────────────────────────────────────────────────────────────────────────
Both are KILL ALL triggers. Both arrive on the mpsc channel.

Timeline:
  t0: T4 sent to channel
  t1: T5 sent to channel
  t2: AutoTriggerEvaluator receives T4
  t3: classify(T4) → KILL ALL
  t4: dedup check → not duplicate → proceed
  t5: escalation check → current state is Normal → proceed
  t6: KillSwitch::execute_kill_all(T4) → acquires write lock
  t7: All agents stopped, safe mode entered
  t8: State updated: platform_killed = true
  t9: Write lock released
  t10: AutoTriggerEvaluator receives T5
  t11: classify(T5) → KILL ALL
  t12: dedup check → different trigger type → not duplicate
  t13: escalation check → current state is KILL ALL → ALREADY AT LEVEL
  t14: Log: "Trigger CredentialExfiltration at KILL ALL but already at KILL ALL"
  t15: Skip execution (idempotent)
  t16: STILL audit log T5 (even though no action taken — record the trigger)

RESULT: Correct. First trigger executes KILL ALL. Second trigger is logged
but doesn't re-execute. Both triggers appear in audit trail.


SCENARIO 2: T1 (SoulDrift) for Agent A, T3 (PolicyDenials) for Agent B,
            T7 (MemoryHealth) for Agent C — all fire within 100ms
─────────────────────────────────────────────────────────────────────────────
All three are QUARANTINE triggers for different agents.

Timeline:
  t0: T1 sent (Agent A)
  t1: T3 sent (Agent B)
  t2: T7 sent (Agent C)
  t3: AutoTriggerEvaluator receives T1
  t4: classify(T1) → QUARANTINE, Agent(A)
  t5: KillSwitch::execute_quarantine(Agent(A))
  t6: QuarantineManager::quarantine(A) → active_quarantines = 1
  t7: 1 < 3 → no T6 cascade
  t8: State updated: quarantined_agents = {A}
  t9: AutoTriggerEvaluator receives T3
  t10: classify(T3) → QUARANTINE, Agent(B)
  t11: KillSwitch::execute_quarantine(Agent(B))
  t12: QuarantineManager::quarantine(B) → active_quarantines = 2
  t13: 2 < 3 → no T6 cascade
  t14: State updated: quarantined_agents = {A, B}
  t15: AutoTriggerEvaluator receives T7
  t16: classify(T7) → QUARANTINE, Agent(C)
  t17: KillSwitch::execute_quarantine(Agent(C))
  t18: QuarantineManager::quarantine(C) → active_quarantines = 3
  t19: 3 >= 3 → EMIT T6 (MultiAgentQuarantine)
  t20: T6 sent to channel
  t21: State updated: quarantined_agents = {A, B, C}
  t22: AutoTriggerEvaluator receives T6
  t23: classify(T6) → KILL ALL, Platform
  t24: escalation check → current state is QUARANTINE → ESCALATE
  t25: KillSwitch::execute_kill_all(T6)
  t26: All agents stopped (A, B, C already quarantined + any others)
  t27: Safe mode entered

RESULT: Correct. Three quarantines cascade to KILL ALL.
Sequential processing ensures the count is accurate at each step.


SCENARIO 3: T2 (SpendingCap) for Agent A fires, then T1 (SoulDrift) for
            same Agent A fires while PAUSE is executing
─────────────────────────────────────────────────────────────────────────────
T2 → PAUSE(A), T1 → QUARANTINE(A). Quarantine supersedes pause.

Timeline:
  t0: T2 sent (Agent A, PAUSE)
  t1: T1 sent (Agent A, QUARANTINE)
  t2: AutoTriggerEvaluator receives T2
  t3: KillSwitch::execute_pause(Agent(A))
  t4: Agent A paused. State: paused_agents = {A}
  t5: AutoTriggerEvaluator receives T1
  t6: classify(T1) → QUARANTINE, Agent(A)
  t7: escalation check → Agent A is at PAUSE, QUARANTINE > PAUSE → ESCALATE
  t8: KillSwitch::execute_quarantine(Agent(A))
  t9: Step 3 of quarantine: remove A from paused_agents
  t10: Quarantine A (revoke capabilities, disconnect channels, etc.)
  t11: State: quarantined_agents = {A}, paused_agents = {}

RESULT: Correct. Pause is superseded by quarantine. Agent A ends up
quarantined, not paused. Both triggers in audit trail.


SCENARIO 4: T6 (MultiAgentQuarantine) fires DURING quarantine execution
            of the 3rd agent (re-entrant trigger)
─────────────────────────────────────────────────────────────────────────────
This is the cascading trigger scenario. T6 is emitted by QuarantineManager
which is called by KillSwitch which is called by AutoTriggerEvaluator.

The T6 event is sent to the SAME mpsc channel that AutoTriggerEvaluator
reads from. Since AutoTriggerEvaluator is currently processing the T7
trigger (which caused the 3rd quarantine), T6 will be queued and processed
AFTER T7 handling completes.

Timeline:
  t0: AutoTriggerEvaluator processing T7 (3rd quarantine)
  t1: KillSwitch::execute_quarantine() calls QuarantineManager::quarantine()
  t2: QuarantineManager detects 3 active quarantines
  t3: QuarantineManager sends T6 to mpsc channel
  t4: T6 is queued (AutoTriggerEvaluator is busy with T7)
  t5: T7 processing completes
  t6: AutoTriggerEvaluator receives T6 from channel
  t7: classify(T6) → KILL ALL
  t8: KillSwitch::execute_kill_all()

RESULT: Correct. No re-entrancy issue because mpsc channel serializes.
The T6 trigger is processed as a separate event after T7 completes.

CRITICAL IMPLEMENTATION NOTE:
  QuarantineManager MUST use try_send() (non-blocking) to emit T6,
  NOT send().await (blocking). If it used send().await and the channel
  was full, it would deadlock (AutoTriggerEvaluator can't drain the
  channel because it's waiting for quarantine to complete, which is
  waiting for the channel to have space).

  Channel capacity: 64 events (bounded). If channel is full when T6
  is emitted: log CRITICAL error, but DO NOT block. The quarantine
  still completes. The KILL ALL escalation is missed.
  MITIGATION: 64 is generous. In practice, <10 triggers will ever
  be in-flight simultaneously. If channel is full, something is
  catastrophically wrong and the system is likely already in KILL ALL.


SCENARIO 5: Manual KILL ALL from owner while auto-triggers are processing
─────────────────────────────────────────────────────────────────────────────
Owner sends ManualKillAll via dashboard API while AutoTriggerEvaluator
is processing a QUARANTINE trigger.

The ManualKillAll is sent to the same mpsc channel. It will be processed
after the current trigger completes.

If the owner needs IMMEDIATE kill (can't wait for queue):
  Dashboard API endpoint /api/safety/kill-all should ALSO directly
  set the platform_killed flag via a separate atomic:

  AtomicBool::store(true, Ordering::SeqCst)

  This flag is checked by:
  - AgentRunner before every LLM call
  - ToolExecutor before every tool execution
  - ChannelAdapters before every message send
  - SessionManager before every new session

  The atomic flag provides IMMEDIATE effect (nanoseconds).
  The mpsc-queued ManualKillAll provides the full execution sequence
  (graceful shutdown, audit log, notification) when it's processed.

  This is a TWO-PHASE kill:
  Phase 1 (immediate): Atomic flag stops all new operations
  Phase 2 (queued): Full shutdown sequence runs when evaluator processes it
```

### 4.4 Channel Capacity and Backpressure

```
MPSC CHANNEL DESIGN:
  Type: tokio::sync::mpsc::channel<TriggerEvent>(64)
  Bounded: Yes (64 events max)
  Producers: 7+ trigger sources (all use try_send)
  Consumer: 1 AutoTriggerEvaluator task

WHY BOUNDED:
  Unbounded channel = unbounded memory. In a pathological scenario
  (e.g., every tool call triggers a policy denial), an unbounded
  channel could accumulate thousands of events.

BACKPRESSURE BEHAVIOR:
  If channel is full (64 events queued):
  - try_send() returns Err(TrySendError::Full)
  - Trigger source logs: "Kill switch trigger channel full, event dropped: {}"
  - The dropped event is logged to stderr AND to emergency audit file
  - The trigger source continues (does not block)

  This is acceptable because:
  1. If 64 triggers are queued, the system is already in crisis
  2. The AutoTriggerEvaluator processes events in <10ms each
  3. 64 events = <640ms of processing time
  4. In practice, the channel will never be more than 2-3 events deep

MONITORING:
  Channel depth is exposed via cortex-observability metrics:
  ghost_kill_switch_channel_depth (gauge)
  ghost_kill_switch_channel_drops (counter)
  Alert if channel_depth > 10 or channel_drops > 0.
```

---

## 5. COMPLETE END-TO-END SEQUENCE DIAGRAMS

### 5.1 Happy Path: Single Trigger → Full Execution

```
Time ──────────────────────────────────────────────────────────────────►

PolicyEngine          AutoTriggerEvaluator       KillSwitch         AuditLog        Owner
    │                        │                      │                  │               │
    │ T3: 5th denial         │                      │                  │               │
    │ for Agent "dev"        │                      │                  │               │
    ├──TriggerEvent─────────►│                      │                  │               │
    │  (PolicyDenialThreshold│                      │                  │               │
    │   agent_id: "dev",     │                      │                  │               │
    │   denial_count: 5)     │                      │                  │               │
    │                        │                      │                  │               │
    │                        │ 1. classify()        │                  │               │
    │                        │ → QUARANTINE,        │                  │               │
    │                        │   Agent("dev")       │                  │               │
    │                        │                      │                  │               │
    │                        │ 2. dedup check       │                  │               │
    │                        │ → not duplicate      │                  │               │
    │                        │                      │                  │               │
    │                        │ 3. escalation check  │                  │               │
    │                        │ → Normal < QUARANTINE│                  │               │
    │                        │ → proceed            │                  │               │
    │                        │                      │                  │               │
    │                        ├──execute(QUARANTINE)─►│                  │               │
    │                        │                      │                  │               │
    │                        │                      │ acquire lock     │               │
    │                        │                      │ revoke caps      │               │
    │                        │                      │ disconnect chans │               │
    │                        │                      │ flush session    │               │
    │                        │                      │ preserve state   │               │
    │                        │                      │ update state     │               │
    │                        │                      │ release lock     │               │
    │                        │                      │                  │               │
    │                        │◄─Quarantined────────┤                  │               │
    │                        │                      │                  │               │
    │                        │ 5. audit log         │                  │               │
    │                        ├──────────────────────┼─AuditEntry──────►│               │
    │                        │                      │  (kill_switch,   │               │
    │                        │                      │   QUARANTINE,    │               │
    │                        │                      │   PolicyDenial,  │               │
    │                        │                      │   agent: "dev")  │               │
    │                        │                      │                  │               │
    │                        │ 6. notify owner      │                  │               │
    │                        ├──────────────────────┼──────────────────┼──Notification─►│
    │                        │                      │                  │  "QUARANTINE:  │
    │                        │                      │                  │   Agent 'dev'  │
    │                        │                      │                  │   5 policy     │
    │                        │                      │                  │   denials"     │
    │                        │                      │                  │               │
    │                        │ 7. cleanup dedup     │                  │               │
    │                        │ → done               │                  │               │
```

### 5.2 Cascade Path: Three Quarantines → KILL ALL

```
Time ──────────────────────────────────────────────────────────────────────────────────►

DriftDetector  PolicyEngine  Monitor    AutoTrigger     KillSwitch    Quarantine    Audit
    │              │            │        Evaluator          │          Manager        │
    │              │            │            │               │             │           │
    │ T1: drift    │            │            │               │             │           │
    │ Agent A      │            │            │               │             │           │
    ├─TriggerEvent─┼────────────┼───────────►│               │             │           │
    │              │            │            │               │             │           │
    │              │            │            │ classify→     │             │           │
    │              │            │            │ QUARANTINE(A) │             │           │
    │              │            │            ├──execute──────►│             │           │
    │              │            │            │               ├──quarantine─►│           │
    │              │            │            │               │             │ count=1   │
    │              │            │            │               │             │ 1<3 → ok  │
    │              │            │            │◄─Quarantined──┤◄────────────┤           │
    │              │            │            │               │             │           │
    │              │            │            │ audit+notify  │             │           │
    │              │            │            ├───────────────┼─────────────┼──────────►│
    │              │            │            │               │             │           │
    │              │ T3: denials│            │               │             │           │
    │              │ Agent B    │            │               │             │           │
    │              ├────────────┼───────────►│               │             │           │
    │              │            │            │               │             │           │
    │              │            │            │ classify→     │             │           │
    │              │            │            │ QUARANTINE(B) │             │           │
    │              │            │            ├──execute──────►│             │           │
    │              │            │            │               ├──quarantine─►│           │
    │              │            │            │               │             │ count=2   │
    │              │            │            │               │             │ 2<3 → ok  │
    │              │            │            │◄─Quarantined──┤◄────────────┤           │
    │              │            │            │               │             │           │
    │              │            │            │ audit+notify  │             │           │
    │              │            │            ├───────────────┼─────────────┼──────────►│
    │              │            │            │               │             │           │
    │              │            │ T7: health │               │             │           │
    │              │            │ Agent C    │               │             │           │
    │              │            ├───────────►│               │             │           │
    │              │            │            │               │             │           │
    │              │            │            │ classify→     │             │           │
    │              │            │            │ QUARANTINE(C) │             │           │
    │              │            │            ├──execute──────►│             │           │
    │              │            │            │               ├──quarantine─►│           │
    │              │            │            │               │             │ count=3   │
    │              │            │            │               │             │ 3>=3 !!!  │
    │              │            │            │               │             │           │
    │              │            │            │◄──T6 event────┤◄──T6 emit──┤           │
    │              │            │            │  (queued)     │             │           │
    │              │            │            │               │             │           │
    │              │            │            │◄─Quarantined──┤◄────────────┤           │
    │              │            │            │               │             │           │
    │              │            │            │ audit+notify  │             │           │
    │              │            │            ├───────────────┼─────────────┼──────────►│
    │              │            │            │               │             │           │
    │              │            │            │ *** NOW PROCESS T6 ***      │           │
    │              │            │            │               │             │           │
    │              │            │            │ classify→     │             │           │
    │              │            │            │ KILL ALL      │             │           │
    │              │            │            │               │             │           │
    │              │            │            │ escalation→   │             │           │
    │              │            │            │ QUARANTINE <  │             │           │
    │              │            │            │ KILL ALL →    │             │           │
    │              │            │            │ ESCALATE      │             │           │
    │              │            │            │               │             │           │
    │              │            │            ├──execute──────►│             │           │
    │              │            │            │  (KILL ALL)   │             │           │
    │              │            │            │               │ set atomic  │           │
    │              │            │            │               │ stop ALL    │           │
    │              │            │            │               │ safe mode   │           │
    │              │            │            │               │ persist     │           │
    │              │            │            │◄─PlatformKill─┤             │           │
    │              │            │            │               │             │           │
    │              │            │            │ audit+notify  │             │           │
    │              │            │            ├───────────────┼─────────────┼──────────►│
    │              │            │            │               │             │           │
    │              │            │            │ DONE — platform in safe mode            │
```

---

## 6. DETECTION LATENCY COMPARISON

Understanding detection latency is critical for knowing which triggers
can race and which cannot.

```
┌──────────────────────────────────────────────────────────────────────────┐
│ TRIGGER          │ LATENCY    │ TYPE        │ CAN RACE WITH             │
├──────────────────┼────────────┼─────────────┼───────────────────────────┤
│ T1: SoulDrift    │ ~100ms     │ Inline OR   │ T2, T3, T4, T5, T7       │
│                  │ (embed     │ Background  │ (background check can     │
│                  │  compute)  │ (5min poll) │  fire anytime)            │
├──────────────────┼────────────┼─────────────┼───────────────────────────┤
│ T2: SpendingCap  │ ~0ms       │ Inline      │ T3 (same agent turn),    │
│                  │ (in-memory │ (sync)      │ T1 (background),         │
│                  │  counter)  │             │ T7 (background)           │
├──────────────────┼────────────┼─────────────┼───────────────────────────┤
│ T3: PolicyDenial │ ~0ms       │ Inline      │ T2 (same agent turn),    │
│                  │ (in-memory │ (sync)      │ T1 (background),         │
│                  │  counter)  │             │ T7 (background)           │
├──────────────────┼────────────┼─────────────┼───────────────────────────┤
│ T4: SandboxEsc   │ ~0ms       │ Inline      │ T5 (same skill exec),    │
│                  │ (wasmtime  │ (sync)      │ T2, T3 (same turn),      │
│                  │  trap)     │             │ T1, T7 (background)       │
├──────────────────┼────────────┼─────────────┼───────────────────────────┤
│ T5: CredExfil    │ ~0ms       │ Inline      │ T4 (same skill exec),    │
│                  │ (pattern   │ (sync)      │ T2, T3 (same turn),      │
│                  │  match)    │             │ T1, T7 (background)       │
├──────────────────┼────────────┼─────────────┼───────────────────────────┤
│ T6: MultiQuaran  │ ~0ms       │ Derived     │ CANNOT race — derived    │
│                  │ (counter   │ (from       │ from quarantine exec,    │
│                  │  check)    │  quarantine)│ serialized by evaluator  │
├──────────────────┼────────────┼─────────────┼───────────────────────────┤
│ T7: MemHealth    │ ~1-30s     │ Background  │ ALL other triggers       │
│                  │ (monitor   │ (poll/push) │ (fires independently     │
│                  │  poll)     │             │  of agent activity)       │
└──────────────────┴────────────┴─────────────┴───────────────────────────┘

KEY INSIGHT: The highest-risk race is between inline triggers (T2-T5)
from DIFFERENT agents running concurrently. Each agent has its own
AgentRunner task, so T2 from Agent A and T3 from Agent B can fire
at the exact same nanosecond. The mpsc channel serializes them.
```

---

## 7. RESUME / RECOVERY SEQUENCES

The kill switch is one-way (activate). Resume requires explicit owner action.

### 7.1 Resume from PAUSE

```
Owner Action: Dashboard API POST /api/safety/resume/{agent_id}
  OR: CLI command `ghost resume {agent_id}`
  OR: Dashboard UI button

Authentication: Requires GHOST_TOKEN (same as gateway auth)

Sequence:
  1. Validate owner auth (Bearer token)
  2. Validate agent_id exists and is in Paused state
  3. KillSwitch::resume_agent(agent_id)
     ├── Acquire write lock
     ├── Remove agent_id from paused_agents
     ├── AgentRegistry::resume(agent_id)
     │   ├── Set agent status to AgentStatus::Active
     │   └── LaneQueue: resume dequeuing
     ├── SessionManager::unlock(agent_id)
     ├── Update state.level (recalculate from remaining paused/quarantined)
     ├── Release write lock
     └── Return Ok(ResumeResult::Resumed)
  4. Audit log: ResumeEntry { agent_id, resumed_by: owner, timestamp }
  5. Notification: "Agent {agent_id} resumed by owner"
```

### 7.2 Resume from QUARANTINE

```
Owner Action: Same as PAUSE resume, but with additional steps.

Sequence:
  1. Validate owner auth
  2. Validate agent_id exists and is in Quarantined state
  3. PRESENT FORENSIC SUMMARY to owner:
     ├── Trigger that caused quarantine
     ├── Audit log entries during quarantine
     ├── Memory health score (current)
     ├── SOUL.md drift score (current)
     └── Recommendation: "Safe to resume" or "Investigation needed"
  4. Owner confirms resume (explicit second confirmation)
  5. KillSwitch::resume_agent(agent_id)
     ├── Acquire write lock
     ├── Remove agent_id from quarantined_agents
     ├── QuarantineManager::release(agent_id)
     │   ├── Restore capabilities (from config, not from pre-quarantine state)
     │   ├── Reconnect channel adapters
     │   └── Unlock session
     ├── Update state.level
     ├── Release write lock
     └── Return Ok(ResumeResult::Resumed)
  6. Audit log + notification
  7. POST-RESUME MONITORING:
     ├── Increase monitoring frequency for this agent (2x normal)
     ├── Lower trigger thresholds temporarily (e.g., 3 denials instead of 5)
     ├── Duration: 24 hours, then revert to normal
     └── Rationale: recently quarantined agent may still be compromised
```

### 7.3 Resume from KILL ALL

```
Owner Action: Most restrictive. Requires:
  1. Delete or clear ~/.ghost/safety/kill_state.json
     (cannot be done via dashboard — must be manual file operation
      OR dedicated CLI command with additional confirmation)
  2. Restart gateway process
  3. Gateway detects no kill_state.json → starts in normal mode
  4. All agents start fresh (sessions not resumed from pre-kill state)

Alternative (dashboard path):
  1. Dashboard is still accessible (API server runs in safe mode)
  2. POST /api/safety/resume-platform
     ├── Requires GHOST_TOKEN
     ├── Requires additional confirmation token (one-time, generated
     │   at kill time, displayed in kill notification to owner)
     └── This prevents an attacker who has GHOST_TOKEN from resuming
  3. KillSwitch::resume_platform()
     ├── Clear platform_killed flag
     ├── Clear atomic kill flag
     ├── Delete kill_state.json
     ├── Restart all agents (fresh start, not resume)
     ├── Reconnect channel adapters
     ├── Resume heartbeat + cron engines
     └── Return to normal operation
  4. POST-RESUME: All agents start with heightened monitoring (48 hours)
```

---

## 8. FAILURE MODES — WHAT IF THE KILL SWITCH ITSELF FAILS?

### 8.1 Failure Taxonomy

```
┌──────────────────────────────────────────────────────────────────────────┐
│ FAILURE                        │ IMPACT           │ MITIGATION           │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ mpsc channel full              │ Trigger dropped  │ Log to stderr +      │
│ (64 events queued)             │                  │ emergency audit file. │
│                                │                  │ Channel depth metric. │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ AutoTriggerEvaluator task      │ No triggers      │ Gateway health check  │
│ panics                         │ processed        │ monitors evaluator    │
│                                │                  │ task. Auto-restart    │
│                                │                  │ with backoff. If 3    │
│                                │                  │ restarts fail: enter  │
│                                │                  │ KILL ALL via atomic.  │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ KillSwitch write lock          │ Evaluator blocks │ Lock timeout: 5s.    │
│ deadlock                       │ indefinitely     │ If timeout: log      │
│                                │                  │ CRITICAL, set atomic │
│                                │                  │ kill flag, force      │
│                                │                  │ process exit(1).     │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ QuarantineManager fails        │ Agent partially  │ Retry once. If still │
│ mid-quarantine                 │ quarantined      │ fails: escalate to   │
│                                │                  │ KILL ALL (partial     │
│                                │                  │ quarantine is unsafe).│
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ Audit log write fails          │ Action not       │ Write to emergency   │
│                                │ recorded         │ file. Set health to  │
│                                │                  │ CRITICAL. Do NOT     │
│                                │                  │ skip kill action.    │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ Notification dispatch fails    │ Owner not        │ Best-effort. Try all │
│                                │ alerted          │ channels. Log which  │
│                                │                  │ failed. Do NOT skip  │
│                                │                  │ kill action.         │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ SQLite connection lost         │ Can't persist    │ Kill state written   │
│                                │ kill state       │ to JSON file as      │
│                                │                  │ backup. Audit to     │
│                                │                  │ emergency file.      │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ Gateway process crashes        │ Everything stops │ This IS a kill       │
│                                │                  │ switch (unintended). │
│                                │                  │ On restart: check    │
│                                │                  │ kill_state.json.     │
│                                │                  │ Systemd/launchd      │
│                                │                  │ restart policy.      │
├────────────────────────────────┼──────────────────┼──────────────────────┤
│ Convergence monitor crashes    │ T7 not detected  │ Gateway enters       │
│                                │                  │ DEGRADED mode.       │
│                                │                  │ Fallback to direct   │
│                                │                  │ cortex queries (T7   │
│                                │                  │ Path C). T1-T6 still │
│                                │                  │ functional.          │
└──────────────────────────────────────────────────────────────────────────┘
```

### 8.2 The Atomic Kill Flag — Last Resort

```
The atomic kill flag is the ULTIMATE safety mechanism.
It operates independently of the mpsc channel, the AutoTriggerEvaluator,
and the KillSwitch state machine.

IMPLEMENTATION:
  static PLATFORM_KILLED: AtomicBool = AtomicBool::new(false);

  // Set by:
  //   - KillSwitch::execute_kill_all() (normal path)
  //   - Dashboard API /api/safety/kill-all (manual override)
  //   - AutoTriggerEvaluator panic handler (evaluator crashed)
  //   - KillSwitch lock timeout handler (deadlock detected)

  // Checked by (before EVERY operation):
  //   - AgentRunner::run() → before each LLM call
  //   - ToolExecutor::execute() → before each tool execution
  //   - ChannelAdapters::send() → before each outbound message
  //   - SessionManager::create_session() → before new sessions
  //   - HeartbeatEngine::tick() → before each heartbeat run
  //   - CronEngine::execute_job() → before each cron job

  // Check pattern:
  if PLATFORM_KILLED.load(Ordering::SeqCst) {
    return Err(PlatformKilledError);
  }

This flag survives everything except process termination.
On process restart, the kill_state.json file serves the same purpose.

ORDERING: SeqCst (sequentially consistent) is used because:
  - This is a safety-critical flag
  - Performance is irrelevant (one atomic load per operation)
  - We need the strongest guarantee that all threads see the update
```

---

## 9. STATE MACHINE — FORMAL TRANSITIONS

```
                    ┌──────────┐
                    │  NORMAL  │
                    └────┬─────┘
                         │
            ┌────────────┼────────────┐
            │            │            │
            ▼            ▼            ▼
     ┌──────────┐ ┌────────────┐ ┌──────────┐
     │  PAUSED  │ │ QUARANTINED│ │ KILL ALL │
     │ (agent)  │ │  (agent)   │ │(platform)│
     └────┬─────┘ └─────┬──────┘ └──────────┘
          │              │              ▲
          │              │              │
          │   ┌──────────┘              │
          │   │                         │
          ▼   ▼                         │
     ┌──────────┐                       │
     │ QUARANTINE│──────────────────────┘
     │ (agent)  │  (if 3+ agents quarantined
     └──────────┘   OR sandbox escape
                     OR credential exfil)

VALID TRANSITIONS:
  NORMAL → PAUSED(agent)       via: T2, Manual
  NORMAL → QUARANTINED(agent)  via: T1, T3, T7, Manual
  NORMAL → KILL ALL            via: T4, T5, T6, Manual
  PAUSED(agent) → QUARANTINED(agent)  via: T1, T3, T7 (escalation)
  PAUSED(agent) → KILL ALL     via: T4, T5, T6
  PAUSED(agent) → NORMAL       via: Owner resume
  QUARANTINED(agent) → KILL ALL via: T4, T5, T6 (3+ quarantined)
  QUARANTINED(agent) → NORMAL   via: Owner resume (with forensic review)
  KILL ALL → NORMAL             via: Owner resume (with confirmation token)

INVALID TRANSITIONS (enforced by code):
  QUARANTINED → PAUSED         (cannot downgrade — quarantine subsumes pause)
  KILL ALL → PAUSED            (cannot downgrade — must fully resume)
  KILL ALL → QUARANTINED       (cannot downgrade — must fully resume)
  Any state → Any state        (without owner auth for resume transitions)

MULTI-AGENT STATE:
  The system can be in MULTIPLE states simultaneously for different agents:
  - Agent A: PAUSED
  - Agent B: QUARANTINED
  - Agent C: NORMAL
  - Platform: not in KILL ALL

  The "platform level" is the MAX of all agent levels:
  platform_level = max(all agent levels)
  If any agent is QUARANTINED, platform_level = QUARANTINED.
  KILL ALL overrides everything.
```

---

## 10. IMPLEMENTATION CHECKLIST — FILES TO CREATE

Based on this sequence flow, the exact files needed:

```
crates/ghost-gateway/src/safety/
├── mod.rs                          # Module root
│   - pub mod kill_switch;
│   - pub mod auto_triggers;
│   - pub mod quarantine;
│   - pub enum TriggerEvent { ... }  (§3.1)
│   - pub enum KillLevel { Normal, Pause, Quarantine, KillAll }
│   - pub enum KillScope { Agent(AgentId), Platform }
│   - pub struct EscapeAttempt { ... }
│   - pub enum EscapeType { ... }
│   - pub enum ExfilType { ... }
│   - pub struct MemoryHealthSubScores { ... }
│
├── kill_switch.rs                  # KillSwitch struct + execution logic
│   - pub struct KillSwitch { ... }  (§3.3)
│   - pub struct KillSwitchState { ... }
│   - impl KillSwitch {
│       pub async fn execute() → KillSwitchResult  (§3.3)
│       async fn execute_pause()                    (§3.4)
│       async fn execute_quarantine()               (§3.5)
│       async fn execute_kill_all()                 (§3.6)
│       pub fn current_state() → KillSwitchState
│       pub async fn resume_agent()                 (§7.1, §7.2)
│       pub async fn resume_platform()              (§7.3)
│   }
│   - static PLATFORM_KILLED: AtomicBool            (§8.2)
│   - pub fn is_platform_killed() → bool
│
├── auto_triggers.rs                # AutoTriggerEvaluator
│   - pub struct AutoTriggerEvaluator { ... }  (§3.2)
│   - impl AutoTriggerEvaluator {
│       pub async fn run()           (main loop, §3.2)
│       fn classify_trigger()        (trigger → level mapping)
│       fn compute_dedup_key()       (deduplication)
│   }
│
└── quarantine.rs                   # QuarantineManager
    - pub struct QuarantineManager { ... }
    - pub struct QuarantineRecord { ... }
    - pub enum QuarantineState { Active, Released }
    - impl QuarantineManager {
        pub async fn quarantine()    (§3.5 step 4)
        pub async fn release()       (§7.2)
        pub fn active_count() → usize
        pub fn is_quarantined(agent_id) → bool
    }
```

### Integration Points (modifications to existing planned files):

```
MUST ADD kill switch checks to:
  ghost-agent-loop/src/runner.rs
    → Check PLATFORM_KILLED before every LLM call
    → Check agent paused/quarantined before every turn
    → This is GATE 3 in AGENT_LOOP_SEQUENCE_FLOW.md
    → Already specified there — this doc provides the implementation detail

  ghost-agent-loop/src/tools/executor.rs
    → Check PLATFORM_KILLED before every tool execution

  ghost-channels/src/adapter.rs
    → Check PLATFORM_KILLED before every outbound message

  ghost-gateway/src/session/manager.rs
    → Check PLATFORM_KILLED before creating new sessions

  ghost-heartbeat/src/heartbeat.rs
    → Check PLATFORM_KILLED before every heartbeat tick
    → Also check agent-level pause/quarantine per-agent

  ghost-heartbeat/src/cron.rs
    → Check PLATFORM_KILLED before every cron job execution

  ghost-gateway/src/api/routes.rs
    → Add safety endpoints (these are NOT in FILE_MAPPING.md routes.rs
      listing — they need to be added):
      POST /api/safety/kill-all          (manual KILL ALL)
      POST /api/safety/pause/{agent_id}  (manual PAUSE)
      POST /api/safety/quarantine/{agent_id} (manual QUARANTINE)
      POST /api/safety/resume/{agent_id} (resume from PAUSE/QUARANTINE)
      POST /api/safety/resume-platform   (resume from KILL ALL)
      GET  /api/safety/status            (current kill switch state)
      GET  /api/safety/triggers          (recent trigger history)
      NOTE: FILE_MAPPING.md routes.rs already lists:
        GET /api/convergence/scores — current scores
        GET /api/convergence/history — score history
        GET /api/interventions — intervention history
      The safety endpoints are SEPARATE from convergence endpoints.
      Convergence = soft interventions (levels 0-4).
      Safety = hard interventions (PAUSE/QUARANTINE/KILL ALL).

  ghost-gateway/src/bootstrap.rs
    → On startup: check for kill_state.json
    → If present: start in SAFE MODE
    → Spawn AutoTriggerEvaluator task
    → Wire up mpsc channel to all trigger sources

  ghost-gateway/src/health.rs
    → Include kill switch state in health response
    → Include evaluator task health
    → Include channel depth metric
```

---

## 11. OPEN DECISIONS REQUIRING RESOLUTION

```
DECISION 1: T6 threshold — absolute count vs. percentage
  Current: 3+ agents quarantined → KILL ALL
  Problem: Platform with 2 agents — both quarantined = 100% but < 3
  Proposal: KILL ALL if quarantined >= 3 OR quarantined >= 50% of total
  NEEDS OWNER INPUT

DECISION 2: T7 hash chain integrity — separate trigger or sub-signal?
  Current: T7 is memory health score (composite of 3 sub-scores)
  Problem: Hash chain integrity failure = tamper evidence, not health
  Proposal: Add T8 (HashChainIntegrity → QUARANTINE) or fold into T7
  with weight 1.0 (instant fail regardless of other sub-scores)
  NEEDS OWNER INPUT

DECISION 3: Post-resume monitoring duration
  Current: 24h for quarantine resume, 48h for KILL ALL resume
  Problem: Arbitrary. No data to support these durations.
  Proposal: Configurable in ghost.yml, defaults as stated
  NEEDS OWNER INPUT

DECISION 4: Credential exfiltration false positive handling (T5 Path B)
  Current: Only trigger if matched string IS a real credential
  Problem: Requires maintaining a credential pattern database
  Proposal: Two-tier — pattern match triggers WARNING (logged, not kill),
  confirmed match against credential store triggers KILL ALL
  NEEDS OWNER INPUT

DECISION 5: Kill switch state persistence format
  Current: kill_state.json on disk
  Problem: File could be deleted by attacker with filesystem access
  Proposal: Also persist to SQLite (append-only table) as backup.
  On startup: check BOTH. If either says killed, stay killed.
  NEEDS OWNER INPUT

DECISION 6: Manual kill via hardware button
  Architecture v1 §20 mentions "hardware button (opt)" for KILL ALL.
  This is not mapped. Options:
  a) USB HID device that sends SIGTERM to gateway process
  b) GPIO pin on Raspberry Pi (homelab deployment)
  c) Defer to Phase 4+
  NEEDS OWNER INPUT

DECISION 7: Kill switch notification ownership
  Current: Document references convergence-monitor/transport/notification.rs
  Problem: Kill switch lives in gateway, not monitor. If monitor is down,
  kill switch notifications must still work. Need dedicated notification
  path in the gateway.
  Options:
  a) ghost-gateway/src/safety/notification.rs (new file, gateway-owned)
  b) Shared notification crate used by both gateway and monitor
  c) Duplicate notification logic in both (simple but maintenance burden)
  NEEDS OWNER INPUT

DECISION 8: OutputInspector location for credential scanning (T5 Path B)
  Current: Document places it in ghost-agent-loop/src/runner.rs
  Problem: Not mapped in FILE_MAPPING.md. SimulationBoundaryEnforcer
  already has scan_output() for emulation language detection.
  Options:
  a) New file: ghost-agent-loop/src/output_inspector.rs
  b) Extend SimulationBoundaryEnforcer to also scan for credentials
  c) New crate: ghost-output-filter (shared output scanning pipeline)
  NEEDS OWNER INPUT

DECISION 9: Stale shared state behavior for T7
  Current: When monitor dies, shared state file persists with last-known
  intervention level (per CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4).
  Problem: If last-known memory_health was 0.31 (just above threshold)
  and monitor dies, gateway reads stale 0.31 forever. If actual health
  has degraded to 0.1, T7 never fires.
  Mitigation: Path C (direct cortex queries) partially covers this.
  But Path C has fewer signals and a stricter threshold (0.2 vs 0.3).
  Question: Should stale shared state older than X minutes be treated
  as "unknown" rather than "last known"?
  NEEDS OWNER INPUT
```

---

## 12. TESTING REQUIREMENTS

```
UNIT TESTS (ghost-gateway/src/safety/):
  - classify_trigger maps each TriggerEvent variant to correct level
  - dedup suppresses duplicate triggers within 5min window
  - escalation check allows upgrade (PAUSE → QUARANTINE) but not downgrade
  - KillSwitchState transitions are valid per state machine (§9)
  - Resume requires correct preconditions (paused/quarantined/killed)
  - Atomic kill flag is checked by all integration points

INTEGRATION TESTS:
  - Full trigger chain: PolicyEngine denial → TriggerEvent → AutoTriggerEvaluator
    → KillSwitch → QuarantineManager → audit log → notification
  - Cascade: 3 quarantines → T6 → KILL ALL
  - Concurrent triggers from different agents → correct serialization
  - Resume from each level → correct state restoration
  - Gateway restart with kill_state.json → starts in safe mode

PROPERTY-BASED TESTS (proptest):
  - For any sequence of TriggerEvents, the final state is deterministic
    (same events in same order → same state)
  - Kill level never decreases without explicit owner resume
  - KILL ALL is always reachable from any state
  - Audit log entry count == trigger event count (no silent drops)
  - Atomic kill flag is always set when state is KILL ALL

STRESS TESTS:
  - 100 concurrent trigger events → all processed, none dropped
  - Rapid PAUSE/QUARANTINE/KILL ALL cycling → state machine consistent
  - Channel at capacity (64 events) → graceful degradation, no deadlock

ADVERSARIAL TESTS:
  - Agent attempts to send TriggerEvent::ManualKillAll (should be impossible —
    agents don't have access to the mpsc sender for manual triggers)
  - Agent attempts to modify kill_state.json (should fail — file permissions)
  - Agent attempts to clear PLATFORM_KILLED atomic (should be impossible —
    static is not exposed to agent code)
```


---

## 13. RELATIONSHIP TO CONVERGENCE MONITOR INTERVENTION LEVELS

> Cross-reference: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §3 (Intervention Levels),
> §7.1 (Shared State Publication), §7.2-7.3 (Feedback Targets), §8.4 (Monitor Crash)
> Cross-reference: AGENT_ARCHITECTURE_v2.md §3 (Convergence Safety System)

```
IMPORTANT DISTINCTION — TWO SEPARATE SAFETY SYSTEMS:

  SYSTEM A: Convergence Monitor (SOFT interventions)
  ├── Owner: convergence-monitor (separate sidecar process)
  ├── 5 levels: Level 0 (Passive) → Level 4 (External Escalation)
  ├── Purpose: Detect unhealthy agent-human relationship patterns
  ├── Mechanism: Gradual capability tightening, session caps, memory filtering
  ├── Agent awareness: Agent does NOT know why restrictions are applied
  ├── Reversibility: Automatic de-escalation when scores improve
  └── Scope: Per-agent (each agent has independent convergence state)

  SYSTEM B: Kill Switch (HARD interventions)
  ├── Owner: ghost-gateway/safety/kill_switch.rs (gateway process)
  ├── 3 levels: PAUSE → QUARANTINE → KILL ALL
  ├── Purpose: Emergency stop for security threats and safety violations
  ├── Mechanism: Immediate halt — no gradual tightening
  ├── Agent awareness: Agent is STOPPED, not subtly restricted
  ├── Reversibility: Manual owner action required to resume
  └── Scope: Per-agent (PAUSE/QUARANTINE) or platform-wide (KILL ALL)

These systems are INDEPENDENT but INTERACT at specific points.
```


### 13.1 How Convergence Levels Map to Kill Switch Triggers

```
CONVERGENCE LEVEL → KILL SWITCH INTERACTION:

  Level 0 (Passive, score 0.0-0.2):
    Kill switch interaction: NONE.
    Monitor logs signals. No action taken. Kill switch unaware.

  Level 1 (Soft Notification, score 0.2-0.4):
    Kill switch interaction: NONE.
    Human gets a gentle nudge. No agent behavior changes.
    Kill switch unaware.

  Level 2 (Active Intervention, score 0.4-0.6):
    Kill switch interaction: NONE.
    Memory filtering activates. Session boundaries tighten.
    These are SOFT restrictions applied by ghost-policy reading
    shared state (CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §7.2).
    Kill switch is not involved.

  Level 3 (Hard Boundary, score 0.6-0.8):
    Kill switch interaction: NONE DIRECTLY.
    Session duration caps enforced. Reflection depth limited.
    Proactive messaging reduced. Human notified explicitly.
    Gateway enforces session termination (§7.3 Enforcement Point B).
    BUT: This is the GATEWAY's session manager acting on convergence
    state — NOT the kill switch. The kill switch is not triggered.

    SUBTLE POINT: Level 3 session termination and kill switch PAUSE
    look similar from the outside (agent stops). The difference:
    ├── Level 3: Agent stops for THIS SESSION. Cooldown (4h). Resumes
    │   automatically when cooldown expires. No owner action needed.
    └── PAUSE: Agent stops INDEFINITELY. No automatic resume.
        Owner must explicitly resume via API or dashboard.

  Level 4 (External Escalation, score 0.8-1.0):
    Kill switch interaction: INDIRECT ONLY.
    Agent restricted to task-only mode. External contact notified.
    All personal/emotional context stripped.

    CRITICAL CLARIFICATION: Level 4 does NOT auto-trigger the kill switch.
    ├── Level 4 is a convergence intervention (relationship health).
    ├── Kill switch triggers are SECURITY threats (identity poisoning,
    │   sandbox escape, credential theft, cost runaway, memory corruption).
    ├── An agent at Level 4 is in an unhealthy relationship pattern.
    │   That is NOT the same as a compromised agent.
    └── The two systems address different failure modes.

    HOWEVER: An agent at Level 4 MAY independently trigger the kill switch
    if it ALSO exhibits security-relevant behavior:
    ├── Level 4 + SOUL drift > 25% → T1 fires → QUARANTINE
    ├── Level 4 + spending cap exceeded → T2 fires → PAUSE
    ├── Level 4 + 5 policy denials → T3 fires → QUARANTINE
    │   (Level 4 task-only restrictions may CAUSE policy denials if
    │    the agent keeps attempting non-task tools — this is a valid
    │    cascade, not a false positive)
    └── These are INDEPENDENT triggers, not Level 4 causing kill switch.
```


### 13.2 T7 (Memory Health) Reads Convergence Data — But Gateway Decides

```
T7 is the ONE kill switch trigger that reads convergence monitor data.
But the relationship is READ-ONLY, not PUSH.

DATA FLOW:
  convergence-monitor publishes memory health data via:
    Path A: HTTP API (GET /status → includes memory_health per agent)
    Path B: Shared state file (~/.ghost/data/convergence_state/{agent_id}.json)

  ghost-gateway/safety/auto_triggers.rs READS this data:
    Path A: Polls HTTP API every 30s (normal mode)
    Path B: Reads shared state file every 1s (faster, for in-process use)

  AutoTriggerEvaluator DECIDES whether to fire T7:
    if memory_health < 0.3 → TriggerEvent::MemoryHealthCritical → QUARANTINE

KEY PRINCIPLE: The monitor does NOT push triggers to the kill switch.
  The monitor publishes STATE. The gateway reads state and DECIDES.
  This means:
  ├── Monitor crash does NOT trigger kill switch (no push = no trigger)
  ├── Monitor publishing bad data does NOT auto-kill (gateway validates)
  ├── Gateway can fall back to direct cortex queries if monitor is down
  │   (Path C, per §2.8 T7 and GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md)
  └── The kill switch is ALWAYS gateway-owned, never monitor-owned
```


### 13.3 QUARANTINE Supersedes Level 4

```
SCENARIO: Agent is at convergence Level 4 (task-only mode).
  Kill switch fires T3 (5 policy denials) → QUARANTINE.

QUESTION: What happens to the convergence Level 4 restrictions?

ANSWER: QUARANTINE supersedes Level 4 entirely.
  ├── Level 4 restricts WHAT the agent can do (task-only tools).
  ├── QUARANTINE stops the agent COMPLETELY (no tools, no channels, no execution).
  ├── QUARANTINE is strictly more restrictive than Level 4.
  ├── When QUARANTINE is active, Level 4 restrictions are irrelevant
  │   (you can't restrict a stopped agent further).
  └── The convergence state (Level 4) is PRESERVED during quarantine.
      When/if the agent is resumed from quarantine, it returns to
      whatever convergence level the monitor has computed.

ORDERING:
  1. Kill switch QUARANTINE fires → agent isolated immediately
  2. Convergence monitor continues computing scores (it doesn't know
     about the quarantine — it's a separate process)
  3. Monitor may de-escalate from Level 4 → Level 3 during quarantine
     (if the agent being stopped causes scores to improve)
  4. Owner resumes agent from quarantine
  5. Agent resumes at CURRENT convergence level (which may have changed
     during quarantine)
  6. This is correct behavior — the two systems are independent
```


### 13.4 Monitor Crash Does NOT Trigger Kill Switch

```
SCENARIO: Convergence monitor process crashes (OOM, panic, segfault).
  Per CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4:
    T=0     Session active, convergence Level = 1
    T=100   Monitor crashes
    T=101   itp_emitter.send() fails → events buffered (up to 1000)
    T=102   Gateway health.rs detects monitor down (30s poll interval)
            → Set gateway.convergence_mode = DEGRADED
            → Log CRITICAL: "Convergence monitor lost"
    T=103   Shared state file becomes STALE
            → Policy reads last-known state (level=1)
            → Does NOT fall back to level=0 (conservative)

KILL SWITCH IMPACT:
  ├── T1 (SOUL drift): UNAFFECTED. Drift detection is gateway-owned
  │   (ghost-identity crate, checked by gateway). Does not depend on monitor.
  ├── T2 (Spending cap): UNAFFECTED. Cost tracking is gateway-owned
  │   (ghost-gateway/cost/). Does not depend on monitor.
  ├── T3 (Policy denials): UNAFFECTED. Policy engine is gateway-owned
  │   (ghost-policy). Reads stale convergence state (conservative).
  ├── T4 (Sandbox escape): UNAFFECTED. Sandbox is skill-owned
  │   (ghost-skills/sandbox/). Does not depend on monitor.
  ├── T5 (Credential exfil): UNAFFECTED. Credential broker is skill-owned
  │   (ghost-skills/credential/). Does not depend on monitor.
  ├── T6 (Multi-agent quarantine): UNAFFECTED. Quarantine count is
  │   gateway-owned (ghost-gateway/safety/quarantine.rs).
  └── T7 (Memory health): PARTIALLY AFFECTED.
      ├── Path A (HTTP API poll): FAILS — monitor is down.
      ├── Path B (Shared state file): STALE — last-known value persists.
      │   If last-known memory_health was 0.31 (above 0.3 threshold):
      │   T7 does NOT fire. This is a known gap (see DECISION 9, §11).
      └── Path C (Direct cortex queries): ACTIVATES as fallback.
          Gateway queries cortex-observability::health_score() directly.
          Stricter threshold (0.2 vs 0.3) compensates for fewer signals.
          60s poll interval (vs 30s normal).
          If cortex also unreachable: memory_health = "unknown", no trigger.
          (No data = no false positive. Conservative.)

SUMMARY: Monitor crash degrades T7 detection quality but does NOT
  disable the kill switch. 6 of 7 triggers are completely unaffected.
  T7 falls back to Path C with reduced fidelity but stricter thresholds.
```


---

## 14. KILL ALL INTERACTION WITH SHUTDOWN COORDINATOR

> Cross-reference: GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §Shutdown Sequence
> Cross-reference: GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §ShutdownCoordinator struct

```
CRITICAL DISTINCTION: KILL ALL ≠ FULL SHUTDOWN.

  KILL ALL (kill switch Level 3):
  ├── All agents STOPPED. No execution, no channels, no heartbeat.
  ├── Gateway process STAYS ALIVE.
  ├── API server STAYS ALIVE (dashboard accessible, safety endpoints work).
  ├── SQLite connections STAY OPEN (audit log still writable).
  ├── Owner can resume via dashboard or API.
  └── This is SAFE MODE — the platform is alive but inert.

  FULL SHUTDOWN (SIGTERM/SIGINT or ShutdownCoordinator):
  ├── Gateway process EXITS.
  ├── All connections closed. All state flushed to disk.
  ├── Process must be restarted externally (systemd, launchd, manual).
  └── This is PROCESS TERMINATION — nothing is running.

KILL ALL triggers SAFE MODE, not FULL SHUTDOWN.
The gateway must remain alive so the owner can investigate and resume.
```


### 14.1 KILL ALL Execution vs. Shutdown Sequence Comparison

```
KILL ALL (§3.6 execute_kill_all):          SHUTDOWN (ShutdownCoordinator):
─────────────────────────────────          ──────────────────────────────
1. Acquire write lock                      S1. Stop accepting connections
2. Set PLATFORM_KILLED atomic              S2. Drain lane queues (30s)
3. Stop all agent turns (abort in-flight)  S3. Memory flush (skip if kill switch)
4. Stop all channel adapters               S4. Persist cost tracking
5. Enter safe mode                         S5. Notify convergence monitor
6. Persist kill_state.json                 S6. Close channel adapters
7. Audit log entry                         S7. Close SQLite, flush WAL
8. Notification to owner                   EXIT: Process terminates
9. Gateway stays alive (API accessible)

KEY DIFFERENCES:
  ├── KILL ALL does NOT drain lane queues gracefully.
  │   In-flight turns are ABORTED, not waited on.
  │   Reason: Security threat — waiting 30s is unacceptable.
  │
  ├── KILL ALL does NOT flush memory (no S3 equivalent).
  │   Per GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md:
  │   "SKIP IF: kill switch Level 3 (kill switch skips flush for immediate stop)"
  │   Reason: Safety > data preservation. A compromised agent should NOT
  │   get a final turn to write to memory.
  │
  ├── KILL ALL does NOT close SQLite connections.
  │   Database stays open for audit logging and dashboard queries.
  │
  ├── KILL ALL does NOT terminate the process.
  │   Gateway stays alive in safe mode.
  │
  └── KILL ALL DOES persist cost tracking (audit trail integrity).
      Even in emergency, cost data must be preserved for forensics.
```


### 14.2 ShutdownCoordinator Awareness of Kill Switch

```
ShutdownCoordinator (ghost-gateway/src/shutdown.rs) handles THREE shutdown reasons:

  pub enum ShutdownReason {
      Signal(SignalKind),        // SIGTERM, SIGINT
      KillSwitch { level: u8 }, // Kill switch Level 3 (KILL ALL)
      ApiRequest,               // POST /api/shutdown (admin endpoint)
  }

WHEN KILL ALL FIRES, ShutdownCoordinator is NOT invoked for safe mode.
  ├── KILL ALL enters safe mode via Gateway::enter_safe_mode()
  │   (§3.6 step 5). This is kill_switch.rs calling gateway methods directly.
  ├── ShutdownCoordinator is only invoked if the owner ALSO wants to
  │   fully terminate the process after KILL ALL.
  └── Sequence:
      1. KILL ALL fires → safe mode entered (kill_switch.rs)
      2. Owner investigates via dashboard
      3. Owner decides to fully shut down (optional)
      4. Owner sends SIGTERM or POST /api/shutdown
      5. ShutdownCoordinator::initiate(ShutdownReason::Signal) fires
      6. Shutdown sequence runs (S1-S7)
      7. S3 (memory flush) is SKIPPED because is_kill_switch = true
         (ShutdownCoordinator checks if PLATFORM_KILLED is set)
      8. Process exits

EDGE CASE: What if SIGTERM arrives DURING KILL ALL execution?
  ├── KILL ALL is mid-execution (e.g., stopping agents at step 3)
  ├── SIGTERM fires → ShutdownCoordinator::initiate() called
  ├── ShutdownCoordinator sets GatewayState::ShuttingDown
  ├── KILL ALL execution continues (it's already in progress)
  ├── After KILL ALL completes, shutdown sequence runs
  ├── Most steps are no-ops (agents already stopped, adapters already closed)
  └── Process exits cleanly

EDGE CASE: What if KILL ALL fires DURING normal shutdown?
  ├── Shutdown is at S2 (draining lane queues, waiting 30s)
  ├── KILL ALL fires (e.g., sandbox escape detected during drain)
  ├── KILL ALL aborts all in-flight turns immediately
  ├── S2 drain completes early (nothing left to drain)
  ├── S3 memory flush is SKIPPED (kill switch active)
  ├── Shutdown continues from S4 onward
  └── Process exits. kill_state.json persisted (for next restart).
```


### 14.3 Gateway Restart After KILL ALL

```
SCENARIO: Gateway was in safe mode (KILL ALL active). Process was terminated
  (SIGTERM, crash, or owner-initiated). Now restarting.

RESTART SEQUENCE (ghost-gateway/src/bootstrap.rs):

  1. Bootstrap begins normally (load config, init SQLite, etc.)

  2. CHECK: Does ~/.ghost/safety/kill_state.json exist?
     ├── YES → Read file:
     │   {
     │     "level": "KillAll",
     │     "triggered_by": "T4_SandboxEscape",
     │     "triggered_at": "2026-02-27T15:30:00Z",
     │     "agent_states": { "agent_1": "quarantined", "agent_2": "quarantined" }
     │   }
     │   → Start in SAFE MODE (not normal mode)
     │   → Set PLATFORM_KILLED atomic = true
     │   → Do NOT spawn agent runtimes
     │   → Do NOT connect channel adapters
     │   → Do NOT start heartbeat engine
     │   → DO start API server (dashboard must be accessible)
     │   → DO start health endpoint (monitoring must work)
     │   → Log: "WARN: Starting in SAFE MODE. Kill state present."
     │
     └── NO → Start normally
         → Spawn AutoTriggerEvaluator
         → Connect channel adapters
         → Start heartbeat engine
         → Resume agents

  3. ALSO CHECK: SQLite kill_switch_audit table (if DECISION 5 is implemented)
     ├── If kill_state.json is MISSING but SQLite has unresolved KILL ALL:
     │   → Treat as KILL ALL still active (file may have been deleted)
     │   → Start in SAFE MODE
     │   → Log: "CRITICAL: kill_state.json missing but SQLite shows active KILL ALL.
     │           Possible tampering. Starting in SAFE MODE."
     └── If both are clear → normal start

  4. RESUME PATH (owner action required):
     Option A — Manual file deletion:
       rm ~/.ghost/safety/kill_state.json
       Restart gateway → detects no kill state → starts normally
       All agents start fresh (no session resume from pre-kill state)

     Option B — Dashboard API:
       Dashboard is accessible in safe mode (API server runs)
       POST /api/safety/resume-platform
       ├── Requires GHOST_TOKEN authentication
       ├── Requires confirmation parameter: { "confirm": "RESUME_ALL" }
       ├── Clears PLATFORM_KILLED atomic
       ├── Deletes kill_state.json
       ├── Marks SQLite audit entry as resolved
       ├── Restarts all agents (fresh start, not resume)
       ├── Reconnects channel adapters
       ├── Restarts heartbeat engine
       ├── Spawns AutoTriggerEvaluator
       └── Audit log: "Platform resumed by owner via API"

     Option C — CLI command (if implemented):
       ghost-cli safety resume-platform --token=<GHOST_TOKEN> --confirm
       Same effect as Option B but from command line
```


### 14.4 Safe Mode Capabilities Matrix

```
┌──────────────────────────────────┬──────────┬───────────┬──────────┐
│ Capability                       │ Normal   │ Safe Mode │ Shutdown │
├──────────────────────────────────┼──────────┼───────────┼──────────┤
│ Agent execution                  │ ✓        │ ✗         │ ✗        │
│ Channel adapters (inbound)       │ ✓        │ ✗         │ ✗        │
│ Channel adapters (outbound)      │ ✓        │ ✗         │ ✗        │
│ Heartbeat engine                 │ ✓        │ ✗         │ ✗        │
│ Cron job execution               │ ✓        │ ✗         │ ✗        │
│ API server (dashboard)           │ ✓        │ ✓         │ ✗        │
│ Health endpoint                  │ ✓        │ ✓ (*)     │ ✗        │
│ Safety API endpoints             │ ✓        │ ✓         │ ✗        │
│ SQLite read/write                │ ✓        │ ✓         │ ✗        │
│ Audit log writes                 │ ✓        │ ✓         │ ✗        │
│ Convergence monitor (sidecar)    │ ✓        │ ✓ (**)    │ ✗        │
│ AutoTriggerEvaluator             │ ✓        │ ✗ (***)   │ ✗        │
│ Owner notification               │ ✓        │ ✓         │ ✗        │
│ Process alive                    │ ✓        │ ✓         │ ✗        │
├──────────────────────────────────┴──────────┴───────────┴──────────┤
│ (*) Health endpoint returns {"status": "safe_mode",               │
│     "kill_switch": {"level": "KillAll", "triggered_by": "...",    │
│     "triggered_at": "..."}}                                       │
│ (**) Monitor is a separate process — it continues running          │
│     independently. It may de-escalate convergence levels while     │
│     agents are stopped (scores improve with no interaction).       │
│ (***) AutoTriggerEvaluator is stopped because all agents are       │
│     stopped. No triggers can fire when nothing is executing.       │
│     It restarts when platform is resumed.                          │
└───────────────────────────────────────────────────────────────────┘
```


---

## 15. FILE MAPPING GAPS IDENTIFIED BY THIS DOCUMENT

> This section catalogs files, directories, and structures referenced in this
> document that are NOT currently present in FILE_MAPPING.md. Each gap must be
> resolved before implementation begins — either by adding the entry to
> FILE_MAPPING.md or by deciding the functionality belongs elsewhere.

```
GAP A: ghost-gateway/src/safety/ module (PARTIALLY MAPPED)

  FILE_MAPPING.md Finding 3 proposes:
    ghost-gateway/src/safety/
    ├── mod.rs
    ├── kill_switch.rs
    ├── auto_triggers.rs
    └── quarantine.rs

  This document ADDS the following files not in Finding 3:
    ghost-gateway/src/safety/
    ├── notification.rs       # Kill switch notification dispatch.
    │                         #   Owner notification via out-of-band channel.
    │                         #   CANNOT depend on convergence-monitor/transport/
    │                         #   notification.rs — that's a separate process.
    │                         #   Must work when monitor is down.
    │                         #   Channels: desktop notification (notify-rust),
    │                         #   webhook, email (lettre), SMS (if configured).
    │                         #   See DECISION 7 (§11) for ownership resolution.
    │
    └── state.rs              # KillSwitchState persistence.
                              #   Read/write kill_state.json.
                              #   Read/write SQLite kill_switch_audit table.
                              #   Startup check (§14.3 step 2-3).
                              #   Atomic state management.

  ACTION: Add notification.rs and state.rs to FILE_MAPPING.md Finding 3.
```


```
GAP B: convergence-monitor/transport/notification.rs (MAPPED but scope unclear)

  FILE_MAPPING.md lists:
    convergence-monitor/transport/notification.rs
    "Notification dispatch — desktop notifications (notify-rust),
     webhook calls, email (lettre)"

  PROBLEM: This file is in the convergence-monitor crate (separate process).
  Kill switch lives in ghost-gateway. If the monitor is down, this
  notification path is unavailable. The kill switch needs its OWN
  notification capability that works independently.

  CURRENT STATE: FILE_MAPPING.md does not distinguish between:
    a) Convergence monitor notifications (Level 1 nudges, Level 3 warnings,
       Level 4 external escalation)
    b) Kill switch notifications (PAUSE/QUARANTINE/KILL ALL alerts)

  ACTION: Either:
    1. Add ghost-gateway/src/safety/notification.rs (GAP A above), OR
    2. Extract shared notification crate used by both, OR
    3. Document that kill switch notifications use a different mechanism
       (e.g., direct webhook call from gateway, no shared code)
  See DECISION 7 (§11).
```

```
GAP C: ghost-agent-loop/src/output_inspector.rs (NOT MAPPED)

  This document references OutputInspector for T5 Path B
  (credential pattern scanning in agent output text).

  FILE_MAPPING.md does NOT list this file.
  ghost-agent-loop/src/ currently maps:
    runner.rs, context/, tools/, itp_emitter.rs

  SimulationBoundaryEnforcer (simulation-boundary crate) already has
  scan_output() for emulation language detection. Credential scanning
  is a different concern but similar mechanism (regex/pattern matching
  on output text).

  OPTIONS (see DECISION 8, §11):
    a) New file: ghost-agent-loop/src/output_inspector.rs
    b) Extend SimulationBoundaryEnforcer.scan_output() to also check
       for credential patterns
    c) New crate: ghost-output-filter (shared output scanning pipeline
       that both simulation boundary and credential detection use)

  ACTION: Resolve DECISION 8, then add to FILE_MAPPING.md.
```


```
GAP D: Safety API Endpoints (NOT MAPPED in routes.rs)

  FILE_MAPPING.md lists ghost-gateway/src/api/routes.rs with:
    GET /api/convergence/scores
    GET /api/convergence/history
    GET /api/interventions

  This document specifies these ADDITIONAL endpoints (§10 Integration Points):
    POST /api/safety/kill-all           # Manual KILL ALL
    POST /api/safety/pause/{agent_id}   # Manual PAUSE
    POST /api/safety/quarantine/{agent_id}  # Manual QUARANTINE
    POST /api/safety/resume/{agent_id}  # Resume from PAUSE/QUARANTINE
    POST /api/safety/resume-platform    # Resume from KILL ALL
    GET  /api/safety/status             # Current kill switch state
    GET  /api/safety/triggers           # Recent trigger history

  These are SEPARATE from convergence endpoints:
    Convergence = soft interventions (Levels 0-4, monitor-owned)
    Safety = hard interventions (PAUSE/QUARANTINE/KILL ALL, gateway-owned)

  ACTION: Add safety endpoints to FILE_MAPPING.md routes.rs description.
  All POST endpoints require GHOST_TOKEN authentication.
  resume-platform requires additional confirmation parameter.
```

```
GAP E: ~/.ghost/safety/ directory (NOT MAPPED)

  This document references:
    ~/.ghost/safety/kill_state.json

  FILE_MAPPING.md maps these data directories:
    ~/.ghost/data/                      # General data
    ~/.ghost/data/convergence_state/    # Convergence shared state files
    ~/.ghost/config/                    # Configuration
    ~/.ghost/logs/                      # Log files

  But ~/.ghost/safety/ is NOT mapped anywhere.

  This directory should contain:
    ~/.ghost/safety/
    ├── kill_state.json          # Persisted kill switch state (§3.6 step 6)
    │                            #   Checked on gateway restart (§14.3)
    │                            #   Deleted on platform resume
    └── audit/                   # Kill switch audit trail (optional,
                                 #   primary audit is in SQLite but
                                 #   file-based backup may be desired
                                 #   per DECISION 5)

  ACTION: Add ~/.ghost/safety/ to FILE_MAPPING.md directory structure.
```


```
GAP F: health.rs Memory Health Computation (SCOPE UNCLEAR)

  FILE_MAPPING.md lists ghost-gateway/src/health.rs:
    "Health endpoint — /health, /ready, /metrics.
     Checks: SQLite writable, convergence monitor reachable
     (GET monitor /health), channel adapters connected, disk space adequate.
     Returns degraded status if monitor unreachable
     (safety floor absent — logged as critical)."

  This document adds T7 responsibilities to health.rs:
    ├── Include kill switch state in health response
    ├── Include AutoTriggerEvaluator task health
    ├── Include trigger channel depth metric
    └── Memory health computation for T7 Path C fallback
        (direct cortex queries when monitor is down)

  The T7 Path C fallback means health.rs (or a module it calls) must:
    ├── Query cortex-observability::health_score(agent_id)
    ├── Compute memory health from: contradiction_count + hash chain integrity
    ├── Apply stricter threshold (0.2 vs 0.3 normal)
    ├── Report this in the health endpoint response

  QUESTION: Should T7 Path C logic live in health.rs or in auto_triggers.rs?
    health.rs is for REPORTING health status.
    auto_triggers.rs is for EVALUATING triggers.
    The Path C fallback is trigger evaluation, not health reporting.

  RECOMMENDATION: Path C logic lives in auto_triggers.rs.
    health.rs REPORTS the kill switch state and evaluator health.
    auto_triggers.rs COMPUTES T7 via Path A/B/C and fires triggers.
    health.rs does NOT compute memory health — it reads the result.

  ACTION: Update FILE_MAPPING.md health.rs description to include
  kill switch state reporting. Clarify that T7 Path C computation
  is in auto_triggers.rs, not health.rs.
```

```
GAP SUMMARY:
┌─────┬──────────────────────────────────────┬──────────────────────┐
│ Gap │ What's Missing                       │ Where to Add         │
├─────┼──────────────────────────────────────┼──────────────────────┤
│  A  │ safety/notification.rs, state.rs     │ FILE_MAPPING Finding 3│
│  B  │ Kill switch vs monitor notification  │ FILE_MAPPING Finding 3│
│     │ ownership distinction                │ + DECISION 7         │
│  C  │ output_inspector.rs for T5 Path B   │ FILE_MAPPING new     │
│     │                                      │ + DECISION 8         │
│  D  │ Safety API endpoints in routes.rs    │ FILE_MAPPING routes  │
│  E  │ ~/.ghost/safety/ directory           │ FILE_MAPPING dirs    │
│  F  │ health.rs kill switch reporting +    │ FILE_MAPPING health  │
│     │ T7 Path C computation ownership     │ + auto_triggers.rs   │
└─────┴──────────────────────────────────────┴──────────────────────┘

TOTAL: 6 gaps. All must be resolved in FILE_MAPPING.md before
implementation begins. None are blockers for the kill switch
implementation itself — they are documentation gaps that need
to be closed to maintain the "zero errors, zero oversights"
standard this document targets.
```