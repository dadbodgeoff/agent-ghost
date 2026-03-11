# Sessions Remediation Design

## Decision

The ADE `Sessions` surface becomes the authoritative runtime-session control plane for the product.

`/sessions` is not a disposable list view. It is the canonical operator entry point for:

- runtime session discovery
- session lineage
- replay and branching
- session integrity state
- cross-linking from Agents and Observability

Studio chat sessions remain a separate product concept unless explicitly bridged. The runtime Sessions subsystem must not guess or blur that boundary.

## End State

The final subsystem has these properties:

- one canonical runtime session contract from gateway to SDK to dashboard
- one canonical frontend session normalization path
- one session list response shape
- one cursor strategy with deterministic tie-breaking
- one mutation model for bookmarks and branching
- one cross-surface rule for how an agent page or observability page refers to a session
- one validation gate proving list, detail, replay, bookmark, and branch behavior end to end

## Hard Rules

### 1. Runtime sessions are a real domain object

The subsystem may continue deriving session summaries from `itp_events`, but the public contract must treat a runtime session as a first-class entity with stable semantics.

### 2. The public list contract is cursor-first

The ADE must never present a full session table while only loading the backend default page.

### 3. Session cursors must be deterministic

Ordering by `last_event_at` alone is insufficient. Cursor progression must use a stable tie-breaker:

- `last_event_at DESC`
- `session_id DESC`

The cursor itself should be opaque and encode both fields.

### 4. Sequence semantics must be explicit

Replay and branching must not rely on UI array index. The public checkpoint identifier is `sequence_number`.

### 5. Mutations are authoritative only after commit

Bookmark creation, bookmark deletion, and session branching must update UI state from server-confirmed responses, not from wishful local mutations.

### 6. Cross-surface session references must be true

Agents, Observability, Sessions, and Replay must all consume the same normalized session model and must never infer agent ownership by position, truncation, or malformed fallback data.

### 7. Audit lineage is part of correctness

Bookmark and branch mutations must record the real owning session in audit records. A successful mutation without valid provenance is an incomplete implementation.

## Current Defects This Design Removes

- silent list truncation at 50 sessions
- split frontend wiring between route-local fetches and a mostly-unused shared store
- bookmark deletion without session ownership enforcement
- branch success on zero copied events
- bookmark and delete UX that lies on persistence failure
- agent detail showing unrelated global sessions
- inconsistent refresh behavior between Sessions and other websocket-resync-aware surfaces
- index-based replay semantics masquerading as sequence-based semantics

## Target Architecture

### Backend

The gateway owns:

- `GET /api/sessions`
- `GET /api/sessions/:id`
- `GET /api/sessions/:id/events`
- `GET /api/sessions/:id/bookmarks`
- `POST /api/sessions/:id/bookmarks`
- `DELETE /api/sessions/:id/bookmarks/:bookmark_id`
- `POST /api/sessions/:id/branch`

The gateway must also emit a session change signal over websocket or, at minimum, guarantee that all affected consumers refresh on `Resync`.

### SDK

`packages/sdk` is a thin typed wrapper over generated types.

It may normalize transport details, but it may not invent alternate request or response shapes for runtime sessions.

### Frontend

The dashboard owns one session store for:

- list pages
- list pagination state
- list invalidation
- detail cache invalidation
- websocket resync refresh

The route components render store state. They do not each invent their own fetch, normalization, retry, and error semantics.

### Cross-Surface Integration

- Agents page shows only sessions that actually include that agent.
- Observability session picker uses the same list contract and normalization path as `/sessions`.
- Replay is a detail-mode view of the same runtime session identity, not a side system.

## Required New Capabilities

### Canonical summary endpoint

Add `GET /api/sessions/:id` so detail and replay views can load session metadata without pretending that the events response is the only summary source.

### Canonical session normalization

`agents` becomes `agent_ids: string[]` in the public contract. The backend must stop leaking comma-separated storage formatting into the API.

### Replay-safe checkpoint model

Branching and bookmarks move from array index semantics to `sequence_number` semantics.

### Deterministic refresh path

The Sessions list must refresh via the shared store on:

- first mount
- explicit retry
- websocket resync
- local bookmark or branch mutation that changes visible session state

## Non-Goals

- redesigning Studio into the Sessions surface
- changing runtime session storage away from `itp_events`
- building a separate search engine for sessions
- speculative UI polish before the contracts and invariants are corrected
