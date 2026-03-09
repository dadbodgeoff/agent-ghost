# Overnight Autonomous Plan

## Objective

Build out the live verification program section by section without requiring user check-ins for normal implementation decisions.

The goal is to wake up with:

- more real live suites in place
- more of the repo covered by real browser, CLI, websocket, SSE, and persistence flows
- real product bugs fixed when found
- artifact trails and summary docs for every new suite or failure

## Execution Rules

These are the rules to follow overnight.

### 1. Default To Action

If a choice is local, reversible, and supported by repo evidence, make the decision and keep moving.

Examples:

- choose route order
- choose harness structure
- use temp configs, temp DBs, and temp homes
- use local mock servers where external providers are not required
- seed deterministic data when a live surface does not naturally create it fast enough

### 2. Use Real Product Surfaces First

Prefer, in order:

1. real dashboard/browser interactions
2. real gateway HTTP and websocket routes
3. real CLI commands
4. direct sqlite seeding only when needed to make a live verification deterministic

Do not replace a real product surface with a lower-fidelity shortcut if the real surface is accessible.

### 3. Fix Real Bugs, Not Hypothetical Ones

When a live run fails:

- first decide whether it is a product bug or a harness bug
- fix the smallest thing that makes the live path reliable
- rerun the same journey
- keep the journey permanently once stable

### 4. Preserve Safety Boundaries

Do not:

- run destructive git commands
- revert unrelated user changes
- change production-like secrets or credentials outside temp environments
- mutate non-temp user data when a temp target is possible

### 5. Keep Evidence

Every new or failing suite should preserve:

- summary JSON
- browser console and page errors
- request and response logs when applicable
- websocket frames when applicable
- gateway and dashboard logs
- screenshots and traces for browser runs

### 6. Manage Disk Aggressively When Needed

If disk pressure appears, clear generated artifacts and temp state before stopping for help.

Safe cleanup targets:

- old `artifacts/live-*` run directories
- temporary suite homes, temp DBs, temp configs, and restored temp targets
- stale browser traces and screenshots from superseded runs
- Rust build artifacts when necessary

Cleanup order:

1. remove old live artifact directories first
2. remove temporary suite working directories
3. if still blocked, run targeted Rust cleanup
4. if still blocked, run broader build cleanup

Allowed commands when needed:

- `rm -rf artifacts/live-*`
- `cargo clean -p <crate>`
- `cargo clean`

Rule:

- prefer deleting reproducible artifacts over anything user-authored
- never delete non-generated repo content

## Stop Conditions

Do not stop for routine implementation choices.

Only stop and wait for the user if one of these happens:

1. A task requires external credentials, OAuth provider setup, or a real third-party account that cannot be safely mocked.
2. A necessary change would be destructive outside temp directories.
3. The repo has conflicting user edits in the same files that make a safe merge unclear.
4. A design choice would force a broad architectural rewrite instead of a contained live verification addition.

If none of those are true, continue.

## Priority Order

Work in this order unless a newly found bug makes a lower layer unstable.

### Phase 1: Runtime Live Suite

Build `audit:runtime-live` first.

Why first:

- current coverage proves Studio and infra
- it does not yet prove the general runtime engine
- this is the next highest-value path by crate and route coverage

Target crates:

- `ghost-agent-loop`
- `ghost-llm`
- `ghost-policy`
- `ghost-kill-gates`
- `ghost-heartbeat`
- `read-only-pipeline`
- `simulation-boundary`
- `cortex-validation`

Target routes and surfaces:

- `/api/agents`
- `/api/agent/chat`
- `/api/agent/chat/stream`
- `/api/goals`
- `/api/traces/:session_id`
- `/api/live-executions/:execution_id`
- `/api/safety/status`
- `/api/safety/pause/:agent_id`
- `/api/safety/resume/:agent_id`
- `/api/safety/quarantine/:agent_id`
- `/api/safety/kill-all`
- `/api/costs`
- `/api/profiles`
- `/api/agents/:id/profile`
- `/api/sessions/:id/heartbeat`
- dashboard routes for agents, goals, approvals, costs, security, sessions, orchestration

Minimum journeys:

- create agent
- assign profile
- run blocking agent chat
- run streaming agent chat
- verify traces and live execution
- verify at least one goal appears and can be approved or rejected
- exercise pause, resume, quarantine, and kill-all
- verify a denied path from policy or simulation-boundary behavior
- verify cost counters move
- verify heartbeat creates observable session activity

Exit criteria:

- `pnpm audit:runtime-live` exists
- it passes end to end on a fresh temp environment
- failures found during buildout are fixed or documented with blocker notes

### Phase 2: Knowledge Live Suite

Build `audit:knowledge-live` next.

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

Target surfaces:

- memory routes
- search routes
- sessions and bookmarks
- audit query and export routes
- admin export
- dashboard memory, search, sessions, goals, ITP pages

Minimum journeys:

- write memory
- read memory
- archive and unarchive
- search and graph verification
- bookmark and branch session
- audit query, aggregation, export
- one `ghost-export` smoke path
- one non-destructive `ghost-migrate` smoke path

Exit criteria:

- `pnpm audit:knowledge-live` exists and passes

### Phase 3: I/O Live Suite

Build `audit:io-live`.

Target crates:

- `ghost-skills`
- `ghost-channels`
- `ghost-oauth`
- `ghost-secrets`
- `ghost-egress`
- `ghost-proxy`
- `ghost-drift`

Minimum journeys:

- install and execute skill
- quarantine and resolve skill
- create and reconnect channel
- inject channel message
- OAuth mock callback flow
- OAuth execute and disconnect
- provider key CRUD
- webhook test delivery
- proxy or egress allow/deny smoke path
- `ghost-drift` MCP smoke path

Exit criteria:

- `pnpm audit:io-live` exists and passes with mocks where real providers are unnecessary

### Phase 4: Distributed Live Suite

Build `audit:distributed-live`.

Target crates:

- `ghost-mesh`
- `ghost-marketplace`
- `cortex-multiagent`
- `ghost-signing`
- `ghost-identity`

Minimum journeys:

- dual gateway boot
- A2A discovery
- task send and task stream
- trust graph and consensus read
- marketplace agent registration
- contract lifecycle smoke path
- review flow

Exit criteria:

- `pnpm audit:distributed-live` exists and passes in a local dual-node setup

### Phase 5: Ops Live Suite

Build `audit:ops-live`.

Target crates:

- `ghost-pc-control`
- `cortex-observability`
- `cortex-napi`

Minimum journeys:

- read and mutate PC control policy
- safe no-op or mocked PC control action
- observability surface smoke path
- Node binding smoke path if shippable from local environment

Exit criteria:

- `pnpm audit:ops-live` exists and passes

### Phase 6: Aggregate Repo Suite

Build `audit:repo-live` after the child suites stabilize.

Rules:

- keep `audit:critical-live` focused on must-not-break paths
- use `audit:repo-live` as the full orchestrator
- do not collapse everything into one huge script

### Phase 7: Harness Infra Hardening

Build the shared operating layer that makes the verifier production-ready.

Required outputs:

- shared harness utilities for boot, login, artifacts, websocket capture, and temp environments
- local mock services for OAuth, webhook, channel, and peer-node flows
- `audit:preflight-live`
- coverage manifests for crates, routes, and dashboard pages
- artifact pruning and storage-cleanup helpers
- aggregate reporting for `audit:repo-live`

Rule:

- if a suite starts duplicating large amounts of boot/login/artifact logic, factor that logic into shared harness code before adding more suites

## Finalized Vision

The finished system is not just a pile of audit scripts. It is a production-grade live verification program for the whole workspace.

### End-State Coverage

The final live POC should exercise every meaningful crate through one of these paths:

1. direct runtime coverage through its own binary or service
2. gateway route coverage through real HTTP, SSE, websocket, or browser flows
3. CLI coverage for operator-facing commands
4. indirect subsystem coverage only when the crate is a leaf or support crate

End-state scope across crate groups:

- gateway and dashboard
- agent runtime and safety
- convergence, memory, integrity, retrieval, and search
- backup, export, migrate, and persistence tooling
- skills, channels, OAuth, secrets, egress, proxy, and drift
- mesh, A2A, marketplace, identity, and signing
- PC control, observability, and Node bindings

### Production-Ready Script Requirements

`audit:repo-live` is production-ready only when it has all of these properties:

- one top-level command to run the full repo suite
- ability to run child suites independently
- deterministic temp environments per suite
- no dependency on existing local user state
- machine-readable summaries and stable exit codes
- artifact capture on failure by default
- optional artifact retention on success
- selective rerun of failed components
- component-level timeouts
- safe cleanup of temp space and old artifacts
- support for mock local services where external providers are not required
- explicit reporting of uncovered or skipped surfaces
- documentation that maps suites to crates and routes

### What “Done” Looks Like

This live verification program is done when:

- every major workspace crate is either directly covered or intentionally marked as indirect/support-only
- every major gateway route family has at least one real live journey
- every dashboard section has at least a smoke-level real browser check
- the full suite can run from a clean checkout on temp state alone
- the full suite finds real regressions instead of mostly harness noise
- operators can trust `audit:critical-live` for release gating
- operators can trust `audit:repo-live` for full-system validation

## Required Deliverables

By the time the user returns, leave behind:

- new suite scripts under `dashboard/scripts`
- root and dashboard package scripts for each suite
- updated plan docs when scope expands
- passing summary JSON for every completed suite
- direct notes on any real product bugs found and fixed
- blocker notes only where the stop conditions were hit

## Reporting Format

Each completed phase should leave:

- artifact directory path
- summary JSON path
- command used to rerun it
- one short note on what subsystem it proves
- one short note on any residual risk

## References

- [LIVE_REPO_VERIFICATION_PLAN.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/LIVE_REPO_VERIFICATION_PLAN.md)
- [LIVE_CRATE_COVERAGE_AUDIT.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/LIVE_CRATE_COVERAGE_AUDIT.md)
