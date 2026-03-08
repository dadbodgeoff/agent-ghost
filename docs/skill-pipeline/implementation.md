# Skill Pipeline Hardening Implementation

## Intent

This document turns the verified design into a concrete implementation plan.

It answers:

- what code changes should be made
- where those changes should live
- what order they should land in
- how compatibility should be handled
- what tests and evidence are required before calling the work complete

This document is still pre-task-planning. It is the engineering bridge between
`design.md` and the eventual `task.md`.

## Chosen Strategy For This Wave

This implementation wave will choose **compiled-skill truth first**.

That means:

- the live product surface will be defined around compiled Rust skills
- gateway, SDK, CLI, and dashboard will be made truthful around that model
- file-backed discovery, signing, quarantine, and WASM execution will **not**
  be presented as live platform guarantees until they are actually wired into
  the runtime

This is the lowest-risk path that still fixes the real defects.

## Implementation Goals

By the end of this wave:

1. the gateway has one canonical skill authority
2. install state affects runtime behavior
3. all skill execution paths use one policy-enforced execution seam
4. `agent.skills` and runtime policy capability grants are aligned
5. the skill DTO is truthful for humans and for clients
6. docs and CLI stop overstating file-backed skill support

## Code Changes

### 1. Add a Gateway-Owned Skill Catalog Domain

Add a new top-level module:

- `crates/ghost-gateway/src/skill_catalog/mod.rs`
- `crates/ghost-gateway/src/skill_catalog/definitions.rs`
- `crates/ghost-gateway/src/skill_catalog/service.rs`
- `crates/ghost-gateway/src/skill_catalog/dto.rs`
- `crates/ghost-gateway/src/skill_catalog/executor.rs`

Purpose:

- move skill truth out of ad hoc maps and API handlers
- give bootstrap, runtime, API, CLI, and dashboard one shared authority

### 1.1 `definitions.rs`

Own the canonical compiled skill definitions.

This module should expose one shared builder used by both bootstrap and CLI:

- `build_compiled_skill_definitions(config: &GhostConfig) -> Vec<SkillDefinition>`

`SkillDefinition` should contain:

- `name`
- `version`
- `description`
- `source`
- `removable`
- `always_on`
- `installable`
- `default_enabled`
- `execution_mode`
- `policy_capability`
- `privileges`
- `skill: Box<dyn Skill>`

Important implementation rule:

- bootstrap and `cli/chat.rs` must stop assembling separate skill catalogs
- both call the same compiled definition builder

This removes one major source of drift immediately.

### 1.2 `service.rs`

Own merged lifecycle state and runtime resolution.

Primary responsibilities:

- load compiled definitions
- merge persisted install state
- resolve installed vs available vs always-on status
- resolve final skill set for a specific agent/runtime request
- provide API-facing summaries

Recommended core types:

- `SkillCatalogService`
- `CatalogSkillState`
- `ResolvedSkillSet`

Suggested methods:

- `list_skills() -> SkillListView`
- `get_skill(name: &str) -> Option<SkillView>`
- `install(name: &str, actor: Option<&str>) -> Result<SkillView, SkillCatalogError>`
- `uninstall(name: &str, actor: Option<&str>) -> Result<SkillView, SkillCatalogError>`
- `resolve_for_runtime(agent: &ResolvedRuntimeAgent, allowlist: Option<&[String]>) -> ResolvedSkillSet`
- `resolve_for_execute(name: &str, agent: &ResolvedRuntimeAgent) -> Result<ResolvedSkill, SkillCatalogError>`

`ResolvedSkillSet` should include:

- `skills: Arc<HashMap<String, Box<dyn Skill>>>`
- `granted_policy_capabilities: Vec<String>`
- `visible_skill_names: Vec<String>`

### 1.3 `dto.rs`

Own truthful API and SDK-facing DTOs.

Recommended DTOs:

- `SkillSummaryDto`
- `SkillListResponseDto`
- `ExecuteSkillRequestDto`
- `ExecuteSkillResponseDto`

Recommended fields on `SkillSummaryDto`:

- `id`
- `name`
- `version`
- `description`
- `source`
- `removable`
- `installable`
- `execution_mode`
- `policy_capability`
- `privileges`
- `state`

Optional fields:

- `quarantine_reason`
- `enabled_for_agent`

Compatibility field:

- keep `capabilities` temporarily as a deprecated alias containing only
  `[policy_capability]`

Rules:

- dashboard and CLI must stop using `capabilities` for human review
- new code uses `policy_capability` and `privileges`

### 1.4 `executor.rs`

Own the canonical non-agent-loop execution seam for externally invoked skill
execution.

This module exists to eliminate direct `skill.execute(...)` calls from route
handlers.

It should:

- resolve the skill through `SkillCatalogService`
- open a write-capable DB connection through `DbPool::legacy_connection()`
- build the same `SkillContext` shape used elsewhere
- enforce install and exposure checks before execution
- return normalized execution errors

It should not bypass:

- install state
- removability rules
- exposure rules
- future auditing hooks

If practical, this module should share as much logic as possible with runtime
skill dispatch rather than introducing a second behavior tree.

## 2. Replace the Current `safety_skills` AppState Field

Change [state.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/state.rs) so `AppState` no longer carries:

- `safety_skills: Arc<HashMap<String, Box<dyn Skill>>>`

Replace it with:

- `skill_catalog: Arc<SkillCatalogService>`

This rename is architecturally important:

- the current field name is already misleading because it contains far more
  than Phase 5 safety skills
- a service boundary is required if install state and runtime state are going
  to stop drifting

All manual `AppState` fixtures that currently set `safety_skills` must be
updated to use a helper constructor such as:

- `SkillCatalogService::empty_for_tests()`

This affects:

- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-gateway/src/api/profiles.rs`
- `crates/ghost-gateway/src/api/pc_control.rs`
- `crates/ghost-gateway/tests/common/mod.rs`
- other manual `AppState` constructors returned by `rg "safety_skills:"`

## 3. Persist Install State Without Persisting Definition Truth

The current `installed_skills` table stores duplicated metadata that should not
be authoritative.

This wave should add a new migration and stop using the old duplicated metadata
as truth.

### 3.1 Add a New Storage Migration

Add a migration after the current latest migration, for example:

- `crates/cortex/cortex-storage/src/migrations/v034_skill_install_state.rs`

Add a new table:

```sql
CREATE TABLE IF NOT EXISTS skill_install_state (
    skill_name   TEXT PRIMARY KEY,
    state        TEXT NOT NULL,
    updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_by   TEXT
);
CREATE INDEX IF NOT EXISTS idx_skill_install_state_state
    ON skill_install_state(state);
```

Permitted values for `state` in this wave:

- `installed`
- `disabled`

### 3.2 Migration Rules

Migration behavior:

1. create the new table
2. copy existing `installed_skills.skill_name` entries into
   `skill_install_state(skill_name, state='installed')`
3. leave `installed_skills` in place as legacy data for now
4. stop reading definition metadata from `installed_skills`

### 3.3 Seeding Rules

At catalog initialization:

- for every compiled skill definition marked `installable` and
  `default_enabled`, insert a missing `skill_install_state` row with
  `state='installed'`
- do not overwrite existing rows

Important:

- uninstall must set `state='disabled'`
- uninstall must not delete the row

Otherwise disabled skills would be silently reinstalled on restart.

## 4. Rework Bootstrap To Build One Catalog

In [bootstrap.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs):

Current behavior:

- build `all_skills` as an in-memory map
- place it directly into `AppState`

Target behavior:

1. call `build_compiled_skill_definitions(config)`
2. construct `SkillCatalogService` with those definitions plus DB-backed install
   state
3. store only the catalog service in `AppState`

The compiled definition builder should be the only place that decides:

- which compiled skills exist
- which are always-on
- which are installable
- which are removable
- which have which human-readable privileges

### 4.1 Default Classification Rules

For this implementation wave:

- Phase 5 safety skills: `always_on = true`, `installable = false`
- git/code-analysis/delegation operational skills: default enabled, but whether
  they are `always_on` or `installable` must be decided once in the definition
  builder and used consistently everywhere
- bundled user-facing skills: `installable = true`, `default_enabled = true`
- disabled PC control features remain absent when feature flags/config disable
  them

Recommended stance:

- safety skills remain always-on
- optional bundled skills become installable
- default to installed on migration to preserve current behavior

## 5. Align Runtime Skill Registration With Policy Grants

In [runtime_safety.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/runtime_safety.rs):

Current behavior:

- runner receives a raw skill map
- `agent.skills` controls registration visibility
- policy grants come from `agent.capabilities`

Target behavior:

1. resolve the final skill set through `state.skill_catalog.resolve_for_runtime(...)`
2. register only the resolved skills into the bridge
3. grant `skill:<name>` policy capabilities automatically for the resolved set
4. keep `agent.capabilities` for non-skill tools and explicit low-level powers

This changes the meaning of `agent.skills`:

- it becomes the operator-facing skill exposure control
- it no longer requires duplicating the same choice inside `agent.capabilities`

### 5.1 Runtime Dependencies Change

`RuntimeRunnerDependencies` should no longer carry the entire catalog as a raw
skill map.

Instead it should carry either:

- the `SkillCatalogService`

or:

- an already resolved `ResolvedSkillSet`

Preferred implementation:

- resolve before building the bridge
- pass `ResolvedSkillSet.skills` into `SkillBridge`
- grant `ResolvedSkillSet.granted_policy_capabilities` into the policy engine

### 5.2 CLI Chat Must Use The Same Resolution Rules

In [cli/chat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/chat.rs):

- stop assembling skills independently
- call the same compiled definition builder and resolution rules as bootstrap

The CLI must not be a parallel skill-policy universe.

## 6. Replace the Skill API Handlers

### 6.1 `GET /api/skills`

Rework [api/skills.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/skills.rs):

- remove direct iteration over `state.safety_skills`
- remove hardcoded `available: []`
- remove synthetic privilege disclosure
- generate DTOs from `SkillCatalogService`

Response grouping for this wave:

- `installed`: installable skills currently installed plus always-on skills
- `available`: installable compiled skills currently disabled

Each item must carry `state`, so clients can distinguish:

- `always_on`
- `installed`
- `available`

### 6.2 `POST /api/skills/:id/install`

Target behavior:

- call `state.skill_catalog.install(...)`
- set persisted state to `installed`
- reject `always_on` or non-installable skills with `409`
- broadcast `SkillChange`
- return the truthful DTO from catalog state

### 6.3 `POST /api/skills/:id/uninstall`

Target behavior:

- call `state.skill_catalog.uninstall(...)`
- set persisted state to `disabled`
- reject non-removable or always-on skills with `409`
- broadcast `SkillChange`

### 6.4 `POST /api/skills/:name/execute`

Rework [api/skill_execute.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/skill_execute.rs):

- stop reading from `state.safety_skills`
- stop calling `skill.execute(...)` directly
- stop using `DbPool::read()`
- execute through `skill_catalog::executor`

Recommended request handling additions:

- accept `Extension<Claims>` or derive operator identity from middleware
- record the actor for audit and lifecycle updates

Recommended error mapping:

- `404` for unknown skill names
- `409` for known-but-disabled or known-but-not-installable execution requests
- `403` for exposure or policy denial
- `400` for invalid skill input

## 7. Update OpenAPI To Match Reality

In [api/openapi.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/openapi.rs):

- replace placeholder skill route descriptions with real DTO schemas
- stop using `inline(serde_json::Value)` for execute request and response
- derive `ToSchema` for the new skill DTO types

This is necessary so:

- generated SDK types become truthful
- the OpenAPI surface stops hiding response bodies behind `content?: never`

After that, regenerate:

- [generated-types.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/generated-types.ts)

## 8. Update The SDK Surface

Update [packages/sdk/src/skills.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/skills.ts):

- replace the loose `Skill` interface with the truthful DTO
- add `policy_capability`, `privileges`, `state`, `installable`, and `removable`
- keep `capabilities` only as a deprecated compatibility field if needed

Update tests in:

- [client.test.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/__tests__/client.test.ts)

New SDK test expectations:

- `list()` returns the new DTO shape
- install and uninstall honor new state semantics
- execute route DTOs parse correctly if exposed through the SDK later

## 9. Update The Dashboard Skill UX

Update:

- [dashboard/src/routes/skills/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/skills/+page.svelte)
- [dashboard/src/components/SkillCard.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/components/SkillCard.svelte)

Required changes:

- render `privileges` in the review dialog
- stop treating `capabilities` as the safety review surface
- render `state` explicitly
- disable install/uninstall actions based on `installable` and `removable`
- show always-on skills truthfully rather than as ordinary installed skills

Recommended UI treatment:

- `always_on` badge
- `installed` badge
- `available` badge
- `policy capability` shown only in an advanced/details section

### 9.1 Remove Untruthful Marketing Copy

Update [dashboard/src/routes/studio/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/studio/+page.svelte):

- remove the hardcoded `"44 skills auto-injected"` claim
- either replace it with neutral wording or derive a real count from the
  resolved runtime if that count is available cheaply and truthfully

## 10. Demote CLI Skill Direct Mode For Now

In [cli/skill.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/skill.rs):

Current direct mode behavior:

- scans `~/.ghost/skills`
- expects `manifest.json`
- implies file-backed skill packaging support that the live gateway does not
  actually own

For this wave:

- make `ghost skill list`, `ghost skill install`, and `ghost skill inspect`
  HTTP-backed only
- if direct mode is selected, return a clear error explaining that skill
  management currently requires a running gateway

This is intentionally conservative.

It is better to remove a false operator path than to keep a misleading one.

## 11. Update Documentation To Match The Live Product

Update:

- [agent/AGENT_ARCHITECTURE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/agent/AGENT_ARCHITECTURE.md)
- [wiki/ghost-skills.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/wiki/ghost-skills.md)

Required doc posture:

- compiled Rust skills are the live supported model
- file-backed registry, signature verification, quarantine visibility, and WASM
  execution are future work unless and until they are wired into the gateway

Do not keep language that implies:

- live SKILL.md discovery
- real signature enforcement
- real untrusted WASM execution
- active learned skill proposal flow

## Migration Order

The implementation should land in this order.

### Step 1. Storage and Catalog Foundations

- add `skill_install_state` migration
- add query helpers in `cortex_storage`
- add `skill_catalog` module and compiled definition builder

### Step 2. Bootstrap and AppState Cutover

- construct `SkillCatalogService` during bootstrap
- replace `AppState.safety_skills` with `AppState.skill_catalog`
- add test helpers for empty or fixture catalogs

### Step 3. Runtime Resolution Cutover

- update runtime builder to resolve final skill sets from the catalog
- auto-grant `skill:<name>` policy capabilities for resolved skills
- update CLI chat to use the same skill definition and resolution path

### Step 4. API Cutover

- rewrite `api/skills.rs`
- rewrite `api/skill_execute.rs`
- remove direct SQL and direct `skill.execute(...)` usage from route handlers

### Step 5. Contract Surface Cutover

- update OpenAPI
- regenerate SDK types
- update SDK skill client types
- update dashboard skill UI
- remove hardcoded studio skill count copy
- demote CLI direct mode

### Step 6. Documentation and Drift Cleanup

- update architecture and wiki docs
- remove or demote stale claims about file-backed skill execution

## Compatibility Posture

This wave should be **compatibility-aware but truth-first**.

Recommended posture:

- preserve top-level `installed` and `available` response lists
- preserve route paths
- add truthful fields rather than forcing a full client rewrite at once
- keep `capabilities` as a deprecated alias for one transition window only

Do not preserve:

- fake privilege disclosure
- fake install semantics
- fake direct-mode skill packaging support

## Test Plan

### Gateway Integration Tests

Add a new test file:

- `crates/ghost-gateway/tests/skills_api_tests.rs`

Cover:

- `GET /api/skills` returns truthful installed and available groups
- always-on skills cannot be uninstalled
- uninstalling an installable skill moves it to available
- reinstalling returns it to installed
- executing a write-capable skill succeeds through the execute route
- executing a disabled skill fails with the chosen conflict/error contract

### Runtime Tests

Add or extend tests around:

- `crates/ghost-gateway/src/runtime_safety.rs`

Cover:

- resolved skills receive matching `skill:<name>` grants
- `agent.skills` allowlist actually controls what is callable
- always-on safety skills remain visible
- non-resolved skills are neither registered nor granted

### SDK Tests

Update:

- `packages/sdk/src/__tests__/client.test.ts`

Cover:

- new DTO parsing
- install/uninstall request and response expectations
- compatibility handling for deprecated `capabilities`

### Dashboard Tests

Add:

- `dashboard/tests/skills.spec.ts`

Cover:

- install review shows `privileges`
- always-on skills do not render uninstall actions
- available skills can be installed
- installed skills can be disabled when removable

### CLI Tests

Extend:

- `crates/ghost-gateway/tests/cli_tests.rs`

Cover:

- skill commands require HTTP backend in this wave
- clear error messaging when direct mode is attempted

## Evidence Gates

The work is not complete until all of these are true:

1. `/api/skills` no longer lies about install state
2. install and uninstall change runtime-visible behavior
3. `POST /api/skills/:name/execute` no longer uses `DbPool::read()`
4. no route handler calls `skill.execute(...)` directly from the catalog map
5. runtime skill exposure and policy grants are aligned under test
6. dashboard review shows real privileges instead of only `skill:<name>`
7. docs no longer claim live file-backed or sandboxed support that is not wired

## Deferred Work

These items are intentionally not part of this wave:

- integrating `SkillRegistry` into production discovery
- real Ed25519 signature enforcement for file-backed skills
- quarantine surfaced for file-backed community skills
- real WASM sandbox execution
- workflow recorder to proposal pipeline
- automatic or human-approved learned skill creation

Those should become a separate implementation document after this hardening
wave is complete.
