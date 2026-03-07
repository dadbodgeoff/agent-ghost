# SDK-First Desktop Migration Tasks

## Objective

Execute the migration defined in [design.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/design.md) without a flag day rewrite.

Target end state:

- `packages/sdk` is the only gateway client used by the dashboard.
- Tauri is the source of truth for desktop runtime, auth, config, and gateway lifecycle.
- `dashboard/src/lib/api.ts` is removed.
- `dashboard/src/lib/auth.ts` no longer exists as a session hydration workaround.

## Non-Negotiable Rules

These rules apply to every PR in this migration:

- Do not add new gateway calls through raw `fetch` in dashboard code.
- Do not add new route-local gateway response interfaces if the type belongs in the SDK.
- Do not mix old transport and new transport inside the same store or route.
- Keep `main` releasable after every merged PR.
- Prefer deleting compatibility code quickly once the replacement path is proven.

## Critical Path

There is one correct order:

1. freeze the old path
2. inventory dashboard endpoint usage
3. establish the platform runtime contract
4. fill critical SDK coverage gaps
5. establish the SDK-backed app client
6. migrate shared stores
7. migrate shell boot
8. migrate high-complexity routes
9. remove old helpers and add guardrails

Do not start route-by-route cleanup before the inventory, platform contract, and SDK-backed client exist. That creates rework.

## Merge Gates

Every migration PR must satisfy all of:

- `cargo test --workspace`
- dashboard typecheck passes
- dashboard build passes
- no new raw dashboard gateway `fetch(` calls were introduced
- no new direct reads of `window.__GHOST_GATEWAY_PORT__`
- no new direct auth reads/writes to `sessionStorage` outside the platform layer

## Task List

## T0. Freeze the old path

Purpose:

- stop architectural backsliding while migration is in progress

Actions:

- mark `dashboard/src/lib/api.ts` as deprecated in comments
- mark `dashboard/src/lib/auth.ts` as temporary migration debt
- add a short note to PR guidance or contributing docs for this migration

Done when:

- reviewers have a documented basis to reject new raw transport code

Verification:

- `rg -n "fetch\\(" dashboard/src`
- `rg -n "sessionStorage|__GHOST_GATEWAY_PORT__|__TAURI__" dashboard/src`

## T1. Inventory dashboard endpoint usage

Purpose:

- eliminate unknown scope before migration fans out

Actions:

- produce a mapping of dashboard callsites to SDK coverage
- classify each endpoint as `covered`, `missing-sdk`, or `desktop-runtime`

Known call clusters from current code:

- auth/login/refresh
- agents
- convergence
- costs
- sessions
- goals
- safety
- skills
- search
- memory
- workflows
- orchestration mesh/A2A
- observability/traces
- channels
- webhooks
- profiles
- provider keys
- admin backups
- studio sessions/run
- push notifications
- PC control

Files with heavy usage:

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/routes/studio/+page.svelte`
- `dashboard/src/routes/orchestration/+page.svelte`
- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/lib/stores/studioChat.svelte.ts`

Done when:

- there is no unknown endpoint used by the dashboard
- missing SDK coverage is explicit and prioritized

Verification:

- `rg -n "api\\.(get|post|put|del)\\(|fetch\\(" dashboard/src`

## T2. Define the platform runtime contract

Files:

- `dashboard/src/lib/platform/runtime.ts`
- `dashboard/src/lib/platform/tauri.ts`
- `dashboard/src/lib/platform/web.ts`

Actions:

- define one interface for runtime access
- include `getBaseUrl()`
- include `getToken()`, `setToken()`, `clearToken()`
- include `isDesktop()`
- include gateway lifecycle/status methods needed by the shell
- include a token change subscription hook if required by boot/session state

Constraints:

- route files must not import Tauri plugins directly
- route files must not inspect window globals directly

Done when:

- all runtime-specific reads can be routed through one interface
- desktop and browser adapters both satisfy the same interface

Verification:

- `rg -n "__TAURI__|__GHOST_GATEWAY_PORT__|sessionStorage" dashboard/src`

Expected result after T2:

- only platform files may still match

## T3. Fill SDK coverage gaps

Files:

- `packages/sdk/src/index.ts`
- `packages/sdk/src/generated-types.ts`
- new or expanded domain modules under `packages/sdk/src`

Actions:

- add SDK APIs for every dashboard-used endpoint missing coverage
- add typed WebSocket support for dashboard event usage
- keep naming consistent with existing SDK domains

Priority order:

1. shell boot and auth-adjacent coverage
2. store-backed high-frequency domains
3. large route-only domains
4. low-frequency admin/settings screens

Done when:

- every dashboard gateway call is representable through the SDK

Verification:

- no migration PR needs a new raw dashboard fetch because "the SDK does not support it yet"

## T4. Build the SDK-backed app client

Files:

- `dashboard/src/lib/ghost-client.ts`
- `packages/sdk/src/*` as needed

Actions:

- create one dashboard-facing wrapper around `GhostClient`
- wire it to the platform runtime contract
- centralize error normalization and auth invalidation handling
- expose a consistent way for stores/routes to obtain the configured client

Constraints:

- do not reimplement request transport already in `packages/sdk/src/client.ts`
- do not keep `dashboard/src/lib/api.ts` as a peer abstraction

Done when:

- at least one store can migrate without importing `dashboard/src/lib/api.ts`

Verification:

- dashboard typecheck passes
- `packages/sdk` build passes

## T5. Migrate shared stores first

Files:

- `dashboard/src/lib/stores/agents.svelte.ts`
- `dashboard/src/lib/stores/audit.svelte.ts`
- `dashboard/src/lib/stores/convergence.svelte.ts`
- `dashboard/src/lib/stores/costs.svelte.ts`
- `dashboard/src/lib/stores/memory.svelte.ts`
- `dashboard/src/lib/stores/safety.svelte.ts`
- `dashboard/src/lib/stores/sessions.svelte.ts`
- `dashboard/src/lib/stores/studioChat.svelte.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`

Actions:

- replace `api.ts` imports with the SDK-backed client
- replace route-string endpoint knowledge with SDK domain methods
- normalize error handling
- isolate retry/reconnect behavior in one place

Order:

1. `safety`
2. `convergence`
3. `agents`
4. `sessions`
5. `costs`
6. `audit`
7. `memory`
8. `studioChat`
9. `websocket`

Done when:

- migrated stores no longer import `dashboard/src/lib/api.ts`
- migrated stores do not hardcode endpoint strings

Verification:

- `rg -n "from '\\$lib/api'|from \"\\$lib/api\"" dashboard/src/lib/stores`

Expected result after T5:

- no store imports `$lib/api`

## T6. Migrate shell boot

Primary file:

- `dashboard/src/routes/+layout.svelte`

Supporting files:

- `dashboard/src/lib/auth.ts`
- `dashboard/src/lib/ghost-client.ts`
- `dashboard/src/lib/platform/*`

Actions:

- remove base URL inference from the layout
- remove auth hydration from the layout
- move token/runtime initialization behind the platform runtime + app client
- keep shell responsibilities limited to app startup orchestration and rendering

Specific current debt to remove:

- direct health probe fetch in the layout
- direct reliance on `BASE_URL`
- token bootstrap via `sessionStorage`
- mixed desktop/web startup logic scattered through the component

Done when:

- the layout no longer owns transport setup details
- the layout no longer needs to know how Tauri persists auth

Verification:

- `rg -n "BASE_URL|sessionStorage|fetch\\(" dashboard/src/routes/+layout.svelte`

Expected result after T6:

- no gateway transport calls remain in `+layout.svelte`

## T7. Migrate login and auth flows

Primary files:

- `dashboard/src/routes/login/+page.svelte`
- `dashboard/src/routes/studio/+page.svelte`
- any auth-related SDK module additions

Actions:

- move login/refresh to SDK-backed flows
- ensure token persistence is mediated by the platform runtime
- make unauthorized handling consistent across desktop and web

Constraints:

- do not keep a separate login transport path if the SDK can own it

Done when:

- login, refresh, logout, and unauthorized redirect behavior use one client path

Verification:

- desktop login works after app restart
- browser login still works in dev mode

## T8. Migrate highest-risk routes

Primary targets:

- `dashboard/src/routes/orchestration/+page.svelte`
- `dashboard/src/routes/studio/+page.svelte`
- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/routes/workflows/+page.svelte`

Actions:

- strip transport code out of the route
- move domain data loading into stores/services
- replace route-local interfaces with SDK types where they belong

Done when:

- route files are mostly orchestration of stores/components
- network details are not embedded in route components

Verification:

- `rg -n "api\\.(get|post|put|del)\\(|fetch\\(" dashboard/src/routes`

Expected result after T8:

- only intentionally deferred routes still match

## T9. Migrate settings and admin surfaces

Targets:

- `dashboard/src/routes/settings/providers/+page.svelte`
- `dashboard/src/routes/settings/profiles/+page.svelte`
- `dashboard/src/routes/settings/webhooks/+page.svelte`
- `dashboard/src/routes/settings/backups/+page.svelte`
- `dashboard/src/routes/settings/oauth/+page.svelte`
- `dashboard/src/routes/settings/notifications/+page.svelte`
- `dashboard/src/routes/settings/channels/+page.svelte`

Actions:

- move these onto SDK coverage or platform runtime where appropriate
- separate browser APIs from gateway APIs

Special note:

- service worker fetches are not automatically in scope if they are browser/runtime fetches rather than gateway client fetches

Done when:

- settings pages no longer depend on `$lib/api`

## T10. Remove old transport/auth helpers

Targets:

- `dashboard/src/lib/api.ts`
- `dashboard/src/lib/auth.ts` or its migration-only portions

Actions:

- delete old helper once all imports are gone
- delete session mirroring logic that existed only to support `api.ts`
- remove dead compatibility comments and shims

Done when:

- the old path cannot accidentally be reused

Verification:

- `rg -n "\\$lib/api|BASE_URL|sessionStorage" dashboard/src`

Expected result after T10:

- only approved platform files or browser-specific code remain

## T11. Add CI and lint guardrails

Actions:

- add a dashboard check that rejects raw gateway fetch usage
- add a check that rejects direct `sessionStorage` auth usage outside platform code
- add a check that rejects direct injected-global runtime reads outside platform code
- switch JS workflow execution to the workspace package manager if not already done

Suggested checks:

- `rg -n "fetch\\(" dashboard/src`
- `rg -n "__GHOST_GATEWAY_PORT__|sessionStorage|__TAURI__" dashboard/src`

Done when:

- the architecture can defend itself after migration

## T12. Final cutover verification

Actions:

- run full workspace verification
- verify desktop boot, login, restart, and reconnect flows
- verify core route set on the migrated architecture

Required manual scenarios:

1. launch Tauri app
2. confirm sidecar boots and dashboard connects
3. log in
4. restart app and confirm auth persists correctly
5. open Studio and confirm sessions load
6. open Agents and Security views
7. confirm WebSocket-backed updates still flow

Done when:

- there is one obvious client path for any new dashboard backend work

## PR Plan

Recommended PR slicing:

1. PR-1: T0 + T1
2. PR-2: T2 platform runtime contract
3. PR-3: T3 SDK coverage batch A
4. PR-4: T4 SDK-backed app client + minimal shell wiring
5. PR-5: T5 store migration batch A
6. PR-6: T6 shell boot migration
7. PR-7: T7 auth/login migration
8. PR-8: T8 route migration batch A
9. PR-9: T9 settings/admin migration
10. PR-10: T10 cleanup
11. PR-11: T11 guardrails + T12 verification

Do not put T1 through T10 into one PR. That is an avoidable review failure.

## Definition of Done

This migration is done only when all of the following are true:

- `dashboard/src/lib/api.ts` has been removed
- dashboard gateway calls flow through `packages/sdk`
- desktop runtime resolution flows through Tauri + platform runtime only
- no dashboard auth flow depends on `sessionStorage` mirroring
- large routes no longer embed transport logic
- CI prevents regression to raw gateway transport code

## Fast Audit Commands

Use these repeatedly during the migration:

```bash
rg -n "from '\\$lib/api'|from \"\\$lib/api\"" dashboard/src
rg -n "api\\.(get|post|put|del)\\(" dashboard/src
rg -n "fetch\\(" dashboard/src/routes dashboard/src/lib dashboard/src/components
rg -n "__GHOST_GATEWAY_PORT__|__TAURI__|sessionStorage" dashboard/src
pnpm --dir dashboard check
pnpm --dir dashboard build
cargo test --workspace
```
