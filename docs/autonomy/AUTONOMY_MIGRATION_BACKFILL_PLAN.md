# Autonomy Migration And Backfill Plan

## Legacy Inventory

- Live gateway bootstrap had no canonical autonomy runtime before this cut.
- `ghost-heartbeat` existed as a dormant seam and policy library candidate, not a live gateway owner.
- `ghost cron` and `ghost heartbeat status` inferred state from unrelated or nonexistent fields.
- Workflow execution was live.
- Workflow schedule ownership was not live as a durable first-class runtime.

## Ownership Classification

- Canonical owner after this cut:
  - `crates/ghost-gateway/src/autonomy.rs`
- Transitional or demoted:
  - `crates/ghost-gateway/src/periodic.rs`
  - `crates/ghost-heartbeat/src/heartbeat.rs`
  - `crates/ghost-heartbeat/src/cron.rs`

## Implemented Backfill

- On gateway bootstrap, `AutonomyService::reconcile_bootstrap_jobs()` projects one recurring `heartbeat_observe` job per registered agent.
- The heartbeat job id is deterministic:
  - `heartbeat:{agent_id}`
- This preserves due work without creating duplicate heartbeat jobs across restarts.

## Explicit Non-Backfilled Surfaces

- Workflow schedules:
  - not backfilled because the live workflow API does not expose a canonical persisted schedule field.
  - scheduled workflow unification remains a follow-on workstream.
- `ghost-heartbeat` in-memory notions of cadence:
  - not migrated; those values were not authoritative.

## Duplicate-Execution Controls

- The live runtime uses:
  - durable job lease rows
  - durable run rows
  - durable side-effect correlation keys
  - lease epoch rebinding on recovery
- Backfill does not create a second scheduler or timer authority.

## Shadow Coverage In This Cut

- Status shadow:
  - `/api/health.autonomy` is derived from the same service status projection as `/api/autonomy/status`.
- Backfill shadow:
  - bootstrap reconciliation is tested against the durable ledger before runtime start.

## Verification

- `legacy_schedule_backfill_preserves_due_work`
- `health_autonomy_section_matches_runtime_state`
- `due_jobs_dispatch_under_single_owner`
