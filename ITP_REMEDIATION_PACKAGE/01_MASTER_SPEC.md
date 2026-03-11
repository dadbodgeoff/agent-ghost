# ITP Events Master Spec

Status: March 11, 2026

Purpose: define the authoritative remediation and build specification for the ADE `ITP Events` capability across extension capture, gateway ingest, persistence, websocket fan-out, dashboard exploration, and session drilldown.

This document is based on the live codebase. If this spec conflicts with older architecture notes or loose TODOs, this spec wins.

## Standard

This work is held to the following non-negotiable bar:

- No public `ITP` contract without one explicit canonical schema.
- No field name, status label, or metric name may imply semantics the system does not actually compute.
- No ADE route may claim live behavior if it is only polling or one-shot snapshot loading.
- No critical-path event producer may exist in two divergent implementations.
- No event shown in the global ITP view may be a dead end; every row must drill into a durable session-oriented truth surface.
- No implementation is done until reconnect, resync, and stale-data behavior are explicitly designed and tested.

## Scope

This spec covers:

- browser extension event capture relevant to ITP
- gateway ingest and normalization of ITP events
- durable `itp_events` persistence semantics
- websocket live event delivery for ITP-related activity
- dashboard `ITP Events` route
- integration between the global ITP route and existing session detail/replay surfaces
- SDK contract shape for REST and websocket consumption
- verification, parity gates, and release readiness requirements

This spec does not cover:

- redesign of the broader convergence model
- replacement of the existing session replay system
- extension support for new third-party chat platforms beyond the currently supported set

## Primary Sources

- `dashboard/src/routes/itp/+page.svelte`
- `dashboard/src/routes/sessions/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`
- `dashboard/src/routes/sessions/[id]/replay/+page.svelte`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/lib/stores/studioChat.svelte.ts`
- `packages/sdk/src/itp.ts`
- `packages/sdk/src/websocket.ts`
- `crates/ghost-gateway/src/api/itp.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/cortex/cortex-storage/src/migrations/v017_convergence_tables.rs`
- `crates/cortex/cortex-storage/src/queries/itp_event_queries.rs`
- `extension/src/content/observer.ts`
- `extension/src/content/observer.js`
- `extension/src/background/service-worker.ts`
- `extension/src/background/service-worker.js`
- `extension/src/background/itp-emitter.ts`
- `extension/src/background/itp-emitter.js`

## Confirmed Current-State Findings

### F1. The current `ITP Events` route is a snapshot log, not a live ADE surface.

The route performs one fetch on mount, exposes a manual refresh button, and does not subscribe to websocket updates or resync handling.

Implication:

- operators can stare at stale data while believing the page reflects current state
- the route is inconsistent with other live ADE surfaces
- “cohesive ADE” is not achieved

### F2. `/api/itp/events` currently returns misleading field semantics.

The route returns:

- `buffer_count` derived from total persisted rows
- `extension_connected` derived from monitor health
- `platform` hardcoded as `gateway`
- `source` inferred from `sender`

Implication:

- the API contract is semantically false
- the dashboard labels are therefore false
- downstream SDK and tests encode misinformation

### F3. The ADE already has stronger session-oriented event surfaces than the ITP route.

Session detail and replay already provide:

- ordered event retrieval
- chain validation
- attributes
- bookmarks
- branching
- event-level drilldown context

Implication:

- the global ITP route is currently weaker than adjacent ADE capabilities
- the problem is not missing components, it is missing integration and contract discipline

### F4. The extension-side ITP pipeline is fragmented.

The repository contains parallel TS and JS implementations with divergent:

- message types
- field naming conventions
- privacy processing
- native host naming
- local IndexedDB names

Implication:

- there is no single trustworthy extension event contract
- future fixes can land in one path and miss the other
- build output can drift from intended runtime behavior

### F5. The system lacks one canonical end-to-end ITP flow.

Different parts of the repository currently imply different “truths”:

- extension emits platform observations or local fallback events
- gateway persists runtime session events to `itp_events`
- dashboard consumes a reduced snapshot shape
- websocket already supports related `SessionEvent` traffic but the ITP route ignores it

Implication:

- “ITP” is a name shared by multiple partially disconnected realities
- the first requirement is contract unification, not page polish

## Target Outcome

The ADE must expose one coherent ITP capability with the following operator experience:

1. A real event occurs from a supported producer.
2. The event is normalized into one canonical ITP schema.
3. The event is durably persisted with truthful metadata.
4. A live event notification reaches subscribed clients with replay-safe semantics.
5. The global `ITP Events` route updates live and can recover from reconnect gaps.
6. Each event row links to its owning session and the user can inspect full durable context.
7. The REST, SDK, websocket, dashboard, and tests all agree on the same contract.

## Architectural Decision

### Canonical Model

The canonical source of truth is the durable gateway-side `itp_events` persistence model plus explicitly versioned transport contracts derived from it.

The browser extension is a producer, not a source of truth.

The dashboard route is a projection, not a source of truth.

The websocket stream is a live transport, not a source of truth.

### Required Layers

The final architecture must have these layers:

1. Producer layer
   - browser extension
   - gateway runtime session persistence path
   - any additional internal producer explicitly accepted into the contract
2. Normalization layer
   - one gateway-owned canonical shape
   - one field mapping policy
   - one ownership boundary for semantics
3. Persistence layer
   - durable `itp_events` rows
   - append-only semantics
   - sequence-safe and chain-safe behavior
4. Live transport layer
   - websocket event that represents new durable ITP activity
   - replay/resync behavior defined
5. Read-model layer
   - list/query endpoints for dashboard use
   - drilldown endpoints delegated to existing session APIs where appropriate
6. UX layer
   - live route
   - filters
   - session linkage
   - state/status that maps to real backend facts

## Invariants

The final implementation must satisfy all of these invariants:

- There is exactly one canonical transport schema for ITP list rows.
- Every surfaced field has a written semantic definition and one owner.
- `platform`, `source`, `sender`, `monitor_connected`, `extension_connected`, `persisted_count`, and any backlog metric are distinct concepts.
- No status field may be inferred from an unrelated subsystem health bit.
- Any “live” ITP view must subscribe to live transport and handle `Resync`.
- The global ITP route must link every row to a durable session-oriented detail path.
- The extension must have exactly one maintained implementation path for event capture and forwarding.
- Local extension fallback buffering must have explicit flush behavior or must be treated as local-only and never implied as gateway truth.
- REST OpenAPI, generated SDK types, hand-written wrappers, dashboard consumers, and tests must match.

## Public Contract Requirements

The REST list endpoint must, at minimum, support:

- explicit pagination or cursor semantics
- explicit filtering by session, event type, source, and time range
- truthful counters with precise names
- enough row identity to navigate into session detail

The websocket contract must, at minimum, support:

- notification of new durable ITP activity
- session linkage
- sequence or replay-safe refresh behavior
- clear client action on reconnect gaps

The dashboard route must, at minimum, support:

- initial load
- live updates
- reconnect/resync recovery
- filterable list exploration
- link to session detail
- empty, degraded, and stale states that are truthful

## UX Requirements

The global `ITP Events` route is not a terminal log. It is an event explorer.

The route must answer these operator questions:

- What changed most recently?
- Is the page live or stale?
- Which subsystem produced this event?
- Which session owns this event?
- Can I see the durable full detail?
- Is the event extension-originated, runtime-originated, or other?
- Is there any ingest or buffering problem right now?

The route must not answer with:

- fake platform values
- fake connectivity values
- fake backlog counts
- disconnected rows with no drilldown path

## Ownership Model

The following ownership is required:

- Gateway API owner
  - owns semantics of public ITP REST and websocket contracts
- Dashboard owner
  - owns rendering, filtering, live refresh behavior, and session drilldown UX
- Extension owner
  - owns only event production and local buffering semantics
- SDK owner
  - owns parity of generated and wrapped client contracts

No one else is allowed to redefine field meaning by convenience.

## Deliverables

A correct implementation of this spec produces:

- one canonical ITP event contract
- one corrected read API
- one live dashboard route
- one unified extension producer path
- one integration story with existing session detail/replay surfaces
- one verification stack that prevents semantic drift from returning
