# Integrity Hardening Build Tasks

## Objective

Execute the integrity hardening program defined in:

- [INTEGRITY_HARDENING_PROGRAM.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_HARDENING_PROGRAM.md)
- [REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md)
- [PROPOSAL_LINEAGE_AND_DECISION_STATE_MACHINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/PROPOSAL_LINEAGE_AND_DECISION_STATE_MACHINE.md)
- [INTEGRITY_TEST_DOCTRINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_TEST_DOCTRINE.md)

and bring the proposal and mutation path to a state where:

- one logical write has one durable identity
- human decisions are bound to reviewed lineage and revision
- supersession is canonical in storage
- replay safety is real, not implied
- mixed-version clients fail closed instead of drifting silently

## Program Rules

These are blocking rules, not suggestions.

- Do not implement idempotency only in clients. Dedupe must be gateway-owned.
- Do not approve or reject proposals by proposal ID alone after the new decision contract is introduced.
- Do not derive canonical subject identity from `goal_text` except in an explicitly marked legacy-compatibility path.
- Do not mutate proposal fact rows after insert in the new model.
- Do not treat `resolved_at IS NULL` as sufficient protection for human decisions.
- Do not keep a fake replay header or stale-session mechanism once real replay semantics exist.
- Do not leave logout, auth reset, or account switch behavior unspecified for durable local queues.
- Do not silently fall back when expected lineage or revision preconditions are missing.
- Do not remove legacy reads until compatibility coverage is green.
- Do not mark historical backfill as authoritative if lineage certainty was not durably stored.
- Do not use mocks as the sole evidence for gateway, transition-engine, service-worker replay, desktop capability, or compatibility-critical behavior.
- Do not count happy-path tests alone as sufficient coverage for a completed workstream.
- Do not leave crash-recovery, restart, or version-skew cases unexecuted for touched critical paths.

## Global Release Gates

Release is blocked until all are true:

- all policy-A mutating routes carry operation identity
- the gateway journal dedupes duplicate retries and replays committed responses
- proposal approvals and rejections require expected state plus reviewed lineage context
- supersession is persisted transactionally in storage
- stale approval after supersession is rejected
- logout and account switch invalidate durable queued writes
- compatibility checks exist for mixed client and gateway versions
- unit, integration, and adversarial tests cover retry, stale decision, restart, and replay cases
- no critical-path release gate relies only on mocks or synthetic in-memory substitutes
- crash recovery and version-skew evidence exists for touched workstreams

## Forbidden Shortcuts

These count as failure even if tests appear green:

- storing only `operation_id` without a durable idempotency record
- adding new mutable columns to `goal_proposals` and calling that the state machine
- representing lineage solely as `goal_text`
- accepting approval requests without `expected_*` preconditions after phase 3
- replaying queued writes after auth or identity change
- returning `409` for all proposal conflicts without distinguishing stale vs duplicate vs mismatch
- claiming replay safety without logout and identity-switch invalidation evidence
- marking a workstream done without the required named fault-class tests from the test doctrine

## Enforcement Queries

These should trend toward the target state during implementation:

- old approval path assumptions:
  `rg -n "approve\\(|reject\\(|/api/goals/.*/approve|/api/goals/.*/reject" packages/sdk dashboard/src crates/ghost-gateway`
- legacy mutable proposal writes:
  `rg -n "resolved_at IS NULL|UPDATE goal_proposals SET decision" crates`
- text-derived lineage:
  `rg -n "goal_text|pending_by_goal|Superseded" crates/ghost-agent-loop crates/ghost-gateway crates/cortex`
- missing operation identity on writes:
  `rg -n "request\\<|fetch\\(|POST|PATCH|DELETE" packages/sdk crates/ghost-gateway/src/cli`
- unfinished critical-path code:
  `rg -n "TODO|FIXME|stub|unimplemented!|todo!" crates/ghost-gateway crates/ghost-agent-loop crates/cortex dashboard/src packages/sdk`

## Workstreams

All workstreams must satisfy the no-mock and fault-injection requirements in:

- [INTEGRITY_TEST_DOCTRINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_TEST_DOCTRINE.md)

## W1. Operation Envelope Foundation

### Goal

Create the transport and provenance layer for all replayable writes.

### Tasks

- add request options and ID generation to [`packages/sdk/src/client.ts`](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/client.ts)
- extend mutating SDK APIs to accept operation-aware options
- preserve logical operation identity across CLI retries
- update gateway CORS allow and expose headers
- add gateway operation-context middleware

### File Targets

- [`packages/sdk/src/client.ts`](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/client.ts)
- [`packages/sdk/src/goals.ts`](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/goals.ts)
- [`crates/ghost-gateway/src/bootstrap.rs`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs)
- new gateway middleware file under [`crates/ghost-gateway/src/api/`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/)
- CLI HTTP client files under [`crates/ghost-gateway/src/cli/`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/)

### Done When

- every policy-A write can carry `X-Request-ID`, `X-Ghost-Operation-ID`, and `Idempotency-Key`
- retries regenerate only `X-Request-ID`
- response headers expose idempotency status

### Adversarial Tests

- same write retried 100 times commits once
- same idempotency key with different payload returns a conflict
- commit-before-response timeout returns replayed success on retry

### Production Evidence

- real gateway plus SQLite integration run
- property test proving canonical fingerprint stability
- named retry-after-commit crash or failpoint test

## W2. Gateway Journal And Replay Enforcement

### Goal

Make the gateway the only owner of dedupe and replay behavior.

### Tasks

- add `operation_journal` migration
- implement fingerprinting over method, route template, actor, and canonicalized body
- add idempotent execution helper for JSON mutations
- persist replay metadata into audit paths

### File Targets

- new migration under [`crates/cortex/cortex-storage/src/migrations/`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/cortex/cortex-storage/src/migrations/)
- gateway handler helpers under [`crates/ghost-gateway/src/api/`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/)
- audit-related storage and handler files

### Done When

- duplicate retry returns original committed response
- in-progress duplicate returns deterministic conflict
- mismatch reuse returns deterministic conflict

### Adversarial Tests

- kill process after commit but before response, then retry
- restart gateway and retry same operation
- query audit trail by operation ID

### Production Evidence

- journal replay verified without mocked storage
- stored response replay verified after process restart
- audit query by operation ID demonstrated in integration output

## W3. Proposal V2 Schema And Transition Engine

### Goal

Replace mutable proposal resolution with immutable facts plus append-only lifecycle transitions.

### Tasks

- add `goal_proposals_v2`
- add `goal_proposal_transitions`
- add `goal_lineage_heads`
- create transition engine with transactional `BEGIN IMMEDIATE` semantics
- separate validation disposition from lifecycle state

### File Targets

- new migrations under [`crates/cortex/cortex-storage/src/migrations/`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/cortex/cortex-storage/src/migrations/)
- proposal query layer under [`crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs)
- gateway goals API under [`crates/ghost-gateway/src/api/goals.rs`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/goals.rs)
- agent-loop persistence under [`crates/ghost-agent-loop/src/runner.rs`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-agent-loop/src/runner.rs)

### Done When

- proposal facts are immutable after insert
- exactly one terminal lifecycle transition exists per proposal
- supersession and timeout are stored as transitions, not field rewrites

### Adversarial Tests

- concurrent approve vs reject yields one terminal state
- timeout vs approve race yields one valid outcome
- two competing superseding proposals preserve one canonical head

### Production Evidence

- transition engine tested against real transaction boundaries
- property or concurrency test proves single terminal state invariant
- migration rehearsal result for v2 tables captured

## W4. Decision Contract Migration

### Goal

Bind human decisions to the exact reviewed lineage and revision.

### Tasks

- add approval and rejection request bodies with `expected_*` preconditions
- keep route ergonomics while routing through one internal transition engine
- expose lineage and reviewed revision in goal detail reads
- update SDK `GoalsAPI.approve()` and `GoalsAPI.reject()` to send preconditions
- update dashboard goal and approvals screens to use fresh detail reads, not list-row assumptions

### File Targets

- [`packages/sdk/src/goals.ts`](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/goals.ts)
- [`dashboard/src/routes/approvals/+page.svelte`](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/approvals/+page.svelte)
- [`dashboard/src/routes/goals/+page.svelte`](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/goals/+page.svelte)
- [`dashboard/src/routes/goals/[id]/+page.svelte`](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/goals/[id]/+page.svelte)
- [`crates/ghost-gateway/src/api/goals.rs`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/goals.rs)

### Done When

- approval requests fail if state, lineage, subject, or reviewed revision is stale
- dashboard does not approve from proposal ID alone
- gateway distinguishes stale decision from duplicate retry

### Adversarial Tests

- approve stale proposal after superseding replacement exists
- replay old approve after restart
- reject with wrong reviewed revision

### Production Evidence

- dashboard and SDK decision flow tested against real gateway contract
- stale decision conflict types are distinguishable in test assertions
- no approval path remains that sends only proposal ID in touched clients

## W5. Agent-Loop Supersession Canonicalization

### Goal

Stop relying on in-memory supersession as the source of truth.

### Tasks

- stop using `pending_by_goal` as canonical supersession authority
- compute or assign stable subject identity during proposal creation
- persist `supersedes_proposal_id` when a new proposal replaces a pending one
- use storage-backed lineage head checks instead of process-only maps for final authority

### File Targets

- [`crates/ghost-agent-loop/src/proposal/router.rs`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-agent-loop/src/proposal/router.rs)
- [`crates/ghost-agent-loop/src/runner.rs`](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-agent-loop/src/runner.rs)
- storage query layer

### Done When

- restart does not forget supersession lineage
- new proposal creation updates canonical lineage atomically

### Adversarial Tests

- restart between old proposal and superseding proposal
- multi-process or repeated run path still converges to one lineage head

### Production Evidence

- restart test proves supersession survives process loss
- no canonical decision depends only on in-memory `pending_by_goal`

## W6. Durable Replay And Session Epoch Binding

### Goal

Make offline and restore behavior real and identity-safe.

### Tasks

- extend pending action queue format to store operation envelope
- require `client_id` and `session_epoch` for durable replay
- invalidate queued writes on logout, auth reset, and account switch
- either finish service-worker replay fully or remove stale safety signals until complete

### File Targets

- [`dashboard/src/service-worker.ts`](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/service-worker.ts)
- auth/session state files in dashboard and desktop runtime

### Done When

- queued writes survive reconnect with same identity
- queued writes do not replay across session or identity boundaries
- no dead replay header remains that the server ignores

### Adversarial Tests

- queue a write, logout, and reconnect
- queue a write, switch account, and reconnect
- queue a write, then retry after gateway-side revocation

### Production Evidence

- browser-level test with real durable queue storage
- explicit logout and account-switch invalidation evidence
- no stale replay header remains without server enforcement

## W7. Compatibility Contract

### Goal

Prevent mixed-version clients from using semantics they no longer understand.

### Tasks

- add a connect-time compatibility response from the gateway
- advertise supported ranges for dashboard, desktop, and CLI
- block policy-A writes on unsupported combinations
- test N-2, N-1, and current mixes for critical routes

### File Targets

- gateway bootstrap and health or compatibility endpoints
- SDK and desktop bootstrap logic

### Done When

- unsupported client and gateway pairs fail closed
- rollback scenarios do not silently reuse incompatible proposal semantics

### Adversarial Tests

- new gateway plus old dashboard approval flow
- downgraded desktop with migrated local queue

### Production Evidence

- N-2, N-1, and current compatibility results recorded
- unsupported pairs fail closed in automated tests

## Ordered Execution Plan

Execute in this order. Do not reorder unless a concrete dependency forces it.

1. W1 operation envelope foundation
2. W2 gateway journal and replay enforcement
3. W3 proposal v2 schema and transition engine
4. W4 decision contract migration
5. W5 agent-loop supersession canonicalization
6. W6 durable replay and session epoch binding
7. W7 compatibility contract

## Checkpoints

### Checkpoint A

After W2:

- the system can prove one logical write commits once
- but proposal stale-decision safety is not yet complete

### Checkpoint B

After W4:

- the system can reject stale human decisions with explicit preconditions
- but restart-safe supersession is not yet complete until W5 is done

### Checkpoint C

After W6:

- replay safety is real across transport and local queue paths

## Stop Conditions

Stop and escalate if any of these happen:

- stable subject identity cannot be defined without a broader goal-entity design
- legacy client behavior cannot support safe compatibility shims for approvals
- migration needs destructive rewriting of historical data to appear consistent
- dashboard or desktop auth model cannot provide a durable session epoch

## Final Verification

Do not mark complete until all are true:

- unit tests cover header generation, fingerprinting, and transition validation
- integration tests cover retry, stale approval, supersession, restart, and timeout races
- adversarial tests cover replay, account switch, and mixed-version failure
- no-mock zones in the test doctrine have real-boundary evidence
- crash-recovery and skew tests executed for touched workstreams
- compatibility behavior is documented
- no critical route still relies only on `resolved_at IS NULL` for human decision safety

## Deliverable

An agent implementing this file should produce:

- code changes for the ordered workstreams
- migrations with rollback notes
- new and updated tests
- documentation updates where contracts changed
- a short risk memo listing any deferred legacy behavior
