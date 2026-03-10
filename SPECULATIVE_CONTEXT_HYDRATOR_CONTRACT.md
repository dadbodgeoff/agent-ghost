# Speculative Context Hydrator Contract

Status: March 10, 2026

Purpose: define the authoritative hydrator interface, payload shape, ownership boundaries, and runtime integration contract for speculative context in the live GHOST runtime.

This document follows:

- `SPECULATIVE_CONTEXT_LAYER_DESIGN.md`
- `SPECULATIVE_CONTEXT_EXECUTION_PLAN.md`
- `SPECULATIVE_CONTEXT_HOT_PATH_CONTRACT.md`

It is the first implementation-facing planning artifact. Its job is to define exactly what the runtime hydrator produces and where that payload enters the request path.

## Why This Exists

The runtime already has the pieces needed for a correct shape, but not yet the correct assembly:

- `RunnerBuildOptions` can inject conversation history and system prompt
- `AgentRunner::pre_loop()` creates an immutable `AgentSnapshot`
- `read-only-pipeline` already exposes `SnapshotAssembler`
- the tool executor can read `snapshot_memories`

What is missing is one shared hydrator that:

- loads conversation history
- resolves durable memory
- resolves allowed speculative context
- assembles a bounded snapshot payload
- applies the same rules for both `agent_chat` and Studio

Without this contract, speculative context would be tempted to enter through route-local glue or prompt text splicing. That would drift quickly and weaken safety guarantees.

## Contract Goals

- define one shared runtime hydrator used by `agent_chat` and Studio
- keep conversation history and snapshot payload separate on purpose
- ensure speculative context enters only through filtered, bounded, structured payloads
- align prompt visibility and tool visibility
- preserve durable-memory authority over speculative context
- make degraded behavior explicit

## Non-Goals

- defining the storage schema
- defining the async job engine
- defining full promotion logic
- implementing cross-session speculative retrieval in phase 1

## Ownership Boundary

The hydrator owns:

- request-time reads
- bounded filtering
- payload assembly
- conversion into runner-facing inputs

The hydrator does not own:

- durable memory writes
- speculative attempt creation
- deep validation
- promotion

## Canonical Placement

The hydrator should live behind the runtime-preparation seam, not in route handlers.

Primary candidates:

- `crates/ghost-gateway/src/api/runtime_execution.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`

Recommended shape:

- define the hydrator in the gateway runtime layer
- keep route handlers thin
- have both `agent_chat` and Studio call the same hydrator-backed preparation path

## Core Design Decision

The hydrator output is split into two classes:

1. `RunnerBuildOptions` payload
2. `AgentSnapshot` payload

This split is deliberate.

### Why conversation history stays separate

Conversation history is chat-ordered, message-shaped, and naturally maps to `RunnerBuildOptions`.

### Why memory and speculative context must feed snapshot assembly

Durable memory and speculative context are not just extra prompt text. They are stateful runtime inputs that must:

- obey convergence filtering
- obey retrieval weighting
- be visible to prompt assembly and memory-read tooling consistently

That makes `AgentSnapshot` the correct authority seam.

## Canonical Hydrator Output

The hydrator must produce a bounded payload with these components.

### 1. Conversation History Payload

Used for:

- multi-turn replay
- preserving conversational continuity

Shape:

- ordered chat messages
- bounded by route-specific history policy

Destination:

- `RunnerBuildOptions.conversation_history`

### 2. Durable Memory Payload

Used for:

- task-relevant historical context
- authoritative memory grounding

Shape:

- bounded list of `BaseMemory`

Destination:

- `AgentSnapshot.memories`
- prompt compilation via structured formatting
- tool executor snapshot memory view

### 3. Speculative Context Payload

Used for:

- short-term continuity from post-turn compacted summaries

Shape:

- bounded list of speculative entries already filtered to request-safe state

Phase 1 destination:

- converted into bounded `BaseMemory`-compatible or formatted summary inputs
- merged into the snapshot-side memory context with lower authority

### 4. Goals Payload

Used for:

- active task focus
- scope checking

Shape:

- bounded list of `AgentGoalContent`

Destination:

- `AgentSnapshot.goals`

### 5. Reflections Payload

Used for:

- bounded self-context where policy allows it

Shape:

- bounded list of `AgentReflectionContent`

Destination:

- `AgentSnapshot.reflections`

### 6. Convergence Payload

Used for:

- convergence-aware filtering
- prompt visibility
- intervention shaping

Shape:

- score and level

Destination:

- `AgentSnapshot.convergence_state`

## Proposed Runtime Types

These are contract types, not implementation commitments.

```rust
pub struct HydratedRuntimeContext {
    pub build_options: RunnerBuildOptions,
    pub snapshot_input: HydratedSnapshotInput,
}

pub struct HydratedSnapshotInput {
    pub goals: Vec<AgentGoalContent>,
    pub reflections: Vec<AgentReflectionContent>,
    pub durable_memories: Vec<BaseMemory>,
    pub speculative_entries: Vec<HydratedSpeculativeEntry>,
    pub convergence_score: f64,
    pub convergence_level: u8,
}

pub struct HydratedSpeculativeEntry {
    pub id: String,
    pub kind: HydratedSpeculativeKind,
    pub content: String,
    pub retrieval_weight: f64,
    pub created_at: String,
    pub source_refs: Vec<String>,
}
```

Phase 1 may keep `HydratedSpeculativeEntry` intentionally narrow and summary-oriented.

## Interface Contract

Recommended interface:

```rust
pub trait RuntimeHydrator {
    fn hydrate_for_request(
        &self,
        request: HydrationRequest,
    ) -> Result<HydratedRuntimeContext, HydrationError>;
}
```

Suggested request shape:

```rust
pub struct HydrationRequest {
    pub agent_id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    pub route_kind: RouteKind,
    pub user_message: String,
}
```

Suggested route kinds:

- `AgentChat`
- `Studio`
- future: `Cli`
- future: `Autonomy`

## Assembly Contract

The hydrator must assemble its output in this order:

1. conversation history
2. convergence state
3. durable memory
4. speculative context
5. goals
6. reflections

This order matters because:

- convergence informs memory filtering
- durable memory sets the authority floor
- speculative context is layered on top, not treated as primary truth

## Snapshot Construction Contract

The hydrator must not hand-roll snapshot creation separately for each route.

Recommended direction:

- use `read-only-pipeline::SnapshotAssembler`
- feed it with hydrated goals, reflections, memories, convergence score, and level

This is the cleanest existing authority seam.

### Important nuance

The current `SnapshotAssembler` takes one memory vector.

That means the runtime must define one merge policy before snapshot assembly:

- durable memories are primary
- speculative entries are secondary
- speculative entries must be bounded and lower authority

Phase 1 acceptable strategy:

- translate only retrievable speculative summaries into a narrow memory-like summary form
- append them after durable memories
- bound count and total tokens tightly

## Merge Policy Contract

Speculative context must not simply be concatenated without rules.

The merge policy must guarantee:

- durable memory outranks speculative context
- contradicted speculative entries are absent
- speculative entries remain same-session only in phase 1
- speculative entries occupy a smaller token budget slice

### Recommended phase 1 merge rules

- include durable memories first
- include at most N speculative summaries
- exclude any speculative entry above severity threshold
- exclude expired entries
- exclude blocked entries
- exclude flagged entries in phase 1
- prefer most recent retrievable summaries

## Prompt Contract

The prompt compiler should receive structured content, not raw speculative prose pasted into the system message by route handlers.

Recommended direction:

- keep conversation history in `RunnerBuildOptions`
- derive memory-related prompt content from the snapshot-side payload
- keep speculative context visible only through the same prompt assembly logic that handles durable memory

This prevents route-level drift.

## Tool Contract

If the memory read tool is available, its snapshot-backed memory view must be derived from the same hydrated payload used by prompt assembly.

This means:

- the hydrator or runtime-preparation layer must populate `snapshot_memories`
- blocked or expired speculative entries must never appear there
- prompt and tool memory views must not disagree

If the prompt path excludes something, the tool path excludes it too.

## Error Contract

### If conversation history hydration fails

- fail the request if the route depends on persisted session history for correctness
- do not silently continue with partial replay while presenting it as full continuity

### If durable memory hydration fails

- allowed phase 1 behavior:
  - continue without durable memory only if this degraded mode is explicit and observable
- forbidden behavior:
  - silently treat missing durable memory as normal healthy state

### If speculative context hydration fails

- continue without speculative context
- emit observability signal
- do not widen any other retrieval scope to compensate

### If convergence hydration fails

- preserve existing convergence degraded-mode rules already defined by the runtime
- do not silently promote speculative context because convergence data is unavailable

## Performance Contract

The hydrator may do:

- bounded session-history read
- bounded durable-memory read
- bounded speculative-context read
- bounded convergence read

The hydrator may not do:

- deep validation
- durable promotion
- embeddings generation
- unbounded historical scans
- full contradiction recomputation

## Phase 1 Contract

Phase 1 hydrator behavior must be intentionally narrow.

### Required phase 1 payload

- conversation history
- convergence state
- durable memory if already available in bounded form
- retrievable same-session speculative summaries only

### Forbidden phase 1 payload

- cross-session speculative entries
- flagged speculative entries
- blocked speculative entries
- cross-agent speculative context
- promotion candidates treated as truth

## Test Contract

The hydrator contract is not satisfied unless tests prove:

- `agent_chat` and Studio produce the same hydration semantics for the same underlying session state
- blocked speculative entries do not appear in build options, snapshot inputs, or tool memory view
- expired speculative entries do not appear in build options, snapshot inputs, or tool memory view
- speculative context remains same-session only in phase 1
- missing speculative context degrades safely
- missing durable memory is observable and does not silently masquerade as healthy full-state hydration

## Recommended Follow-On Refactors

These are likely needed after the contract is accepted:

- extend runtime-preparation flow so it accepts hydrated snapshot input
- stop relying on route-local history assembly in Studio
- stop using `RunnerBuildOptions::default()` in `agent_chat`
- add one shared conversion path from hydrated snapshot input to:
  - `AgentSnapshot`
  - prompt-facing memory content
  - tool-facing snapshot memory view

## Final Position

The hydrator is the keystone of this design.

If it is done correctly:

- speculative context remains bounded
- route behavior aligns
- prompt and tool memory views stay consistent
- future promotion logic has a clean authority boundary

If it is done poorly:

- speculative context leaks through route glue
- Studio and `agent_chat` drift again
- blocked entries can appear in one path but not another
- the system gains complexity without gaining a defensible memory model
