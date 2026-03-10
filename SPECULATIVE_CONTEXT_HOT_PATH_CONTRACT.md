# Speculative Context Hot-Path Contract

Status: March 10, 2026

Purpose: define the authoritative request-time contract for speculative context in the live GHOST runtime.

This is not a general architecture note. It is a hard contract for what the hot path may load, what it may write, what it must defer, and how `agent_chat` and Studio must converge on one hydration model.

Primary dependencies:

- `SPECULATIVE_CONTEXT_LAYER_DESIGN.md`
- `SPECULATIVE_CONTEXT_EXECUTION_PLAN.md`

Primary runtime sources:

- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/api/runtime_execution.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/tools/executor.rs`

## Standard

The request path is held to the following bar:

- no speculative context may reach the prompt path without passing a fast gate
- no blocked or expired speculative entry may be retrievable
- no deep validation step may block the user request
- no authority crossing from speculative context to durable memory may happen on the request path
- no semantic divergence between `agent_chat` and Studio is allowed without an explicit exception record

## Why This Contract Exists

The current runtime already reveals the core mismatch this contract must resolve:

- `agent_chat` builds runtime execution with `RunnerBuildOptions::default()`
- Studio reconstructs history and injects it through `RunnerBuildOptions`
- `AgentRunner::pre_loop()` creates an immutable snapshot, but the current hot path fills it with real convergence state and empty goals, reflections, and memories
- prompt compilation still relies on mostly default dynamic context fields

That means speculative context cannot be added safely as an ad hoc string append. The runtime needs one explicit hydration seam and one explicit request-time safety boundary.

## Current-State Constraints

### C1. `agent_chat` is currently under-hydrated.

Today:

- request-time history is not injected into `agent_chat`
- speculative context has no seam to enter `agent_chat`
- this route cannot yet serve as the canonical spawn-aware entrypoint

Contract implication:

- phase 1 must add a hydration path here before speculative context becomes meaningful

### C2. Studio is the current reference shape.

Today:

- Studio already rebuilds conversation history before constructing runtime execution

Contract implication:

- Studio’s request-time hydration behavior is the nearest live reference
- `agent_chat` must be aligned to this model, not left as a second-class stateless path

### C3. Snapshot assembly exists but is not yet fully fed.

Today:

- the runner assembles an immutable `AgentSnapshot`
- convergence state is present
- goals, reflections, and memories are empty in the current hot path

Contract implication:

- speculative context must be designed to flow into structured runtime state, not only text prompt stuffing

### C4. Memory-read tooling depends on snapshot-fed state.

Today:

- the tool executor can read from `snapshot_memories`
- the live runtime does not appear to populate that field in the current request path

Contract implication:

- speculative context integration must include runtime hydration for memory-aware tool behavior, not only prompt-layer visibility

## Canonical Request-Time Model

Every live request must pass through these stages in this order:

1. request admission
2. runtime hydration
3. pre-loop gate evaluation
4. model execution
5. post-turn speculative write
6. fast gate classification
7. response finalization
8. deferred background work scheduling

This ordering is mandatory.

## Stage 1: Request Admission

### Inputs

- stable or requested `agent_id`
- `session_id`
- user message
- route identity (`agent_chat` or `studio`)

### Required Actions

- resolve agent identity
- resolve session identity
- enforce idempotency and execution ownership already required by the route

### Forbidden Actions

- durable memory writes
- speculative promotion
- deep validation

## Stage 2: Runtime Hydration

This is the most important stage introduced by this contract.

### Goal

Produce one bounded runtime context bundle before `pre_loop()`.

### Canonical Hydration Bundle

The hydrator must assemble:

- conversation history
- durable memory payload
- speculative context payload
- convergence payload

Optional later additions:

- active goals
- recent reflections
- explicit task focus state

### Hydration Sources

#### Conversation history

- authoritative source: session persistence
- Studio already does this
- `agent_chat` must be upgraded to do the same class of work

#### Durable memory payload

- authoritative source: durable memory storage
- bounded by retrieval policy and token budget

#### Speculative context payload

- source: `context_attempts`
- filtered by:
  - `status = retrievable`
  - same `session_id`
  - `expires_at > now`
  - severity threshold
  - contradiction suppression

#### Convergence payload

- authoritative source: current convergence shared state already consulted by the runner

### Contract for Hydration Cost

Hydration may perform:

- one bounded session-history read
- one bounded durable-memory read
- one bounded speculative-context read
- one bounded convergence read

Hydration may not perform:

- deep validation
- embeddings generation
- promotion
- broad scans over historical speculative attempts
- unbounded joins across all sessions

### Canonical Integration Seam

The runtime must use a dedicated hydrator abstraction behind:

- `crates/ghost-gateway/src/api/runtime_execution.rs`
- and/or `crates/ghost-gateway/src/runtime_safety.rs`

The hydrator must not be duplicated independently in:

- `agent_chat`
- Studio

One implementation, shared by both routes.

## Stage 3: Pre-Loop Gate Evaluation

After hydration, the runner may enter `pre_loop()`.

### Allowed Work

- kill and pause enforcement
- convergence health checks
- session boundary checks
- immutable snapshot assembly

### Required Contract Change

Snapshot assembly must evolve from:

- convergence-only plus empty memory payload

to:

- convergence plus hydrated durable/speculative context inputs

This does not require phase 1 to hydrate every context class, but it does require one explicit structured seam for future growth.

## Stage 4: Model Execution

The model execution stage may use:

- system prompt
- conversation history
- hydrated durable memory
- hydrated speculative context
- tool schemas
- convergence-aware constraints

### Prompt Authority Rules

- durable memory outranks speculative context
- speculative context must be lower-weight and lower-budget
- blocked and expired entries must be absent, not merely deprioritized

### Tool Authority Rules

If memory-reading tools are enabled, their snapshot-fed memory view must respect the same retrieval filtering as the prompt path.

No route may allow the tool path to read speculative entries that the prompt path would have excluded.

## Stage 5: Post-Turn Speculative Write

Immediately after a successful turn, the request path may create speculative attempts.

### Request-Time Write Scope

The hot path may:

- create one or more speculative attempts
- create initial validation rows for fast-gate outcomes
- enqueue background jobs

The hot path may not:

- promote to durable memory
- run contradiction-heavy validation
- compute broad embeddings batches

### Phase 1 Bound

Phase 1 should emit:

- one bounded `summary` attempt per turn

This intentionally constrains write amplification while the layer is still being proven.

## Stage 6: Fast Gate Classification

This stage is request-adjacent and mandatory before any speculative entry can become retrievable.

### Fast Gate Must Check

- malformed or empty content
- provenance presence
- token and size bounds
- duplicate detection
- explicit severity threshold
- credential or secret leakage
- emulation-language hard fail if applicable

### Fast Gate Outputs

- `retrievable`
- `flagged`
- `blocked`

### Fast Gate Contract

- `retrievable` entries may be used on later turns under same-session retrieval rules
- `flagged` entries are not retrievable until explicitly allowed by policy
- `blocked` entries are never retrievable

## Stage 7: Response Finalization

The user response may be finalized after:

- the model result is computed
- speculative attempts are persisted
- fast gate classification is complete or safely defaults to non-retrievable

### Request Must Not Wait For

- deep validation
- promotion
- contradiction graph updates
- TTL cleanup

## Stage 8: Deferred Background Work

These tasks must be deferred:

- deep validation
- contradiction checks
- durable promotion
- embeddings creation if needed
- archival or expiration cleanup beyond request-time TTL checks

This work is owned by the job system, not the request path.

## Allowed Blocking Work

The request path may block on:

- session history read
- bounded speculative read
- bounded durable memory read
- current convergence read
- fast gate classification
- speculative attempt inserts

## Forbidden Blocking Work

The request path may not block on:

- deep validation
- promotion to durable memory
- full compaction retry loops
- batch reindexing
- cross-session speculative scans
- long-running contradiction recomputation

## Route Alignment Contract

### `agent_chat`

Must evolve from:

- `RunnerBuildOptions::default()`

To:

- shared hydrator-backed runtime preparation

### Studio

Must evolve from:

- route-local history hydration only

To:

- shared hydrator-backed runtime preparation

### Non-Negotiable Rule

There must be one runtime hydration policy with route-specific request/response shells around it, not two parallel implementations.

## Degraded-Mode Contract

### D1. Fast gate unavailable

Behavior:

- speculative entries are stored as non-retrievable or `pending`
- prompt and tool retrieval exclude them

Rule:

- fail closed for retrieval

### D2. Deep validation unavailable

Behavior:

- existing retrievable same-session summaries may remain available until TTL
- no promotion is allowed

Rule:

- fail closed for authority crossing

### D3. Contradiction check unavailable

Behavior:

- no promotion
- no expansion of retrieval scope

Rule:

- durable authority may not be weakened during degraded contradiction state

### D4. TTL cleanup unavailable

Behavior:

- request-time retrieval still checks `expires_at`

Rule:

- expiration enforcement cannot depend solely on background deletion

## Security Contract

### SC1. No blocked-entry leaks

Blocked speculative entries must not appear in:

- prompt compilation
- memory-read tool results
- hydration payloads

### SC2. No speculative authority crossing on request path

The request path may create speculative entries. It may not convert them into durable truth.

### SC3. No route-specific retrieval loopholes

If Studio excludes a speculative entry, `agent_chat` must exclude it too. If `agent_chat` excludes it, Studio must exclude it too.

### SC4. No speculative citation chains

Speculative entries may not be treated as authoritative supporting evidence for later speculative promotion decisions.

## Phase 1 Contract

Phase 1 is intentionally narrow.

### Phase 1 Must Deliver

- shared hydrator abstraction
- same-session speculative retrieval only
- one bounded speculative summary attempt per successful turn
- fast gate classification
- blocked-entry exclusion from prompt and tool paths

### Phase 1 Must Not Deliver

- durable promotion
- cross-session speculative retrieval
- multi-agent speculative sharing
- rich speculative fact graphs

## Acceptance Criteria

This contract is satisfied only if all conditions hold:

- `agent_chat` and Studio use one hydration model
- blocked speculative entries never appear in prompt assembly
- expired speculative entries never appear in prompt assembly
- request-time speculative work stays within the accepted latency budget
- deep validation and promotion are fully deferred
- speculative context remains same-session only in phase 1

## Immediate Follow-On Work

After this contract is accepted, the next planning artifacts should be:

1. hydrator interface and payload contract
2. schema and migration draft
3. phase 1 acceptance test matrix
4. feasibility spike implementation plan

## Final Position

The hot path can support speculative context, but only if the system is disciplined about one thing:

**hydration and authority must be explicit.**

If speculative context is added as loose text or route-local glue, it will drift, leak, and eventually bypass validation.

If it enters through one shared hydrator, one fast gate, and one retrieval policy, it can improve continuity without corrupting the memory model.
