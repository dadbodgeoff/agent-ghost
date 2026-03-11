# Autonomy SLO And Rollback

## Initial Operating Targets

- Due-job selection lag:
  - target under 5 seconds in the single-gateway deployment mode
- Oldest overdue job age:
  - target under 60 seconds during normal operation
- Lease recovery window:
  - bounded by lease TTL plus scheduler poll interval
- Duplicate run rate:
  - target zero duplicate side effects inside the declared correlation-key scope
- Manual-review visibility:
  - target 100% of exhausted/poisoned work visible in status surfaces

## Rollback Triggers

- sustained dispatcher saturation with blocked due jobs growing without operator explanation
- unexpected duplicate side effects inside the declared durable correlation scope
- autonomy runs entering hidden or non-visible terminal states
- health/autonomy or CLI status drift from durable runtime truth

## Rollback Steps

1. Pause autonomy at the platform scope:
   - `PUT /api/autonomy/policies/global` with `pause = true`
2. Confirm new due work stops advancing to `running`.
3. Inspect:
   - `/api/autonomy/status`
   - `/api/autonomy/jobs`
   - `/api/autonomy/runs`
4. Continue manual operator handling for any `manual_review_required` work.
5. If required, stop the gateway process; lease expiry preserves recovery semantics on restart.

## Post-Rollback Validation

- `/api/autonomy/status.runtime_state` remains truthful
- `paused_jobs` or `waiting_jobs` reflect the rollback state
- no new side effects occur while the global pause remains active
- manual-review counts remain visible

## Truthful Limits

- This cut does not provide a second legacy scheduler to roll back to.
- Rollback means pausing the single control plane and reverting to manual execution paths where needed.
