# Authz Shadow Diff Report

Status: closed on March 9, 2026.

This artifact records the cutover evidence that was gathered before the legacy
authorizer was deleted. The final runtime no longer has a shadow mode; the
typed authorizer is the only live authorization path.

Evidence captured during the cutover:

| Action | Route ID | Principal summary | Historical result | Status |
| --- | --- | --- | --- | --- |
| `AdminBackupCreate` | `admin_backup_create` | `admin` | legacy and typed decisions matched | closed |
| `SafetyResumeAgent` | `safety_resume_agent` | legacy `security_reviewer` | legacy and typed decisions matched after capability normalization | closed |
| `LiveExecutionRead` | `live_execution_by_id` | owner subject | legacy and typed decisions matched for owner-visible reads | closed |

Notes:

- this report is based on the equivalence tests added during the remediation, not on long-lived production telemetry
- no unexplained mismatch remained in the covered cutover set when the legacy evaluator was removed

## Exit Gate

This report is complete now that:

- the typed authorizer is the only runtime authorizer
- no unexplained mismatch remains in the cutover evidence set
- any remaining compatibility behavior is isolated to claim normalization, not duplicate authorizers
