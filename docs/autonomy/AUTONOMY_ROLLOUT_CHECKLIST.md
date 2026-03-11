# Autonomy Rollout Checklist

## Before Enabling

- Apply migrations through schema version 59.
- Verify schema readiness.
- Confirm `GET /api/autonomy/status` responds.
- Confirm `GET /api/health` contains the `autonomy` section.

## Runtime Verification

- Confirm deployment mode reads `single_gateway_leased`.
- Confirm heartbeat bootstrap jobs exist for registered agents.
- Confirm blocked due jobs and manual-review counts are visible.
- Confirm CLI reads:
  - `ghost heartbeat status`
  - `ghost cron list`
  - `ghost cron history`

## Policy Verification

- Set `pause = true` at `platform/global` and confirm jobs stop reaching `running`.
- Approve one held run and confirm execution only proceeds when policy still allows it.
- Create one suppression and confirm a matching future run is suppressed.

## Recovery Verification

- Restart the gateway and confirm durable jobs remain visible.
- Confirm lease recovery does not produce duplicate side effects inside the declared correlation scope.

## Sign-Off

- Rust checks/tests green for autonomy runtime and storage
- SDK typecheck green
- docs under `docs/autonomy/` match the delivered behavior
