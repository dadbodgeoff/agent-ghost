# Integrity Test Doctrine

## Purpose

This document defines the minimum testing and release-evidence standard for the integrity hardening program.

It exists to prevent a common failure mode:

- strong design docs
- plausible implementation
- weak test discipline
- silent production failure anyway

This doctrine is intentionally stricter than a normal unit or integration test plan.

## Core Standard

The goal is not "good coverage."

The goal is:

- silent failure should be hard to introduce
- invariant violations should fail loudly
- retries, crashes, restarts, stale decisions, and version skew should be exercised before release
- critical paths should be proven in realistic environments, not only in mocks

No serious test plan can guarantee all possible flaws are gone. That is not the standard.

The real standard is:

- identify the highest-risk failure classes
- build tests that directly attack them
- block release if those tests are missing or flaky

## Test Doctrine

### 1. Critical paths must be tested at the real boundary

For integrity-critical behavior, tests must cross the real system boundary whenever practical:

- real gateway HTTP stack
- real SQLite database
- real migrations
- real service-worker queueing path
- real desktop runtime boundary where capability or secret access is in scope

Pure mocks are not sufficient evidence for:

- idempotent replay
- proposal transition correctness
- migration safety
- logout and account-switch invalidation
- compatibility handshake behavior

### 2. Happy-path tests are necessary but not sufficient

Every critical flow needs:

- one minimal happy path
- one duplicate or retry path
- one stale or conflicting path
- one crash or restart path
- one compatibility or rollback path if versioning is involved

### 3. Invariants outrank implementation details

Tests should primarily assert invariants, not incidental function behavior.

Examples:

- one logical write commits once
- one proposal has one terminal lifecycle outcome
- stale approval fails
- logout invalidates queued writes
- unsupported version pairs fail closed

### 4. Release gates must be evidence-based

"We believe this is safe" is not a release artifact.

Required release evidence is:

- passing test outputs
- migration rehearsal result
- compatibility matrix result
- static enforcement result
- explicit deferred-risk memo if anything remains transitional

## No-Mock Zones

Mocks are allowed for leaf utilities and narrow failure injection helpers.

Mocks are not allowed as the only evidence for these zones:

### Zone A: Gateway mutation semantics

Must use:

- real axum router
- real middleware chain
- real SQLite database
- real migrations

Not sufficient:

- unit-only handler tests with fake state
- fake in-memory journal standing in for DB dedupe

### Zone B: Proposal transition engine

Must use:

- real storage tables
- real transactions
- concurrent execution where relevant

Not sufficient:

- mock repository tests that bypass uniqueness and transaction rules

### Zone C: Service worker replay and durable queueing

Must use:

- real browser test environment
- real IndexedDB or browser-equivalent storage
- real auth/session invalidation path

Not sufficient:

- pure function tests over queue item JSON

### Zone D: Desktop capability boundary

Must use:

- real Tauri runtime integration tests or equivalent red-team harness

Not sufficient:

- static review of capability JSON alone
- mocked renderer calls that never touch runtime

### Zone E: Compatibility handshake and migration safety

Must use:

- real versioned client/server matrix runs
- real migrated database copies or representative snapshots

Not sufficient:

- mocked version strings
- schema-only diff review without runtime behavior

## Required Test Layers

## Layer 1: Static Enforcement

These are fast blockers that fail before expensive test execution.

Required checks:

- no `TODO`, `FIXME`, `stub`, `unimplemented!`, `todo!`, or placeholder compatibility comments in touched critical-path files
- no new direct mutable `UPDATE goal_proposals SET decision` style writes after v2 transition work begins
- no policy-A write route added without operation identity coverage
- no dashboard approval action that sends only proposal ID after decision migration
- no new raw renderer access to forbidden Tauri plugins

Recommended enforcement:

- `rg`-based scripts for forbidden patterns
- lint or CI assertions for touched-file rules

## Layer 2: Unit And Property Tests

Use for:

- ID generation and request envelope helpers
- fingerprint canonicalization
- transition validation rules
- route policy classification
- subject identity derivation logic

Property tests are required for:

- request fingerprint stability under JSON key reordering
- transition engine terminal-state uniqueness logic
- legacy compatibility mapping from old decision values to new lifecycle states

## Layer 3: Integration Tests

Use for:

- real gateway requests through middleware to SQLite
- idempotent replay semantics
- proposal state machine correctness
- audit and provenance persistence

Required scenarios:

- same write retried after timeout
- duplicate request while first execution still in progress
- stale approval with wrong reviewed revision
- approve vs reject race
- superseding proposal while approval is pending

## Layer 4: Fault Injection And Crash Tests

These are production-blocking for this program.

Required scenarios:

- crash after durable commit but before HTTP response
- crash during proposal transition transaction
- crash during queue replay
- restart after logout with stale queued work on disk
- restart after migration rollback scenario

Required mechanism:

- deterministic failpoints or kill hooks where practical
- process kill and restart harness where failpoints are not available

If crash tests are skipped, release is blocked.

## Layer 5: Adversarial And Concurrency Tests

Required scenarios:

- duplicate write storm with same idempotency key
- key reuse with payload mismatch
- stale replay after supersession
- approval against a lineage head that changed after UI read
- account switch before queued replay
- old client hitting a new gateway decision route

Concurrency tests must attempt:

- parallel approve and reject
- parallel approve and supersede
- parallel timeout and approve
- multiple concurrent proposals for the same subject lineage

## Layer 6: Cross-Version Matrix

This is production-blocking for decision-path changes.

Minimum matrix:

- N-2 client vs current gateway
- N-1 client vs current gateway
- current client vs current gateway
- current client vs N-1 gateway if rollback is supported

Required assertions:

- unsupported combinations fail closed
- supported combinations preserve approval semantics
- migrated local state does not replay under unsupported semantics

## Required Fault Classes

The following failure classes must each have at least one explicit named test:

- duplicate transport retry
- retry-after-commit
- stale human decision
- lost in-memory supersession after restart
- queue replay after logout
- queue replay after account switch
- mixed-version semantic drift
- migration backfill ambiguity
- idempotency-key reuse mismatch
- terminal-state race on one proposal

If any class has no explicit test, release is blocked.

## CI Blocking Matrix

Minimum CI groups:

### Group A: Static gates

- forbidden-pattern scan
- contract drift scan
- OpenAPI and router parity checks if touched

### Group B: Fast semantic tests

- unit tests
- property tests
- focused integration tests for request envelope and transition validation

### Group C: Real-boundary integration

- gateway plus SQLite replay tests
- proposal transition tests
- audit/provenance tests

### Group D: Browser and desktop critical-path tests

- service worker replay invalidation tests
- desktop runtime boundary tests if desktop code changed

### Group E: Crash and skew tests

- crash recovery harness
- version skew matrix

CI may parallelize these groups, but release must block on all groups relevant to touched workstreams.

## Required Naming And Traceability

Every critical test should make the attacked failure mode obvious from its name.

Examples:

- `retry_after_commit_replays_original_response`
- `stale_approval_after_supersession_is_rejected`
- `logout_invalidates_queued_write_before_replay`
- `n_minus_1_client_fails_closed_on_missing_decision_contract`

Each workstream in the build task file should map to:

- implementation tasks
- named tests
- release evidence

## Production Evidence Checklist

Release owner must have all of:

- test run output for all required groups
- migration rehearsal notes
- version matrix result
- explicit list of any legacy compatibility paths still enabled
- explicit list of any inferred legacy data behavior
- confirmation that no critical-path `TODO` or stub remains

No checklist item, no release.

## Red Flags That Block Release

- tests rely mainly on mocks for gateway or transition correctness
- crash recovery is described but not executed
- service-worker replay is claimed safe without auth or identity invalidation tests
- proposal lineage is called canonical but still depends on `goal_text`
- compatibility support is described but not proven by matrix tests
- conflict responses are still generic enough that operators cannot tell stale from duplicate from mismatch

## Bottom Line

For this hardening program, the quality bar is not:

- feature works
- unit tests pass

The quality bar is:

- the system has been attacked in the failure modes most likely to cause silent corruption
- those failures are either prevented or made loud
- release is blocked without real evidence
