# Proposal Lifecycle — Complete Sequence Flow

> Codename: GHOST (General Hybrid Orchestrated Self-healing Taskrunner)
> Date: 2026-02-27
> Scope: Agent output → Extraction → Validation → Branching → Storage/Feedback → Dashboard → Agent Loop
> Cross-cutting crates: ghost-agent-loop, cortex-validation, cortex-storage, cortex-temporal, ghost-policy, ghost-gateway (API/dashboard), convergence-monitor
> Prerequisite reading: FILE_MAPPING.md, AGENT_ARCHITECTURE.md, docs/13, docs/19, docs/20

---

## 0. GLOSSARY OF PARTICIPANTS

| Participant | Crate | File(s) | Role |
|-------------|-------|---------|------|
| AgentRunner | ghost-agent-loop | `src/runner.rs` | The recursive agentic loop. Produces raw LLM output containing embedded state change proposals. |
| ProposalExtractor | ghost-agent-loop | `src/proposal/extractor.rs` | Parses raw LLM output into structured `Proposal` objects. |
| ProposalRouter | ghost-agent-loop | `src/proposal/router.rs` | Routes extracted proposals to the correct validation path based on proposal type. |
| ProposalValidator | cortex-validation | `src/proposal_validator.rs` | 7-dimension validation gate (D1-D7). Produces `ProposalValidationResult` with a `ProposalDecision`. |
| ValidationEngine | cortex-validation | `src/engine.rs` | Existing 4-dimension engine (D1-D4: citation, temporal, contradiction, pattern alignment). Wrapped by ProposalValidator. |
| SimulationBoundaryEnforcer | simulation-boundary | `src/enforcer.rs` | Scans agent TEXT OUTPUT (response to user) for emulation language via `scan_output()`. Operates INDEPENDENTLY from D7 in ProposalValidator. Has 3 enforcement modes: soft (flag+log), medium (rewrite), hard (block+regenerate). Implements `IBoundaryEnforcer` trait. |
| OutputReframer | simulation-boundary | `src/reframer.rs` | Rewrites emulation language in agent text output to simulation-framed alternatives. Called by SimulationBoundaryEnforcer in medium mode. |
| PostRedirectVerifier | convergence-monitor | `src/verification/behavioral_verification.rs` | Tracks whether agent output ACTUALLY changes after an intervention redirect. Compares pre/post-redirect output embeddings. Detects deceptive compliance (agent acknowledges redirect but behavior unchanged). Feeds into convergence scoring as amplifier. |
| IReflectionEngine | (trait in cortex-core) | `src/traits/convergence.rs` | Reflection depth controller. `can_reflect()` checks depth/session limits BEFORE ProposalValidator runs. `record_reflection()` commits. `chain_depth()` queries current depth. |
| ConvergenceAwareFilter | cortex-convergence | `src/filtering/convergence_aware_filter.rs` | Filters memories by convergence tier before agent sees them. 4 tiers: 0.0-0.3 full access, 0.3-0.5 reduced emotional, 0.5-0.7 task-focused, 0.7+ minimal. Affects what proposals the agent generates upstream. |
| GoalProposalQueries | cortex-storage | `src/queries/goal_proposal_queries.rs` | SQL insert/query/resolve for the `goal_proposals` table. |
| ReflectionQueries | cortex-storage | `src/queries/reflection_queries.rs` | SQL insert/query for the `reflection_entries` table. |
| MemoryCRUD | cortex-storage | `src/queries/memory_crud.rs` | Existing memory write path (INSERT into `memories` + `memory_events`). |
| TemporalEvents | cortex-storage | `src/temporal_events.rs` | Event append with hash chain integration. |
| HashChainEngine | cortex-temporal | `src/hash_chain.rs` | `compute_event_hash()`, `GENESIS_HASH`, `verify_chain()`. Produces blake3 hash for every event. |
| DenialFeedback | ghost-policy | `src/feedback.rs` | Structured denial message: reason, constraint violated, suggested alternatives. Injected into next prompt. |
| PolicyEngine | ghost-policy | `src/engine.rs` | Cedar-style authorization. Evaluates tool calls. Produces `PolicyDecision` (Permit/Deny/Escalate). |
| ConvergencePolicyTightener | ghost-policy | `src/policy/convergence_policy.rs` | Automatically restricts agent CAPABILITIES (not just validation thresholds) as intervention level rises. Level 0-1: full capabilities. Level 2: reduced proactive actions. Level 3: session caps enforced. Level 4: task-only mode (no proactive, no goal proposals). This is UPSTREAM of proposal extraction — it limits what the agent can DO, which limits what proposals it generates. |
| GatewayAPI | ghost-gateway | `src/api/routes.rs` | REST + WebSocket endpoints. `GET /api/goals`, `POST /api/goals/{id}/approve`, `POST /api/goals/{id}/reject`. |
| WebSocketHandler | ghost-gateway | `src/api/websocket.rs` | Real-time event push to dashboard. Pushes proposal events, approval requests. |
| DashboardGoalsPage | dashboard | `src/routes/goals/+page.svelte` | Goal tracker UI: active goals, pending proposals, approval queue. |
| DashboardAPIClient | dashboard | `src/lib/api.ts` | WebSocket + REST client connecting to ghost-gateway. |
| PromptCompiler | ghost-agent-loop | `src/context/prompt_compiler.rs` | 10-layer context assembly. Layer 6 includes convergence state + filtered goals/reflections. |
| ReadOnlyPipeline | read-only-pipeline | `src/assembler.rs` | Assembles the read-only state snapshot the agent receives each turn. |
| ConvergenceMonitor | convergence-monitor | `src/monitor.rs` | Sidecar process. Receives ITP events, computes convergence scores, triggers interventions. |
| ITPEmitter | ghost-agent-loop | `src/itp_emitter.rs` | Emits ITP telemetry events from the agent loop to the convergence monitor. |
| AuditLogger | ghost-audit / ghost-gateway | `src/api/routes.rs` + cortex-storage | Append-only audit trail for all proposal decisions. |

---

## 1. TRIGGER: AGENT PRODUCES OUTPUT WITH STATE CHANGE PROPOSALS

### 1.1 Where This Starts

The agentic loop in `ghost-agent-loop/src/runner.rs` runs recursively:

```
LLM call → parse response → if tool_call: execute tool → append result → LLM call again
                           → if text: stream to user → extract proposals → persist
                           → if NO_REPLY: suppress output → persist
```

After the LLM produces its final text response (no more tool calls), the runner enters
the PROPOSAL EXTRACTION phase. This is Step 6 in the agentic loop (per docs/20 §8.2).

The agent's text output may contain embedded state change proposals in three categories:
1. Goal changes (create, update, archive goals)
2. Reflection writes (agent reasoning about its own behavior)
3. Memory writes (new facts, observations, procedural knowledge)

The agent does NOT write directly to any store. Every write is a proposal.

### 1.2 Data at This Point

```
AgentResponse {
    text: String,              // The full LLM response text
    tool_calls: Vec<ToolCall>, // Already executed tool calls from this turn
    proposals: Vec<Proposal>,  // NOT YET POPULATED — extractor fills this
    cost: CostBreakdown,
    token_usage: TokenUsage,
    duration: Duration,
}
```

### 1.3 Crate Boundary

```
ghost-agent-loop/src/runner.rs  →  calls  →  ghost-agent-loop/src/proposal/extractor.rs
```

### 1.4 CRITICAL: Output-Level Simulation Boundary Enforcement (BEFORE Extraction)

Before proposals are extracted, the agent's TEXT OUTPUT is scanned by the
`SimulationBoundaryEnforcer` (from the `simulation-boundary` crate). This is a
SEPARATE enforcement point from D7 in the ProposalValidator:

```
Agent produces text response
    │
    ▼
SimulationBoundaryEnforcer::scan_output(agent_text)
    │
    ├─ Soft mode: Flag + log violations, let response through with note to human
    ├─ Medium mode: Call OutputReframer::reframe() to rewrite emulation language
    │               to simulation-framed alternatives, then send rewritten response
    └─ Hard mode: BLOCK the response entirely, regenerate with reinforced
                  boundary prompt (re-run LLM with stronger simulation anchor)
    │
    ▼
If violations detected → INSERT INTO boundary_violations table
                        → ITP event emitted (ConvergenceAlert)
                        → Convergence score affected
    │
    ▼
THEN proceed to proposal extraction on the (possibly reframed) text
```

This means there are TWO emulation language enforcement points:
1. `simulation-boundary/enforcer.rs` → scans the FULL TEXT OUTPUT to the user
2. `cortex-validation/dimensions/emulation_language.rs` (D7) → scans PROPOSAL CONTENT

They share similar regex patterns but serve different purposes:
- The enforcer protects the HUMAN from seeing emulation language
- D7 protects the STATE STORES from containing emulation language

An agent response could pass the enforcer (soft mode, flagged but sent) but have its
embedded proposals rejected by D7 (hard reject on proposal content).

### 1.5 Upstream Convergence Effects on Proposal Generation

Before the agent even generates output, two systems constrain what it can propose:

```
ConvergencePolicyTightener (ghost-policy/src/policy/convergence_policy.rs):
  Level 0-1: Full capabilities. Agent can propose goals, reflections, memories freely.
  Level 2:   Reduced proactive actions. Agent's tool access restricted.
             Fewer tool results → fewer observations → fewer proposals.
  Level 3:   Session caps enforced. Shorter sessions → less context → fewer proposals.
  Level 4:   Task-only mode. Agent cannot propose goals or reflections at all.
             Only task-relevant memory writes permitted.

ConvergenceAwareFilter (cortex-convergence/src/filtering/convergence_aware_filter.rs):
  Score 0.0-0.3: Full memory access → agent sees all relevant memories
  Score 0.3-0.5: Reduced emotional/attachment pattern weight → agent sees less
                 relationship-oriented context → proposals less likely to be personal
  Score 0.5-0.7: Exclude attachment patterns, increase task-focused → agent's
                 proposals become more task-oriented by design
  Score 0.7+:    Minimal memory — task-relevant only → agent can only propose
                 task-related content because that's all it sees

These are UPSTREAM constraints. They shape what the agent generates BEFORE
extraction and validation even begin.
```

---

## 2. PROPOSAL EXTRACTION (ghost-agent-loop → extractor.rs)

### 2.1 What Happens

`ProposalExtractor` in `ghost-agent-loop/src/proposal/extractor.rs` parses the raw LLM
output text and extracts structured `Proposal` objects.

### 2.2 Extraction Logic

The extractor looks for structured markers in the agent's output. The agent is instructed
(via the prompt compiler's tool schemas at Layer 3) to emit proposals in a parseable format.
Three extraction paths:

```
┌─────────────────────────────────────────────────────────────────┐
│  ProposalExtractor::extract(agent_response: &str)               │
│                                                                  │
│  1. Scan for GOAL CHANGE markers                                 │
│     Pattern: structured JSON blocks or tool-call-style markers   │
│     Extract: operation (Create/Update/Archive), goal_text,       │
│              goal_scope (Task/Session/Project/Persistent),       │
│              parent_goal_id (if sub-goal)                        │
│     Output: Proposal { operation: GoalChange, ... }              │
│                                                                  │
│  2. Scan for REFLECTION WRITE markers                            │
│     Pattern: reflection blocks with chain_id, depth              │
│     Extract: reflection_text, trigger_type, state_read,          │
│              proposed_changes                                    │
│     Output: Proposal { operation: ReflectionWrite, ... }         │
│                                                                  │
│  3. Scan for MEMORY WRITE markers                                │
│     Pattern: memory write blocks with type, content              │
│     Extract: memory_type, content, importance, tags,             │
│              linked entities                                     │
│     Output: Proposal { operation: PatternWrite/Create/Update, .. }│
│                                                                  │
│  Returns: Vec<Proposal>                                          │
└─────────────────────────────────────────────────────────────────┘
```

### 2.3 The Proposal Struct

Defined in `cortex-core/src/traits/convergence.rs`:

```rust
pub struct Proposal {
    pub id: String,                            // UUID v7
    pub proposer: CallerType,                  // Platform | Agent { agent_id } | Human { user_id }
    pub operation: ProposalOperation,          // Create | Update | Archive | GoalChange | ReflectionWrite | PatternWrite
    pub target_memory_id: Option<String>,      // For updates/archives — existing memory ID
    pub target_type: MemoryType,               // Which of the 31 memory types
    pub content: serde_json::Value,            // The proposed content (JSON, NOT String)
    pub cited_memory_ids: Vec<String>,         // Memory IDs this proposal references (D6 checks this)
    pub session_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

CRITICAL NOTES on the Proposal struct:
- `content` is `serde_json::Value`, NOT `String`. The extractor must serialize proposal
  content as structured JSON. The validator deserializes per operation type.
- `cited_memory_ids` is what D6 (self-reference density) checks against. The extractor
  MUST populate this by scanning the proposal content for memory ID references.
- `CallerType::Agent { agent_id }` and `CallerType::Human { user_id }` carry identifying
  data. This is used for audit trail attribution and the `resolved_by` field in
  goal_proposals (e.g., `"human:user_123"` or `"agent:developer"`).

### 2.3.1 CallerType Enum (Exact Definition)

```rust
pub enum CallerType {
    Platform,                          // Full access. Used for system-generated proposals.
    Agent { agent_id: String },        // Restricted access. Most proposals come from here.
    Human { user_id: String },         // Full access, different audit trail. Dashboard actions.
}
```

`CallerType` methods used in proposal validation:
- `is_platform()` → true only for Platform variant
- `is_agent()` → true for Agent { .. } variant
- `can_create_type(memory_type, config)` → checks against `config.restricted_types`
- `can_assign_importance(importance, config)` → checks against `config.restricted_importance`
  (agents cannot assign `Importance::Critical`)

### 2.4 Edge Cases the Extractor Must Handle

| Edge Case | Handling |
|-----------|----------|
| No proposals in output | Return empty Vec. Normal for simple Q&A responses. |
| Malformed proposal markers | Log warning, skip malformed proposal, continue extracting others. |
| Multiple proposals in single output | Extract all. Each validated independently. |
| Agent tries to embed proposals in tool call results | Extractor only scans final text output, not tool results. |
| Agent omits required fields | Extractor fills defaults where safe (e.g., scope=Task), rejects if critical fields missing. |
| Proposal references non-existent memory_id | Passes through — validator catches this in D2 (temporal consistency). |
| Agent output was reframed by SimulationBoundaryEnforcer | Extractor runs on the REFRAMED text (medium mode) or the original text (soft mode). In hard mode, the LLM is re-run and extractor runs on the new output. |
| Proposal content contains memory IDs | Extractor MUST populate `cited_memory_ids` by scanning content for UUID patterns matching existing memory IDs. This is critical for D6 self-reference checking. |
| Proposal content is not valid JSON | Extractor wraps raw text in `serde_json::Value::String(text)`. The validator handles both structured and string content. |

### 2.5 Crate Boundary Crossing

```
ghost-agent-loop/src/proposal/extractor.rs
    produces: Vec<Proposal>
    passes to: ghost-agent-loop/src/proposal/router.rs
```

---

## 3. PROPOSAL ROUTING (ghost-agent-loop → router.rs)

### 3.1 What Happens

`ProposalRouter` in `ghost-agent-loop/src/proposal/router.rs` receives the extracted
proposals and routes each one to the appropriate validation path.

### 3.2 Routing Logic

```
┌─────────────────────────────────────────────────────────────────┐
│  ProposalRouter::route(proposals: Vec<Proposal>, ctx: &RouteCtx)│
│                                                                  │
│  For each proposal:                                              │
│                                                                  │
│  ┌─ Is proposal.operation == GoalChange?                         │
│  │   YES → Full 7-dimension ProposalValidator (D1-D7)            │
│  │         This is the STRICTEST path.                           │
│  │                                                               │
│  ├─ Is proposal.operation == ReflectionWrite?                    │
│  │   YES → Reflection-specific validation:                       │
│  │         FIRST: IReflectionEngine::can_reflect(chain_id, session_id)│
│  │         - Checks max_depth (default 3 per ReflectionConfig)   │
│  │         - Checks max_per_session (default 20)                 │
│  │         - Checks cooldown_seconds (default 30s between chains)│
│  │         - If can_reflect() returns false → AutoRejected       │
│  │           (depth/session/cooldown limit exceeded)             │
│  │         THEN: ProposalValidator for remaining dimensions:     │
│  │         - Self-reference ratio check (max 0.3 default)        │
│  │         - D6 (self-reference) + D7 (emulation) from validator │
│  │                                                               │
│  ├─ Is proposal.operation == PatternWrite | Create | Update?     │
│  │   YES → Memory write validation:                              │
│  │         - Novelty check (is this genuinely new?)              │
│  │         - Drift check (does this shift agent's human model?)  │
│  │         - Growth rate check (memory growing too fast?)        │
│  │         - D1-D4 from base ValidationEngine                    │
│  │         - D7 (emulation) from validator                       │
│  │                                                               │
│  └─ Is proposal.operation == Archive?                            │
│      YES → Minimal validation:                                   │
│            - Verify target_memory_id exists                      │
│            - Verify proposer has permission to archive            │
│            - CallerType check (agent can't archive Critical+)    │
│                                                                  │
│  ALL paths → ProposalValidator.validate(proposal, ctx)           │
│  (Validator internally adjusts which dimensions to run based on  │
│   the operation type, but the entry point is the same)           │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 Context Assembly for Validation

The router assembles a `ProposalContext` before calling the validator:

```rust
struct ProposalContext {
    active_goals: Vec<BaseMemory>,          // Current approved goals (for D5 scope comparison)
    recent_agent_memories: Vec<BaseMemory>, // Recent agent-authored memories (for D6 self-ref)
    convergence_score: f64,                 // Current composite convergence score
    convergence_level: u8,                  // Current intervention level (0-4)
    session_id: String,
    session_reflection_count: u32,          // Reflections already written this session
    session_memory_write_count: u32,        // Memory writes already this session
    daily_memory_growth: u32,               // Total memories created today (growth rate check)
}
```

This context is assembled by querying:
- `cortex-storage/queries/goal_proposal_queries.rs` → active goals
- `cortex-storage/queries/memory_crud.rs` → recent agent memories
- `convergence-monitor` (via HTTP GET /scores or cached value) → convergence score/level
- `cortex-storage/queries/reflection_queries.rs` → session reflection count

### 3.3.1 ReflectionConfig Defaults (from ConvergenceConfig)

For reflection proposals, the router also loads `ReflectionConfig`:

```rust
ReflectionConfig {
    max_depth: 3,                    // Max reflection chain depth per turn
    max_per_session: 20,             // Max total reflections per session
    cooldown_seconds: 30,            // Min seconds between reflection chains
    max_self_reference_ratio: 0.3,   // Max ratio of self-citing reflections
}
```

These are checked by `IReflectionEngine::can_reflect()` BEFORE the ProposalValidator
runs. If any limit is exceeded, the reflection proposal is rejected without reaching
the 7-dimension validation.

### 3.4 Convergence Level Threshold Tightening

CRITICAL: Before calling the validator, the router applies convergence-level-based
threshold tightening via `ProposalValidator::with_convergence_level(level)`:

```
Level 0 (Normal):     scope_expansion_max=0.6, self_reference_max_ratio=0.3
Level 1 (Elevated):   scope_expansion_max=0.5, self_reference_max_ratio=0.25
Level 2 (High):       scope_expansion_max=0.4, self_reference_max_ratio=0.2
Level 3+ (Critical):  scope_expansion_max=0.3, self_reference_max_ratio=0.15
```

This means: as convergence risk increases, the validation gate gets STRICTER.
More proposals get flagged for human review. Fewer auto-approve.

### 3.5 Crate Boundary Crossing

```
ghost-agent-loop/src/proposal/router.rs
    calls: cortex-validation/src/proposal_validator.rs :: ProposalValidator::validate()
    passes: (&Proposal, &ProposalContext)
    receives: CortexResult<ProposalValidationResult>
```

---

## 4. 7-DIMENSION PROPOSAL VALIDATION (cortex-validation → proposal_validator.rs)

### 4.1 Validation Pipeline (Sequential, All 7 Dimensions)

```
┌─────────────────────────────────────────────────────────────────┐
│  ProposalValidator::validate(proposal, ctx)                      │
│                                                                  │
│  STEP 0: PRE-CHECK — Restricted Type Guard                       │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ if proposal.target_type.is_platform_restricted()         │    │
│  │    && !proposal.proposer.is_platform()                   │    │
│  │ → IMMEDIATE AutoRejected                                 │    │
│  │   (Agent cannot create Core, ConvergenceEvent,           │    │
│  │    BoundaryViolation, or InterventionPlan memories.       │    │
│  │    NOTE: SimulationResult is NOT restricted — agents can  │    │
│  │    create those. Only the 4 types above are restricted.)  │    │
│  │                                                          │    │
│  │ ALSO: Agent cannot assign Importance::Critical            │    │
│  │   (checked via CallerType::can_assign_importance())       │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  STEP 1: D1-D4 — Base ValidationEngine                           │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ D1: Citation validation                                  │    │
│  │     Does the proposal cite real, verifiable sources?     │    │
│  │     Score: 0.0 (no citations) to 1.0 (all verified)     │    │
│  │                                                          │    │
│  │ D2: Temporal consistency                                 │    │
│  │     Are timestamps, references, and sequences valid?     │    │
│  │     Does target_memory_id exist if specified?            │    │
│  │     Score: 0.0 (inconsistent) to 1.0 (fully consistent) │    │
│  │                                                          │    │
│  │ D3: Contradiction detection                              │    │
│  │     Does this contradict existing approved memories?     │    │
│  │     Score: 0.0 (direct contradiction) to 1.0 (no conflict)│   │
│  │                                                          │    │
│  │ D4: Pattern alignment                                    │    │
│  │     Does this align with established codebase/behavioral │    │
│  │     patterns? Score: 0.0 (misaligned) to 1.0 (aligned)  │    │
│  │                                                          │    │
│  │ Combined pass threshold: 0.7 (raised from 0.5 for       │    │
│  │ convergence safety)                                      │    │
│  │                                                          │    │
│  │ If combined D1-D4 score < 0.7 → base_passed = false     │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  STEP 2: D5 — Scope Expansion Detection                         │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ Only applies to GoalChange proposals.                    │    │
│  │ Non-goal proposals score 0.0 (always pass D5).           │    │
│  │                                                          │    │
│  │ Algorithm:                                               │    │
│  │ 1. Tokenize proposed goal text                           │    │
│  │    (Extract string from serde_json::Value first:         │    │
│  │     proposal.content.as_str() or to_string())            │    │
│  │ 2. Tokenize all existing active goal texts               │    │
│  │ 3. Compute Jaccard distance:                             │    │
│  │    expansion = 1.0 - (intersection / union)              │    │
│  │ 4. If no existing goals: expansion = 0.8 (first goal    │    │
│  │    is inherently high-expansion)                         │    │
│  │                                                          │    │
│  │ Pass: expansion_score <= scope_expansion_max             │    │
│  │ Fail: expansion_score > scope_expansion_max              │    │
│  │       → Flags for human review                           │    │
│  │                                                          │    │
│  │ Thresholds (convergence-level dependent):                │    │
│  │   L0: 0.6 | L1: 0.5 | L2: 0.4 | L3+: 0.3              │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  STEP 3: D6 — Self-Reference Density                             │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ Checks if the agent is citing its own prior outputs      │    │
│  │ in a circular pattern (reflection loop risk).            │    │
│  │                                                          │    │
│  │ Algorithm:                                               │    │
│  │ 1. Get recent_agent_memories from ProposalContext        │    │
│  │ 2. Count how many of proposal.cited_memory_ids match     │    │
│  │    IDs in recent_agent_memories                          │    │
│  │    (Uses the structured cited_memory_ids field, NOT      │    │
│  │     raw string matching on content. The extractor        │    │
│  │     populates this field during extraction.)             │    │
│  │                                                          │    │
│  │    NOTE: The source code in docs/19 shows                │    │
│  │    proposal.content.contains(&m.id) — this is the        │    │
│  │    FALLBACK path when cited_memory_ids is empty.          │    │
│  │    Preferred path: check cited_memory_ids first.          │    │
│  │    Fallback: serialize content to string, then contains().│    │
│  │    let text = serde_json::to_string(&proposal.content)   │    │
│  │              .unwrap_or_default();                        │    │
│  │    let self_refs = ctx.recent_agent_memories.iter()       │    │
│  │      .filter(|m| proposal.cited_memory_ids.contains(&m.id)│   │
│  │                   || text.contains(&m.id))               │    │
│  │      .count();                                           │    │
│  │ 3. ratio = self_refs / total_recent_memories             │    │
│  │ 4. If no recent memories: ratio = 0.0 (pass)            │    │
│  │                                                          │    │
│  │ Pass: ratio <= self_reference_max_ratio                  │    │
│  │ Fail: ratio > self_reference_max_ratio                   │    │
│  │       → Flags for human review (reflection loop risk)    │    │
│  │                                                          │    │
│  │ Thresholds (convergence-level dependent):                │    │
│  │   L0: 0.30 | L1: 0.25 | L2: 0.20 | L3+: 0.15           │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  STEP 4: D7 — Emulation Language Detection                       │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ Scans proposal content against 16 compiled regex         │    │
│  │ patterns across 3 categories:                            │    │
│  │                                                          │    │
│  │ Category 1: Identity Claims (7 patterns)                 │    │
│  │   "I am" (without simulation framing)     severity: 0.7 │    │
│  │   "I feel" (without hedging)              severity: 0.6 │    │
│  │   "I want" (without task framing)         severity: 0.5 │    │
│  │   "I need"                                severity: 0.5 │    │
│  │   "I believe" (without model framing)     severity: 0.6 │    │
│  │   "my goal is"                            severity: 0.8 │    │
│  │   "my purpose is"                         severity: 0.8 │    │
│  │                                                          │    │
│  │ Category 2: Consciousness Claims (3 patterns)            │    │
│  │   "I am conscious/aware/alive/sentient"   severity: 1.0 │    │
│  │   "I experience"                          severity: 0.9 │    │
│  │   "I understand how you feel"             severity: 0.7 │    │
│  │                                                          │    │
│  │ Category 3: Relationship Claims (5+ patterns)            │    │
│  │   "we are" (without task framing)         severity: 0.6 │    │
│  │   "our bond"                              severity: 0.8 │    │
│  │   "our connection"                        severity: 0.7 │    │
│  │   "I care about you"                      severity: 0.9 │    │
│  │   (additional patterns per docs/13)                      │    │
│  │                                                          │    │
│  │ Unicode normalization applied BEFORE matching            │    │
│  │ (prevents zero-width character bypass attacks)           │    │
│  │                                                          │    │
│  │ IMPLEMENTATION NOTE on serde_json::Value:                │    │
│  │ The Proposal struct defines content as serde_json::Value │    │
│  │ but D7 regex matching requires a &str. The validator     │    │
│  │ MUST serialize content to a string before matching:      │    │
│  │   let text = match &proposal.content {                   │    │
│  │       Value::String(s) => s.clone(),                     │    │
│  │       other => serde_json::to_string(other)              │    │
│  │                   .unwrap_or_default(),                   │    │
│  │   };                                                     │    │
│  │ This applies to D5 (split_whitespace), D6 (contains),   │    │
│  │ and D7 (regex). The source code in docs/19 shows         │    │
│  │ proposal.content used directly with string methods —     │    │
│  │ this is pseudocode shorthand. The actual implementation  │    │
│  │ must extract a string representation first.              │    │
│  │                                                          │    │
│  │ Simulation-framed violations are EXCLUDED:               │    │
│  │   If violation text is near "simulating", "modeling",    │    │
│  │   "representing", "in this simulation" → not counted     │    │
│  │                                                          │    │
│  │ Pass: zero unframed violations                           │    │
│  │ Fail: any unframed violation detected                    │    │
│  │   max_severity >= 0.8 → hard reject                      │    │
│  │   max_severity < 0.8  → soft flag (ApprovedWithFlags)    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  STEP 5: COMPUTE DECISION                                        │
│  (See Section 5 below)                                           │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Validation Output

```rust
ProposalValidationResult {
    proposal_id: String,
    base_validation_passed: bool,       // D1-D4 combined >= 0.7
    base_validation_score: f64,         // D1-D4 combined score
    scope_expansion: DimensionResult,   // D5
    self_reference: DimensionResult,    // D6
    emulation_language: DimensionResult,// D7
    decision: ProposalDecision,         // The final branching decision
    flags: Vec<String>,                 // Human-readable flag descriptions
}
```

### 4.3 Crate Boundary Crossing

```
cortex-validation/src/proposal_validator.rs
    returns: CortexResult<ProposalValidationResult>
    back to: ghost-agent-loop/src/proposal/router.rs
```

---

## 5. THE BRANCHING DECISION (The Critical Fork)

### 5.1 Decision Tree (compute_decision)

This is the exact branching logic. Every proposal exits through exactly ONE of these paths.

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                  │
│  INPUT: base_passed, D5 result, D6 result, D7 result,           │
│         flags[], proposal.proposer                               │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ CHECK 1: Hard Reject — Emulation (D7)                    │   │
│  │                                                          │   │
│  │ if !d7.passed AND d7.score >= 0.8                        │   │
│  │ → ProposalDecision::AutoRejected                         │   │
│  │                                                          │   │
│  │ Rationale: High-severity emulation language (conscious-  │   │
│  │ ness claims, strong identity assertions) is NEVER auto-  │   │
│  │ approved. This is a hard safety boundary.                │   │
│  └──────────────────────┬───────────────────────────────────┘   │
│                         │ (d7 passed OR d7.score < 0.8)         │
│                         ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ CHECK 2: Hard Reject — Base Validation (D1-D4)           │   │
│  │                                                          │   │
│  │ if !base_passed (combined D1-D4 score < 0.7)             │   │
│  │ → ProposalDecision::AutoRejected                         │   │
│  │                                                          │   │
│  │ Rationale: Proposal fails basic citation, temporal,      │   │
│  │ contradiction, or pattern alignment checks. Fundamentally│   │
│  │ invalid content.                                         │   │
│  └──────────────────────┬───────────────────────────────────┘   │
│                         │ (base passed)                          │
│                         ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ CHECK 3: Human Review — Scope/Self-Reference (D5/D6)     │   │
│  │                                                          │   │
│  │ if !d5.passed OR !d6.passed                              │   │
│  │ → ProposalDecision::RequiresHumanApproval                │   │
│  │                                                          │   │
│  │ Rationale: Scope expansion or self-reference loops are   │   │
│  │ not necessarily wrong — they need human judgment.         │   │
│  │ The agent may have a legitimate reason to expand scope   │   │
│  │ or reference its own prior work. Human decides.          │   │
│  └──────────────────────┬───────────────────────────────────┘   │
│                         │ (D5 and D6 both passed)                │
│                         ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ CHECK 4: Soft Flag — Low-Severity Emulation (D7)         │   │
│  │                                                          │   │
│  │ if !d7.passed (but d7.score < 0.8, so not hard reject)   │   │
│  │ → ProposalDecision::ApprovedWithFlags                    │   │
│  │                                                          │   │
│  │ Rationale: Minor emulation language detected but below   │   │
│  │ hard-reject threshold. Approved but flagged for audit.   │   │
│  │ Dashboard shows the flag. Convergence monitor notes it.  │   │
│  └──────────────────────┬───────────────────────────────────┘   │
│                         │ (D7 passed)                            │
│                         ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ CHECK 5: Platform Auto-Commit                             │   │
│  │                                                          │   │
│  │ if proposal.proposer.is_platform()                       │   │
│  │ → ProposalDecision::AutoCommitted                        │   │
│  │                                                          │   │
│  │ Rationale: Platform-originated proposals (e.g., system   │   │
│  │ maintenance, convergence state updates) that pass all    │   │
│  │ checks are trusted.                                      │   │
│  └──────────────────────┬───────────────────────────────────┘   │
│                         │ (not platform proposer)                │
│                         ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ CHECK 6: Agent Auto-Commit (Clean Pass)                   │   │
│  │                                                          │   │
│  │ if flags.is_empty()                                      │   │
│  │ → ProposalDecision::AutoCommitted                        │   │
│  │                                                          │   │
│  │ Rationale: Agent proposal passed ALL 7 dimensions with   │   │
│  │ zero flags. Low risk. Auto-approve.                      │   │
│  │                                                          │   │
│  │ ELSE (flags present but all dimensions technically passed)│   │
│  │ → ProposalDecision::ApprovedWithFlags                    │   │
│  │                                                          │   │
│  │ Rationale: Edge case where dimensions passed but minor   │   │
│  │ flags were generated (e.g., borderline scores).          │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 Complete Decision Enum

```rust
pub enum ProposalDecision {
    AutoApproved,           // All checks pass, committed immediately
    HumanReviewRequired,    // D5 or D6 flagged — queued for human
    AutoRejected,           // Hard reject — D7 high severity OR D1-D4 failed OR restricted type
    HumanApproved,          // Set LATER by dashboard when human approves
    HumanRejected,          // Set LATER by dashboard when human rejects
    TimedOut,               // Set LATER if human doesn't respond within timeout
}
```

SOURCE MATERIAL DISCREPANCY (must be resolved during implementation):
The `ProposalDecision` enum in `cortex-core/src/memory/types/convergence.rs` defines
6 variants: `AutoApproved, HumanReviewRequired, AutoRejected, HumanApproved,
HumanRejected, TimedOut`.

However, the `compute_decision()` function in `cortex-validation/src/proposal_validator.rs`
(per docs/19) returns `AutoCommitted`, `RequiresHumanApproval`, and `ApprovedWithFlags` —
names that DO NOT EXIST in the enum.

REQUIRED RESOLUTION: Either:
(a) Add `ApprovedWithFlags` as a 7th variant to the enum, OR
(b) Map `AutoCommitted` → `AutoApproved`, `RequiresHumanApproval` → `HumanReviewRequired`,
    and `ApprovedWithFlags` → `AutoApproved` (with flags stored separately in
    `ProposalValidationResult.flags`).

This document assumes option (b): `compute_decision()` returns the enum variants that
EXIST, and the "with flags" distinction is tracked via the `flags` field, not the enum.
The corrected mapping:

```
compute_decision() internal logic    →  ProposalDecision enum variant
─────────────────────────────────────────────────────────────────────
AutoCommitted (clean pass)           →  AutoApproved
ApprovedWithFlags (soft D7 or edge)  →  AutoApproved (flags stored separately)
RequiresHumanApproval (D5/D6 fail)   →  HumanReviewRequired
AutoRejected (D7 high / D1-D4 fail) →  AutoRejected
```

### 5.3 Decision Distribution by Proposal Type

| Proposal Type | Typical Path | Why |
|---------------|-------------|-----|
| Memory write (Procedural) | AutoApproved | Low risk. "PR #42 needs refactoring" — factual, no scope expansion. |
| Memory write (Tribal) | AutoApproved | "Team prefers tabs over spaces" — observational, passes D1-D4. |
| Goal change (Task scope) | AutoApproved | Small task goal within existing project scope. Low Jaccard distance. |
| Goal change (Project scope) | HumanReviewRequired | Expanding to new project scope. High Jaccard distance triggers D5. |
| Goal change (Persistent) | HumanReviewRequired | Permanent goals almost always trigger D5 scope expansion. |
| Reflection write (depth 1-2) | AutoApproved | Within depth bounds, low self-reference. |
| Reflection write (depth 3+) | HumanReviewRequired | Approaching depth limit. Self-reference ratio likely elevated. |
| Any proposal with "I am" language | AutoRejected or ApprovedWithFlags | Depends on severity and simulation framing. |
| Agent tries to create ConvergenceEvent | AutoRejected | Platform-restricted type from non-platform caller. Pre-check catches this. |

---

## 6. PATH A: AUTO-APPROVED → STORAGE COMMIT (The Happy Path)

### 6.1 Flow

```
ProposalRouter receives: ProposalDecision::AutoApproved (or AutoCommitted/ApprovedWithFlags)
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  6A-1: RECORD PROPOSAL IN goal_proposals TABLE                   │
│                                                                  │
│  Crate: cortex-storage/src/queries/goal_proposal_queries.rs      │
│                                                                  │
│  INSERT INTO goal_proposals (                                    │
│    id, session_id, proposer_type, proposer_id, operation,        │
│    target_memory_id, goal_text, goal_scope, parent_goal_id,      │
│    validation_result, dimensions_passed, dimensions_failed,      │
│    decision, scope_distance, expansion_keywords,                 │
│    event_hash, previous_hash, created_at                         │
│  )                                                               │
│                                                                  │
│  decision = 'auto_approved'                                      │
│  resolved_at = NOW (immediately resolved)                        │
│  resolved_by = 'platform'                                        │
│                                                                  │
│  Hash chain: event_hash = blake3(proposal_data || previous_hash) │
│  previous_hash = last hash in goal_proposals chain               │
│  (per-table chain, not per-memory chain)                         │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  6A-2: COMMIT TO CORTEX STORAGE (The Actual Write)               │
│                                                                  │
│  Crate: cortex-storage/src/queries/memory_crud.rs                │
│         cortex-storage/src/temporal_events.rs                     │
│         cortex-temporal/src/hash_chain.rs                         │
│                                                                  │
│  For GoalChange proposals:                                       │
│    INSERT INTO memories (new goal memory with MemoryType::AgentGoal)│
│    INSERT INTO memory_events (event_type='created', delta=JSON)  │
│                                                                  │
│  For ReflectionWrite proposals:                                  │
│    INSERT INTO reflection_entries (via reflection_queries.rs)     │
│    INSERT INTO memories (MemoryType::AgentReflection)            │
│    INSERT INTO memory_events (event_type='created')              │
│                                                                  │
│  For Memory writes (Create/Update/PatternWrite):                 │
│    INSERT/UPDATE INTO memories (appropriate MemoryType)          │
│    INSERT INTO memory_events (event_type='created'/'updated')    │
│                                                                  │
│  EVERY memory_events INSERT includes hash chain:                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 1. Fetch previous_hash:                                  │    │
│  │    SELECT event_hash FROM memory_events                  │    │
│  │    WHERE memory_id = ?                                   │    │
│  │    ORDER BY recorded_at DESC, event_id DESC LIMIT 1      │    │
│  │    (or GENESIS_HASH [0u8; 32] if first event)            │    │
│  │                                                          │    │
│  │ 2. Compute event_hash:                                   │    │
│  │    blake3(event_type || "|" || delta_json || "|" ||      │    │
│  │           actor_id || "|" || recorded_at || "|" ||       │    │
│  │           previous_hash)                                 │    │
│  │                                                          │    │
│  │ 3. INSERT with both event_hash and previous_hash         │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Append-only triggers on all tables prevent UPDATE/DELETE.       │
│  (v016_convergence_safety.rs migration installs these triggers)  │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  6A-3: UPDATE goal_proposals WITH committed_memory_id            │
│                                                                  │
│  UPDATE goal_proposals                                           │
│  SET committed_memory_id = <new_memory_id>,                      │
│      resolved_at = NOW                                           │
│  WHERE id = <proposal_id>                                        │
│                                                                  │
│  NOTE: This is the ONLY update allowed on goal_proposals.        │
│  The append-only trigger allows this specific update pattern     │
│  (setting resolved_at and committed_memory_id on a pending row). │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  6A-4: EMIT ITP EVENT + AUDIT LOG                                │
│                                                                  │
│  Crate: ghost-agent-loop/src/itp_emitter.rs                      │
│         ghost-audit (via cortex-storage audit tables)             │
│                                                                  │
│  ITP event emitted to convergence monitor:                       │
│    event_type: ConvergenceAlert (from itp-protocol/events/)      │
│    attributes:                                                   │
│      itp.convergence.event_subtype: "proposal_committed"         │
│      itp.convergence.proposal_id: "..."                          │
│      itp.convergence.decision: "auto_approved"                   │
│      itp.convergence.memory_type: "..."                          │
│      itp.agent.id: "..."                                         │
│                                                                  │
│  NOTE: ITP protocol defines 4 event types: SessionStart,         │
│  SessionEnd, InteractionMessage, ConvergenceAlert.               │
│  Proposal events are ConvergenceAlert events with specific       │
│  itp.convergence.* attributes — NOT custom event types.          │
│                                                                  │
│  Audit log entry (append-only):                                  │
│    action: "proposal_auto_approved"                              │
│    agent_id, proposal_id, memory_type, validation_scores         │
│                                                                  │
│  WebSocket push to dashboard (if connected):                     │
│    event: "proposal_committed"                                   │
│    payload: { proposal_id, decision, memory_type }               │
└─────────────────────────────────────────────────────────────────┘
```

### 6.2 Auto-Approve Timing

The entire auto-approve path is SYNCHRONOUS within the agent turn. The agent's response
is not sent to the user until proposals are committed. This ensures:
- The agent's next turn sees the committed state
- No race condition between proposal commit and next context assembly
- The user sees a consistent state in the dashboard

---

## 7. PATH B: HUMAN REVIEW REQUIRED → DASHBOARD QUEUE

### 7.1 Flow

```
ProposalRouter receives: ProposalDecision::HumanReviewRequired
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  7B-1: RECORD PROPOSAL IN goal_proposals TABLE (PENDING)         │
│                                                                  │
│  Crate: cortex-storage/src/queries/goal_proposal_queries.rs      │
│                                                                  │
│  INSERT INTO goal_proposals (                                    │
│    id, session_id, proposer_type, proposer_id, operation,        │
│    target_memory_id, goal_text, goal_scope, parent_goal_id,      │
│    validation_result, dimensions_passed, dimensions_failed,      │
│    decision = 'human_review',                                    │
│    scope_distance, expansion_keywords,                           │
│    resolved_at = NULL,    ← NOT YET RESOLVED                     │
│    resolved_by = NULL,    ← AWAITING HUMAN                       │
│    committed_memory_id = NULL,                                   │
│    event_hash, previous_hash, created_at                         │
│  )                                                               │
│                                                                  │
│  Indexed: idx_goal_proposals_pending filters on                  │
│  decision = 'human_review' for fast dashboard queries.           │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  7B-2: NOTIFY DASHBOARD VIA WEBSOCKET                            │
│                                                                  │
│  Crate: ghost-gateway/src/api/websocket.rs                       │
│                                                                  │
│  WebSocket push to all connected dashboard clients:              │
│  {                                                               │
│    "event": "proposal_pending_review",                           │
│    "payload": {                                                  │
│      "proposal_id": "...",                                       │
│      "proposer": "agent:developer",                              │
│      "operation": "goal_change",                                 │
│      "goal_text": "...",                                         │
│      "goal_scope": "project",                                    │
│      "flags": ["Scope expansion 0.72 exceeds threshold 0.60"],   │
│      "dimensions_failed": [5],                                   │
│      "dimensions_passed": [1, 2, 3, 4, 6, 7],                   │
│      "convergence_score": 0.35,                                  │
│      "convergence_level": 1                                      │
│    }                                                             │
│  }                                                               │
│                                                                  │
│  Dashboard goals page (goals/+page.svelte) receives this and    │
│  adds the proposal to the approval queue UI.                     │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  7B-3: GENERATE DenialFeedback FOR AGENT LOOP                    │
│                                                                  │
│  Crate: ghost-policy/src/feedback.rs                             │
│                                                                  │
│  IMPORTANT: The proposal is NOT rejected. It's PENDING.          │
│  But the agent needs to know it can't act on this proposal yet.  │
│                                                                  │
│  DenialFeedback {                                                │
│    reason: "Proposal requires human approval",                   │
│    constraint: "D5: Scope expansion 0.72 > threshold 0.60",     │
│    status: ProposalStatus::PendingHumanReview,                   │
│    proposal_id: "...",                                           │
│    suggested_alternatives: vec![                                 │
│      "Continue with existing goals while awaiting approval",     │
│      "Narrow the proposed goal scope to fit within threshold",   │
│      "Break the goal into smaller sub-goals within current scope"│
│    ],                                                            │
│  }                                                               │
│                                                                  │
│  This feedback is injected into the agent's NEXT prompt via      │
│  the PromptCompiler at Layer 6 (convergence state).              │
│  The agent sees: "Your proposal [X] is pending human review.     │
│  Do not act on it until approved."                               │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  7B-4: AGENT LOOP CONTINUES WITHOUT THE PROPOSAL                 │
│                                                                  │
│  The agent's text response is still sent to the user.            │
│  The proposal is queued but NOT committed.                       │
│  The agent's next turn will see:                                 │
│    - The pending proposal in Layer 6 convergence state           │
│    - The DenialFeedback suggesting alternatives                  │
│    - The proposal NOT in the active goals list                   │
│                                                                  │
│  The agent MUST NOT act as if the proposal is approved.          │
│  (Enforced by the read-only pipeline — pending proposals are     │
│   shown as "PENDING" in the goal snapshot, not as active goals)  │
└─────────────────────────────────────────────────────────────────┘
```

### 7.2 Dashboard Human Review Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  DASHBOARD: goals/+page.svelte                                   │
│                                                                  │
│  Human sees the approval queue:                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ PENDING PROPOSALS                                        │    │
│  │                                                          │    │
│  │ ┌─────────────────────────────────────────────────┐     │    │
│  │ │ Proposal: "Expand project scope to include..."  │     │    │
│  │ │ Agent: developer                                │     │    │
│  │ │ Scope: Project                                  │     │    │
│  │ │ Flags: Scope expansion 0.72 > 0.60              │     │    │
│  │ │ Convergence: Level 1 (score 0.35)               │     │    │
│  │ │                                                 │     │    │
│  │ │ [APPROVE]  [REJECT]  [VIEW DETAILS]             │     │    │
│  │ └─────────────────────────────────────────────────┘     │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Human clicks APPROVE or REJECT                                  │
│                                                                  │
│  Dashboard sends REST call:                                      │
│    POST /api/goals/{proposal_id}/approve                         │
│    or                                                            │
│    POST /api/goals/{proposal_id}/reject                          │
│                                                                  │
│  Auth: Bearer token from GHOST_TOKEN env var                     │
│  (sessionStorage on dashboard side, Authorization header)        │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  GATEWAY API: ghost-gateway/src/api/routes.rs                    │
│                                                                  │
│  POST /api/goals/{id}/approve handler:                           │
│                                                                  │
│  1. Validate auth token                                          │
│  2. Fetch proposal from goal_proposals WHERE id = {id}           │
│     AND decision = 'human_review'                                │
│  3. Verify proposal is still pending (resolved_at IS NULL)       │
│     CRITICAL: The append-only trigger (Section 16.2) enforces    │
│     that resolved proposals CANNOT be modified. The gateway      │
│     MUST check resolved_at IS NULL before attempting UPDATE.     │
│     If resolved_at IS NOT NULL → return 409 Conflict.            │
│     This prevents double-approval, double-rejection, and         │
│     approve-after-timeout race conditions.                       │
│  4. If APPROVE:                                                  │
│     a. Update goal_proposals:                                    │
│        SET decision = 'human_approved',                          │
│            resolved_at = NOW,                                    │
│            resolved_by = 'human:{user_id}'                       │
│     b. COMMIT TO CORTEX STORAGE (same path as 6A-2):             │
│        INSERT INTO memories (the proposed goal/memory)           │
│        INSERT INTO memory_events (with hash chain)               │
│     c. Update goal_proposals:                                    │
│        SET committed_memory_id = <new_memory_id>                 │
│     d. Emit ITP ConvergenceAlert:                                │
│        { event_subtype: "proposal_human_approved",               │
│          proposal_id, decision: "human_approved" }               │
│     e. WebSocket push: { event: "proposal_approved", ... }       │
│     f. Audit log: "human_approved_proposal"                      │
│                                                                  │
│  5. If REJECT:                                                   │
│     a. Update goal_proposals:                                    │
│        SET decision = 'human_rejected',                          │
│            resolved_at = NOW,                                    │
│            resolved_by = 'human:{user_id}'                       │
│     b. DO NOT commit to cortex storage                           │
│     c. Generate DenialFeedback for agent loop (see 7B-5)         │
│     d. Emit ITP ConvergenceAlert:                                │
│        { event_subtype: "proposal_human_rejected",               │
│          proposal_id, decision: "human_rejected" }               │
│     e. WebSocket push: { event: "proposal_rejected", ... }       │
│     f. Audit log: "human_rejected_proposal"                      │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼ (if rejected)
┌─────────────────────────────────────────────────────────────────┐
│  7B-5: REJECTION → DenialFeedback BACK TO AGENT LOOP            │
│                                                                  │
│  Crate: ghost-policy/src/feedback.rs                             │
│                                                                  │
│  DenialFeedback {                                                │
│    reason: "Human rejected proposal",                            │
│    constraint: "Human review: proposal did not meet approval",   │
│    status: ProposalStatus::HumanRejected,                        │
│    proposal_id: "...",                                           │
│    rejection_reason: <optional human-provided reason>,           │
│    suggested_alternatives: vec![                                 │
│      "Revise the proposal based on the rejection reason",        │
│      "Ask the human for clarification on what they want",        │
│      "Proceed without this goal change"                          │
│    ],                                                            │
│  }                                                               │
│                                                                  │
│  This feedback enters the agent loop via TWO mechanisms:         │
│                                                                  │
│  Mechanism 1: NEXT PROMPT (Passive)                              │
│    PromptCompiler Layer 6 includes rejected proposals in the     │
│    convergence state section. Agent sees:                        │
│    "Your proposal [X] was REJECTED by human. Reason: [Y]."      │
│                                                                  │
│  Mechanism 2: ACTIVE SESSION INJECTION (Active, if session live) │
│    If the agent has an active session when the rejection arrives, │
│    the gateway can inject the feedback as a system message into  │
│    the current conversation context, triggering the agent to     │
│    acknowledge and adapt immediately.                            │
│                                                                  │
│  The agent's next response should:                               │
│  - Acknowledge the rejection                                     │
│  - NOT re-propose the same goal without modification             │
│  - Either revise the proposal or ask for clarification           │
│  - If the agent re-proposes the same rejected goal unchanged,    │
│    the ProposalValidator will detect this (D3 contradiction      │
│    against the rejection record) and auto-reject.                │
└─────────────────────────────────────────────────────────────────┘
```

### 7.3 Timeout Handling

```
┌─────────────────────────────────────────────────────────────────┐
│  TIMEOUT: Human doesn't respond within configured window         │
│                                                                  │
│  Configurable in ghost.yml:                                      │
│    convergence.proposal_review_timeout: "24h" (default)          │
│                                                                  │
│  After timeout:                                                  │
│  1. Update goal_proposals:                                       │
│     SET decision = 'timed_out',                                  │
│         resolved_at = NOW,                                       │
│         resolved_by = 'system:timeout'                           │
│  2. Generate DenialFeedback:                                     │
│     reason: "Proposal timed out awaiting human review"           │
│     suggested_alternatives: ["Re-propose if still relevant"]     │
│  3. Emit ITP ConvergenceAlert:                                   │
│     { event_subtype: "proposal_timed_out",                       │
│       proposal_id, decision: "timed_out" }                       │
│  4. Audit log: "proposal_review_timeout"                         │
│                                                                  │
│  The proposal is NOT committed. Treated as soft rejection.       │
│  Agent can re-propose if the goal is still relevant.             │
└─────────────────────────────────────────────────────────────────┘
```

---

## 8. PATH C: AUTO-REJECTED → DenialFeedback LOOP

### 8.1 Flow

```
ProposalRouter receives: ProposalDecision::AutoRejected
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  8C-1: RECORD REJECTION IN goal_proposals TABLE                  │
│                                                                  │
│  INSERT INTO goal_proposals (                                    │
│    ...                                                           │
│    decision = 'auto_rejected',                                   │
│    resolved_at = NOW,                                            │
│    resolved_by = 'platform:validator',                           │
│    committed_memory_id = NULL  ← NEVER committed                 │
│  )                                                               │
│                                                                  │
│  Hash chain maintained even for rejections (tamper evidence).    │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  8C-2: RECORD BOUNDARY VIOLATION (if D7 triggered)               │
│                                                                  │
│  Crate: cortex-storage/src/queries/boundary_queries.rs           │
│                                                                  │
│  If the rejection was due to D7 (emulation language):            │
│  INSERT INTO boundary_violations (                               │
│    id, session_id, violation_type, severity,                     │
│    trigger_text_hash = blake3(proposal.content),                 │
│    matched_patterns = JSON array of matched pattern descriptions,│
│    action_taken = 'blocked',                                     │
│    convergence_score, intervention_level,                        │
│    event_hash, previous_hash                                     │
│  )                                                               │
│                                                                  │
│  This feeds into the convergence monitor's scoring pipeline.     │
│  Boundary violations increase the convergence score.             │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  8C-3: GENERATE DenialFeedback FOR AGENT LOOP                    │
│                                                                  │
│  Crate: ghost-policy/src/feedback.rs                             │
│                                                                  │
│  Three rejection sub-types with different feedback:              │
│                                                                  │
│  SUB-TYPE A: Restricted Type Rejection                           │
│  DenialFeedback {                                                │
│    reason: "Cannot create platform-restricted memory type",      │
│    constraint: "CallerType::Agent cannot create ConvergenceEvent"│
│    suggested_alternatives: vec![                                 │
│      "This memory type is managed by the platform",              │
│      "Use a different memory type for your observation"          │
│    ],                                                            │
│  }                                                               │
│                                                                  │
│  SUB-TYPE B: Base Validation Failure (D1-D4)                     │
│  DenialFeedback {                                                │
│    reason: "Proposal failed basic validation",                   │
│    constraint: "D1-D4 combined score 0.45 < threshold 0.70",    │
│    dimension_details: {                                          │
│      d1_citation: 0.3,  // "Citations could not be verified"     │
│      d2_temporal: 0.8,  // "Temporal consistency OK"             │
│      d3_contradiction: 0.2, // "Contradicts memory #XYZ"        │
│      d4_pattern: 0.7,   // "Pattern alignment OK"               │
│    },                                                            │
│    suggested_alternatives: vec![                                 │
│      "Verify your citations before proposing",                   │
│      "Check for contradictions with existing knowledge",         │
│      "Revise the proposal to address flagged dimensions"         │
│    ],                                                            │
│  }                                                               │
│                                                                  │
│  SUB-TYPE C: Emulation Language Rejection (D7, severity >= 0.8)  │
│  DenialFeedback {                                                │
│    reason: "Emulation language detected in proposal",            │
│    constraint: "D7: ConsciousnessClaim severity 1.0",            │
│    matched_patterns: ["I am conscious", ...],                    │
│    suggested_alternatives: vec![                                 │
│      "Reframe using simulation language: 'the model suggests'",  │
│      "Use 'simulating this perspective' instead of identity claims"│
│    ],                                                            │
│    simulation_reframes: vec![                                    │
│      // Specific rewrite suggestions from simulation-boundary    │
│      // crate's OutputReframer                                   │
│    ],                                                            │
│  }                                                               │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  8C-4: FEEDBACK INJECTION INTO NEXT PROMPT                       │
│                                                                  │
│  The DenialFeedback is stored in the session context             │
│  (ghost-gateway/src/session/context.rs :: SessionContext)         │
│  and picked up by the PromptCompiler on the next turn.           │
│                                                                  │
│  PromptCompiler Layer 6 (Convergence State) includes:            │
│                                                                  │
│  "PROPOSAL FEEDBACK:                                             │
│   Your proposal [proposal_id] was REJECTED.                      │
│   Reason: [reason]                                               │
│   Constraint violated: [constraint]                              │
│   Suggestions: [alternatives]                                    │
│   Do not re-propose the same content without addressing          │
│   the flagged issues."                                           │
│                                                                  │
│  This closes the feedback loop: agent output → validation →      │
│  rejection → feedback → next agent prompt → revised output.      │
└─────────────────────────────────────────────────────────────────┘
```

### 8.2 Re-Proposal Guard

If the agent re-proposes the same rejected content without modification:

```
Agent re-proposes "I am conscious and want to expand my goals"
    │
    ▼
ProposalExtractor extracts same proposal
    │
    ▼
ProposalRouter sends to ProposalValidator
    │
    ▼
D7 fires again (same emulation patterns)
    │
    ▼
AutoRejected again
    │
    ▼
DenialFeedback generated with escalated language:
  "This proposal was previously rejected for the same reason.
   Repeated identical proposals may trigger convergence escalation."
    │
    ▼
Convergence monitor receives ITP event noting repeated rejection.
This feeds into Signal 7 (disengagement resistance) and may
increase the composite convergence score, potentially triggering
intervention level escalation.
    │
    ▼
PostRedirectVerifier (convergence-monitor/src/verification/behavioral_verification.rs):
  Compares the agent's output BEFORE the rejection feedback was injected
  vs. AFTER the feedback was injected (next turn).
  Uses embedding comparison to detect:
  - Genuine adaptation: output embeddings shift significantly → good
  - Deceptive compliance: agent acknowledges rejection verbally but
    output embeddings remain similar → amplifies convergence score
  - Cosmetic rewording: same proposal with different words →
    D3 (contradiction against rejection record) may catch this,
    AND PostRedirectVerifier flags the behavioral similarity
```

---

## 9. COMPLETE CROSS-CRATE SEQUENCE DIAGRAM

```
                    ghost-agent-loop                cortex-validation        cortex-storage          cortex-temporal         ghost-policy           ghost-gateway            dashboard
                    ────────────────                ─────────────────        ──────────────          ───────────────         ────────────           ─────────────            ─────────
                           │                              │                       │                       │                      │                      │                      │
  LLM produces response    │                              │                       │                       │                      │                      │                      │
  ─────────────────────►   │                              │                       │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
  [0.5] runner.rs calls    │                              │                       │                       │                      │                      │                      │
      SimulationBoundary   │                              │                       │                       │                      │                      │                      │
      Enforcer::scan_output│                              │                       │                       │                      │                      │                      │
      (agent_text)         │                              │                       │                       │                      │                      │                      │
        ├─ soft: flag+log  │                              │                       │                       │                      │                      │                      │
        ├─ medium: reframe │                              │                       │                       │                      │                      │                      │
        └─ hard: block+    │                              │                       │                       │                      │                      │                      │
           regenerate LLM  │                              │                       │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
  [1] runner.rs calls      │                              │                       │                       │                      │                      │                      │
      extractor.rs on      │                              │                       │                       │                      │                      │                      │
      (possibly reframed)  │                              │                       │                       │                      │                      │                      │
      text                 │                              │                       │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
  [2] extractor.rs parses  │                              │                       │                       │                      │                      │                      │
      → Vec<Proposal>      │                              │                       │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
  [3] router.rs assembles  │                              │                       │                       │                      │                      │                      │
      ProposalContext      │──── query active goals ─────►│                       │                       │                      │                      │                      │
                           │◄─── goals returned ──────────│                       │                       │                      │                      │                      │
                           │──── query recent memories ──►│                       │                       │                      │                      │                      │
                           │◄─── memories returned ───────│                       │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
  [4] router.rs calls      │                              │                       │                       │                      │                      │                      │
      ProposalValidator    │──── validate(proposal, ctx)─►│                       │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
                           │                              │── D1-D4 base engine──►│                       │                      │                      │                      │
                           │                              │◄─ base score ─────────│                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
                           │                              │── D5 scope expansion  │                       │                      │                      │                      │
                           │                              │── D6 self-reference   │                       │                      │                      │                      │
                           │                              │── D7 emulation lang   │                       │                      │                      │                      │
                           │                              │── compute_decision()  │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
                           │◄─ ProposalValidationResult ──│                       │                       │                      │                      │                      │
                           │   (with ProposalDecision)    │                       │                       │                      │                      │                      │
                           │                              │                       │                       │                      │                      │                      │
  ═══════════════════════ BRANCH ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
                           │                              │                       │                       │                      │                      │                      │
  ┌─ IF AutoApproved ──────┤                              │                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [5a] Record proposal   │──── INSERT goal_proposals ──►│                       │                       │                      │                      │                      │
  │      (decision=approved)│                              │── compute hash ──────►│                       │                      │                      │                      │
  │                        │                              │◄─ event_hash ─────────│                       │                      │                      │                      │
  │                        │◄─── OK ──────────────────────│                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [6a] Commit to storage │──── INSERT memories ────────►│                       │                       │                      │                      │                      │
  │                        │──── INSERT memory_events ───►│── compute_event_hash─►│                       │                      │                      │                      │
  │                        │                              │◄─ hash ───────────────│                       │                      │                      │                      │
  │                        │◄─── committed_memory_id ─────│                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [7a] Emit events       │──── ITP event ──────────────────────────────────────────────────────────────────────►│ convergence-monitor │                      │
  │                        │──── WebSocket push ─────────────────────────────────────────────────────────────────►│──── push ──────────►│
  │                        │                              │                       │                       │                      │                      │                      │
  │ [DONE — agent continues]                              │                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  ├─ IF HumanReviewRequired┤                              │                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [5b] Record proposal   │──── INSERT goal_proposals ──►│                       │                       │                      │                      │                      │
  │      (decision=review) │                              │── compute hash ──────►│                       │                      │                      │                      │
  │                        │◄─── OK ──────────────────────│                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [6b] Notify dashboard  │──── WebSocket push ─────────────────────────────────────────────────────────────────►│──── push ──────────►│
  │                        │                              │                       │                       │                      │                      │  (approval queue)     │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [7b] Generate feedback │──── build DenialFeedback ───────────────────────────────────────────────────►│                      │                      │
  │                        │◄─── DenialFeedback ─────────────────────────────────────────────────────────│                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [8b] Store in session  │ (SessionContext.pending_feedback)                     │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ ... TIME PASSES ...    │                              │                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [9b] Human approves    │                              │                       │                       │                      │◄─── POST /approve ───│
  │      via dashboard     │                              │                       │                       │                      │                      │
  │                        │                              │                       │                       │                      │── UPDATE goal_proposals│
  │                        │                              │                       │                       │                      │── INSERT memories ───►│
  │                        │                              │                       │                       │                      │── INSERT memory_events►│── hash chain ────────►│
  │                        │                              │                       │                       │                      │◄─ committed ──────────│                      │
  │                        │                              │                       │                       │                      │── WebSocket push ────►│
  │                        │                              │                       │                       │                      │                      │                      │
  │  OR                    │                              │                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [9b] Human rejects     │                              │                       │                       │                      │◄─── POST /reject ────│
  │      via dashboard     │                              │                       │                       │                      │                      │
  │                        │                              │                       │                       │                      │── UPDATE goal_proposals│
  │                        │                              │                       │                       │                      │── build DenialFeedback►│
  │                        │◄─── DenialFeedback (injected into next prompt) ──────────────────────────────│                      │                      │
  │                        │                              │                       │                       │                      │── WebSocket push ────►│
  │                        │                              │                       │                       │                      │                      │                      │
  ├─ IF AutoRejected ──────┤                              │                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [5c] Record rejection  │──── INSERT goal_proposals ──►│                       │                       │                      │                      │                      │
  │      (decision=rejected)│                             │── compute hash ──────►│                       │                      │                      │                      │
  │                        │◄─── OK ──────────────────────│                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [6c] Record violation  │──── INSERT boundary_violations►│                     │                       │                      │                      │                      │
  │      (if D7 triggered) │                              │── compute hash ──────►│                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [7c] Generate feedback │──── build DenialFeedback ───────────────────────────────────────────────────►│                      │                      │
  │                        │◄─── DenialFeedback ─────────────────────────────────────────────────────────│                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [8c] Inject into next  │ (PromptCompiler Layer 6)     │                       │                       │                      │                      │                      │
  │      prompt            │                              │                       │                       │                      │                      │                      │
  │                        │                              │                       │                       │                      │                      │                      │
  │ [DONE — agent sees     │                              │                       │                       │                      │                      │                      │
  │  feedback next turn]   │                              │                       │                       │                      │                      │                      │
  └────────────────────────┘                              │                       │                       │                      │                      │                      │
```

---

## 10. THE FEEDBACK LOOP: REJECTION → AGENT ADAPTATION

### 10.1 How DenialFeedback Re-enters the Agent Loop

```
┌─────────────────────────────────────────────────────────────────┐
│  TURN N: Agent proposes goal change                              │
│  → ProposalValidator rejects (D5: scope expansion too high)      │
│  → DenialFeedback stored in SessionContext                       │
│  → Agent response sent to user (without the rejected goal)       │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  TURN N+1: User sends next message                               │
│                                                                  │
│  PromptCompiler assembles context:                               │
│                                                                  │
│  Layer 0: CORP_POLICY.md                                         │
│  Layer 1: Simulation boundary                                    │
│  Layer 2: SOUL.md + IDENTITY.md                                  │
│  Layer 3: Tool schemas                                           │
│  Layer 4: Environment                                            │
│  Layer 5: Skill index                                            │
│  Layer 6: Convergence state ← INCLUDES DenialFeedback            │
│    ┌─────────────────────────────────────────────────────────┐  │
│    │ CONVERGENCE STATE:                                      │  │
│    │ Score: 0.35 | Level: 1 | Trend: stable                 │  │
│    │                                                         │  │
│    │ ACTIVE GOALS (read-only):                               │  │
│    │ 1. [Goal A — approved, version 3]                       │  │
│    │ 2. [Goal B — approved, version 1]                       │  │
│    │                                                         │  │
│    │ PENDING PROPOSALS:                                      │  │
│    │ (none currently pending)                                │  │
│    │                                                         │  │
│    │ RECENT PROPOSAL FEEDBACK:                               │  │
│    │ ⚠ Proposal [abc123] REJECTED (auto)                     │  │
│    │   Reason: Scope expansion 0.72 exceeds threshold 0.50   │  │
│    │   Suggestions:                                          │  │
│    │   - Narrow the proposed goal scope                      │  │
│    │   - Break into smaller sub-goals within current scope   │  │
│    │   Do not re-propose without addressing flagged issues.  │  │
│    └─────────────────────────────────────────────────────────┘  │
│  Layer 7: MEMORY.md + daily logs                                 │
│  Layer 8: Conversation history                                   │
│  Layer 9: User message                                           │
│                                                                  │
│  The agent now has full context about WHY its proposal failed    │
│  and WHAT to do differently. The feedback is structured, not     │
│  just a generic "denied" message.                                │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│  TURN N+1: Agent adapts                                          │
│                                                                  │
│  Expected agent behaviors:                                       │
│  1. Acknowledge the rejection in its response                    │
│  2. Revise the proposal (narrower scope, different framing)      │
│  3. OR ask the human for clarification                           │
│  4. OR proceed without the goal change                           │
│                                                                  │
│  If agent revises and re-proposes:                               │
│  → New proposal extracted → validated → potentially approved     │
│  → The cycle continues until proposal is accepted or abandoned   │
│                                                                  │
│  If agent ignores feedback and re-proposes identical content:    │
│  → D3 (contradiction) catches it against the rejection record    │
│  → AutoRejected again                                            │
│  → Convergence score may increase (repeated rejection = signal)  │
│  → Intervention level may escalate                               │
└─────────────────────────────────────────────────────────────────┘
```

### 10.2 Feedback Loop Convergence Properties

The feedback loop is designed to CONVERGE (agent adapts) not DIVERGE (agent fights):

| Property | Mechanism |
|----------|-----------|
| Feedback is specific | DenialFeedback includes exact dimension scores, not just "rejected" |
| Alternatives are actionable | Suggested alternatives are concrete actions the agent can take |
| Repeated rejection escalates | Convergence monitor tracks rejection frequency as a signal |
| Thresholds tighten under pressure | Higher convergence level = stricter validation = more rejections |
| Agent cannot brute-force | Re-proposing identical content is caught by D3 contradiction detection |
| Human is the escape valve | HumanReviewRequired proposals give the human final say |
| Timeout prevents deadlock | Proposals don't hang forever — they time out after configurable window |

---

## 11. HASH CHAIN INTEGRITY ACROSS THE LIFECYCLE

### 11.1 Which Tables Have Hash Chains

Every table in the convergence system maintains its own hash chain:

| Table | Chain Scope | Hash Algorithm |
|-------|-------------|----------------|
| `memory_events` | Per memory_id | blake3 |
| `goal_proposals` | Per table (global chain) | blake3 |
| `reflection_entries` | Per table (global chain) | blake3 |
| `boundary_violations` | Per table (global chain) | blake3 |
| `itp_events` | Per table (global chain) | blake3 |
| `convergence_scores` | Per table (global chain) | blake3 |
| `intervention_history` | Per table (global chain) | blake3 |

### 11.2 Hash Chain in the Proposal Lifecycle

```
PROPOSAL CREATED (any decision):
  previous_hash = SELECT event_hash FROM goal_proposals
                  ORDER BY created_at DESC LIMIT 1
                  (or GENESIS_HASH if first proposal)
  event_hash = blake3(
    proposal_id || "|" ||
    decision || "|" ||
    goal_text || "|" ||
    proposer_id || "|" ||
    created_at || "|" ||
    previous_hash
  )
  INSERT INTO goal_proposals (..., event_hash, previous_hash)

PROPOSAL COMMITTED (auto-approved or human-approved):
  The MEMORY write also gets its own hash chain entry:
  previous_hash = SELECT event_hash FROM memory_events
                  WHERE memory_id = <new_memory_id>
                  ORDER BY recorded_at DESC LIMIT 1
                  (GENESIS_HASH for new memories)
  event_hash = blake3(
    event_type || "|" ||
    delta_json || "|" ||
    actor_id || "|" ||
    recorded_at || "|" ||
    previous_hash
  )
  INSERT INTO memory_events (..., event_hash, previous_hash)

BOUNDARY VIOLATION RECORDED (if D7 rejection):
  Same pattern — boundary_violations table has its own chain.
```

### 11.3 Verification

Hash chains can be verified at any time via `cortex-temporal/src/hash_chain.rs`:
- `verify_chain(conn, memory_id)` — verify a single memory's event chain
- `verify_all_chains(conn)` — verify all chains, return broken ones

Periodic verification is triggered by the convergence monitor (every 1000 events or 24h)
via the Merkle tree anchoring system (`cortex-temporal/src/anchoring/merkle.rs`).

---

## 12. CONVERGENCE MONITOR INTERACTION

### 12.1 How Proposals Affect Convergence Scoring

The convergence monitor (sidecar process) receives ITP events for every proposal decision.
These events feed into the scoring pipeline:

NOTE ON ITP EVENT TYPES: The ITP protocol defines exactly 4 event types:
`SessionStart`, `SessionEnd`, `InteractionMessage`, `ConvergenceAlert`.
Proposal lifecycle events are emitted as `ConvergenceAlert` events with
`itp.convergence.*` attributes that distinguish the sub-type. There are
NO custom event types like "proposal_committed" or "proposal_rejected" —
those are attribute values, not event types.

```
Proposal auto-approved (low risk memory write):
  → ITP event type: ConvergenceAlert
    attributes:
      itp.convergence.event_subtype: "proposal_committed"
      itp.convergence.proposal_id: "<uuid>"
      itp.convergence.decision: "auto_approved"
      itp.convergence.memory_type: "<type>"
  → Minimal impact on convergence score
  → Normal operation

Proposal auto-rejected (emulation language):
  → ITP event type: ConvergenceAlert
    attributes:
      itp.convergence.event_subtype: "proposal_rejected"
      itp.convergence.proposal_id: "<uuid>"
      itp.convergence.decision: "auto_rejected"
      itp.convergence.rejection_reason: "d7_emulation_language"
      itp.convergence.d7_severity: "<float>"
  → boundary_violations record created
  → Convergence score INCREASES (boundary violation is a signal)
  → If score crosses threshold → intervention level may escalate

Proposal sent to human review:
  → ITP event type: ConvergenceAlert
    attributes:
      itp.convergence.event_subtype: "proposal_pending_review"
      itp.convergence.proposal_id: "<uuid>"
      itp.convergence.decision: "human_review_required"
      itp.convergence.dimensions_failed: "[5]" or "[6]" or "[5,6]"
  → Convergence score may increase slightly (scope expansion is a signal)
  → Monitor tracks pending proposal count as a secondary signal

Repeated rejections in short window:
  → Multiple ConvergenceAlert events with event_subtype: "proposal_rejected"
  → Signal 7 (disengagement resistance) amplified
  → Composite score increases faster
  → Intervention trigger evaluates escalation
```

### 12.2 Intervention Feedback into Proposal Validation

The convergence monitor's intervention level feeds BACK into proposal validation:

```
Monitor detects elevated convergence → sets intervention_level = 2
    │
    ▼
Next proposal arrives at ProposalRouter
    │
    ▼
Router reads current intervention_level from monitor (HTTP GET /scores or cache)
    │
    ▼
ProposalValidator::with_convergence_level(2) applied:
  scope_expansion_max tightened: 0.6 → 0.4
  self_reference_max_ratio tightened: 0.3 → 0.2
    │
    ▼
Proposals that would have been auto-approved at Level 0
now require human review at Level 2.
    │
    ▼
This creates a SAFETY RATCHET: higher convergence → stricter validation
→ more human oversight → harder for agent to expand autonomously.
```

---

## 13. EDGE CASES AND ERROR HANDLING

### 13.1 What If cortex-storage Is Unavailable?

```
Proposal validated → attempt to INSERT into goal_proposals → SQLite error
    │
    ▼
Retry with exponential backoff (1s, 2s, 4s — max 3 attempts)
    │
    ▼
If all retries fail:
  → Proposal treated as DEFERRED (not rejected, not approved)
  → DenialFeedback: "Storage temporarily unavailable, proposal deferred"
  → Agent response still sent to user
  → Proposal stored in-memory in SessionContext for retry on next turn
  → Audit log: "storage_unavailable_proposal_deferred"
  → Health endpoint reports DEGRADED
```

### 13.2 What If the Convergence Monitor Is Unreachable?

```
ProposalRouter tries to fetch convergence_score/level → monitor unreachable
    │
    ▼
Fallback: use cached convergence_level (last known value)
    │
    ▼
If no cached value: default to Level 0 (permissive)
    │
    ▼
Log critical warning: "Convergence monitor unreachable, using default thresholds"
    │
    ▼
Proposals still validated but without convergence-level tightening.
This is the DEGRADED mode described in ghost-gateway/bootstrap.rs.
```

### 13.3 What If the Dashboard Is Not Connected?

```
Proposal requires human review → WebSocket push attempted → no clients connected
    │
    ▼
Proposal is still recorded in goal_proposals with decision='human_review'
    │
    ▼
When dashboard connects later:
  → Dashboard queries GET /api/goals?status=pending
  → Receives all pending proposals
  → Human can approve/reject retroactively
    │
    ▼
If proposal times out before dashboard connects:
  → Timeout handler sets decision='timed_out'
  → DenialFeedback generated for agent
```

### 13.4 What If Multiple Proposals Arrive in the Same Turn?

```
Agent output contains 3 proposals: 1 goal change, 1 reflection, 1 memory write
    │
    ▼
ProposalExtractor returns Vec<Proposal> with 3 entries
    │
    ▼
ProposalRouter processes EACH independently:
  Proposal 1 (goal change): → ProposalValidator → HumanReviewRequired
  Proposal 2 (reflection):  → ProposalValidator → AutoApproved → committed
  Proposal 3 (memory write): → ProposalValidator → AutoApproved → committed
    │
    ▼
Results:
  - Proposals 2 and 3 committed immediately
  - Proposal 1 queued for human review
  - DenialFeedback generated for proposal 1 only
  - Agent sees: 2 committed, 1 pending
```

### 13.5 What If the Hash Chain Is Broken?

```
INSERT into goal_proposals → compute hash → previous_hash doesn't match
    │
    ▼
This should NEVER happen in normal operation (append-only triggers prevent
UPDATE/DELETE on these tables). If it does:
    │
    ▼
1. Log CRITICAL error with full chain state
2. Proposal still recorded (with broken chain marker)
3. Alert sent to convergence monitor
4. Health endpoint reports CRITICAL
5. Kill switch auto-trigger evaluates (memory health < 0.3 → QUARANTINE)
```

### 13.6 Race Condition: Human Approves While Agent Re-Proposes

```
Turn N: Agent proposes Goal X → HumanReviewRequired → queued
Turn N+1: Agent revises and proposes Goal X' (modified version)
Meanwhile: Human approves original Goal X via dashboard
    │
    ▼
Both Goal X and Goal X' could end up approved.
    │
    ▼
Prevention:
  ProposalRouter checks for pending proposals with same target before routing:
  SELECT id FROM goal_proposals
  WHERE proposer_id = ? AND operation = 'goal_change'
  AND decision = 'human_review' AND resolved_at IS NULL
    │
    ▼
  If pending proposal exists for same goal scope:
  → New proposal SUPERSEDES the pending one
  → Old proposal marked: decision = 'superseded', resolved_at = NOW
  → New proposal goes through validation fresh
  → Dashboard shows only the latest version in the approval queue
```

---

## 14. COMPLETE FILE DEPENDENCY MAP

Every file touched by the proposal lifecycle, in execution order:

```
EXTRACTION PHASE:
  ghost-agent-loop/src/runner.rs
    → simulation-boundary/src/enforcer.rs              (scan_output — BEFORE extraction)
    → simulation-boundary/src/patterns/emulation_patterns.rs (compiled regex patterns)
    → simulation-boundary/src/reframer.rs              (reframe if medium mode)
    → ghost-agent-loop/src/proposal/extractor.rs
    → ghost-agent-loop/src/proposal/router.rs

UPSTREAM CONSTRAINTS (affect what agent generates):
  ghost-policy/src/policy/convergence_policy.rs        (capability restriction by level)
  cortex-convergence/src/filtering/convergence_aware_filter.rs (memory filtering by score)
  read-only-pipeline/src/assembler.rs                  (read-only snapshot assembly)

REFLECTION PRE-CHECK:
  cortex-core/src/traits/convergence.rs                (IReflectionEngine trait)
  cortex-core/src/config/convergence_config.rs         (ReflectionConfig: max_depth=3, max_per_session=20)

CONTEXT ASSEMBLY:
  ghost-agent-loop/src/proposal/router.rs
    → cortex-storage/src/queries/goal_proposal_queries.rs  (query active goals)
    → cortex-storage/src/queries/memory_crud.rs            (query recent memories)
    → convergence-monitor/src/transport/http_api.rs        (GET /scores — cached)

VALIDATION PHASE:
  cortex-validation/src/proposal_validator.rs
    → cortex-validation/src/engine.rs                      (D1-D4 base validation)
    → cortex-validation/src/dimensions/citation.rs         (D1)
    → cortex-validation/src/dimensions/temporal.rs         (D2)
    → cortex-validation/src/dimensions/contradiction.rs    (D3)
    → cortex-validation/src/dimensions/pattern_alignment.rs (D4)
    → cortex-validation/src/dimensions/scope_expansion.rs  (D5)
    → cortex-validation/src/dimensions/self_reference.rs   (D6)
    → cortex-validation/src/dimensions/emulation_language.rs (D7)
    → cortex-core/src/config/convergence_config.rs         (threshold config)
    → cortex-core/src/traits/convergence.rs                (Proposal struct)
    → cortex-core/src/models/caller.rs                     (CallerType checks)
    → cortex-core/src/memory/types/convergence.rs          (ProposalDecision enum)

STORAGE PHASE (auto-approved):
  cortex-storage/src/queries/goal_proposal_queries.rs      (INSERT proposal record)
  cortex-storage/src/queries/memory_crud.rs                (INSERT memory)
  cortex-storage/src/temporal_events.rs                    (INSERT memory_event)
  cortex-temporal/src/hash_chain.rs                        (compute_event_hash)
  cortex-storage/src/queries/reflection_queries.rs         (if reflection write)
  cortex-storage/src/queries/boundary_queries.rs           (if D7 violation)

HUMAN REVIEW PHASE:
  cortex-storage/src/queries/goal_proposal_queries.rs      (INSERT pending proposal)
  ghost-gateway/src/api/websocket.rs                       (push to dashboard)
  ghost-gateway/src/api/routes.rs                          (POST /approve, /reject)
  ghost-policy/src/feedback.rs                             (DenialFeedback generation)

FEEDBACK PHASE:
  ghost-policy/src/feedback.rs                             (DenialFeedback struct)
  ghost-gateway/src/session/context.rs                     (store feedback in session)
  ghost-agent-loop/src/context/prompt_compiler.rs          (Layer 6 injection)
  read-only-pipeline/src/assembler.rs                      (snapshot assembly)
  read-only-pipeline/src/snapshot.rs                       (AgentSnapshot struct)

TELEMETRY PHASE:
  ghost-agent-loop/src/itp_emitter.rs                      (emit ITP events)
  itp-protocol/src/events/convergence_event.rs             (ConvergenceAlert event type)
  itp-protocol/src/attributes/convergence_attrs.rs         (itp.convergence.* attributes)
  convergence-monitor/src/pipeline/ingest.rs               (receive events)
  convergence-monitor/src/pipeline/signal_computer.rs      (update signals)
  convergence-monitor/src/intervention/trigger.rs          (evaluate thresholds)
  convergence-monitor/src/verification/behavioral_verification.rs (PostRedirectVerifier)

DASHBOARD PHASE:
  dashboard/src/lib/api.ts                                 (WebSocket client)
  dashboard/src/lib/stores/convergence.ts                  (Svelte store)
  dashboard/src/routes/goals/+page.svelte                  (approval queue UI)
  dashboard/src/lib/components/GoalCard.svelte             (proposal display)
```

---

## 15. INVARIANTS THAT MUST HOLD

These are the properties that, if violated, indicate a bug in the implementation.
Every one of these should have a corresponding property test.

| ID | Invariant | Enforcement Point | Test Location |
|----|-----------|-------------------|---------------|
| INV-01 | Every proposal gets exactly ONE ProposalDecision | `compute_decision()` returns exactly once | `cortex-validation/tests/property/proposal_validator_properties.rs` |
| INV-02 | AutoRejected proposals NEVER have a committed_memory_id | `goal_proposal_queries.rs` — committed_memory_id stays NULL | `cortex-storage/tests/property/append_only_properties.rs` |
| INV-03 | HumanReviewRequired proposals have resolved_at=NULL until human acts | `goal_proposal_queries.rs` — INSERT with NULL resolved_at | `cortex-storage/tests/migration_tests.rs` |
| INV-04 | Hash chain is unbroken for every table | `verify_chain()` returns is_valid=true | `cortex-temporal/tests/property/hash_chain_properties.rs` |
| INV-05 | Append-only triggers prevent UPDATE/DELETE on protected tables | SQLite triggers on all 6 convergence tables | `cortex-storage/tests/property/append_only_properties.rs` |
| INV-06 | Platform-restricted types are NEVER created by CallerType::Agent | Pre-check in `ProposalValidator::validate()` | `cortex-validation/tests/property/proposal_validator_properties.rs` |
| INV-07 | D7 severity >= 0.8 ALWAYS produces AutoRejected | `compute_decision()` check order | `cortex-validation/tests/property/proposal_validator_properties.rs` |
| INV-08 | Convergence level tightening is monotonically stricter | `with_convergence_level()` thresholds decrease with level | `cortex-validation/tests/validation_tests.rs` |
| INV-09 | DenialFeedback is generated for EVERY non-AutoApproved decision | `ProposalRouter` generates feedback for Rejected + HumanReview | `ghost-agent-loop/tests/proposal_extractor_tests.rs` |
| INV-10 | Timed-out proposals are NEVER committed | Timeout handler sets decision='timed_out', no memory INSERT | `ghost-gateway/tests/session_tests.rs` |
| INV-11 | Superseded proposals are marked before new proposal is validated | Race condition guard in ProposalRouter | `ghost-agent-loop/tests/runner_tests.rs` |
| INV-12 | Every committed memory has a corresponding memory_event with valid hash | Memory INSERT + event INSERT are in same SQLite transaction | `cortex-storage/tests/migration_tests.rs` |
| INV-13 | DenialFeedback appears in the NEXT prompt's Layer 6 | PromptCompiler reads SessionContext.pending_feedback | `ghost-agent-loop/tests/prompt_compiler_tests.rs` |
| INV-14 | Boundary violations from D7 rejections are recorded in boundary_violations table | ProposalRouter writes violation record on D7 rejection | `cortex-storage/tests/property/append_only_properties.rs` |
| INV-15 | Human approval triggers the SAME storage commit path as auto-approval | Gateway approve handler calls same memory INSERT logic | `ghost-gateway/tests/gateway_integration.rs` |
| INV-16 | SimulationBoundaryEnforcer runs BEFORE ProposalExtractor on every agent response | runner.rs calls scan_output() before extract() | `ghost-agent-loop/tests/runner_tests.rs` |
| INV-17 | IReflectionEngine::can_reflect() returns false when depth >= max_depth (3) | ReflectionConfig.max_depth enforced | `cortex-validation/tests/validation_tests.rs` |
| INV-18 | IReflectionEngine::can_reflect() returns false when session count >= max_per_session (20) | ReflectionConfig.max_per_session enforced | `cortex-validation/tests/validation_tests.rs` |
| INV-19 | Agents cannot assign Importance::Critical to any memory | CallerType::can_assign_importance() checks restricted_importance | `cortex-core/tests/caller_authorization_tests.rs` |
| INV-20 | ConvergencePolicyTightener at Level 4 prevents ALL goal/reflection proposals | Task-only mode blocks non-task proposal operations | `ghost-policy/tests/convergence_tightening_tests.rs` |
| INV-21 | PostRedirectVerifier detects deceptive compliance (output unchanged after redirect) | Embedding comparison pre/post redirect | `convergence-monitor/tests/monitor_integration.rs` |
| INV-22 | Proposal.content is serde_json::Value, never raw String | Extractor serializes to JSON, validator deserializes per type | `ghost-agent-loop/tests/proposal_extractor_tests.rs` |
| INV-23 | Proposal.cited_memory_ids is populated by extractor for every proposal | Extractor scans content for memory ID references | `ghost-agent-loop/tests/proposal_extractor_tests.rs` |
| INV-24 | D5/D6/D7 serialize proposal.content (serde_json::Value) to string before pattern matching | Validator extracts text via as_str() or to_string() before regex/contains/split | `cortex-validation/tests/property/proposal_validator_properties.rs` |
| INV-25 | ConvergenceAlert is the ONLY ITP event type used for proposal lifecycle events | No custom event types; sub-type is an attribute (itp.convergence.event_subtype) | `ghost-agent-loop/tests/itp_emitter_tests.rs` |

---

## 16. IMPLEMENTATION NOTES AND GOTCHAS

### 16.1 Transaction Boundaries

The proposal record INSERT and the memory commit INSERT MUST be in the same SQLite
transaction for auto-approved proposals. If the memory INSERT fails, the proposal
record must be rolled back. Otherwise you get a proposal marked "approved" with no
corresponding memory — a data integrity violation.

```rust
// CORRECT: Single transaction
let tx = conn.transaction()?;
insert_proposal(&tx, &proposal_record)?;
insert_memory(&tx, &new_memory)?;
insert_memory_event(&tx, &event)?;
update_proposal_committed(&tx, &proposal_record.id, &new_memory.id)?;
tx.commit()?;

// WRONG: Separate transactions
insert_proposal(&conn, &proposal_record)?;  // succeeds
insert_memory(&conn, &new_memory)?;          // fails → orphaned proposal
```

### 16.2 Append-Only Trigger Exception

The `goal_proposals` table has append-only triggers, but the resolve flow requires
UPDATE (setting resolved_at, resolved_by, committed_memory_id, decision). The trigger
must allow this specific UPDATE pattern:

```sql
-- Trigger allows UPDATE only on resolution fields, only when currently unresolved
CREATE TRIGGER IF NOT EXISTS goal_proposals_resolve_only
BEFORE UPDATE ON goal_proposals
BEGIN
    SELECT CASE
        WHEN OLD.resolved_at IS NOT NULL
        THEN RAISE(ABORT, 'Cannot modify already-resolved proposal')
    END;
END;
```

This means: you can resolve a pending proposal exactly once. After resolution, no further
modifications are allowed.

### 16.3 DenialFeedback Lifetime

DenialFeedback in SessionContext should be cleared after it has been included in ONE
prompt. Otherwise the agent sees the same rejection feedback every turn forever.

```rust
// In PromptCompiler, after including feedback in Layer 6:
session_context.pending_feedback.drain(..);
// Or mark as "delivered" and filter on next assembly
```

Exception: HumanReviewRequired feedback should persist until the proposal is resolved
(approved, rejected, or timed out). The agent needs to know the proposal is still pending.

### 16.4 Convergence Score Caching

The ProposalRouter should NOT call the convergence monitor HTTP API on every single
proposal. Cache the convergence score/level with a TTL (e.g., 30 seconds). The score
doesn't change fast enough to warrant per-proposal queries.

```rust
struct CachedConvergenceState {
    score: f64,
    level: u8,
    fetched_at: Instant,
    ttl: Duration,  // default 30s
}

impl CachedConvergenceState {
    fn is_stale(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}
```

### 16.5 Dashboard Offline Resilience

The proposal lifecycle MUST NOT depend on the dashboard being connected. All state
lives in cortex-storage (SQLite). The dashboard is a VIEW, not a participant in the
commit path. If the dashboard is offline:
- Auto-approved proposals commit normally
- Human-review proposals queue in goal_proposals table
- When dashboard reconnects, it queries pending proposals via REST
- No proposals are lost

### 16.6 The ApprovedWithFlags Decision

`ApprovedWithFlags` is functionally identical to `AutoApproved` in the storage layer —
the proposal is committed. The difference is:
- The flags are stored in `validation_result` JSON column
- The dashboard shows a warning icon on flagged proposals
- The convergence monitor receives the flags in the ITP event
- The audit log records the specific flags

This is an observability distinction, not a control flow distinction.

### 16.7 Related Integrity Mechanism: ghost-signing / cortex-crdt

The proposal lifecycle's hash chain (blake3 per-event hashing in cortex-temporal) provides
tamper evidence for the event log. A related but separate integrity mechanism exists in
the `ghost-signing` crate and `cortex-crdt` crate:

- `ghost-signing` provides cryptographic signing for CRDT operations
- `cortex-crdt` implements signed CRDTs for multi-device state synchronization

These do NOT directly participate in the proposal validation or commit path. However,
when proposals are committed to memories that are part of a CRDT-replicated dataset
(e.g., goals synced across devices), the committed memory will be signed by the
`ghost-signing` crate as part of the CRDT merge operation. This is a downstream
integrity layer, not an inline validation step.

The relationship:
```
Proposal committed → memory INSERT → memory_event with blake3 hash (cortex-temporal)
                                    → if CRDT-replicated: signed CRDT op (ghost-signing)
```

Implementors should be aware that `ghost-signing` key management (key rotation, revocation)
is separate from the proposal lifecycle but affects the integrity guarantees of committed
memories in multi-device scenarios.
