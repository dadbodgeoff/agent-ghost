# Speculative Context Integration Impact Map

Status: March 10, 2026

Purpose: map phase 1 of the Speculative Context Layer to the concrete files, seams, and responsibilities in the current GHOST codebase.

This document exists to answer one question:

**If phase 1 starts now, exactly where does the work go?**

Primary dependencies:

- `SPECULATIVE_CONTEXT_LAYER_DESIGN.md`
- `SPECULATIVE_CONTEXT_EXECUTION_PLAN.md`
- `SPECULATIVE_CONTEXT_HOT_PATH_CONTRACT.md`
- `SPECULATIVE_CONTEXT_HYDRATOR_CONTRACT.md`
- `SPECULATIVE_CONTEXT_SCHEMA_DRAFT.md`

## Reading Guide

Each file is assigned one of four impact levels:

- `P0 modify now`
- `P1 likely modify in phase 1`
- `P2 later phase`
- `No change in phase 1`

The goal is not to maximize touched files. The goal is to identify the minimum coherent slice of changes required to make phase 1 real.

## Phase 1 Summary

Phase 1 must deliver:

- one shared hydrator
- same-session speculative retrieval only
- one bounded summary attempt per successful turn
- fast-gate classification
- blocked and expired entry exclusion from prompt and tool views

Phase 1 must not deliver:

- durable promotion
- cross-session speculative retrieval
- multi-agent speculative sharing

## Runtime Entry Points

### 1. `crates/ghost-gateway/src/api/agent_chat.rs`

Impact: `P0 modify now`

### Why it matters

This route is currently under-hydrated relative to Studio.

Observed live seam:

- the blocking path uses `prepare_requested_runtime_execution(..., RunnerBuildOptions::default())`
- the streaming paths also use `RunnerBuildOptions::default()`

### Phase 1 responsibilities

- stop using raw default runtime build options for live chat execution
- call the shared hydrator-backed preparation path
- participate in shared post-turn speculative attempt write flow
- ensure streaming and blocking variants use the same hydration semantics

### Expected changes

- replace direct `RunnerBuildOptions::default()` use
- thread hydrated context into runtime preparation
- invoke shared post-turn attempt writer helper after successful turn completion
- ensure failure behavior remains aligned with the hot-path contract

### Risk if unchanged

- `agent_chat` remains a second-class path
- speculative context only works in Studio-like flows
- route divergence worsens

## 2. `crates/ghost-gateway/src/api/studio_sessions.rs`

Impact: `P0 modify now`

### Why it matters

Studio already has the closest live hydration pattern because it reconstructs conversation history before runtime execution.

### Phase 1 responsibilities

- stop owning route-local hydration logic independently
- delegate history plus speculative hydration to the shared hydrator path
- participate in the same shared post-turn speculative attempt write flow as `agent_chat`

### Expected changes

- replace direct route-local `build_conversation_history(...)` orchestration as the sole hydration source
- call the shared hydrator-backed runtime preparation path
- keep Studio-specific request/response persistence behavior outside the hydrator

### Risk if unchanged

- Studio and `agent_chat` evolve different speculative semantics
- blocked-entry guarantees may hold in one route and fail in another

## 3. `crates/ghost-gateway/src/api/runtime_execution.rs`

Impact: `P0 modify now`

### Why it matters

This is the cleanest current seam for shared runtime preparation.

Current role:

- resolve runtime agent
- prepare runner
- return `PreparedRuntimeExecution`

### Phase 1 responsibilities

- become the canonical entrypoint for hydrator-backed runtime preparation
- accept hydrator output
- separate route-level shell logic from runtime assembly logic

### Expected changes

- extend preparation flow to load or receive `HydratedRuntimeContext`
- construct `PreparedRuntimeExecution` from:
  - shared runtime safety context
  - shared build options
  - hydrated snapshot-side inputs

### Recommended direction

- avoid putting SQL reads directly into route handlers
- push hydration orchestration into this layer or a sibling runtime module

## 4. `crates/ghost-gateway/src/runtime_safety.rs`

Impact: `P0 modify now`

### Why it matters

This file currently constructs the live runner and is the natural place to accept additional runner-facing context beyond conversation history.

Current relevant seams:

- `RunnerBuildOptions`
- `build_live_runner`
- `build_live_runner_with_dependencies`

### Phase 1 responsibilities

- evolve runtime-preparation types so they can carry hydrator-produced snapshot input
- populate runner state needed for prompt and tool consistency

### Expected changes

- extend `RunnerBuildOptions` or add adjacent hydrated runtime types
- thread hydrated snapshot payload into runner setup
- populate tool-executor snapshot memory view from the same authoritative hydration path

### Important constraint

Do not overload `RunnerBuildOptions` into a dumping ground for every state class. Conversation history belongs there. Snapshot-side memory state likely needs an adjacent type.

## Runner and Prompt Assembly

### 5. `crates/ghost-agent-loop/src/runner.rs`

Impact: `P0 modify now`

### Why it matters

This file owns:

- `pre_loop()` snapshot assembly
- prompt-input compilation

Current live gap:

- the runner assembles a snapshot with real convergence state but empty goals, reflections, and memories
- prompt input uses mostly default dynamic context fields

### Phase 1 responsibilities

- consume hydrated snapshot input instead of assembling an empty memory state
- ensure speculative context influences the runner only through structured bounded inputs

### Expected changes

- add a seam for injecting hydrated snapshot-side data before or during `pre_loop()`
- stop defaulting snapshot memory payload to empty when hydrator data exists
- ensure prompt input can reflect hydrated snapshot-derived memory state

### Risk if unchanged

- speculative context would exist in storage but not in the actual runtime cognition path

## 6. `crates/ghost-agent-loop/src/context/prompt_compiler.rs`

Impact: `P1 likely modify in phase 1`

### Why it matters

Prompt compilation is where structured memory and convergence information must become visible to the model without route-level glue.

### Phase 1 responsibilities

- accept bounded structured memory-related context produced from the hydrated snapshot path
- preserve durable-memory authority over speculative context

### Expected changes

- likely wire non-empty `convergence_state`
- likely wire bounded memory-related content derived from snapshot inputs

### Constraint

Do not add a speculative-only prompt lane that bypasses the snapshot/memory authority model.

## Tooling

### 7. `crates/ghost-agent-loop/src/tools/executor.rs`

Impact: `P0 modify now`

### Why it matters

The memory read tool uses `snapshot_memories`, but the live runtime does not appear to populate that field in the current hot path.

Current relevant seam:

- `set_snapshot_memories(...)`
- `read_memories(..., &self.snapshot_memories)`

### Phase 1 responsibilities

- receive the same hydrated memory view used by prompt assembly
- ensure blocked and expired speculative entries never appear in tool results

### Expected changes

- populate `snapshot_memories` during runner setup or pre-loop setup from the shared hydrator output
- align tool-side filtering with prompt-side filtering

### Risk if unchanged

- prompt path and tool path diverge
- blocked content could be hidden from prompt assembly but still leak via `memory_read`

## Read-Only Snapshot Pipeline

### 8. `crates/read-only-pipeline/src/assembler.rs`

Impact: `P1 likely modify in phase 1`

### Why it matters

This file already contains the correct abstraction for convergence-aware snapshot assembly.

### Phase 1 responsibilities

- likely remain the core snapshot constructor
- may need no behavior changes if the hydrator pre-merges durable and speculative memory inputs before assembly

### Expected changes

Two acceptable paths exist:

1. no phase 1 code change here
   - hydrator prepares one merged memory vector
   - assembler remains unchanged

2. small phase 1 extension
   - if snapshot construction needs a clearer durable/speculative merge hook

### Recommendation

Prefer path 1 in phase 1. Keep the assembler stable unless a real implementation need proves otherwise.

## Gateway Session and Compaction

### 9. `crates/ghost-gateway/src/session/compaction.rs`

Impact: `P1 likely modify in phase 1`

### Why it matters

This file is the conceptual home of compaction, but the current live chat path does not appear to rely on it end-to-end.

### Phase 1 responsibilities

- not to implement full in-band compaction
- possibly provide a narrow summary-generation helper or reuse point if phase 1 derives its one summary attempt from compaction logic

### Recommendation

Do not force full compaction wiring in phase 1 if the post-turn speculative summary can be produced through a smaller shared summarization seam.

## 10. `crates/ghost-gateway/src/session/lane_queue.rs`

Impact: `No change in phase 1`

### Why it matters

This is already the session-serialization story.

### Phase 1 role

- existing queue semantics remain valid
- speculative context does not require new queue semantics in phase 1

### Revisit later if

- speculative post-turn writes materially change per-session latency behavior

## Storage Layer

### 11. `crates/cortex/cortex-storage/src/migrations/mod.rs`

Impact: `P0 modify now`

### Why it matters

Migration registration authority.

### Phase 1 responsibilities

- register `v060_speculative_context_phase1`
- bump `LATEST_VERSION`
- append migration tuple

## 12. `crates/cortex/cortex-storage/src/migrations/v060_speculative_context_phase1.rs`

Impact: `P0 add now`

### Why it matters

This is the new migration implementation file for the phase 1 tables and indexes.

### Phase 1 responsibilities

- create `context_attempts`
- create `context_attempt_validation`
- create `context_attempt_jobs`
- create required retrieval and worker indexes

## 13. `crates/cortex/cortex-storage/src/schema_contract.rs`

Impact: `P1 likely modify in phase 1`

### Why it matters

Once speculative-context tables exist, the schema contract should know they are part of the verified storage authority.

### Phase 1 responsibilities

- add the new tables and indexes to schema verification metadata

### Recommendation

Do not leave speculative context as a “real but undocumented” schema extension.

## 14. `crates/cortex/cortex-storage/src/queries/`

Impact: `P0 add now`

### Why it matters

The runtime should not hand-write SQL in route handlers for speculative context.

### Expected new query modules

- `context_attempt_queries.rs`
- `context_attempt_validation_queries.rs`
- `context_attempt_job_queries.rs`

### Phase 1 responsibilities

- insert attempt
- update status
- list retrievable same-session attempts
- insert validation rows
- enqueue and dequeue jobs
- expire attempts

## Memory and Promotion

### 15. `crates/ghost-gateway/src/api/memory.rs`

Impact: `No change in phase 1`

### Why it matters

This is the eventual durable promotion anchor, but phase 1 explicitly forbids speculative-to-durable promotion.

### Revisit in later phase for

- controlled promotion worker
- durable memory write reuse

## Tests and Verification

### 16. `crates/ghost-integration-tests/` and `tests/integration/`

Impact: `P0 add now`

### Why it matters

The phase 1 guarantees are runtime guarantees and must be tested as such.

### Required new test categories

- shared hydrator parity between `agent_chat` and Studio
- blocked speculative entries absent from prompt path
- blocked speculative entries absent from tool path
- expired speculative entries absent from all request-time retrieval
- same-session retrieval works
- cross-session retrieval does not happen

### Suggested test placement

- gateway/runtime integration tests for hydration parity
- agent-loop integration tests for prompt/tool memory alignment
- storage/query tests for TTL and status filtering

## Observability and Metrics

### 17. `crates/ghost-gateway/src/api/audit.rs` or adjacent observability paths

Impact: `P2 later phase` or `P1 if counters already exist nearby`

### Why it matters

Phase 1 needs metrics, but not necessarily a large new API surface.

### Recommendation

Start with internal counters/logging if that is cheaper and already aligned with the runtime observability model. Promote to public API surface only if operators actually need it during rollout.

## Suggested Implementation Order

This is the minimum coherent file order for phase 1.

1. add migration file
2. register migration in `migrations/mod.rs`
3. add storage query modules
4. add hydrator/runtime payload types in runtime layer
5. modify `runtime_execution.rs` to use hydrator-backed preparation
6. modify `runtime_safety.rs` to feed runner and tool memory state
7. modify `runner.rs` to consume hydrated snapshot-side inputs
8. modify `agent_chat.rs`
9. modify `studio_sessions.rs`
10. add runtime and integration tests

## Files Most Important to Get Right

If only four files are treated as truly critical in phase 1, they are:

- `crates/ghost-gateway/src/api/runtime_execution.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/cortex/cortex-storage/src/queries/context_attempt_queries.rs`

Those four files determine whether speculative context is:

- actually hydrated
- actually bounded
- actually visible in the runner
- actually consistent across prompt and tool views

## Final Position

The integration surface is smaller than it first appears, but only if the team is disciplined.

The failure mode to avoid is obvious:

- add storage
- add route-local reads
- splice text into prompts
- call it done

That would create a speculative context layer in name only.

The real phase 1 cut is:

- one storage authority
- one hydrator
- one runtime preparation path
- one prompt/tool memory view
- two route shells (`agent_chat` and Studio) on top of the same core
