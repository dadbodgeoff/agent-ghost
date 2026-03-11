# Current State And Gap Audit

## What Is Already Strong

- Operation journal leases already use owner-token and lease-epoch CAS semantics.
- `agent_chat` streaming fails closed on stream-event persistence errors instead of pretending recovery is still safe.
- Live executions already exist as a first-class durable object for accepted-boundary routes.
- The runner already supports cooperative cancellation via `CancellationToken`.

## What Is Not Good Enough

### 1. Live execution state transitions are not CAS-protected

Current live execution updates are blind row rewrites. That means a stale cancel or recovery write can overwrite a more recent terminal state.

Examples in code:

- `crates/ghost-gateway/src/api/live_executions.rs`
- `crates/cortex/cortex-storage/src/queries/live_execution_queries.rs`

Impact:

- completed -> cancelled corruption
- completed -> recovery_required corruption
- stale read + blind write races during cancel / complete overlap

### 2. Studio stream durability is fail-open

The studio stream path continues serving the client after durable stream writes fail. Worse, the text buffer is cleared even when the persistence call fails.

Examples in code:

- `crates/ghost-gateway/src/api/studio_sessions.rs`

Impact:

- durable event log no longer matches what the client saw
- reconnect / restart can lose already-emitted content
- terminal success can be published after degraded persistence

### 3. Studio stream terminal success does not require durable canonical result storage

Assistant message insertion is best-effort. The stream can still emit `TurnComplete`, and the execution can still be marked `completed`.

Impact:

- session history can miss the assistant answer
- replay depends on partial stream logs instead of a durable canonical result
- "completed" no longer means "fully reconstructable"

### 4. Studio stream cancellation registration is late

The execution control is only inserted after the first substantive forwarded event.

Impact:

- long provider startup can be uncancellable
- DB state may say `cancelled` while the live task keeps running

### 5. Blocking recovery does not resume from tool-level checkpoints

The current blocking routes can return `recovery_required`, but they do not reconstruct tool progress from durable per-step records because those records do not exist.

Impact:

- side-effect safety depends on route-specific early exits, not a real checkpoint model
- there is no deterministic resume boundary inside a turn

### 6. Tool execution context still does not carry a durable execution id into replay-sensitive paths

The runner constructs tool execution contexts with `execution_id: None`.

Impact:

- no per-tool checkpoint key
- no exact correlation between live execution and tool attempt journal
- impossible to prove exactly-once for supported side-effecting tools

### 7. The codebase already acknowledges the exact-once gap

The external side-effecting skill route is blocked until durable exactly-once execution exists.

Impact:

- this is not a theoretical concern
- the repo already has the right instinct; the ADE runtime needs to apply the same bar internally

## Audit Conclusion

The current system is usable, but it is not at the target reliability bar.

Most of the remediation belongs in four places:

1. live execution state machine and storage CAS
2. step journal / checkpoint model for tool execution
3. fail-closed streaming durability
4. recovery orchestration and operator review semantics
