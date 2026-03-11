# Autonomy Status Surfaces

## Canonical Surfaces

- `GET /api/autonomy/status`
  - owner: `AutonomyService::status()`
- `GET /api/autonomy/jobs`
  - owner: autonomy ledger query layer
- `GET /api/autonomy/runs`
  - owner: autonomy ledger query layer
- `GET /api/health`
  - autonomy section owner: `AutonomyService::status()`

## Field Ownership

- `deployment_mode`
  - source: hard-coded runtime deployment contract
  - current live value: `single_gateway_leased`
- `runtime_state`
  - source: in-memory autonomy runtime health
- `scheduler_running`
  - source: in-memory autonomy runtime health
- `worker_count`
  - source: configured runtime worker pool size
- `due_jobs`, `leased_jobs`, `running_jobs`, `waiting_jobs`, `paused_jobs`, `quarantined_jobs`, `manual_review_jobs`
  - source: live SQL counts over the autonomy ledger
- `oldest_overdue_at`
  - source: live SQL minimum due timestamp over eligible due work
- `last_successful_dispatch_at`
  - source: in-memory runtime health updated after successful dispatch completion
- `saturation.*`
  - source: runtime queue reservation and dispatcher fairness tracking

## CLI Mapping

- `ghost heartbeat status`
  - reads:
    - `/api/autonomy/status`
    - `/api/autonomy/jobs`
- `ghost cron list`
  - reads:
    - `/api/autonomy/jobs`
- `ghost cron history`
  - reads:
    - `/api/autonomy/runs`

## Negative Guarantees

- No CLI field is derived from nonexistent `heartbeat_frequency`, `convergence_tier`, or workflow `schedule` fields.
- No health autonomy field is inferred from logs or dormant timer loops.
- No surface claims exactly-once dispatch.

## Why-Now Visibility

- `GET /api/autonomy/runs` includes `why_now_json`.
- This is the current machine-readable user/operator explanation surface for proactive runs.
