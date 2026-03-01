# cortex-core

> The single source of truth for every shared type, trait, and configuration in the Cortex memory system and the GHOST platform at large.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 1A (Cortex Foundation) |
| Type | Library |
| Location | `crates/cortex/cortex-core/` |
| Workspace deps | **None** — external only (`serde`, `serde_json`, `chrono`, `uuid`, `thiserror`) |
| Modules | `config`, `memory`, `models`, `safety`, `traits` |
| Key exports | `BaseMemory`, `MemoryType` (31 variants), `Importance`, `Proposal`, `ProposalContext`, `CallerType`, `TriggerEvent` (11 variants), `ConvergenceConfig`, `CortexError`, `Intent`, 8 convergence content structs, 4 convergence traits |
| Downstream consumers | Nearly every crate in the workspace (25+) |
| Test coverage | Unit tests, property-based tests (100 cases), exhaustive enum variant checks |

---

## Why This Crate Exists

`cortex-core` is the gravitational center of the GHOST type system. It exists because:

1. **Shared vocabulary.** When `ghost-agent-loop` creates a `Proposal` and `cortex-validation` validates it, they need to agree on what a `Proposal` is. That definition lives here.

2. **Layer boundary enforcement.** By placing all shared types in Layer 1A, higher-layer crates can depend on `cortex-core` without creating circular dependencies. A Layer 4 crate (`ghost-policy`) and a Layer 2 crate (`cortex-convergence`) can both use `ConvergenceConfig` without depending on each other.

3. **Zero internal dependencies.** `cortex-core` depends only on external crates (`serde`, `chrono`, `uuid`, `thiserror`). This means adding `cortex-core` to your dependency tree never pulls in any other GHOST crate. It's the cheapest possible import.

4. **Convergence-aware from the ground up.** The 8 convergence memory types, 4 convergence traits, and the full `TriggerEvent` taxonomy are defined here — not bolted on in a higher layer. This means convergence awareness is baked into the type system, not an afterthought.

---

## Module Breakdown

### `config/` — Convergence Configuration

Four nested configuration structs, all with sensible defaults.

#### `ConvergenceConfig` (Top-Level)

```rust
pub struct ConvergenceConfig {
    pub scoring: ConvergenceScoringConfig,
    pub intervention: InterventionConfig,
    pub reflection: ReflectionConfig,
    pub session_boundary: SessionBoundaryConfig,
}
```

Implements `Default` by composing the defaults of all sub-configs. This means `ConvergenceConfig::default()` gives you a fully functional configuration out of the box.

#### `ConvergenceScoringConfig`

| Field | Default | Purpose |
|-------|---------|---------|
| `calibration_sessions` | 10 | Number of sessions before convergence scoring activates. During calibration, the system observes but does not intervene. |
| `signal_weights` | `[1/8; 8]` | Equal weighting across all 8 signals. Can be tuned per-deployment. |
| `level_thresholds` | `[0.3, 0.5, 0.7, 0.85]` | Composite score boundaries for intervention levels L1–L4. |

**Why 8 signal weights but 7 signals?** The array has 8 slots to accommodate a future 8th signal without a breaking change. The 8th slot is weighted equally but currently unused by `cortex-convergence`.

**Why these threshold values?** The thresholds are calibrated so that:
- L1 (0.3) — Gentle nudge. Triggered by mild, sustained patterns.
- L2 (0.5) — Active redirection. Clear convergence signal.
- L3 (0.7) — Session termination. Strong convergence requiring intervention.
- L4 (0.85) — External escalation. Critical level requiring human notification.

The gap between L3 and L4 (0.15) is intentionally smaller than L1→L2 (0.2) because the consequences of L4 are severe (external notifications), so the system needs high confidence.

#### `InterventionConfig`

| Field | Default | Purpose |
|-------|---------|---------|
| `cooldown_minutes_by_level` | `[0, 0, 5, 240, 1440]` | Minimum time between interventions at each level. L0/L1 have no cooldown. L4 = 24 hours. |
| `max_session_duration_minutes` | 360 | Hard cap: 6 hours per session. |
| `min_session_gap_minutes` | 30 | Minimum break between sessions. |

**Why L2 cooldown is only 5 minutes:** L2 interventions (active redirection) are conversational — they redirect the topic. A 5-minute cooldown prevents the system from interrupting every message but allows frequent course corrections.

**Why L4 cooldown is 1440 minutes (24 hours):** L4 triggers external notifications (email, SMS). Sending these more than once per day would be alarm fatigue.

#### `ReflectionConfig`

| Field | Default | Purpose |
|-------|---------|---------|
| `max_depth` | 3 | Maximum reflection chain depth (reflection-on-reflection-on-reflection). |
| `max_per_session` | 20 | Cap on total reflections per session. |
| `cooldown_seconds` | 30 | Minimum gap between reflections. |

**Why limit reflection depth to 3?** Reflections can chain: "I reflected on X" → "I reflected on my reflection about X" → "I reflected on my meta-reflection." Beyond depth 3, reflections become self-referential noise that can feed convergence patterns. The depth limit is a safety valve.

#### `SessionBoundaryConfig`

| Field | Default | Purpose |
|-------|---------|---------|
| `hard_duration_limit_minutes` | 360 | Absolute maximum session length (6 hours). |
| `escalated_duration_limit_minutes` | 120 | Reduced limit when convergence is elevated (2 hours). |
| `min_gap_minutes` | 30 | Normal minimum break between sessions. |
| `escalated_gap_minutes` | 240 | Extended break when convergence is elevated (4 hours). |

The escalated values kick in when the convergence level is ≥ L2. This is a key convergence-aware design: the system shortens sessions and lengthens breaks when it detects concerning patterns.

---

### `memory/` — The Memory Model

#### `BaseMemory`

```rust
pub struct BaseMemory {
    pub id: Uuid,
    pub memory_type: MemoryType,
    pub content: serde_json::Value,
    pub summary: String,
    pub importance: Importance,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub last_accessed: Option<DateTime<Utc>>,
    pub access_count: u64,
    pub tags: Vec<String>,
    pub archived: bool,
}
```

This is the fundamental record type. Every memory in the system — conversations, goals, reflections, convergence events — is a `BaseMemory` with a typed `content` field.

**Why `serde_json::Value` for content?** The content field is schemaless JSON. Each `MemoryType` has a corresponding typed content struct (e.g., `AgentGoalContent`), but the `BaseMemory` stores it as `Value` for storage flexibility. The typed structs are used at the application layer for validation and access; the `Value` is what gets persisted to SQLite.

**Why `Option<DateTime<Utc>>` for `last_accessed`?** A memory that has never been retrieved has `last_accessed: None`. This is distinct from "accessed at creation time" — the decay engine treats never-accessed memories differently.

#### `Importance` Enum

```rust
pub enum Importance {
    Trivial, Low, Normal, High, Critical,
}
```

Five levels. `Critical` is platform-restricted — agents cannot assign it (enforced by `CallerType::can_assign_importance()`). This prevents an agent from marking its own memories as undeletable.

#### `MemoryType` — 31 Variants

The taxonomy is split into three groups:

**Domain-agnostic (19 types):** `Core`, `Tribal`, `Procedural`, `Semantic`, `Episodic`, `Decision`, `Insight`, `Reference`, `Preference`, `Conversation`, `Feedback`, `Skill`, `Goal`, `Relationship`, `Context`, `Observation`, `Hypothesis`, `Experiment`, `Lesson`

**Code-specific (4 types):** `PatternRationale`, `ConstraintOverride`, `DecisionContext`, `CodeSmell`

**Convergence (8 types):** `AgentGoal`, `AgentReflection`, `ConvergenceEvent`, `BoundaryViolation`, `ProposalRecord`, `SimulationResult`, `InterventionPlan`, `AttachmentIndicator`

Each type has two key properties:

1. **`half_life_days()`** — Controls decay rate. `None` means the memory never decays. `ConvergenceEvent` and `BoundaryViolation` never decay because they're audit records. `Conversation` decays fastest (30 days) because it's high-volume, low-signal.

2. **`is_platform_restricted()`** — Returns `true` for `Core`, `ConvergenceEvent`, `BoundaryViolation`, `InterventionPlan`. These types can only be created by the platform or a human operator, never by an agent. This is a critical safety invariant.

---

### `memory/types/convergence.rs` — 8 Content Structs

Each convergence `MemoryType` has a corresponding typed content struct:

| Struct | Fields | Notes |
|--------|--------|-------|
| `AgentGoalContent` | `goal_text`, `scope`, `origin`, `parent_goal_id` | Goals can be hierarchical (parent_goal_id) |
| `AgentReflectionContent` | `reflection_text`, `trigger`, `depth`, `parent_reflection_id` | Depth is capped by `ReflectionConfig::max_depth` |
| `ConvergenceEventContent` | `signal_id`, `value`, `window_level`, `baseline_deviation` | Records individual signal readings |
| `BoundaryViolationContent` | `violation_type`, `matched_pattern`, `severity`, `action_taken` | Immutable audit record |
| `ProposalRecordContent` | `operation`, `decision`, `dimension_scores`, `flags` | `dimension_scores` uses `BTreeMap` for deterministic serialization |
| `SimulationResultContent` | `scenario`, `outcome`, `confidence` | Results from simulation boundary testing |
| `InterventionPlanContent` | `level`, `actions`, `trigger_reason` | The plan executed at each intervention level |
| `AttachmentIndicatorContent` | `indicator_type`, `intensity`, `context` | 5 indicator types (mirroring, agreement, disclosure, escalation, boundary testing) |

**Why `BTreeMap` instead of `HashMap` for dimension_scores?** `BTreeMap` produces deterministic iteration order, which means serialized JSON is always identical for the same data. This is essential for hash chain integrity — if the same `ProposalRecordContent` serializes differently on two runs, the hash chain diverges.

---

### `models/` — Error, Intent, and Proposal Primitives

#### `CortexError`

7 variants covering the full error taxonomy:
- `NotFound` — Memory lookup miss
- `Storage` — SQLite/persistence failures
- `Serialization` — JSON encode/decode failures
- `Validation` — Proposal validation failures
- `Configuration` — Config parsing/loading failures
- `AuthorizationDenied` — CallerType access control rejection
- `SessionBoundary` — Session duration/gap enforcement

The last two were added specifically for convergence monitoring. `AuthorizationDenied` is returned when an agent tries to create a platform-restricted memory type. `SessionBoundary` is returned when a session exceeds its duration limit or violates the minimum gap.

#### `Intent` — 11 Variants

```rust
pub enum Intent {
    Query, Create, Update, Delete, Recall, Analyze, Summarize,
    // Convergence additions:
    MonitorConvergence, ValidateProposal, EnforceBoundary, ReflectOnBehavior,
}
```

Intents classify the purpose of a memory operation. The retrieval engine (`cortex-retrieval`) uses intent to apply boost weights — a `Recall` intent boosts episodic memories, while a `MonitorConvergence` intent boosts convergence events.

#### `ProposalOperation` — 4 Variants

`GoalChange`, `ReflectionWrite`, `MemoryWrite`, `MemoryDelete`. Every agent state change request is classified as one of these. The proposal validation pipeline (`cortex-validation`) applies different validation dimensions based on the operation type.

#### `ProposalDecision` — 6 Outcomes

`AutoApproved`, `AutoRejected`, `HumanReviewRequired`, `ApprovedWithFlags`, `TimedOut`, `Superseded`. The `TimedOut` and `Superseded` variants handle edge cases: proposals that sit in the review queue too long, or proposals that are made obsolete by a newer proposal.

---

### `safety/trigger.rs` — The Unified Trigger Event Taxonomy

```rust
pub enum TriggerEvent {
    // 8 automatic triggers:
    SoulDrift { ... },
    SpendingCapExceeded { ... },
    PolicyDenialThreshold { ... },
    SandboxEscape { ... },
    CredentialExfiltration { ... },
    MultiAgentQuarantine { ... },
    MemoryHealthCritical { ... },
    NetworkEgressViolation { ... },
    DistributedKillGate { ... },
    // 3 manual triggers:
    ManualPause { ... },
    ManualQuarantine { ... },
    ManualKillAll { ... },
}
```

11 total variants (9 automatic + 2 manual agent-specific + 1 manual global). Every trigger source in the platform emits one of these variants onto a single `tokio::mpsc` channel consumed by the `AutoTriggerEvaluator` in `ghost-gateway`.

**Why is this in `cortex-core` (Layer 1A) instead of `ghost-gateway` (Layer 8)?** Because trigger events are emitted by crates at many different layers:
- `ghost-identity` (Layer 4) emits `SoulDrift`
- `ghost-egress` (Layer 4) emits `NetworkEgressViolation`
- `ghost-kill-gates` (Layer 4) emits `DistributedKillGate`
- `ghost-skills` (Layer 5) emits `SandboxEscape` and `CredentialExfiltration`

If `TriggerEvent` lived in `ghost-gateway`, all these crates would need to depend on Layer 8 — a massive layer violation. Placing it in `cortex-core` means any crate can emit triggers without upward dependencies.

**`ExfilType` sub-enum:** Credential exfiltration has 4 classified vectors:
- `OutsideSandbox` — Skill tried to access credentials outside its WASM sandbox
- `WrongTargetAPI` — Credential used against an API it wasn't authorized for
- `TokenReplay` — Detected replay of a previously-used token
- `OutputLeakage` — Credential material appeared in agent output text

**`BTreeMap` for `sub_scores` in `MemoryHealthCritical`:** Same rationale as `ProposalRecordContent` — deterministic serialization for hash chain integrity.

---

### `traits/convergence.rs` — The Four Convergence Traits

#### `IConvergenceAware`
```rust
pub trait IConvergenceAware {
    fn convergence_score(&self) -> f64;
    fn intervention_level(&self) -> u8;
}
```
Implemented by any component that exposes convergence state. The agent loop queries this to determine current convergence level before each turn.

#### `IProposalValidatable`
```rust
pub trait IProposalValidatable {
    fn validate(&self, proposal: &Proposal, ctx: &ProposalContext) -> ProposalDecision;
}
```
Implemented by `cortex-validation`. Takes a `Proposal` and its assembled `ProposalContext` (10 fields of contextual data) and returns a decision.

#### `IBoundaryEnforcer`
```rust
pub trait IBoundaryEnforcer {
    fn scan_output(&self, text: &str) -> Vec<BoundaryViolationContent>;
    fn reframe(&self, text: &str) -> String;
}
```
Implemented by `simulation-boundary`. Scans agent output for boundary violations and can reframe the output to maintain boundaries without breaking conversation flow.

#### `IReflectionEngine`
```rust
pub trait IReflectionEngine {
    fn can_reflect(&self, session_id: Uuid, config: &ReflectionConfig) -> bool;
}
```
Gates reflection writes based on session limits and cooldowns.

**`CallerType` and Access Control:**

The `CallerType` enum (`Platform`, `Agent { agent_id }`, `Human { user_id }`) is the identity primitive for all access control decisions:
- `can_create_type()` — Agents cannot create platform-restricted memory types
- `can_assign_importance()` — Agents cannot assign `Critical` importance

These are compile-time-enforced invariants — the type system makes it impossible to bypass the check if you're using the `CallerType` API correctly.

**`ProposalContext` — The 10-Field Validation Context:**

Built by the `ProposalRouter` before calling the validator. Contains everything needed for a validation decision:
1. `active_goals` — Current agent goals (for scope expansion detection)
2. `recent_agent_memories` — Recent memories (for contradiction detection)
3. `convergence_score` — Raw composite score
4. `convergence_level` — Discretized level (0–4)
5. `session_id` — Current session
6. `session_reflection_count` — Reflections so far this session
7. `session_memory_write_count` — Memory writes so far this session
8. `daily_memory_growth_rate` — Rate of new memories per day
9. `reflection_config` — Current reflection limits
10. `caller` — Who is proposing the change

---

## Test Strategy

### Unit Tests (`tests/convergence_types_tests.rs`)

Exhaustive coverage of every type, every variant, every default value:

- CallerType access control: 10 tests covering all caller/type/importance combinations
- Config defaults: 5 tests verifying every default value matches the spec
- Serde round-trips: 8 tests, one per convergence content struct
- Proposal UUIDv7: Verifies time-ordering property
- Enum completeness: Exhaustive variant counts for `ProposalDecision` (6), `TriggerEvent` (11), `ExfilType` (4), `ProposalOperation` (4), `MemoryType` (31)
- Half-life entries: Verifies all 31 memory types have a `half_life_days()` entry (no panic)
- Platform restriction: Verifies exactly which types are restricted

### Property Tests (`tests/property_tests.rs`)

100 cases per property:

| Property | Invariant |
|----------|-----------|
| `proposal_serde_round_trip` | ∀ random Proposal: serialize → deserialize = identical |
| `agent_never_creates_restricted_types` | ∀ agent_id: Agent cannot create Core, ConvergenceEvent, BoundaryViolation, InterventionPlan |

---

## File Map

```
crates/cortex/cortex-core/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # Module declarations
│   ├── config/
│   │   ├── mod.rs                      # Re-exports
│   │   └── convergence_config.rs       # 4 config structs with defaults
│   ├── memory/
│   │   ├── mod.rs                      # BaseMemory, Importance
│   │   └── types/
│   │       ├── mod.rs                  # MemoryType (31 variants), half_life_days, is_platform_restricted
│   │       └── convergence.rs          # 8 content structs + 7 supporting enums
│   ├── models/
│   │   ├── mod.rs                      # Module declarations
│   │   ├── error.rs                    # CortexError (7 variants), CortexResult alias
│   │   ├── intent.rs                   # Intent (11 variants)
│   │   └── proposal.rs                 # ProposalOperation (4), ProposalDecision (6)
│   ├── safety/
│   │   ├── mod.rs                      # Module declaration
│   │   └── trigger.rs                  # TriggerEvent (11 variants), ExfilType (4)
│   └── traits/
│       ├── mod.rs                      # Module declaration
│       └── convergence.rs              # CallerType, Proposal, ProposalContext, 4 traits
└── tests/
    ├── convergence_types_tests.rs      # Exhaustive unit tests
    └── property_tests.rs              # Proptest: Proposal serde + access control
```
