# SDK-First Desktop Design

## Summary

This document proposes a desktop-first architecture for GHOST ADE with two hard rules:

1. `packages/sdk` is the only client contract used by the UI.
2. The Tauri app is the source of truth for runtime, auth, config, and gateway lifecycle.

Today, the desktop app mostly launches the gateway and injects a port into the webview, while the dashboard still owns API access, token hydration, and parts of app boot. That splits responsibility across Rust, Svelte, and ad hoc fetch helpers. The result is drift, duplicate logic, and an unclear ownership model.

The target state is:

- Tauri owns process/runtime concerns.
- `packages/sdk` owns HTTP/WebSocket contracts.
- The dashboard owns presentation and local view state only.

## Why Change

The current shape has three problems:

### 1. The dashboard bypasses the SDK

The dashboard talks to the gateway through `dashboard/src/lib/api.ts` and raw `fetch`, even though `packages/sdk` already provides a typed `GhostClient` and generated OpenAPI types.

Current examples:

- `dashboard/src/lib/api.ts`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/routes/orchestration/+page.svelte`
- `packages/sdk/src/client.ts`
- `packages/sdk/src/generated-types.ts`

### 2. Tauri is not the authority for desktop runtime state

The Tauri layer starts the sidecar and resolves the port, but auth and request behavior still depend on web-layer bootstrapping and `sessionStorage`.

Current examples:

- `src-tauri/src/commands/gateway.rs`
- `src-tauri/src/lib.rs`
- `dashboard/src/lib/auth.ts`
- `dashboard/src/routes/+layout.svelte`

### 3. Desktop behavior is implemented as web behavior with Tauri exceptions

The current app treats the dashboard as primary and Tauri as a special-case host. For a desktop-first ADE, that is backward. The desktop container should define runtime truth, and the dashboard should consume that truth through a narrow platform interface.

## Goals

- Make `packages/sdk` the single source of truth for gateway API access.
- Make Tauri the single source of truth for gateway endpoint, auth token, config path, and gateway lifecycle.
- Remove direct dashboard ownership of auth bootstrapping and gateway URL discovery.
- Keep the dashboard focused on rendering, interaction, and local screen state.
- Preserve a browser-compatible path where possible, but optimize for Tauri first.

## Non-Goals

- Rewriting the gateway API surface.
- Replacing Svelte/Tauri with a different frontend stack.
- Forcing all business logic into Rust.
- Removing browser development mode on day one.

## Engineering Bar

This plan is only acceptable if it is enforceable in code review and verifiable in CI.

That means:

- every phase has an explicit exit condition
- architectural rules are stated as invariants, not preferences
- migration can proceed without a flag day rewrite
- desktop and web paths remain intentional rather than accidentally divergent
- rollback is possible at each cutover point

## Architectural Invariants

These are not suggestions. They are rules the repo should converge to.

### Invariant 1: one transport stack

- gateway HTTP and WebSocket access flows through `packages/sdk`
- dashboard code does not implement its own gateway transport rules

### Invariant 2: one desktop runtime authority

- desktop gateway lifecycle, endpoint resolution, and persisted auth are owned by Tauri
- dashboard code does not infer desktop runtime state from globals or storage hacks

### Invariant 3: one contract definition

- generated OpenAPI types plus SDK domain types define the client contract
- route-local copies of gateway contracts are temporary migration debt and should be removed

### Invariant 4: presentation stays in the UI

- Rust owns runtime and capability boundaries
- Svelte owns rendering and transient interaction state

### Invariant 5: migration must be incremental

- each phase must leave `main` releasable
- no phase should require a long-lived fork of the UI architecture

## Current State

### Runtime ownership

- Tauri resolves `ghost.yml`, launches the `ghost` sidecar, waits for health, and injects `window.__GHOST_GATEWAY_PORT__`.
- The dashboard computes `BASE_URL` from local storage, injected window state, or Vite env.
- The dashboard performs auth hydration on mount.

This means runtime truth is split between:

- Rust process management in `src-tauri`
- ad hoc JS boot logic in `dashboard/src/routes/+layout.svelte`
- ad hoc request behavior in `dashboard/src/lib/api.ts`

### Auth ownership

- Tauri store persists the token.
- `sessionStorage` is also used as a synchronous mirror because `api.ts` is synchronous.
- The layout hydrates `sessionStorage` from Tauri on mount.

This is a workaround, not a clean contract.

### API contract ownership

- `packages/sdk` already defines `GhostClient` and API modules.
- `packages/sdk/src/generated-types.ts` exists.
- Dashboard routes still define local interfaces and call raw endpoints directly.

That creates two contract systems:

- the SDK contract
- the dashboard-local contract

The dashboard-local contract must be removed.

## Target Architecture

## Decision 1: `packages/sdk` is the only gateway client

All dashboard data access must go through `packages/sdk`.

Rules:

- No direct `fetch` for gateway API calls in dashboard routes or stores.
- No route-local copies of gateway response types when an SDK type exists or can be generated.
- WebSocket access also flows through the SDK.

The dashboard may still use `fetch` for non-gateway browser APIs, but not for GHOST backend calls.

### Required additions to the SDK

The SDK should expose:

- typed auth-aware request creation
- token update hooks
- base URL injection
- WebSocket creation
- shared error normalization
- route-ready typed APIs for all dashboard-used endpoints

If the current SDK is missing endpoints used by the dashboard, the SDK should be expanded first. The dashboard should not work around missing coverage with raw calls.

## Decision 2: Tauri is the source of truth for desktop runtime

For desktop builds, the dashboard must not infer runtime state on its own.

Tauri owns:

- gateway process lifecycle
- gateway health status
- resolved gateway URL/port
- persisted auth token
- config path and desktop environment state
- desktop-only capabilities such as notifications, filesystem, PTY, and shell

The dashboard receives this through a platform bridge instead of deriving it itself.

### Platform bridge

Introduce a narrow platform layer, for example:

- `dashboard/src/lib/platform/runtime.ts`
- `dashboard/src/lib/platform/tauri.ts`
- `dashboard/src/lib/platform/web.ts`

Responsibilities of the platform layer:

- return resolved base URL
- return current auth token
- subscribe to token changes
- expose desktop capability checks
- expose gateway status and lifecycle commands

The UI should depend on this platform interface, not on `window.__TAURI__`, `sessionStorage`, or injected globals.

## Source-of-Truth Model

This is the ownership model the codebase should enforce.

### Desktop runtime truth

Owned by Tauri Rust:

- `src-tauri/src/lib.rs`
- `src-tauri/src/commands/gateway.rs`

### API contract truth

Owned by SDK:

- `packages/sdk/src/client.ts`
- `packages/sdk/src/generated-types.ts`
- the per-domain SDK modules

### UI rendering truth

Owned by the dashboard:

- routes
- components
- Svelte stores

The dashboard should not own runtime truth or API contract truth.

## Proposed Module Shape

### 1. Replace `dashboard/src/lib/api.ts`

Deprecate the ad hoc REST helper and replace it with an SDK-backed app client wrapper.

Suggested shape:

- `dashboard/src/lib/ghost-client.ts`
- `dashboard/src/lib/platform/runtime.ts`

`ghost-client.ts` should:

- ask the platform layer for base URL and token
- construct `GhostClient`
- expose app-level helpers for stores and routes

This wrapper may adapt the SDK to Svelte usage, but it must not recreate transport logic already present in the SDK.

### 2. Move auth ownership into platform runtime

Current state:

- `dashboard/src/lib/auth.ts` mirrors token into `sessionStorage`
- `dashboard/src/routes/+layout.svelte` hydrates before use

Target state:

- Tauri store is the desktop token source
- web fallback uses a web platform token store
- SDK client reads token through a platform adapter
- routes never manually hydrate auth state

### 3. Make boot explicit

Create a startup flow that resolves runtime before rendering the authenticated shell.

Suggested boot stages:

1. resolve platform
2. resolve gateway base URL
3. resolve auth state
4. construct `GhostClient`
5. connect websocket/session stores
6. render app shell

This replaces the current mixed boot flow in `+layout.svelte`.

### 4. Move stores onto SDK-backed domain services

For each domain:

- agents
- sessions
- convergence
- safety
- goals
- skills
- studio

Use a store or service that depends on `GhostClient`, not on raw endpoint strings.

This allows:

- one error model
- one auth model
- one transport model
- simpler route files

### 5. Treat the dashboard as a desktop UI package

The dashboard should become a view layer that can run in browser dev mode, but its default mental model should be "desktop client for a local gateway" rather than "web app with Tauri exceptions."

## Migration Plan

## Phase 0: Freeze new raw API usage

Before migration:

- stop adding new dashboard calls through `dashboard/src/lib/api.ts`
- stop adding new route-local gateway response interfaces

Acceptance criteria:

- new frontend work must go through the SDK path only

## Phase 1: Build platform runtime abstraction

Add:

- `dashboard/src/lib/platform/runtime.ts`
- `dashboard/src/lib/platform/tauri.ts`
- `dashboard/src/lib/platform/web.ts`

Responsibilities:

- base URL resolution
- token read/write/clear
- desktop capability flags
- gateway status/start/stop hooks

Acceptance criteria:

- no route or component reads `window.__TAURI__` directly
- no route or component reads `window.__GHOST_GATEWAY_PORT__` directly
- no route or component reads/writes `sessionStorage` for auth directly

## Phase 2: Introduce SDK-backed app client

Add:

- `dashboard/src/lib/ghost-client.ts`

Responsibilities:

- create configured `GhostClient`
- expose shared singleton or injected client for stores
- centralize SDK error handling

Acceptance criteria:

- `dashboard/src/lib/api.ts` becomes unused and removable
- auth redirect logic moves out of the transport helper and into app/session flow

## Phase 3: Migrate stores first

Migrate `dashboard/src/lib/stores/*` to use the SDK-backed client.

Reason:

- stores are lower-churn and easier to validate than large route files
- once stores are migrated, routes can slim down quickly

Acceptance criteria:

- core stores no longer import raw endpoint strings

## Phase 4: Migrate largest routes

Migrate the highest-risk routes first:

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/routes/studio/+page.svelte`
- `dashboard/src/routes/orchestration/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/routes/security/+page.svelte`

Acceptance criteria:

- route files mostly compose stores/components
- route-local transport code is removed
- route-local API type definitions are removed where SDK types exist

## Phase 5: Expand SDK coverage to full dashboard surface

Add missing SDK modules/endpoints for:

- marketplace
- webhooks
- profiles
- provider keys
- admin backups
- traces
- mesh/orchestration
- studio-specific flows

Acceptance criteria:

- every dashboard gateway call is representable through the SDK

## Phase 6: Tighten Tauri ownership

After the dashboard is SDK-first:

- move any remaining desktop-only runtime logic out of Svelte boot files
- expose explicit Tauri commands/events for gateway lifecycle and runtime state
- optionally replace injected globals with a formal command-based runtime bootstrap

Acceptance criteria:

- desktop runtime can be reasoned about from `src-tauri` plus the platform adapter
- the dashboard contains no Tauri-specific boot hacks

## Work Packages

The migration should be tracked as a set of work packages, not just broad phases.

### WP1: platform runtime contract

Deliverables:

- runtime interface definition
- Tauri implementation
- web implementation
- unit coverage for runtime resolution and token behavior

### WP2: SDK completion

Deliverables:

- inventory of all dashboard-used endpoints
- missing SDK modules added
- typed WebSocket coverage for dashboard event usage
- generated type refresh workflow

### WP3: dashboard shell migration

Deliverables:

- `+layout.svelte` boot split into startup/runtime/session responsibilities
- removal of direct `sessionStorage` auth hydration
- removal of direct injected-global reads from routes/components

### WP4: store migration

Deliverables:

- each store migrated to the SDK-backed client
- route string literals for migrated domains removed
- store tests updated to mock SDK client boundaries

### WP5: route decomposition

Deliverables:

- high-complexity routes slimmed to composition layers
- route-local response interfaces removed
- domain logic moved into stores/services/components

### WP6: final cutover and cleanup

Deliverables:

- `dashboard/src/lib/api.ts` removed
- `dashboard/src/lib/auth.ts` reduced or removed in favor of platform runtime
- dead compatibility shims removed
- migration guardrails added to CI or linting

## Verification Matrix

This plan should be verified with explicit gates.

### Static verification

- `rg` check confirms no dashboard gateway `fetch(` usage outside approved platform/SDK files
- `rg` check confirms no direct reads of `window.__GHOST_GATEWAY_PORT__` outside platform code
- `rg` check confirms no direct auth reads from `sessionStorage` outside platform code

### Type verification

- dashboard typecheck passes against SDK imports
- SDK build passes after endpoint additions
- generated API types are refreshed and committed when contract changes

### Runtime verification

- desktop app boots with sidecar-managed gateway
- browser dev mode still boots with web runtime adapter
- login/logout/token refresh work in both desktop and web paths
- WebSocket subscriptions reconnect correctly after boot migration

### Regression verification

- core dashboard flows work: agents, sessions, studio, safety, orchestration
- Tauri sidecar lifecycle remains healthy across app restart
- no route depends on removed transport helpers

## Cutover Strategy

The cutover should happen in slices, not all at once.

### Cutover rule

- a domain is considered cut over only when its store and route paths both use the SDK-backed client

### Temporary coexistence rule

- temporary coexistence of old and new client paths is allowed only behind clear file boundaries
- mixed transport usage inside the same store or route is not allowed

### Final cutover rule

- remove `dashboard/src/lib/api.ts` only after all dashboard gateway calls have been migrated

## Rollback Strategy

Each phase must be independently reversible.

- platform runtime introduction can roll back to the existing helper without schema changes
- SDK-backed client introduction can coexist temporarily with the old helper behind file boundaries
- route and store migrations should be shipped in domain-sized PRs so regressions can be reverted narrowly
- no migration step should require destructive data conversion in client storage

## Success Metrics

The architecture is improved only if it produces observable simplification.

- zero dashboard gateway calls through raw `fetch`
- zero route-level gateway contract type definitions for covered endpoints
- zero auth hydration logic in `+layout.svelte`
- zero direct injected-global reads outside platform code
- one documented client path for all new dashboard backend work

## Implementation Notes

### Recommended dependency direction

- `src-tauri` knows nothing about Svelte route internals
- `dashboard` knows only the platform interface and the SDK
- `packages/sdk` knows nothing about Tauri or Svelte

This keeps each layer replaceable.

### Do not duplicate transport logic

Avoid recreating these behaviors in the dashboard:

- auth header composition
- timeout handling
- error normalization
- JSON parsing rules
- WebSocket transport setup

Those belong in the SDK.

### Do not push normal UI state into Rust

Tauri should own runtime truth, not view state.

Keep in the dashboard:

- active tabs
- filters
- panel state
- pagination state
- graph interaction state
- transient editor/input state

## Risks

### Risk 1: SDK coverage is incomplete

This is likely. The fix is to expand the SDK first, not to preserve raw calls as a fallback.

### Risk 2: Browser mode and desktop mode diverge

This is manageable if the platform interface is explicit and both implementations satisfy the same contract.

### Risk 3: Migration churn in large routes

This is real. Migrate stores and shared shell boot first, then route files.

## Acceptance Criteria

The migration is complete when all of the following are true:

- the dashboard no longer uses `dashboard/src/lib/api.ts`
- dashboard gateway calls go through `packages/sdk`
- dashboard auth does not depend on `sessionStorage` hydration hacks
- Tauri owns gateway URL/runtime/auth for desktop
- route-local gateway response interfaces are eliminated where SDK types exist
- the dashboard shell boots from a platform/runtime abstraction
- WebSocket access is also SDK-backed

## Recommended First Cuts

If this work starts now, the first three changes should be:

1. add the platform runtime abstraction
2. add an SDK-backed `ghost-client.ts`
3. migrate `+layout.svelte` off `api.ts`, injected globals, and token hydration

That sequence removes the highest-leverage architectural debt first.
