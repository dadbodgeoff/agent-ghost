# Autonomy Job Schema

## Durable Tables

- `autonomy_jobs`
  - Canonical schedule owner.
  - One row per durable autonomy job.
  - Live job states: `queued`, `leased`, `running`, `waiting`, `paused`, `quarantined`, `failed`, `succeeded`, `aborted`.
- `autonomy_runs`
  - One row per dispatch attempt / durable ownership episode.
  - Carries `why_now_json`, approval state, owner token, lease epoch, side-effect correlation key, result payload, and terminal/manual-review markers.
- `autonomy_leases`
  - Single active lease per job.
  - Ownership fields: `owner_identity`, `owner_token`, `lease_epoch`, `lease_expires_at`.
- `autonomy_policies`
  - Stored policy documents for `platform/global` and `agent/{agent_id}` scopes.
- `autonomy_suppressions`
  - Stored proactive-behavior suppressions keyed by scope + fingerprint.
- `autonomy_notifications`
  - Durable notification/draft/manual-review records keyed by correlation key.

## Versioned Payloads

- Job payloads are versioned with `payload_version`.
- Schedule specs are versioned with `schedule_version`.
- The current live schedule document is `AutonomyScheduleSpec { version: 1 }`.

## Live Job Types

- `heartbeat_observe`
  - Bootstrap-projected recurring observation job per registered agent.
- `workflow_trigger`
  - Scheduled workflow dispatch owned by the autonomy control plane.
- `notification_delivery`
  - Durable draft/manual-review notification delivery work.

## Live Schedule Semantics

- The runtime executes `kind = "interval"` schedules today.
- `cron`, `timezone`, `jitter_seconds`, and `max_runtime_seconds` are persisted in schema, but only `timezone` inside quiet-hours policy is enforced live in this cut.
- Quiet-hours timezone support is explicit:
  - `UTC`
  - fixed UTC offsets such as `-05:00` or `+01:00`

## Approval Policy Values

- Allowed persisted values:
  - `none`
  - `external_only`
  - `always`
- Live approval behavior:
  - any job with `approval_policy != none`, or any effective policy with `approval_required = true`, is held in `waiting` until a valid approval exists.
  - approval scope is one run.
  - approval validity is bounded by `approval_expires_at`.
  - delayed approval is revalidated against current policy and budget before execution.

## Side-Effect Status Values

- `not_started`
- `prepared`
- `applied`
- `manual_review`
- `failed`
- `suppressed`
- `aborted`

## Idempotency Scope

- The live control plane guarantees at-least-once dispatch with durable idempotency.
- The durable correlation key is `side_effect_correlation_key`.
- The schema enforces uniqueness for non-null side-effect correlation keys.
- Duplicate suppression / recovery behavior is driven by:
  - durable run reuse via correlation key
  - lease epoch ownership
  - explicit run-owner rebinding on recovery

## Live Policy Precedence

- `agent/{agent_id}` policy fully overrides `platform/global` when present.
- If no agent policy exists, the runtime falls back to `platform/global`.
- If neither exists, the runtime uses `AutonomyPolicyDocument::default()`.
