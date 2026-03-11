# Autonomy State Machine

## Job States

- `queued`
  - Due or future work awaiting lease.
- `leased`
  - Lease acquired by the gateway-owned control plane.
- `running`
  - Side-effect-capable execution has started.
- `waiting`
  - Deferred without losing durable ownership history.
  - Used for approval holds, quiet hours, and retry delays.
- `paused`
  - Explicitly blocked by policy or pullback.
- `quarantined`
  - Blocked by quarantine state.
- `failed`
  - Terminal run result for the current attempt; may remain operator-visible.
- `succeeded`
  - Completed attempt.
- `aborted`
  - Explicit aborted ownership episode.

## Run Transitions Used Live

- `queued -> leased`
- `leased -> running`
- `leased -> waiting`
- `leased -> succeeded`
  - used for durable suppression completion with `side_effect_status = suppressed`
- `leased -> failed`
- `leased -> paused`
- `leased -> quarantined`
- `leased -> aborted`
- `running -> waiting`
- `running -> succeeded`
- `running -> failed`
- `running -> paused`
- `running -> quarantined`
- `running -> aborted`
- `waiting -> queued`
- `waiting -> paused`
- `waiting -> quarantined`
- `waiting -> aborted`

## Pre-Dispatch Decision Point

The live runtime does not move a leased job straight to `running`.

Before `mark_run_running`, the gateway checks:

- effective policy pause
- quarantine state
- access pullback
- quiet hours
- active suppressions
- approval validity
- daily cost budget

Outcomes:

- pause: `leased -> paused`
- quarantine: `leased -> quarantined`
- approval hold / quiet hours: `leased -> waiting`
- suppression: `leased -> succeeded` with `side_effect_status = suppressed`
- allowed: `leased -> running`

## Delivery Semantics

- Dispatch is at-least-once.
- Recovery uses durable lease expiration and run rebinding.
- The implementation does not claim exactly-once side effects.
- Side effects must be correlated by stable durable keys within the declared scope.

## Manual Review

- Poisoned or exhausted work ends in `failed` with:
  - `manual_review_required = true`
  - `side_effect_status = manual_review`
- Manual-review work remains visible in:
  - `/api/autonomy/status`
  - `/api/autonomy/runs`
  - `/api/health.autonomy`
