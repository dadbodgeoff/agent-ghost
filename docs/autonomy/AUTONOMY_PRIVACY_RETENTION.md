# Autonomy Privacy And Retention

## Stored Autonomy Artifacts

- `why_now_json` in `autonomy_runs`
- suppression records in `autonomy_suppressions`
- approval state and expiry in `autonomy_runs`
- durable notification payloads in `autonomy_notifications`
- failure diagnostics in `autonomy_runs`

## Current Live Controls

- retention preference lives in `AutonomyPolicyDocument.retention_days`
- platform and agent policies are writable through:
  - `PUT /api/autonomy/policies/global`
  - `PUT /api/autonomy/policies/agents/{agent_id}`
- suppressions are writable through:
  - `POST /api/autonomy/suppressions`

## Current Live Enforcement

- approval gating is enforced before side effects
- suppressions are enforced before side effects
- no autonomous side effect bypasses pause, quarantine, pullback, or approval hold

## Explicit Current Gaps

- automated retention deletion is not yet shipped in this cut
- automated redaction/export rules for autonomy artifacts are not yet shipped as dedicated endpoints

Those gaps are intentional documentation, not hidden behavior.

## Operator Rule

- Do not assume autonomy artifacts self-expire today.
- Use policy configuration as the durable retention contract source.
- Treat DB export and backup flows as containing autonomy artifacts unless explicitly filtered.
