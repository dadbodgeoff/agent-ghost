# Target System Spec

## Core Design Principle

There must be one authoritative execution model for:

- blocking accepted-boundary routes
- streaming accepted-boundary routes
- cancellation
- recovery
- replay

Route handlers may differ in transport shape, but not in execution semantics.

## Authoritative State Machine

Each live execution row must have:

- `execution_id`
- `journal_id`
- `operation_id`
- `route_kind`
- `state_version`
- `status`
- `attempt`
- `updated_at`
- `state_json`

Allowed statuses:

- `accepted`
- `preparing`
- `running`
- `cancel_requested`
- `cancelled`
- `completed`
- `recovery_required`
- `needs_review`
- `failed`

Rules:

- status changes must use compare-and-swap on `(execution_id, expected_status, attempt)`
- terminal states are:
  - `cancelled`
  - `completed`
  - `needs_review`
  - `failed`
- terminal states are immutable except via explicit operator repair tooling

## New Durable Tables

### `execution_attempts`

Purpose:

- model retries / takeovers without mutating history away

Fields:

- `execution_id`
- `attempt`
- `owner_token`
- `lease_epoch`
- `status`
- `started_at`
- `ended_at`
- `failure_class`
- `failure_detail`

### `execution_steps`

Purpose:

- durable per-step replay boundary inside an execution

Fields:

- `execution_id`
- `attempt`
- `step_seq`
- `step_kind` (`llm_call`, `tool_call`, `stream_chunk_flush`, `final_result_write`)
- `step_fingerprint`
- `tool_name`
- `status` (`planned`, `started`, `committed`, `failed`, `cancelled`)
- `request_json`
- `result_json`
- `started_at`
- `ended_at`

Constraints:

- unique `(execution_id, attempt, step_seq)`
- unique `(execution_id, step_kind, step_fingerprint)` where semantic dedupe is required

### `execution_stream_events`

Purpose:

- replace "best effort" stream persistence with a route-independent durable event log

Fields:

- `execution_id`
- `seq`
- `event_type`
- `payload`
- `created_at`

Rules:

- event must be durable before SSE delivery
- if persistence fails, execution enters `recovery_required` and stream ends with durable error semantics

## Execution Classes

### Replay-safe

Examples:

- prompt assembly
- read-only file reads
- search / fetch that do not mutate external systems

Recovery:

- may retry from durable step boundary

### Journaled side-effecting

Examples:

- workspace file writes
- supported internal DB mutations
- approved tool operations with durable request/result commit contract

Recovery:

- if `committed`, reuse result
- if `started` but not `committed`, enter `needs_review` unless the tool adapter can prove safe retry

### Unsupported exact-once

Examples:

- arbitrary shell commands with unknown external effects
- external APIs without idempotency key / commit proof

Recovery:

- reliable route must block before execution or require explicit operator approval with `needs_review`

## Tool Adapter Contract

Every tool used on reliable routes must declare:

- `reliability_class`
- `supports_checkpointing`
- `supports_safe_retry`
- `supports_result_replay`

Required adapter hooks:

- `plan(request) -> fingerprint`
- `start(step) -> durable started record`
- `execute(step)`
- `commit(step, result) -> durable committed record`
- `replay(step) -> prior result`
- `recover(step) -> retry | replay | needs_review`

## Blocking Route Semantics

Flow:

1. create / load authoritative execution
2. create attempt row
3. transition `accepted -> preparing -> running`
4. execute steps through durable step journal
5. persist canonical result
6. transition to terminal state via CAS
7. commit idempotent operation journal response

If the process crashes:

- recovery inspects the last attempt and step rows
- replay committed steps into conversation state
- unsupported in-flight side effects move to `needs_review`

## Streaming Route Semantics

Flow:

1. create / load authoritative execution
2. register cancellation control immediately
3. persist `stream_start`
4. for each outward event:
   - persist durable event
   - then emit to client
5. persist canonical result
6. persist durable terminal event
7. transition execution to terminal state via CAS

Rules:

- no "warn and keep going" after durable write failure
- no `TurnComplete` before canonical result is durable
- replay reads from the execution event log, not handler-local assumptions

## Cancellation Semantics

Cancellation has two phases:

1. `cancel_requested`
- durable CAS transition from non-terminal running state
- cooperative signal sent immediately

2. `cancelled`
- only after the worker acknowledges cancel at a safe checkpoint

Rules:

- `cancel_requested` must not overwrite `completed`
- transport cancellation and authoritative execution cancellation must agree

## Recovery Semantics

On takeover / restart:

1. load execution row and latest attempt
2. if terminal, replay terminal response
3. if non-terminal:
   - inspect step journal
   - reconstruct committed conversation / stream state
   - classify last in-flight step
4. choose one:
   - `resume`
   - `replay`
   - `needs_review`
   - `failed`

Recovery must never:

- silently discard an in-flight side-effecting step
- silently retry an unsupported side effect
- publish success without a durable canonical result

## Operator States

The UI and API must surface:

- `recovery_required`
- `needs_review`
- `cancel_requested`
- `cancelled`
- last committed step
- last in-flight step
- replay-safe vs review-required reason
