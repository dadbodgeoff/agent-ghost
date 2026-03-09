# Live Crate Coverage Audit

## Purpose

This audit maps the current live verification layer to the actual workspace crates and identifies what still needs to be added to reach full section-by-section repo coverage.

This is a live-systems audit, not a unit-test inventory. A crate counts as covered only when its real runtime behavior is exercised through the product surface, CLI, or its own long-running binary.

## Current Live Commands

Current commands:

- `pnpm audit:poc-live`
- `pnpm audit:infra-live`
- `pnpm audit:convergence-live`
- `pnpm audit:database-live`
- `pnpm audit:critical-live`

What these commands cover today:

- Studio browser flows, websocket delivery, SSE recovery, and session persistence
- Auth/login/logout/bootstrap compatibility
- Convergence monitor integration, CRDT state, integrity endpoints, and ITP reads
- Backup/restore, DB verify, compaction, restart persistence, and backups UI

## Coverage Status By Crate

### Covered Directly Or Substantially

These crates are already exercised by live harnesses in a meaningful way:

- `ghost-gateway` — partial direct coverage through Studio, auth, convergence, backup, DB, websocket, and recovery paths
- `ghost-backup` — direct backup and restore coverage via CLI and admin UI
- `convergence-monitor` — direct sidecar coverage in the convergence harness
- `cortex-storage` — direct coverage through migrations, status, verify, compaction, restore, memory/ITP persistence
- `cortex-temporal` — direct partial coverage through hash-chain verification and integrity endpoints
- `cortex-crdt` — direct partial coverage through CRDT reconstruction in convergence live audit
- `cortex-convergence` — direct partial coverage through seeded score computation surfaces and watcher updates

### Partially Covered But Not Enough

These crates are touched today, but the current live suite does not validate their important failure modes:

- `ghost-agent-loop` — Studio proves a basic happy path, but not agent chat routes, goal extraction, safety transitions, or runtime event surfaces
- `ghost-llm` — real provider calls work, but fallback routing, circuit breaking, cost routing, and degraded behavior are not covered
- `ghost-skills` — skills page renders, but install, execute, quarantine, reverify, and WASM/user skill flows are not live-covered
- `ghost-policy` — implicit success paths are covered, but explicit allow/deny/tightening behavior is not
- `itp-protocol` — real ITP rows exist, but producer diversity and transport coverage are still narrow
- `read-only-pipeline` — indirectly exercised by agent runs, but not explicitly verified against convergence/profile changes
- `simulation-boundary` — indirectly present in runtime, but no live enforcement path proves reframing/deny behavior
- `ghost-audit` — audit data likely exists, but query/aggregation/export routes are not live-covered

### Not Yet Covered Directly

These crates currently have no dedicated live verification slice:

- `ghost-channels`
- `ghost-drift`
- `ghost-egress`
- `ghost-export`
- `ghost-heartbeat`
- `ghost-identity`
- `ghost-kill-gates`
- `ghost-marketplace`
- `ghost-mesh`
- `ghost-migrate`
- `ghost-oauth`
- `ghost-pc-control`
- `ghost-proxy`
- `ghost-secrets`
- `ghost-signing`
- `cortex-decay`
- `cortex-embeddings`
- `cortex-multiagent`
- `cortex-napi`
- `cortex-observability`
- `cortex-privacy`
- `cortex-retrieval`
- `cortex-validation`

### Internal Or Test-Support Crates

These should be exercised indirectly via parent suites rather than getting their own top-level live harness:

- `cortex-core`
- `cortex/test-fixtures`
- `ghost-integration-tests`

## What Needs To Be Added Next

The repo needs five more suite families to cover the real subsystem boundaries that are still blind.

### 1. `audit:runtime-live`

Highest priority missing slice.

Target crates:

- `ghost-agent-loop`
- `ghost-llm`
- `ghost-policy`
- `ghost-kill-gates`
- `ghost-heartbeat`
- `read-only-pipeline`
- `simulation-boundary`
- `cortex-validation`

Target gateway surfaces:

- `/api/agents`
- `/api/agent/chat`
- `/api/agent/chat/stream`
- `/api/goals`
- `/api/safety/status`
- `/api/safety/pause/:agent_id`
- `/api/safety/resume/:agent_id`
- `/api/safety/quarantine/:agent_id`
- `/api/safety/kill-all`
- `/api/traces/:session_id`
- `/api/live-executions/:execution_id`
- `/api/costs`
- `/api/profiles`
- `/api/agents/:id/profile`
- `/api/sessions/:id/heartbeat`

Minimum live journeys:

- create agent
- run blocking agent chat
- run streaming agent chat
- verify runtime session/traces/live-execution artifacts
- trigger at least one goal/proposal and approve or reject it
- pause, resume, quarantine, and kill-all an agent
- verify policy deny path and simulation-boundary intervention on a disallowed prompt
- verify costs move after a paid provider turn
- verify heartbeat creates a session/event

Why it must come next:

- The current suite proves Studio works, but it does not yet prove the general runtime engine and safety shell work outside the Studio path.

### 2. `audit:knowledge-live`

Second priority. This is the missing memory/retrieval/export slice.

Target crates:

- `ghost-audit`
- `ghost-export`
- `ghost-migrate`
- `cortex-retrieval`
- `cortex-privacy`
- `cortex-decay`
- `cortex-embeddings`
- `cortex-storage`
- `cortex-temporal`

Target gateway and dashboard surfaces:

- `/api/memory`
- `/api/memory/:id`
- `/api/memory/search`
- `/api/memory/graph`
- `/api/memory/archived`
- `/api/search`
- `/api/sessions`
- `/api/sessions/:id/events`
- `/api/sessions/:id/bookmarks`
- `/api/sessions/:id/branch`
- `/api/audit`
- `/api/audit/aggregation`
- `/api/audit/export`
- `/api/admin/export`
- dashboard memory/search/sessions/itp/goals pages

Minimum live journeys:

- write memory through the real API
- verify memory list, detail, search, graph, archive, and unarchive
- create bookmark and branch a session
- confirm session and memory searches return newly written data
- hit audit query, aggregation, and export against data created during the run
- import one small fixture into `ghost-export` and verify analyzer output
- run one non-destructive `ghost-migrate` smoke path against a synthetic source tree
- validate retrieval/embedding-backed search on a populated DB

Why it matters:

- Right now the suite proves persistence integrity, but not knowledge correctness or retrieval usefulness.

### 3. `audit:io-live`

Third priority. This is the external I/O and secret-handling slice.

Target crates:

- `ghost-skills`
- `ghost-channels`
- `ghost-oauth`
- `ghost-secrets`
- `ghost-egress`
- `ghost-proxy`
- `ghost-drift`

Target gateway and dashboard surfaces:

- `/api/skills`
- `/api/skills/:id/install`
- `/api/skills/:id/quarantine`
- `/api/skills/:id/reverify`
- `/api/skills/:name/execute`
- `/api/channels`
- `/api/channels/:id/reconnect`
- `/api/channels/:type/inject`
- `/api/oauth/providers`
- `/api/oauth/connect`
- `/api/oauth/connections`
- `/api/oauth/execute`
- `/api/admin/provider-keys`
- `/api/webhooks`
- `/api/push/*`
- dashboard skills/channels/settings/oauth/providers/webhooks/notifications pages

Minimum live journeys:

- install and execute a skill
- trigger quarantine and reverify resolution
- create a channel, reconnect it, and inject a message
- exercise an OAuth provider via a local/mock callback flow
- execute one OAuth-backed API call and disconnect it
- CRUD a provider key and confirm secret-backed reads behave correctly
- create a webhook and fire a test delivery into a local receiver
- run a proxy or egress allow/deny smoke path using a local test server
- boot `ghost-drift` and run at least one MCP tool request

Why it matters:

- These are the places where silent failures become expensive and user-visible.

### 4. `audit:distributed-live`

Fourth priority. This is the multi-node and marketplace slice.

Target crates:

- `ghost-mesh`
- `ghost-marketplace`
- `cortex-multiagent`
- `ghost-signing`
- `ghost-identity`

Target gateway surfaces:

- `/api/a2a/tasks`
- `/api/a2a/tasks/:task_id`
- `/api/a2a/tasks/:task_id/stream`
- `/api/a2a/discover`
- `/api/mesh/trust-graph`
- `/api/mesh/consensus`
- `/api/mesh/delegations`
- `/api/marketplace/*`

Minimum live journeys:

- boot two gateways against separate temp homes
- discover peer agents
- submit A2A task and stream task status
- inspect trust graph and delegation surfaces
- register an agent in marketplace
- create, accept, start, complete, and review a marketplace contract
- verify signature and identity-backed flows on inter-agent traffic

Why it matters:

- This is the biggest remaining architectural blind spot after runtime and I/O.

### 5. `audit:ops-live`

Fifth priority. This is the operator and edge-device slice.

Target crates:

- `ghost-pc-control`
- `cortex-observability`
- `cortex-napi`

Target gateway and dashboard surfaces:

- `/api/pc-control/status`
- `/api/pc-control/actions`
- `/api/pc-control/allowed-apps`
- `/api/pc-control/blocked-hotkeys`
- `/api/pc-control/safe-zones`
- `/api/admin/backups`
- `/api/admin/restore`
- dashboard pc-control/observability/security/settings pages

Minimum live journeys:

- read and mutate PC control status
- verify app allowlist, blocked hotkey, and safe-zone policy updates
- execute one safe no-op or mocked PC control action path
- verify observability surface returns non-empty metrics state
- if Node bindings are shipped, run one `cortex-napi` smoke call from Node

Why it matters:

- These are high-risk admin surfaces, but they depend on lower layers being trustworthy first.

## Gateway Surface Gaps Still Uncovered

The current live suite still does not touch these major route families in a meaningful way:

- agents
- agent chat
- goals and approvals
- memory and search
- sessions bookmarks and branching
- workflows and orchestration
- audit query/export
- costs
- profiles
- channels
- OAuth
- provider keys
- webhooks
- push notifications
- A2A and mesh
- marketplace
- PC control
- observability/security pages

## Recommended Rollout Order

1. Add `audit:runtime-live`
2. Add `audit:knowledge-live`
3. Add `audit:io-live`
4. Add `audit:distributed-live`
5. Add `audit:ops-live`
6. Expand `audit:critical-live` only after each child suite is stable
7. End with `audit:repo-live` as an orchestrator over all section suites

## End-State Command Set

Target end-state commands:

- `pnpm audit:poc-live`
- `pnpm audit:infra-live`
- `pnpm audit:convergence-live`
- `pnpm audit:database-live`
- `pnpm audit:runtime-live`
- `pnpm audit:knowledge-live`
- `pnpm audit:io-live`
- `pnpm audit:distributed-live`
- `pnpm audit:ops-live`
- `pnpm audit:critical-live`
- `pnpm audit:repo-live`

`audit:critical-live` should stay focused on the must-not-break path.

`audit:repo-live` should be a full orchestrator, not one giant monolithic script.
