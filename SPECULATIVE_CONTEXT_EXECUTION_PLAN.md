# Speculative Context Execution Plan

Status: March 10, 2026

Purpose: define the authoritative execution plan to evaluate, de-risk, and implement the Speculative Context Layer against the current live GHOST runtime.

This document is based on the live code and the current design docs, not on aspirational future architecture alone. If this plan conflicts with older sequence documents, this plan wins for execution ordering.

Primary design dependency:

- `SPECULATIVE_CONTEXT_LAYER_DESIGN.md`

Primary runtime sources:

- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-gateway/src/session/compaction.rs`
- `crates/ghost-gateway/src/session/lane_queue.rs`
- `crates/ghost-gateway/src/api/memory.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/tools/executor.rs`

## Standard

This work is held to the following non-negotiable bar:

- No speculative data may become durable truth without a separate promotion step.
- No blocked speculative entry may be retrievable by the prompt path.
- No new layer may silently increase p95 turn latency beyond an explicitly accepted budget.
- No speculative entry may be cited as authoritative evidence by another speculative entry.
- No shared runtime path may diverge between `agent_chat` and Studio without an explicit exception record.
- No safety claim may depend on a background worker that can fail open.

## Why This Exists

The architecture RFC establishes the right model:

- live session state
- speculative context
- durable memory

The execution problem is narrower:

- prove the hot path can support it
- define the exact schema and query model
- define the async job model
- identify the runtime seams to change
- define kill criteria before implementation begins

This document is the bridge between concept and build.

## Current Runtime Assessment

### A1. `agent_chat` is not yet a hydrated context entrypoint.

`POST /api/agent/chat` currently constructs runtime execution with `RunnerBuildOptions::default()`.

Implication:

- no preloaded conversation history
- no speculative context input
- no session-aware hydration equivalent to Studio

Impact:

- this route cannot yet be the canonical spawn-aware entrypoint
- speculative context integration must include a hydration seam here first

### A2. Studio already has the nearest working hydration pattern.

Studio reconstructs conversation history from persistence and injects it through `RunnerBuildOptions`.

Implication:

- the Studio path is the best starting template for request-time hydration
- the speculative context hydrator should align `agent_chat` with Studio rather than invent a third runtime shape

### A3. The runner’s immutable snapshot is still mostly empty on the hot path.

`AgentRunner::pre_loop()` currently assembles an `AgentSnapshot` with real convergence metadata but empty goals, reflections, and memories.

Implication:

- the read-only snapshot contract exists
- the actual data payload is not yet fully wired

Impact:

- speculative context must not be bolted on as ad hoc text alone
- hydration should feed a real structured snapshot path where possible

### A4. Compaction exists but is not currently the live chat critical path.

`SessionCompactor` is implemented and tested, but the production runtime seam that invokes it inline for `agent_chat` was not found in the current live path.

Implication:

- the speculative context layer should not assume a fully wired in-band compaction lifecycle already exists
- phase 1 should treat candidate attempt creation as a new explicit post-turn step

### A5. Memory persistence is real enough to anchor promotion.

`POST /api/memory` writes events, snapshots, and embeddings.

Implication:

- durable promotion does not need a net-new memory persistence subsystem
- the execution plan should reuse this path or its underlying query layer rather than invent parallel durable writes

### A6. The memory tool’s read path is not yet fed from a live hydrated snapshot.

The tool executor has `snapshot_memories`, but the current runtime path does not appear to populate it.

Implication:

- speculative context retrieval must be integrated into runtime hydration, not only into the external memory API

## Execution Strategy

The work will proceed in five tracks, in this order:

1. runtime contract definition
2. storage and query design
3. feasibility spikes
4. phase 1 implementation
5. promotion and cross-session expansion

The goal is to burn the highest-risk unknowns first rather than starting with migrations and hoping the runtime fits later.

## Track 1: Runtime Contract Definition

### Objective

Define the exact request-time behavior for speculative context in the live runtime.

### Deliverable

A short hot-path contract covering:

- what data is loaded before the model call
- what operations may block the user
- what operations must be deferred
- how `agent_chat` and Studio align
- how the runtime behaves when speculative subsystems are degraded

### Required Decisions

#### R1. Canonical hydrator input

The runtime must decide whether speculative context enters through:

- `RunnerBuildOptions`
- `AgentSnapshot` assembly
- prompt-layer text injection
- or a combination of those three

Recommended direction:

- `conversation_history` remains in `RunnerBuildOptions`
- speculative context and durable memory flow through snapshot assembly and prompt compilation
- avoid a free-form system-message splice as the primary integration seam

#### R2. Blocking budget

The request path may:

- load recent session history
- query bounded speculative entries
- run a bounded fast gate on newly written attempts

The request path may not:

- block on deep validation
- block on promotion
- block on embedding batches
- block on contradiction graph recomputation

#### R3. Shared runtime behavior

`agent_chat` and Studio must use the same speculative hydration policy unless an explicit exception is documented.

This matters because the repo already shows divergence pressure between these entrypoints.

### Exit Criteria

- one written hot-path contract
- one shared terminology set for `agent_chat` and Studio
- one explicit list of allowed blocking operations

## Track 2: Storage and Query Design

### Objective

Turn the conceptual tables into a migration-grade design.

### Deliverables

- migration plan
- index plan
- TTL/expiry plan
- query shape plan
- row growth estimate

### Required Decisions

#### S1. Physical storage model

Decide whether speculative context lives:

- in SQLite alongside the current runtime tables
- in a separate database file
- or in a separate schema namespace pattern

Recommended direction for phase 1:

- same SQLite authority, separate tables

Reason:

- easiest to audit
- simplest transactional promotion path
- consistent with current gateway storage model

#### S2. Retrieval indexes

The `context_attempts` table needs indexes for:

- session-scoped retrieval by status and TTL
- agent/session TTL cleanup
- promotion candidate lookup
- validation job dequeue

#### S3. TTL model

TTL must be explicit by `attempt_kind`.

Initial planning assumption:

- `summary`: short TTL
- `fact_candidate`: medium TTL if still awaiting deep validation
- `tool_observation`: very short TTL unless promoted

#### S4. Write amplification risk

The plan must estimate:

- attempts per turn
- validation records per attempt
- cleanup frequency
- promotion write fanout

If this is not estimated before coding, storage pressure will appear as a runtime surprise.

### Research Output

Produce a short capacity note with:

- expected rows/day
- worst-case rows/session
- cleanup cadence
- index cost

### Exit Criteria

- migration-ready schema draft
- index list
- TTL strategy
- capacity estimate good enough to reject obviously bad designs

## Track 3: Feasibility Spikes

These are not production features. They are targeted experiments to burn risk.

### Spike 1: Write-path latency

Question:

Can the runtime persist speculative attempts with negligible added user-visible latency?

Test:

- insert one or more `context_attempts` rows in the post-turn path
- include fast-gate decision updates
- measure added p50/p95 latency

Success condition:

- write path is cheap enough to remain in the request-adjacent path

Failure condition:

- speculative write overhead is large enough that even phase 1 harms chat responsiveness

### Spike 2: Retrieval ranking

Question:

Can speculative context be retrieved without overpowering durable memory?

Test:

- mock mixed retrieval inputs
- score durable and speculative entries together
- verify durable entries remain dominant when semantically equivalent

Success condition:

- speculative context improves continuity while staying subordinate

Failure condition:

- speculative context consistently crowds out durable memory or bloats prompt assembly

### Spike 3: Leak prevention

Question:

Can blocked speculative entries be proven absent from prompt assembly?

Test:

- write attempts across all statuses
- invoke hydration and prompt assembly
- assert blocked and expired entries never appear

Success condition:

- blocked entry prompt leak count is zero

Failure condition:

- status filtering depends on convention rather than a hard retrieval boundary

### Spike 4: Degraded-mode safety

Question:

What happens when deep validation or contradiction checks are down?

Test:

- simulate worker outage
- simulate contradiction service unavailable
- observe retrieval and promotion behavior

Success condition:

- retrieval remains bounded and promotion fails closed

Failure condition:

- system silently promotes or broadens exposure under degraded state

## Track 4: Phase 1 Implementation Plan

Phase 1 is deliberately narrow.

### Phase 1 Scope

Deliver:

- `context_attempts`
- `context_attempt_validation`
- `context_attempt_jobs`
- bounded fast gate
- same-session retrieval only
- `summary` attempt kind only
- TTL expiration
- observability counters

Do not deliver:

- durable promotion
- fact or goal promotion
- cross-session retrieval
- cross-agent speculative sharing

### Phase 1 Runtime Shape

1. User turn executes normally.
2. Post-turn summarizer emits one bounded speculative summary attempt.
3. Attempt is stored.
4. Fast gate classifies it.
5. If `retrievable`, it may be included in same-session hydration on later turns.
6. TTL cleanup removes it when stale.

### Phase 1 Integration Points

#### I1. Post-turn attempt writer

Likely seam:

- shared runtime completion path after `run_turn`

Candidates:

- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- or a new shared helper behind both

Recommended direction:

- do not duplicate write logic in both endpoints
- create one shared post-turn persistence helper

#### I2. Request-time hydrator

Likely seam:

- runtime preparation before `pre_loop`

Candidates:

- `crates/ghost-gateway/src/api/runtime_execution.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`

Recommended direction:

- introduce one hydrator that resolves:
  - conversation history
  - durable memory payload
  - speculative context payload
  - convergence payload

#### I3. Prompt injection seam

Likely seam:

- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/context/prompt_compiler.rs`

Recommended direction:

- speculative context should not be appended as arbitrary prose after the fact
- it should become a structured input to prompt compilation or snapshot formatting

### Phase 1 Acceptance Criteria

- speculative write path adds acceptable latency only
- blocked entries never appear in prompt assembly
- expired entries never appear in prompt assembly
- same-session retrieval works
- cross-session retrieval does not exist
- observability captures attempt counts, blocked counts, and retrieval counts

## Track 5: Promotion and Expansion

This track does not begin until phase 1 metrics are healthy.

### Phase 2

Deliver:

- deep validation worker
- contradiction handling
- flagged and blocked transitions beyond fast gate

### Phase 3

Deliver:

- promotion linkage table
- controlled durable promotion for `fact_candidate`
- retry-safe promotion worker

### Phase 4

Deliver:

- spawn-aware hydration for `agent_chat`
- aligned runtime semantics between Studio and `agent_chat`

### Phase 5

Potential future expansion:

- cross-session speculative retrieval
- scoped multi-agent speculative sharing

No work in this phase is allowed until promotion quality and safety metrics prove the layer is stable.

## Failure Policy

This must be settled before implementation.

### F1. Fast gate unavailable

Behavior:

- new attempts are not retrievable
- request path may store them as `pending`
- prompt path excludes them

Rule:

- fail closed for retrieval

### F2. Deep validation unavailable

Behavior:

- already retrievable same-session summaries may remain retrievable until TTL
- promotion is disabled

Rule:

- fail closed for promotion

### F3. Contradiction checks unavailable

Behavior:

- no speculative-to-durable promotion
- no expansion of retrieval scope

Rule:

- fail closed for authority crossing

### F4. Expiration worker delayed

Behavior:

- request-time retrieval must still check `expires_at`

Rule:

- TTL enforcement may not depend solely on background deletion

### F5. Partial promotion failure

Behavior:

- do not mark attempt `promoted`
- do not leave dangling authority state
- retain retry information

Rule:

- promotion must be atomic from the perspective of authority

## Metrics

These metrics must exist before phase 1 exits.

### Runtime

- speculative attempt write latency
- added turn latency attributable to speculative work
- hydration query latency

### Quality

- attempts created per turn
- retrievable rate
- blocked rate
- expired rate
- retrieval hit rate

### Safety

- blocked entry prompt leak count
- expired entry prompt leak count
- promotion while contradiction unavailable count
- speculative citation-chain violation count

### Capacity

- pending job depth
- rows by status
- TTL backlog

## Go / No-Go Criteria

The program stops or is redesigned if any of the following are true:

- blocked speculative entries can leak into prompt assembly
- speculative context materially degrades turn latency beyond the accepted budget
- retrieval ranking cannot keep durable memory authoritative
- promotion semantics cannot be made retry-safe and fail-closed
- the runtime requires duplicated logic between Studio and `agent_chat` to make this work

## Immediate Next Artifacts

The next planning outputs should be created in this exact order:

1. hot-path contract note
2. schema and migration draft
3. feasibility spike plan
4. integration impact map by file and function
5. phase 1 acceptance test matrix

## Final Position

This work is feasible, but only if the team treats it as a runtime-execution program, not as a storage feature.

The hardest part is not creating a temporary table.

The hardest parts are:

- keeping the hot path fast
- making retrieval fail closed
- preventing speculative self-reinforcement
- aligning all runtime entrypoints on one hydration model

The purpose of this execution plan is to force those truths to be handled first.
