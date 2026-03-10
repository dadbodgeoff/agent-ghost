# Autonomy Control Plane Tasks

Status: Proposed on March 10, 2026

Objective: execute the autonomy control-plane design in
[design.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/design.md)
without creating another half-live scheduling system.

This file is the execution tracker for the autonomy control plane. If this file
conflicts with the design on ownership, invariants, or end-state semantics, the
design wins.

Implementation note for the live cut:

- deployment mode is `single_gateway_leased`
- quiet-hours enforcement is shipped for `UTC` and fixed UTC offsets
- approval is shipped as per-run TTL-bound approval with revalidation at dispatch

## Engineering Standard

This work is held to the following bar:

- No new background timer may directly invoke the agent loop outside the
  control plane.
- No new autonomy status surface may report inferred or placeholder state.
- No second schedule authority may be introduced beside the control plane.
- No autonomous execution path may bypass pause, quarantine, kill switch,
  capability pullback, cost ceilings, or initiative budgets.
- No schedule semantics may remain implicit. Overlap, catch-up, retry, and
  idempotency must be explicit.
- No autonomy implementation may imply exactly-once execution where the real
  guarantee is at-least-once plus idempotency.
- No exhausted retry path or runtime self-disable may silently drop due work.
- No "tiered heartbeat" claims may remain unless the runtime actually executes
  by tier.
- No `HEARTBEAT.md` runtime contract may ship unless the runtime genuinely
  loads and enforces it.
- No docs, CLI, or dashboard control may imply autonomy behavior that is not
  live in the gateway.
- Reuse existing durable ownership patterns already present in the repo unless
  a written incompatibility requires deviation.
- Keep `main` releasable after every merge.

## Confirmed Gaps

These are already evidenced in the design audit and should be treated as real,
not hypothetical:

1. The live gateway does not start the heartbeat engine or cron engine.
2. The `ghost-heartbeat` crate exists, but the runtime does not use it.
3. Heartbeat tiering exists in code but not in the actual fire path.
4. Heartbeat config fields exist but are not consumed by live runtime wiring.
5. Cron timezone handling is declared but not implemented.
6. Cron semantics are too weak for production scheduling.
7. Heartbeat CLI status is not backed by real health data.
8. Session heartbeat and autonomous heartbeat are conflated by name.
9. `HEARTBEAT.md` is referenced by the runtime model but absent in the active
   environment.
10. Workflows are already a second scheduling-adjacent runtime, creating a real
    split-authority risk.

## Required Artifacts

The following artifacts must exist by the end of this program:

1. `docs/autonomy/AUTONOMY_BOUNDARY_INVENTORY.md`
2. `docs/autonomy/AUTONOMY_JOB_SCHEMA.md`
3. `docs/autonomy/AUTONOMY_STATE_MACHINE.md`
4. `docs/autonomy/AUTONOMY_MIGRATION_BACKFILL_PLAN.md`
5. `docs/autonomy/AUTONOMY_CUTOVER_SHADOW_REPORT.md`
6. `docs/autonomy/AUTONOMY_STATUS_SURFACES.md`
7. `docs/autonomy/AUTONOMY_SLO_AND_ROLLBACK.md`
8. `docs/autonomy/AUTONOMY_PRIVACY_RETENTION.md`
9. `docs/autonomy/AUTONOMY_ROLLOUT_CHECKLIST.md`
10. `AUTONOMY_CONTROL_PLANE_TASKS.md`

## Phase Gates

The work may not advance past a gate until the gate criteria are met.

### Gate 1: Authority Freeze

- all existing autonomy-adjacent runtime seams are inventoried
- no new direct timer-to-agent-loop path is introduced
- design and task docs are accepted as the source of truth

### Gate 2: Ledger Gate

- durable job/run/lease storage exists
- the control plane owns due-job selection
- health surfaces report live autonomy state, not placeholders

### Gate 3: Shadow Gate

- migration and backfill rules are written
- shadow comparison exists for selection/status behavior
- rollback criteria and operator steps exist before old seams are retired

### Gate 4: Schedule Gate

- schedule semantics are explicit
- overlap, catch-up, and retry are durable and tested
- workflow scheduling no longer has split authority

### Gate 5: Heartbeat Gate

- heartbeat tiers are real execution modes
- default heartbeat does not invoke a full agent turn
- initiative budgets are enforced before H3 escalation

### Gate 6: Trust Gate

- user-facing why-now explanations exist
- suppress, pause, and quiet-hours controls are real
- external/autonomous actions can be downgraded to draft or proposal

## Workstreams

## Workstream A: Boundary Freeze and Inventory

Goal: prevent more drift while the control plane is being built.

Tasks:

1. Produce `AUTONOMY_BOUNDARY_INVENTORY.md` with:
   - runtime owners
   - timer loops
   - schedule-like APIs
   - workflow schedule surfaces
   - heartbeat-like surfaces
   - CLI/status surfaces
2. Mark each surface as:
   - `canonical`
   - `transitional`
   - `drifted`
   - `dead`
3. Freeze new autonomy-adjacent runtime entry points until the control plane
   exists.
4. Demote or label any misleading CLI or docs surfaces if immediate correction
   is cheap.

Primary files to inspect or touch:

- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/periodic.rs`
- `crates/ghost-gateway/src/api/health.rs`
- `crates/ghost-gateway/src/cli/heartbeat.rs`
- `crates/ghost-gateway/src/cli/cron.rs`
- `crates/ghost-heartbeat/src/heartbeat.rs`
- `crates/ghost-heartbeat/src/cron.rs`
- `crates/ghost-gateway/src/api/workflows.rs`

Acceptance criteria:

- every autonomy-like surface has an explicit owner
- there is one written basis for rejecting new scheduler drift
- no new hidden timers land during implementation

Verification:

- `rg -n "tokio::spawn|interval\\(|sleep\\(" crates/ghost-gateway/src crates/ghost-heartbeat/src`
- `rg -n "heartbeat|cron|schedule" crates/ghost-gateway/src`

## Workstream B: Durable Ledger and State Machine

Goal: establish one durable control-plane ledger and one run lifecycle.

Tasks:

1. Add storage schema for:
   - `autonomy_jobs`
   - `autonomy_runs`
   - `autonomy_leases`
   - `autonomy_suppressions`
   - `autonomy_policies`
   - `autonomy_notifications`
2. Define typed storage queries for:
   - insert job
   - select due jobs
   - acquire lease
   - renew lease
   - finish run
   - reschedule
   - suppress
   - pause/quarantine visibility
3. Define the canonical state machine:
   - `queued`
   - `leased`
   - `running`
   - `waiting`
   - `succeeded`
   - `failed`
   - `paused`
   - `quarantined`
   - `aborted`
4. Define durable idempotency semantics per job type.
5. Define the lease contract:
   - lease owner identity
   - lease duration and renewal
   - expiry recovery
   - current deployment invariant for single-process versus safe multi-process
     execution
6. Version autonomy payloads and schedule specs.
7. Reuse or intentionally extend the repo's existing `owner_token`,
   `lease_epoch`, and versioned-state patterns from `operation_journal` and
   `workflow_executions`.
8. Define side-effect correlation rules for externally visible actions.

Primary files to add or modify:

- `crates/cortex/cortex-storage/src/migrations/`
- `crates/cortex/cortex-storage/src/queries/`
- `crates/cortex/cortex-storage/src/schema_contract.rs`
- `crates/cortex/cortex-storage/tests/`
- `docs/autonomy/AUTONOMY_JOB_SCHEMA.md`
- `docs/autonomy/AUTONOMY_STATE_MACHINE.md`

Acceptance criteria:

- due work survives gateway restart
- lease and run state survive gateway restart
- run state transitions are explicit and auditable
- no in-memory-only source of truth remains for autonomous job ownership
- lease ownership and recovery semantics are explicit enough to prevent
  accidental duplicate dispatch under the supported deployment model
- payload and schedule contracts are versioned
- external side effects have correlation keys or equivalent idempotent
  ownership within the declared scope

Required tests:

- `autonomy_job_insert_and_select_due`
- `autonomy_lease_acquire_is_exclusive`
- `autonomy_lease_expiry_allows_recovery`
- `autonomy_lease_owner_identity_is_persisted`
- `autonomy_payload_schema_version_is_required`
- `autonomy_run_transition_matrix_valid`
- `autonomy_idempotency_scope_blocks_duplicate_dispatch`

## Workstream C: Control Plane Runtime

Goal: create the single runtime that owns due-job polling, leasing, and dispatch.

Tasks:

1. Add a gateway-owned autonomy runtime module.
2. Move due-job polling into this runtime.
3. Make bootstrap start exactly one autonomy runtime.
4. Stop presenting `PeriodicTaskScheduler` as the owner of autonomy work.
5. Track runtime health:
   - scheduler state
   - dispatcher state
   - due jobs
   - leased jobs
   - terminal/manual-review jobs
   - oldest overdue job
   - last successful dispatch
   - dispatcher backpressure or saturation state
6. Define bounded dispatch behavior:
   - global concurrency
   - per-agent or per-tenant fairness
   - what happens when the dispatcher is saturated
7. Ensure repeated runtime failures cannot silently disable autonomy ownership;
   terminal work must remain visible for operator action.

Primary files to add or modify:

- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/periodic.rs` or new `src/autonomy/`
- `crates/ghost-gateway/src/api/health.rs`

Acceptance criteria:

- the control plane is the only owner of autonomous dispatch
- bootstrap starts the runtime exactly once
- kill switch cleanly stops the runtime
- health surfaces reflect real autonomy state
- dispatcher saturation is visible and bounded
- exhausted retries or poison jobs move to visible terminal/manual-review state
  instead of disappearing

Required tests:

- `bootstrap_starts_one_autonomy_runtime`
- `kill_switch_stops_autonomy_runtime`
- `due_jobs_dispatch_under_single_owner`
- `health_endpoint_reports_real_autonomy_state`
- `dispatcher_respects_per_agent_concurrency_limit`
- `poison_job_moves_to_manual_review_visible_in_health`

## Workstream D: Migration, Backfill, Shadow, And Rollback

Goal: make cutover safe, observable, and reversible instead of folklore.

Tasks:

1. Produce `AUTONOMY_MIGRATION_BACKFILL_PLAN.md` with:
   - legacy state inventory
   - state ownership classification
   - backfill rules
   - explicit non-migrated surfaces and why
2. Add code paths or test fixtures needed to project legacy workflow/schedule
   state into the new ledger without duplicate execution.
3. Add shadow-mode comparison for:
   - due-job selection
   - next-fire computation
   - autonomy status reporting
4. Produce `AUTONOMY_CUTOVER_SHADOW_REPORT.md` with observed diffs and their
   disposition.
5. Produce `AUTONOMY_SLO_AND_ROLLBACK.md` with:
   - rollout thresholds
   - rollback triggers
   - rollback operator steps
   - post-rollback validation

Primary files to add or modify:

- autonomy runtime module
- storage schema and query layer
- `docs/autonomy/AUTONOMY_MIGRATION_BACKFILL_PLAN.md`
- `docs/autonomy/AUTONOMY_CUTOVER_SHADOW_REPORT.md`
- `docs/autonomy/AUTONOMY_SLO_AND_ROLLBACK.md`

Acceptance criteria:

- migrated state does not silently drop due work
- shadow mode compares behavior without duplicate side effects
- rollback thresholds are explicit before old seams are deleted
- cutover ownership is operator-visible and testable

Required tests:

- `legacy_schedule_backfill_preserves_due_work`
- `shadow_selector_reports_diff_without_side_effects`
- `shadow_status_surface_matches_live_runtime_projection`
- `lease_recovery_after_write_failure_requeues_job`
- `concurrent_lease_contenders_do_not_double_dispatch`
- `rollback_restores_single_execution_authority`

## Workstream E: Honest Operator and CLI Surfaces

Goal: make autonomy status observable and truthful.

Tasks:

1. Replace placeholder heartbeat status derivation from `/api/health`.
2. Add dedicated autonomy status payloads or a truthful autonomy section in
   `/api/health`.
3. Rework `ghost heartbeat status` to read actual runtime state.
4. Rework `ghost cron` so it reports the control-plane schedule ledger rather
   than inferring from unrelated workflow fields.
5. Document the exact owner of each operator-facing status field.
6. Ensure operator status exposes terminal/manual-review counts and dispatcher
   saturation instead of burying them in logs.

Primary files to modify:

- `crates/ghost-gateway/src/api/health.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/cli/heartbeat.rs`
- `crates/ghost-gateway/src/cli/cron.rs`
- `packages/sdk`
- `docs/autonomy/AUTONOMY_STATUS_SURFACES.md`

Acceptance criteria:

- every displayed field maps to live runtime state
- no CLI defaults hide missing server fields
- heartbeat and schedule status no longer rely on fake or implied data
- terminal/manual-review and backpressure state are visible to operators
- gateway route contracts, OpenAPI, and SDK types agree for autonomy status
  surfaces

Required tests:

- `heartbeat_cli_fails_if_server_omits_required_status_fields`
- `health_autonomy_section_matches_runtime_state`
- `cron_cli_lists_control_plane_jobs_not_inferred_workflows`

## Workstream F: Scheduling Semantics

Goal: move from toy cron behavior to explicit scheduling semantics.

Tasks:

1. Define canonical schedule fields:
   - timezone
   - overlap policy
   - missed-run policy
   - retry policy
   - max runtime
   - jitter
2. Implement overlap policies:
   - `allow`
   - `forbid`
   - `replace`
   - `queue_one`
3. Implement missed-run policies:
   - `skip`
   - `catch_up_one`
   - `catch_up_all_with_cap`
   - `reschedule_from_now`
4. Implement retry policy fields:
   - attempts
   - max retry duration
   - backoff
   - min backoff
   - max backoff
   - retryable classes
5. Define missed-tick behavior explicitly for runtime timers so catch-up
   semantics are deliberate.

Primary files to add or modify:

- control-plane runtime module
- storage schema and query layer
- `docs/autonomy/AUTONOMY_JOB_SCHEMA.md`
- `docs/autonomy/AUTONOMY_STATE_MACHINE.md`

Acceptance criteria:

- no autonomous schedule relies on implicit timer semantics
- overlap behavior is explicit and tested
- missed runs are handled according to job policy
- retries are durable and bounded

Required tests:

- `schedule_overlap_forbid_blocks_second_run`
- `schedule_overlap_replace_aborts_prior_run`
- `missed_run_skip_drops_old_fire`
- `missed_run_catch_up_one_enqueues_once`
- `retry_backoff_reschedules_with_bounds`

## Workstream G: Workflow Unification

Goal: prevent workflows and cron from becoming permanent competing authorities.

Tasks:

1. Decide the ownership rule:
   - workflows own graph execution
   - the control plane owns scheduling and retries
2. Add workflow-trigger job types to the control-plane ledger.
3. Route scheduled workflow execution through the control plane.
4. Remove or demote any duplicate schedule semantics in workflow-only paths.
5. Make workflow execution history correlate with autonomy job and run IDs.

Primary files to modify:

- `crates/ghost-gateway/src/api/workflows.rs`
- autonomy runtime module
- storage schema and query layer

Acceptance criteria:

- workflow schedules are not a second scheduling runtime
- scheduled workflow runs are visible in the autonomy ledger
- retries and overlap semantics are shared with the rest of the control plane

Required tests:

- `scheduled_workflow_enqueues_control_plane_job`
- `workflow_run_records_autonomy_job_correlation`
- `workflow_schedule_does_not_double_fire_on_restart`

## Workstream H: Heartbeat Redesign

Goal: make heartbeat a real tiered observation system instead of a disguised
full-turn loop.

Tasks:

1. Split heartbeat into:
   - H0 liveness
   - H1 state diff
   - H2 reasoning snapshot
   - H3 full agent turn
2. Make the default heartbeat path stop before H3 unless escalation is
   justified.
3. Use the tier selector in the actual execution path.
4. Persist heartbeat observations and escalation reasons.
5. Demote `HEARTBEAT.md` from runtime authority.
6. If `HEARTBEAT.md` remains, make it generated or explicitly explanatory.

Primary files to modify:

- `crates/ghost-heartbeat/src/heartbeat.rs`
- `crates/ghost-heartbeat/src/tiers.rs`
- autonomy runtime module
- `docs/autonomy/AUTONOMY_STATUS_SURFACES.md`

Acceptance criteria:

- H0/H1/H2 do not run full agent turns
- H3 only fires with recorded justification
- heartbeat tier claims match actual runtime behavior
- runtime no longer depends on a nonexistent `HEARTBEAT.md`

Required tests:

- `heartbeat_h0_updates_liveness_without_agent_turn`
- `heartbeat_h1_persists_state_delta_without_agent_turn`
- `heartbeat_h2_can_enqueue_followup_without_agent_turn`
- `heartbeat_h3_requires_explicit_escalation_reason`
- `heartbeat_tier_selector_controls_runtime_path`

## Workstream I: Initiative Budgets and Policy

Goal: make proactive behavior governed, not just scheduled.

Tasks:

1. Add initiative budgets:
   - cost
   - risk
   - interruption
   - novelty
   - trust
2. Evaluate those budgets before any autonomous run reaches the agent loop.
3. Define downgrade semantics:
   - act -> propose
   - propose -> draft
   - draft -> observe
   - observe -> suppress
4. Integrate pause, quarantine, capability pullback, convergence state, and
   cost tracking into the same evaluation seam.
5. Store structured `why_now` and downgrade reasoning.

Primary files to add or modify:

- autonomy runtime module
- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-gateway/src/state.rs`
- storage schema and query layer

Acceptance criteria:

- autonomous jobs lose power immediately when the agent is pulled back
- initiative overages downgrade behavior before execution
- every autonomous run has a machine-readable explanation

Required tests:

- `initiative_budget_downgrades_notify_to_draft`
- `capability_pullback_blocks_autonomous_tool_use`
- `quarantine_prevents_due_job_dispatch`
- `why_now_record_written_for_each_autonomous_run`

## Workstream J: User Trust, Approval, And Privacy Controls

Goal: expose autonomy in a way users can understand and control.

Tasks:

1. Add user-visible controls for:
   - pause autonomy
   - pause one agent
   - quiet hours
   - suppress similar future actions
   - draft-only mode
   - approval-required mode
2. Surface why a run happened and what changed.
3. Track suppressions and use them to decay trust budgets.
4. Define and implement approval semantics for:
   - what is blocked before approval
   - approval expiry
   - approval scope
   - revalidation on delayed approval
5. Add retention, redaction, and export rules for autonomy artifacts:
   - `why_now`
   - suppressions
   - approval history
   - proactive audit trails
6. Produce `AUTONOMY_PRIVACY_RETENTION.md`.
7. Add reversible handling where practical:
   - cancel pending
   - rollback delivery where supported
   - disable future similar runs

Primary files to modify:

- `dashboard`
- `packages/sdk`
- gateway API routes and types
- `crates/ghost-gateway/src/api/openapi.rs`
- storage schema and query layer
- `docs/autonomy/AUTONOMY_PRIVACY_RETENTION.md`

Acceptance criteria:

- users can understand why a proactive action happened
- users can suppress future similar actions
- suppression changes future control-plane decisions
- approval-required mode blocks side effects until approval is valid
- delayed approval revalidates policy and budget before execution
- retention behavior for autonomy artifacts is documented and enforced where
  applicable

Required tests:

- `user_can_pause_autonomy_from_ui`
- `user_suppression_reduces_future_initiative`
- `why_now_payload_renders_in_dashboard`
- `approval_required_job_blocks_side_effects`
- `expired_approval_cannot_execute_job`
- `delayed_approval_revalidates_policy_and_budget`

## Workstream K: Cleanup and Contract Tightening

Goal: remove misleading transitional surfaces after the control plane is live.

Tasks:

1. Delete or demote dead cron/heartbeat status seams.
2. Remove references that imply runtime `HEARTBEAT.md` authority if that model
   is not retained.
3. Remove duplicate scheduling claims from docs and CLI.
4. Ensure all surviving autonomy docs reflect live ownership, cutover truth,
   and approval/privacy behavior.
5. Complete the rollout checklist artifact.
6. Remove any shadow-only compatibility seam that would create permanent split
   authority after cutover.

Acceptance criteria:

- the autonomy surface is smaller and more truthful than before
- no surviving operator surface implies dead runtime behavior
- the rollout checklist is complete and auditable

## Phase Order

There is one correct order:

1. Workstream A
2. Workstream B
3. Workstream C
4. Workstream D
5. Workstream E
6. Workstream F
7. Workstream G
8. Workstream H
9. Workstream I
10. Workstream J
11. Workstream K

Do not start with UI controls or heartbeat polish. The first hard work is
ledger, ownership, and truthful runtime state.

## Merge Gates

Every PR in this program must satisfy all of:

- targeted crate tests pass
- any new storage migration has rollback-safe validation
- no new timer path bypasses the control plane
- no new status field is sourced from placeholder data
- no new schedule-like surface lacks explicit overlap, retry, and catch-up
  semantics
- no autonomy PR introduces a second execution authority
- no old seam is deleted before replacement status surfaces and rollback notes
  exist
- migration, shadow, and rollback artifacts are updated for any cutover change
- no autonomy PR widens lease or idempotency semantics without explicit schema
  and contract updates

## Exit Criteria

This task is complete only when:

- one durable autonomy control plane exists
- heartbeat, scheduling, retries, and workflow triggers all route through it
- migration/backfill and rollback procedures are written, truthful, and proven
- supported lease semantics are explicit for the deployment model in use
- delivery semantics are explicit: at-least-once dispatch with durable
  idempotency and visible terminal/manual-review handling
- heartbeat tiers are real execution modes
- status surfaces are truthful
- initiative budgets govern proactive behavior
- user controls for pause, suppress, why-now, and approval are live
- privacy and retention behavior for autonomy artifacts is defined and shipped
- obsolete autonomy drift is removed from the repo
