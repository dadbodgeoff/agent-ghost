# Remaining Hardening Phase Design

## Document Intent

This document defines the next hardening phase after the first contract and
runtime stabilization slice.

This phase is not feature work. It exists to make the project safe to operate
as a public OSS system with:

- truthful contracts
- explicit ownership boundaries
- low silent-failure risk
- auditable release gates
- contributor-safe extension points

The central problem is no longer only broken behavior. It is governance drift:
multiple things still look canonical when they are not.

## Executive Goal

Reach a state where an external user or contributor can rely on the following
without reading the whole codebase:

1. The gateway is the only authority on API and event truth.
2. The SDK is the only stable client contract for gateway consumers.
3. The runtime adapter is the only dashboard entrypoint for desktop behavior.
4. Offline and cache behavior cannot silently violate session or auth
   expectations.
5. CI fails immediately when any of those statements stop being true.

## What This Phase Must Close

The first hardening slice addressed:

- false auth probing
- local-only logout
- websocket auth/envelope drift
- some desktop runtime bypasses
- the fake live ITP page
- stale SDK tests

This phase still needs to close:

- stale or partial OpenAPI and generated-type truth
- fake domain contracts, especially approvals
- incomplete runtime ownership for desktop behavior
- incomplete websocket regression protection
- service worker auth/cache safety guarantees
- missing contract governance and CI drift gates

## Why This Work Is Justified

For an OSS project, false contracts are more dangerous than missing features.

External users and contributors will trust:

- exported types
- OpenAPI
- route behavior implied by UI
- desktop affordances that appear available
- cache behavior that appears safe

If those surfaces are only partially real, the project will accumulate:

- hard-to-reproduce regressions
- broken community integrations
- low-trust bug reports
- accidental reintroduction of removed drift

This phase is therefore correctness work, not process theater.

## Scope

### In Scope

- API contract truthfulness
- schema/router parity
- SDK export truthfulness
- approvals contract replacement or demotion
- runtime boundary completion
- websocket contract regression coverage
- service worker auth/session safety
- contract ownership documentation
- CI gates that prevent drift from re-entering

### Out of Scope

- net-new product features
- large UI redesign
- unrelated performance tuning
- broad codebase cleanup outside these boundaries
- replacing Tauri, Svelte, Axum, or the SDK shape wholesale

## Non-Negotiable Invariants

These are release-blocking.

### Invariant 1: No False Source of Truth

No file, export, schema, or generated artifact may be presented as canonical if
it does not represent real gateway behavior.

### Invariant 2: No Duplicate Ownership

For each boundary there must be one owner:

- gateway for API/event semantics
- SDK for client transport and typed client surface
- runtime adapter for desktop behavior
- dashboard for presentation and local view state only

### Invariant 3: No Hidden Bypasses

A supported path must be obvious from the architecture. No dashboard route,
store, or component may bypass the intended SDK/runtime path silently.

### Invariant 4: No Silent Auth Drift

Auth, session, logout, refresh, and cache behavior must distinguish:

- authentication failure
- service failure
- network failure
- stale cached data

### Invariant 5: Every Public Type Must Be Semantically Honest

A type-safe surface that encodes fake semantics is considered broken, even if
it compiles.

### Invariant 6: Drift Must Fail in CI

Any new router/spec mismatch, runtime bypass, unknown websocket event consumer,
or fake public contract must be detectable automatically.

## Boundary Model

### Gateway Owns

- mounted REST routes
- websocket event enum and envelope
- auth/session semantics
- DTO truth
- OpenAPI or any exported schema

### SDK Owns

- HTTP request construction
- websocket connection/auth/replay behavior
- error normalization
- public client DTO exposure
- generated types only if generated source is truthful

### Runtime Adapter Owns

- desktop auth persistence
- gateway lifecycle control
- desktop capability access
- notifications
- keybindings
- PTY capability
- shell/default environment resolution

### Dashboard Owns

- pages
- local interaction state
- rendering
- view-only derivations that do not invent domain semantics

## Required Audit Method

This phase will not proceed by opportunistic edits. It requires a structured
audit pass first.

### Step 1: Build a Boundary Inventory

For every relevant surface, record:

- name
- owner
- transport
- auth mode
- source-of-truth file
- consumer files
- exported type location
- tests covering it
- CI gate covering it
- current status: `canonical`, `transitional`, `drifted`, or `dead`

Boundaries to inventory:

- gateway REST routes
- gateway websocket events
- SDK public modules
- dashboard route/store/component consumers
- runtime adapter methods
- service worker cached API behavior

### Step 2: Build a Source-of-Truth Matrix

For each boundary answer:

1. What is the canonical contract?
2. Who is allowed to call it directly?
3. What invariants must always hold?
4. What test or CI job fails if it drifts?

### Step 3: Classify Every Remaining Gap

Each gap must be categorized as one of:

- `fake-contract`
- `duplicate-contract`
- `runtime-bypass`
- `dead-transitional-code`
- `missing-invariant`
- `missing-test`
- `missing-ci-gate`

### Step 4: Produce Evidence

No audit statement is accepted without:

- implementation file
- consumer file
- impact statement
- recommended owner
- proposed release classification

## Planning Artifacts Required

This phase should generate and maintain the following supporting artifacts.

### `contracts.md`

One section per boundary with:

- contract name
- owner
- request/response/event/command schema
- auth semantics
- allowed consumers
- prohibited bypasses
- test coverage
- migration status

### `drift-register.md`

A structured ledger with columns:

- id
- category
- severity
- source owner
- violating files
- user/runtime impact
- fix strategy
- blocking status
- required tests

### `release-gates.md`

Explicit release conditions with:

- gate name
- rationale
- owner
- automated check or manual check
- failure action

### `execution-plan.md`

Sequenced implementation plan grouped by contract area, not by file.

## Workstreams

## Workstream A: Contract Truth Audit and Freeze

### Goal

Freeze an accurate inventory of all remaining contract surfaces before further
implementation.

### Deliverables

- boundary inventory
- source-of-truth matrix
- drift register
- route/event/consumer maps

### Required Checks

- router-to-schema map
- websocket event-to-consumer map
- runtime method-to-consumer map
- service worker cacheable endpoint map

### Exit Criteria

- every dashboard boundary touchpoint is accounted for
- every remaining drift item is named and classified
- no unknown public contract remains in use

## Workstream B: OpenAPI and Generated-Type Truthfulness

### Problem

The router and the exported generated schema/types are not guaranteed to match.
That makes docs and type exports unsafe.

### Goal

Either:

- make the exported schema truthful and route-complete, or
- stop presenting generated types as canonical until truthfulness exists

### Tasks

- enumerate mounted routes from `build_router`
- compare them against the exported schema
- classify uncovered routes:
  - dashboard-used and must be modeled now
  - internal-only and can remain intentionally omitted if explicitly marked
- decide whether the generated SDK export remains enabled
- add parity checks to CI

### Required Decisions

- route-complete OpenAPI now vs temporary export demotion
- dashboard-used route minimum coverage threshold
- treatment of internal/private routes

### Failure Modes To Block

- route exists, spec missing
- spec exists, route missing
- SDK exported type compiles for a route the gateway does not actually honor

### Definition of Done

- schema truth policy is documented
- generated export policy is documented
- CI fails on router/schema drift

## Workstream C: Approvals Contract Replacement

### Problem

`ApprovalsAPI` currently looks like a first-class domain surface while deriving
semantics heuristically from goals.

### Goal

End the fake approvals contract.

### Options

1. Promote approvals into a real gateway contract with first-class DTOs and
   endpoints.
2. Demote the UI and SDK surface to explicit goal/proposal semantics until a
   real approvals model exists.

### Tasks

- define desired approval semantics
- compare current dashboard behavior to actual gateway data
- remove heuristic inference from public client code
- eliminate N+1 detail fetch behavior
- update UI language if the contract is not truly approvals

### Failure Modes To Block

- typed but misleading metadata
- wrong risk/category labels
- `agent_name` populated from `agent_id`
- performance collapse with large queues

### Definition of Done

- public surface matches real semantics
- no heuristic-only domain type remains public without explicit `Compat` status

## Workstream D: Runtime Ownership Completion

### Problem

Desktop behavior is improved but still not fully owned by the runtime boundary.

### Goal

Make the runtime adapter the exclusive dashboard entrypoint for desktop-only
behavior.

### Tasks

- inventory all desktop-only behaviors
- remove remaining direct plugin or desktop package imports from dashboard code
- decide whether PTY is:
  - fully runtime-owned, or
  - a documented exception with tests and rationale
- formalize capability failure semantics for desktop-only features

### Runtime Capabilities To Cover

- auth persistence
- gateway lifecycle
- notifications
- keybindings
- PTY
- shell resolution
- any future desktop configuration reads

### Failure Modes To Block

- silent no-op when a command is missing
- frontend import of desktop plugin in a non-runtime file
- platform-specific assumptions hidden in components

### Definition of Done

- dashboard has no unapproved desktop bypasses
- capability failure paths are explicit and testable

## Workstream E: Websocket Governance and Regression Protection

### Problem

The transport contract is better, but still under-protected against future
drift.

### Goal

Lock the websocket model behind one canonical contract and strong regression
coverage.

### Tasks

- document the canonical event union and envelope
- document supported auth path
- document replay/resync semantics
- ensure every dashboard consumer maps to a real gateway event
- add gateway and SDK integration coverage for:
  - auth
  - envelope parsing
  - replay
  - resync
  - topic filtering

### Failure Modes To Block

- new dashboard consumer of non-existent event
- renamed field silently consumed via `any`
- replay cursor lost during multi-tab leadership changes
- unsupported auth path reintroduced

### Definition of Done

- websocket contract is documented once
- all consumers are mapped
- CI and tests fail on event drift

## Workstream F: Service Worker Auth and Session Safety

### Problem

The service worker is safer than before, but not yet governed by a strict
session-safe policy.

### Goal

Make offline behavior explicit, scoped, and incapable of leaking data or
crossing auth boundaries silently.

### Tasks

- classify every cached API path as:
  - unauthenticated-safe
  - authenticated-but-cacheable
  - never-cache
- decide whether authenticated cache partitioning is needed or whether
  authenticated API caching should be disabled entirely
- define logout, token rotation, and session change invalidation policy
- test offline replay semantics against session sequencing rules

### Failure Modes To Block

- data from user/session A shown after logout or user/session B login
- auth endpoint cached
- stale authenticated API data shown without attribution
- replay queue applies stale session actions without detection

### Definition of Done

- cache policy matrix exists
- auth-sensitive endpoints are protected by policy and tests
- logout/session transitions invalidate relevant cached state

## Workstream G: Test and CI Governance

### Problem

The codebase can still drift back into broken states because most invariants are
social, not enforced.

### Goal

Turn the critical architectural rules into automated gates.

### Required CI Gates

- router/schema parity gate
- SDK test suite
- gateway critical-path integration tests
- runtime check for missing desktop commands
- dashboard guard against forbidden direct desktop imports
- dashboard guard against unknown websocket event subscriptions
- dashboard guard against raw gateway transport outside approved files

### Required Test Additions

- gateway tests for `GET /api/auth/session`
- websocket contract integration tests
- approvals semantic tests once the contract decision is made
- service-worker auth/cache transition tests
- runtime command tests for keybindings/shell resolution

### Definition of Done

- each major invariant has an automated owner
- CI detects drift before release review

## Adversarial Test Matrix

The phase is not complete without failure-oriented tests.

### Contract Truth

- mounted route absent from schema
- schema route absent from router
- SDK export present for unsupported route

### Approvals

- goal payload shape changes unexpectedly
- empty or partial goal content
- large queue reveals N+1 behavior

### Runtime

- desktop command missing
- notification permission denied
- PTY unavailable
- web mode attempts desktop behavior

### Websocket

- invalid subprotocol auth
- revoked token auth
- malformed envelope
- replay inside buffer
- replay outside buffer
- duplicate event delivery
- follower tab promoted after leader close

### Service Worker

- login A, cache data, logout, login B
- token rotation while offline cache exists
- offline replay against stale session sequence
- safety write attempted offline

## Release Gates

The project should not claim broad OSS readiness for this phase until all are
true:

1. The exported schema policy is truthful and enforced.
2. No fake public domain contract remains undocumented or mislabeled.
3. No unapproved direct desktop/runtime bypass remains in dashboard code.
4. Websocket event consumers all map to real gateway events.
5. Auth/session/cache transitions cannot silently leak or reuse stale data.
6. CI fails on the main classes of architectural drift.

## Sequencing

Recommended execution order:

1. Contract truth audit and freeze
2. OpenAPI/generated-type decision and parity work
3. approvals contract replacement or demotion
4. runtime ownership completion
5. websocket regression protection
6. service-worker safety completion
7. CI gate installation
8. re-audit against release gates

This order matters. It is incorrect to add broad tests before deciding what the
canonical contract actually is.

## Definition of Done

This phase is complete only when all of the following are true:

- every remaining boundary has one declared owner
- every public contract is semantically honest
- every bypass is either removed or explicitly documented as an exception
- every critical failure mode has an adversarial test
- every architectural invariant has a CI check or a documented manual release
  gate
- an external contributor could identify the intended path for API, websocket,
  runtime, and cache behavior from the docs without reverse-engineering the
  repo
