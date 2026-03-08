# Skill Pipeline Hardening Tasks

## Intent

This document is the execution plan for the skill-pipeline hardening workstream.
It is derived from:

- [design.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/skill-pipeline/design.md)
- [implementation.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/skill-pipeline/implementation.md)

This file is intentionally task-shaped:

- ordered work
- concrete acceptance criteria
- explicit verification commands
- rollout checkpoints

## Execution Rules

- Keep the workstream scoped to compiled-skill truth first.
- Do not revive file-backed skill claims during this wave.
- Do not leave transitional contract drift undocumented.
- Do not land UI or SDK changes before the gateway contract exists.
- Do not remove compatibility fields until the dashboard and SDK are updated.

## Exit Criteria

The workstream is only complete when all of these are true:

1. `/api/skills` truthfully reflects install state and always-on state.
2. install and uninstall actions change runtime-visible behavior.
3. `POST /api/skills/:name/execute` no longer uses a read-only DB path.
4. no route handler executes skills by directly calling `skill.execute(...)`
   from the raw catalog map.
5. runtime skill exposure and policy grants are aligned under test.
6. dashboard install review shows real privileges, not only `skill:<name>`.
7. docs no longer claim live file-backed discovery, signing, or sandboxing.

## Task Sequence

### T1. Add Persisted Skill Install State

Deliver:

- new migration for `skill_install_state`
- storage query helpers for read, upsert, and state transitions
- migration copy-forward from legacy `installed_skills`

Primary files:

- `crates/cortex/cortex-storage/src/migrations/`
- `crates/cortex/cortex-storage/src/queries/`

Acceptance criteria:

- database can represent `installed` and `disabled`
- legacy `installed_skills` rows are copied into the new state table
- no new gateway code depends on duplicated metadata from `installed_skills`

Verification:

- run storage migration tests if present
- add a new migration-specific test if none exists

### T2. Create The Skill Catalog Module

Deliver:

- `skill_catalog/mod.rs`
- `skill_catalog/definitions.rs`
- `skill_catalog/service.rs`
- `skill_catalog/dto.rs`
- `skill_catalog/executor.rs`

Primary files:

- `crates/ghost-gateway/src/skill_catalog/`

Acceptance criteria:

- compiled skills are defined in one shared builder
- service can list skills and merge persisted state
- DTOs can express `always_on`, `installed`, and `available`
- executor can resolve a skill and prepare canonical execution context

Verification:

- add unit tests for state merge behavior
- add unit tests for DTO generation
- add unit tests for runtime resolution behavior

### T3. Replace `AppState.safety_skills` With `AppState.skill_catalog`

Deliver:

- `AppState` carries the catalog service, not a raw skill map
- bootstrap constructs the catalog service
- test fixtures use catalog helpers instead of raw `HashMap`s

Primary files:

- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- gateway test helpers and fixture builders

Acceptance criteria:

- bootstrap no longer stores a misleading `safety_skills` field
- test scaffolding compiles with the new `AppState`
- no new production code reads a raw `safety_skills` map from state

Verification:

- `cargo test -p ghost-gateway`

Checkpoint:

- stop here and ensure the gateway still boots and tests compile before moving
  into runtime behavior changes

### T4. Unify Compiled Skill Definition Construction

Deliver:

- one shared compiled skill builder used by bootstrap and CLI chat
- removal of parallel skill assembly logic

Primary files:

- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/cli/chat.rs`
- `crates/ghost-gateway/src/skill_catalog/definitions.rs`

Acceptance criteria:

- bootstrap and CLI chat derive compiled skills from the same source
- skill classification lives in one place
- no duplicate skill-registration lists remain in gateway code

Verification:

- targeted grep confirms one compiled-skill assembly path
- gateway and CLI tests still pass

### T5. Align Runtime Skill Resolution With Policy Grants

Deliver:

- runtime resolves skills through the catalog service
- final resolved skill set is the same set that gets registered
- matching `skill:<name>` capabilities are granted automatically

Primary files:

- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-agent-loop/src/tools/skill_bridge.rs`

Acceptance criteria:

- `agent.skills` controls exposure without requiring duplicated grants in
  `agent.capabilities`
- always-on safety skills remain available
- non-resolved skills are neither registered nor granted

Verification:

- add runtime tests for allowlist, always-on, and denied skill behavior
- `cargo test -p ghost-gateway runtime_safety`

Checkpoint:

- confirm the resolved set and granted set are identical before touching API
  handlers

### T6. Rewrite `GET /api/skills`, Install, and Uninstall

Deliver:

- `api/skills.rs` uses the catalog service exclusively
- `GET /api/skills` returns truthful DTOs
- install moves a skill to `installed`
- uninstall moves a skill to `disabled`
- `SkillChange` events still fire

Primary files:

- `crates/ghost-gateway/src/api/skills.rs`
- `crates/ghost-gateway/src/skill_catalog/service.rs`

Acceptance criteria:

- `available` is no longer hardcoded empty
- always-on skills are represented truthfully
- non-installable or non-removable cases return conflict behavior

Verification:

- add integration tests in `crates/ghost-gateway/tests/skills_api_tests.rs`
- verify install/uninstall changes the next `GET /api/skills` response

### T7. Replace The Direct Skill Execute Bypass

Deliver:

- `api/skill_execute.rs` executes through catalog executor logic
- route no longer uses `DbPool::read()`
- route no longer directly calls `skill.execute(...)` from the raw map

Primary files:

- `crates/ghost-gateway/src/api/skill_execute.rs`
- `crates/ghost-gateway/src/skill_catalog/executor.rs`

Acceptance criteria:

- write-capable skills can execute through the route
- disabled or non-installable skills are rejected correctly
- actor information is preserved for future audit and lifecycle use

Verification:

- integration test: execute a write-capable skill through the route
- integration test: disabled skill execution fails with expected status
- grep check: route handlers no longer call `skill.execute(...)` directly

Checkpoint:

- do not proceed to OpenAPI and SDK work until the route behavior is real and
  tested

### T8. Update OpenAPI And Regenerate Types

Deliver:

- skill DTOs are represented as real OpenAPI schemas
- execute request and response are typed
- generated SDK types stop reporting `content?: never`

Primary files:

- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`

Acceptance criteria:

- `/api/skills` schema reflects actual response bodies
- install and uninstall schemas reflect actual response bodies
- execute schema reflects real request and response DTOs

Verification:

- regenerate the OpenAPI-derived types
- diff `generated-types.ts`
- run SDK tests after regeneration

### T9. Update The SDK Skill Client

Deliver:

- `packages/sdk/src/skills.ts` matches the new DTOs
- compatibility handling for deprecated `capabilities`
- tests updated for the new response shape

Primary files:

- `packages/sdk/src/skills.ts`
- `packages/sdk/src/__tests__/client.test.ts`

Acceptance criteria:

- SDK exposes `policy_capability`, `privileges`, `state`, `installable`,
  `removable`
- dashboard clients can consume the new type without inventing meaning

Verification:

- run SDK unit tests

### T10. Update Dashboard Skill Surfaces

Deliver:

- install review renders `privileges`
- state badges render truthfully
- always-on skills do not present uninstall affordances
- skill card stops relying on `capabilities` as the human review model

Primary files:

- `dashboard/src/routes/skills/+page.svelte`
- `dashboard/src/components/SkillCard.svelte`

Acceptance criteria:

- UI distinguishes `always_on`, `installed`, and `available`
- privilege review is operator-meaningful
- actions are disabled when a skill is not installable or removable

Verification:

- add `dashboard/tests/skills.spec.ts`
- run dashboard tests covering install review and state rendering

### T11. Remove Untruthful Studio Skill Copy

Deliver:

- remove or replace the hardcoded skill-count claim in studio

Primary files:

- `dashboard/src/routes/studio/+page.svelte`

Acceptance criteria:

- no hardcoded skill count remains unless derived truthfully from runtime state

Verification:

- dashboard tests or snapshot checks if available

### T12. Demote CLI Direct Skill Mode

Deliver:

- skill CLI commands require HTTP backend for this wave
- direct mode returns a clear and truthful error

Primary files:

- `crates/ghost-gateway/src/cli/skill.rs`
- CLI tests

Acceptance criteria:

- CLI no longer implies live `manifest.json`-based skill management support
- operator messaging points users to the gateway-backed path

Verification:

- extend `crates/ghost-gateway/tests/cli_tests.rs`

### T13. Update Architecture And Wiki Docs

Deliver:

- docs describe compiled Rust skills as the live supported model
- file-backed registry, signing, quarantine, and sandboxing are clearly marked
  deferred

Primary files:

- `agent/AGENT_ARCHITECTURE.md`
- `wiki/ghost-skills.md`

Acceptance criteria:

- no doc claims active `SKILL.md` discovery or real untrusted WASM execution
- no doc claims active learned skill proposal flow

Verification:

- manual doc review
- grep for known stale phrases

### T14. Full Regression Pass

Deliver:

- complete test run for gateway, SDK, and dashboard surfaces touched by this
  workstream
- evidence captured for each exit criterion

Verification commands:

- `cargo test -p ghost-gateway`
- `cargo test -p ghost-skills`
- project-specific SDK test command
- project-specific dashboard test command

Acceptance criteria:

- all newly added tests pass
- no known skill-pipeline drift remains in the touched surfaces

## Rollout Checkpoints

### Checkpoint A: Data And Catalog Foundation Stable

Must be true after T1-T4:

- install state storage exists
- catalog service exists
- bootstrap and CLI use one definition builder
- tests compile and pass for foundation layers

### Checkpoint B: Runtime And API Truth Stable

Must be true after T5-T7:

- runtime resolution and policy grants align
- `/api/skills` is truthful
- execute route is no longer a bypass

### Checkpoint C: Client Surfaces Stable

Must be true after T8-T12:

- OpenAPI and generated types are truthful
- SDK compiles and tests pass
- dashboard renders the new truth model
- CLI no longer exposes a fake direct path

### Checkpoint D: Governance Stable

Must be true after T13-T14:

- docs match live implementation
- evidence exists for all exit criteria

## Negative Cases That Must Be Tested

- execute a skill that is not installed
- execute a skill that is disabled
- uninstall an always-on skill
- install a non-installable skill
- allowlisted skill without aligned policy grant regression
- write-capable skill through execute route
- dashboard review with sensitive privileges
- CLI skill command in direct mode

## Task Completion Standard

A task is only complete when:

- code is implemented
- tests proving the behavior exist
- stale behavior or stale copy on the same surface is removed
- the next dependent task can proceed without relying on undocumented behavior
