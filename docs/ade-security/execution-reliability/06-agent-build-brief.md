# Agent Build Brief: Execution Reliability Hardening

## Mission

Implement the execution reliability program defined by this package for ADE live execution routes, end to end, without lowering the reliability bar.

Your job is to make accepted-boundary blocking and streaming execution safe under restart, cancel, replay, and persistence failure.

## Required Reading

Read these first and treat them as authoritative:

1. `docs/ade-security/execution-reliability/01-reliability-charter.md`
2. `docs/ade-security/execution-reliability/02-current-state-and-gap-audit.md`
3. `docs/ade-security/execution-reliability/03-target-system-spec.md`
4. `docs/ade-security/execution-reliability/04-delivery-plan.md`
5. `docs/ade-security/execution-reliability/05-verification-and-runbooks.md`

## Build Constraints

- Do not promise exactly-once for arbitrary external side effects.
- Do not keep streaming after a durable persistence failure on reliable routes.
- Do not add more blind status rewrites.
- Do not let terminal success exist without a durable canonical result record.
- Do not create one-off route semantics; keep the execution model unified.

## Required Deliverables

### Storage

- add migrations for `execution_attempts`
- add migrations for `execution_steps`
- upgrade live execution storage to support CAS transitions and immutable terminal states
- unify or replace stream-event storage under an execution-centric event log if needed

### Runtime

- thread `execution_id` through runner and tool executor
- implement step journal checkpoints
- classify tools by reliability class
- block unsupported exact-once side effects on reliable routes

### Route Behavior

- fix cancel semantics to use `cancel_requested`
- register cancellation controls immediately at acceptance
- make streaming durability fail closed
- require durable canonical result storage before terminal success
- implement unified recovery for blocking and streaming paths

### Product Surface

- expose recovery / review states in API, OpenAPI, SDK, and dashboard
- provide operator actions for `recovery_required`, `needs_review`, and stuck `cancel_requested`

### Verification

- add the failure-matrix tests from `05-verification-and-runbooks.md`
- add at least one kill-at-checkpoint integration harness for restart testing

## Suggested Execution Order

1. Implement storage primitives and CAS updates.
2. Convert cancel flow and terminal-state semantics.
3. Fix studio stream fail-open behavior.
4. Make terminal success require durable canonical result writes.
5. Thread `execution_id` through tool execution.
6. Add step journal and reliability classes.
7. Build the unified recovery classifier / engine.
8. Expose operator states and actions.
9. Finish the fault-injection and replay tests.

## Acceptance Criteria

The work is only complete when all of these are true:

- concurrent cancel / complete cannot corrupt a completed result
- studio and agent chat routes share the same execution semantics
- recovery after crash is checkpoint-driven, not guess-driven
- unsupported side effects are blocked or explicitly reviewed
- tests cover restart, replay, cancel races, and durable-write failure

## Minimum Code Areas To Inspect Before Editing

- `crates/ghost-gateway/src/api/live_executions.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/idempotency.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/tools/executor.rs`
- `crates/cortex/cortex-storage/src/queries/live_execution_queries.rs`
- existing migrations and stream event queries

## What To Report Back With

When implementation is complete, report:

1. the final state machine
2. the exact supported reliability classes
3. the unsupported side-effect classes that remain blocked
4. the tests added and failure matrix coverage
5. the remaining residual risk, if any
