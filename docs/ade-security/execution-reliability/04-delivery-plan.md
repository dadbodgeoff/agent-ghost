# Delivery Plan

## Phase 1: Correctness Foundation

Goal:

- eliminate blind state races
- make studio stream fail closed

Work:

1. Add CAS update APIs for live execution rows.
2. Introduce immutable terminal-state semantics.
3. Convert cancel flow to `cancel_requested -> cancelled`.
4. Register cancellation control at execution acceptance for all live routes.
5. Make studio stream durable-write failure terminate into `recovery_required`.
6. Make studio stream terminal success depend on canonical assistant-message durability.

Exit criteria:

- no last-write-wins transition path
- no studio stream best-effort durability path

## Phase 2: Attempt And Step Journal

Goal:

- establish replay boundaries inside a turn

Work:

1. Add `execution_attempts`.
2. Add `execution_steps`.
3. Thread `execution_id` through runner -> tool executor -> adapters.
4. Implement checkpoint hooks for replay-safe and journaled builtins.
5. Mark unsupported side-effect classes as blocked / review-required on reliable routes.

Exit criteria:

- every reliable tool execution has a durable `started` / `committed` model or is blocked

## Phase 3: Unified Recovery Engine

Goal:

- one recovery path for blocking and streaming routes

Work:

1. Build recovery classifier from execution + attempt + step rows.
2. Reconstruct conversation state from committed steps.
3. Reconstruct stream replay from durable execution event log.
4. Add operator `needs_review` state and recovery API surface.

Exit criteria:

- restart / takeover behavior is deterministic for every non-terminal status

## Phase 4: Verification And Hardening

Goal:

- prove the failure matrix, not just compile it

Work:

1. property tests for state transitions
2. concurrency tests for cancel vs complete races
3. fault-injection tests for DB failure during:
   - text flush
   - terminal event persist
   - assistant result write
   - state CAS
4. restart / takeover integration tests
5. UI / API acceptance tests for review and recovery states

Exit criteria:

- verification matrix passes in CI
- operator runbooks are complete

## Implementation Order

Build in this order:

1. storage primitives
2. live execution state machine
3. streaming fail-closed semantics
4. step journal plumbing
5. tool adapter checkpointing
6. recovery engine
7. UI / SDK / OpenAPI
8. tests and runbooks

## Files Most Likely To Change

- `crates/ghost-gateway/src/api/live_executions.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/idempotency.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/tools/executor.rs`
- `crates/cortex/cortex-storage/src/queries/live_execution_queries.rs`
- new storage migrations for attempts / steps / stream events
- dashboard and SDK surfaces for operator recovery

## Things The Implementation Must Not Do

- do not add more "recovery_required and hope the operator figures it out" branches without checkpoint data
- do not keep streaming after durable persistence failure
- do not claim exactly-once for arbitrary shell / external effects
- do not introduce route-specific state machines that diverge again
