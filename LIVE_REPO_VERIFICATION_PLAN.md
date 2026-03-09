# Live Repo Verification Plan

## Goal

Expand the current Studio live audit into a full repo verification program that exercises the real system section by section:

- real gateway
- real database
- real browser and CLI surfaces
- real websocket/SSE paths
- real artifact capture on failure

This is not a unit-test replacement. It is an operator-grade live validation layer for the highest-risk paths first.

Crate-level gap audit:

- [LIVE_CRATE_COVERAGE_AUDIT.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/LIVE_CRATE_COVERAGE_AUDIT.md)

Autonomous execution charter:

- [OVERNIGHT_AUTONOMOUS_PLAN.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/OVERNIGHT_AUTONOMOUS_PLAN.md)

Post-coverage hardening plan:

- [LIVE_HARDENING_PHASE2_PLAN.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/LIVE_HARDENING_PHASE2_PLAN.md)

Operator runbook:

- [LIVE_OPERATOR_RUNBOOK.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/LIVE_OPERATOR_RUNBOOK.md)

## Current Baseline

The current foundation is:

- `pnpm audit:studio-live`
- `pnpm audit:poc-live`

Current live coverage:

- Studio plain reply
- Studio tool call
- Studio persistence after reload
- Studio cancel
- Studio session switching
- Skills page render

## Expansion Rules

Every new section should follow the same rules:

1. Use the real product surface first.
2. Capture logs, websocket frames, SSE/recovery output, page errors, and summary JSON.
3. Treat any live failure as either:
   - a product bug
   - a harness timing/selector bug
4. Fix the smallest thing that makes the live flow reliable.
5. Keep the journey permanently once it is stable.

## Cross-Cutting Harness Infra

The suite family roadmap is not enough by itself. The live verifier also needs shared harness infrastructure so it can operate like a production-grade system instead of a loose set of scripts.

Required cross-cutting additions:

- shared script utilities for gateway boot, dashboard boot, browser login, websocket capture, artifact persistence, and temp-environment setup
- local mock service pack for OAuth callbacks, webhook receivers, channel endpoints, peer gateways, and other external integrations
- preflight runner to check disk, ports, browser availability, binary freshness, and required local tooling before long runs
- artifact management and pruning for old runs, temp directories, traces, screenshots, and build outputs when storage pressure appears
- coverage manifests mapping crates, gateway route families, and dashboard pages to suites and journeys
- deterministic component timeouts, retry policy, and explicit flake classification
- aggregate summary reporting across child suites
- release and nightly wiring once suites stabilize

These should be built alongside the suite families, not deferred to the end.

## Rollout Order

### Phase 0: Finish the current realtime slice

Scope:

- [dashboard/src/routes/studio/+page.svelte](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/routes/studio/+page.svelte)
- [dashboard/src/lib/stores/studioChat.svelte.ts](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/lib/stores/studioChat.svelte.ts)
- [packages/sdk/src/chat.ts](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/packages/sdk/src/chat.ts)
- [packages/sdk/src/sessions.ts](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/packages/sdk/src/sessions.ts)
- [packages/sdk/src/websocket.ts](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/packages/sdk/src/websocket.ts)
- [crates/ghost-gateway/src/api/studio_sessions.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/studio_sessions.rs)
- [crates/ghost-gateway/src/api/websocket.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/websocket.rs)

Why first:

- This is the highest-value user path today.
- It also validates the shared realtime substrate used by the rest of the app.

Add next:

- websocket reconnect and resync
- mid-stream reload and stream recovery
- recovery after forced dashboard refresh
- failed tool call rendering
- auth expiry/refresh inside Studio

Target command:

- keep extending `pnpm audit:poc-live`

### Phase 1: Core infra and operator bootstrap

Scope:

- [crates/ghost-gateway/src/route_sets.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/route_sets.rs)
- [crates/ghost-gateway/src/api/health.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/health.rs)
- [crates/ghost-gateway/src/api/auth.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/auth.rs)
- [crates/ghost-gateway/src/api/compatibility.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/compatibility.rs)
- [crates/ghost-gateway/src/api/rbac.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/rbac.rs)
- [crates/ghost-gateway/src/cli/db.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/cli/db.rs)
- [crates/ghost-gateway/src/cli/commands.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/cli/commands.rs)
- [dashboard/src/routes/+layout.svelte](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/routes/+layout.svelte)
- [dashboard/src/routes/login/+page.svelte](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/routes/login/+page.svelte)

Why first:

- If boot, auth, compatibility, RBAC, or DB state is wrong, every higher-level flow is suspect.

Minimum live journeys:

- gateway boot with fresh DB
- `db migrate`, `db status`, `db verify`
- login, refresh, logout, session restore
- compatibility handshake failure/success
- RBAC deny/allow checks on representative routes
- dashboard boot against authenticated gateway

Target command:

- `pnpm audit:infra-live`

### Phase 2: Runtime execution and safety core

Scope:

- [crates/ghost-agent-loop/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-agent-loop/Cargo.toml)
- [crates/ghost-llm/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-llm/Cargo.toml)
- [crates/ghost-policy/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-policy/Cargo.toml)
- [crates/ghost-kill-gates/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-kill-gates/Cargo.toml)
- [crates/convergence-monitor/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/convergence-monitor/Cargo.toml)
- [crates/ghost-gateway/src/api/agent_chat.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/agent_chat.rs)
- [crates/ghost-gateway/src/api/safety.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/safety.rs)
- [crates/ghost-gateway/src/api/goals.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/goals.rs)
- [crates/ghost-gateway/src/api/sessions.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/sessions.rs)
- [crates/ghost-gateway/src/api/traces.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/traces.rs)

Why next:

- This is the actual execution engine and the safety envelope around it.

Minimum live journeys:

- create agent
- run agent chat and agent chat stream
- produce runtime session events
- inspect traces for that session
- trigger proposal/goal creation and approve/reject
- pause, resume, quarantine, and kill-all behavior
- confirm audit and websocket surfaces reflect those transitions

Target command:

- `pnpm audit:runtime-live`

### Phase 3: State, memory, integrity, and persistence

Scope:

- `crates/cortex/*`
- [crates/ghost-audit/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-audit/Cargo.toml)
- [crates/ghost-backup/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-backup/Cargo.toml)
- [crates/ghost-export/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-export/Cargo.toml)
- [crates/ghost-migrate/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-migrate/Cargo.toml)
- [crates/ghost-gateway/src/api/memory.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/memory.rs)
- [crates/ghost-gateway/src/api/state.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/state.rs)
- [crates/ghost-gateway/src/api/integrity.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/integrity.rs)
- [crates/ghost-gateway/src/api/itp.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/itp.rs)

Why here:

- Once execution is trusted, the next risk is silent persistence drift or invalid state history.

Minimum live journeys:

- write, read, search, archive, and unarchive memory
- inspect memory graph changes
- verify CRDT state endpoint
- verify integrity chain output
- verify ITP events exist for real runtime activity
- create backup, export data, and verify restore in a temp environment

Target command:

- `pnpm audit:state-live`

### Phase 4: Skills, channels, OAuth, and external I/O

Scope:

- [crates/ghost-skills/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-skills/Cargo.toml)
- [crates/ghost-channels/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-channels/Cargo.toml)
- [crates/ghost-oauth/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-oauth/Cargo.toml)
- [crates/ghost-gateway/src/api/skills.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/skills.rs)
- [crates/ghost-gateway/src/api/skill_execute.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/skill_execute.rs)
- [crates/ghost-gateway/src/api/channels.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/channels.rs)
- [crates/ghost-gateway/src/api/oauth_routes.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/oauth_routes.rs)
- [dashboard/src/routes/skills/+page.svelte](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/routes/skills/+page.svelte)
- [dashboard/src/routes/channels/+page.svelte](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/routes/channels/+page.svelte)

Why here:

- External I/O is where silent failures become expensive.

Minimum live journeys:

- skill list, install, quarantine, reverify, execute
- create channel, reconnect channel, inject test message
- OAuth provider list, connect, list connection, execute API call, disconnect
- validate that errors are surfaced in UI and logs, not swallowed

Target command:

- `pnpm audit:io-live`

### Phase 5: Distributed and networked behavior

Scope:

- [crates/ghost-mesh/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-mesh/Cargo.toml)
- [crates/ghost-marketplace/Cargo.toml](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-marketplace/Cargo.toml)
- `crates/cortex/cortex-multiagent/*`
- [crates/ghost-gateway/src/api/a2a.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/a2a.rs)
- [crates/ghost-gateway/src/api/mesh_viz.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/mesh_viz.rs)
- [crates/ghost-gateway/src/api/marketplace.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/marketplace.rs)

Why later:

- High complexity and lower operational criticality than boot/auth/runtime/safety.

Minimum live journeys:

- discover peer
- send A2A task
- stream A2A task status
- inspect trust graph and consensus endpoints
- basic marketplace registration and contract lifecycle smoke path

Target command:

- `pnpm audit:distributed-live`

### Phase 6: Admin and edge surfaces

Scope:

- [crates/ghost-gateway/src/api/admin.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/admin.rs)
- [crates/ghost-gateway/src/api/provider_keys.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/provider_keys.rs)
- [crates/ghost-gateway/src/api/webhooks.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/webhooks.rs)
- [crates/ghost-gateway/src/api/pc_control.rs](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/crates/ghost-gateway/src/api/pc_control.rs)
- [dashboard/src/routes/settings/+page.svelte](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/routes/settings/+page.svelte)
- [dashboard/src/routes/pc-control/+page.svelte](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/dashboard/src/routes/pc-control/+page.svelte)

Minimum live journeys:

- provider key CRUD
- webhook CRUD and test delivery
- backup, export, restore
- PC control status and policy mutation

Target command:

- `pnpm audit:ops-live`

### Phase 7: Harness hardening and repo orchestration

Scope:

- shared harness utilities under `dashboard/scripts`
- mock local service pack
- artifact indexing and pruning
- coverage-manifest generation
- preflight checks
- top-level orchestration and summary reporting

Minimum deliverables:

- `pnpm audit:preflight-live`
- `pnpm audit:repo-live`
- shared harness library reused by all suite scripts
- machine-readable coverage manifest for crates, routes, and dashboard sections
- storage cleanup policy implemented in scriptable form
- aggregate report over all child suites

Why this is required:

- without this layer, the suite collection remains useful but not production-ready
- this layer is what makes the verifier repeatable, operable, and trustworthy

## Final Aggregate Command Layout

Target end state:

- `pnpm audit:poc-live`
- `pnpm audit:infra-live`
- `pnpm audit:runtime-live`
- `pnpm audit:state-live`
- `pnpm audit:io-live`
- `pnpm audit:distributed-live`
- `pnpm audit:ops-live`
- `pnpm audit:repo-live`

`audit:repo-live` should be an orchestrator over the section suites, not a giant single script.

## Production-Ready End State

The finished live verification layer should have these properties:

- full workspace coverage by suite family, not just gateway happy paths
- a crate-to-suite mapping with every major crate either directly covered or intentionally marked indirect-only
- a route-to-journey mapping for every major gateway route family
- real browser smoke coverage for every dashboard section
- isolated temp homes, temp DBs, temp configs, and temp mock services per suite
- deterministic machine-readable summaries and exit codes
- failure artifacts by default
- selective child-suite reruns
- safe cleanup behavior for temp storage pressure
- a narrow `audit:critical-live` for release-confidence checks
- a broad `audit:repo-live` for full-system validation

## Post-Coverage Phase 2

The workspace now has full mapped live POC coverage.

That changes the next problem:

- before: expand coverage to zero uncovered surfaces
- now: harden the verifier so it is trustworthy under repetition, automation, and real interruptions

Phase 2 is defined in:

- [LIVE_HARDENING_PHASE2_PLAN.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/LIVE_HARDENING_PHASE2_PLAN.md)

The required workstreams are:

- soak and flake validation
- CI and nightly wiring
- fault injection and recovery journeys
- external integration certification
- performance budgets and trend tracking
- artifact lifecycle and operator UX

The immediate next move is soak validation plus artifact lifecycle, not more coverage expansion.

## Definition of Done For A Section

A section is considered covered when:

1. The main live user/operator journey is scripted.
2. At least one failure-path journey is scripted.
3. The suite captures enough artifacts to debug silent failures.
4. The journey is stable across repeated runs.
5. The section is included in a higher-level aggregate suite.

## Immediate Next Build

Next, extend the realtime suite with:

1. websocket reconnect and resync
2. mid-stream reload and recovery

After that, start Phase 1 with an `audit:infra-live` runner.
