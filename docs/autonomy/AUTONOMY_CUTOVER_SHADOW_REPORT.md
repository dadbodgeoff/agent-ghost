# Autonomy Cutover And Shadow Report

Date: March 10, 2026

## Scope

This report covers the cut from drifted autonomy-adjacent seams to the gateway-owned autonomy control plane.

## What Was Compared

- bootstrap heartbeat projection versus durable ledger state
- `AutonomyService::status()` versus `/api/health.autonomy`
- durable lease ownership versus double-dispatch attempts on the same due job

## Observed Results

- Bootstrap projection produced one deterministic heartbeat job per registered agent.
- `/api/health.autonomy` matched the service-owned runtime projection.
- A second dispatch contender could not acquire durable ownership while the first lease was live.
- Poisoned work remained visible as manual review instead of disappearing.

## Known Diffs From Older Surfaces

- `ghost heartbeat status`
  - old behavior: guessed from generic health fields
  - new behavior: reads `/api/autonomy/status` and `/api/autonomy/jobs`
- `ghost cron`
  - old behavior: inferred schedules from workflow payloads that were not canonical
  - new behavior: reads `/api/autonomy/jobs` and `/api/autonomy/runs`

## Disposition

- Accepted:
  - removal of implied heartbeat cadence/tier fields from CLI truth surfaces
  - single-gateway leased deployment mode surfaced explicitly
- Deferred:
  - workflow schedule backfill, because no live canonical workflow schedule field exists yet

## Evidence

- `autonomy::tests::legacy_schedule_backfill_preserves_due_work`
- `autonomy::tests::health_autonomy_section_matches_runtime_state`
- `autonomy::tests::due_jobs_dispatch_under_single_owner`
- `autonomy::tests::poison_job_moves_to_manual_review_visible_in_health`
