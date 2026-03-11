# Verification And Runbooks

## Required Test Matrix

### State Machine

- accepted -> preparing -> running -> completed
- accepted -> cancel_requested -> cancelled
- running -> cancel_requested -> cancelled
- running cannot become cancelled after completed CAS wins
- completed cannot regress to recovery_required

### Blocking Recovery

- crash before first tool step
- crash after committed replay-safe step
- crash after committed journaled side-effecting step
- crash during unsupported side-effecting step -> `needs_review`

### Streaming Recovery

- disconnect after `stream_start`
- disconnect after persisted text chunk
- disconnect after terminal event
- DB failure on text chunk persist -> fail closed and durable recovery state
- DB failure on terminal event persist -> fail closed and durable recovery state
- assistant message write failure -> no completed state

### Cancellation

- cancel before first provider token
- cancel during tool execution
- cancel concurrent with terminal success
- repeated cancel is idempotent

### Lease Takeover

- owner loses lease before commit
- new owner takes over non-terminal execution
- stale owner cannot finalize after takeover

### Replay Safety

- same idempotency key replays committed result
- replay does not re-execute committed step
- unsupported side-effect step cannot be silently retried

## CI Requirements

Minimum:

- unit tests for state machine transitions
- integration tests for restart and takeover
- fault-injection suite for DB failures
- property tests for CAS invariants

Strongly recommended:

- chaos harness that kills the worker at named checkpoints
- deterministic fake tool adapter for side-effect commit simulation

## Observability Requirements

Emit metrics for:

- executions by terminal status
- recovery-required count
- needs-review count
- lease-takeover count
- CAS-conflict count
- cancelled-before-first-output count
- stream durable-write failure count
- step replay count
- step review-required count

## Operator Runbooks

### Recovery Required

Operator must see:

- execution id
- route kind
- last committed step
- last in-flight step
- reason recovery could not auto-resume

Allowed actions:

- resume
- mark failed
- escalate to review

### Needs Review

Operator must see:

- tool name
- tool reliability class
- step fingerprint
- request payload
- evidence for why safe retry is not provable

Allowed actions:

- approve retry
- approve replay
- reject and mark failed

### Cancel Requested Stuck

Trigger:

- `cancel_requested` older than threshold with no `cancelled`

Operator actions:

- inspect last checkpoint
- force-fail execution
- inspect worker ownership

## Definition Of Done

Do not declare this program done until:

- all matrix cases have automated coverage or an explicit waived rationale
- runbooks exist for every non-happy-path operator state
- dashboards can show the state needed to execute those runbooks
