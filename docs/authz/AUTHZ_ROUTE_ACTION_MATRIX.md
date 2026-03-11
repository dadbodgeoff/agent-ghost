# Authz Route Action Matrix

Status: complete on March 9, 2026.

This matrix now reflects the live gateway state. Every non-public HTTP route in
`crates/ghost-gateway/src/route_sets.rs` is bound through
`RouteAuthorizationSpec` in `crates/ghost-gateway/src/api/authz_policy.rs`.

Three authorization shapes are live:

- `MinimumRole`: hierarchical role checks for viewer/operator/admin/superadmin actions
- `SafetyReview`: `admin+` or `operator + safety_review` for quarantine resume
- `OwnerOrAdmin`: resource-owner or `admin+` for live execution inspection

Protected routes are no longer left to handler-local role checks. Handler code
now consumes the route-bound result, and the special owner-aware live execution
path is enforced in dedicated middleware.

| Route ID | Method | Path | Action | Compatibility required |
| --- | --- | --- | --- | --- |
| `live_execution_by_id` | `GET` | `/api/live-executions/:execution_id` | `LiveExecutionRead` | `false` |
| `safety_status` | `GET` | `/api/safety/status` | `SafetyStatusRead` | `false` |
| `safety_pause_agent` | `POST` | `/api/safety/pause/:agent_id` | `SafetyPauseAgent` | `true` |
| `safety_resume_agent` | `POST` | `/api/safety/resume/:agent_id` | `SafetyResumeAgent` | `true` |
| `safety_quarantine_agent` | `POST` | `/api/safety/quarantine/:agent_id` | `SafetyQuarantineAgent` | `true` |
| `safety_kill_all` | `POST` | `/api/safety/kill-all` | `SafetyKillAll` | `true` |
| `admin_backup_create` | `POST` | `/api/admin/backup` | `AdminBackupCreate` | `true` |
| `admin_backup_list` | `GET` | `/api/admin/backups` | `AdminBackupReadList` | `false` |
| `admin_export` | `GET` | `/api/admin/export` | `AdminExportRead` | `false` |
| `admin_restore` | `POST` | `/api/admin/restore` | `AdminRestoreVerify` | `true` |
| `provider_keys` | `GET` | `/api/admin/provider-keys` | `ProviderKeyReadList` | `false` |
| `provider_keys` | `PUT` | `/api/admin/provider-keys` | `ProviderKeyWrite` | `true` |
| `provider_key_by_env_name` | `DELETE` | `/api/admin/provider-keys/:env_name` | `ProviderKeyDelete` | `true` |
| `pc_control_status` | `GET` | `/api/pc-control/status` | `PcControlStatusRead` | `false` |
| `pc_control_status` | `PUT` | `/api/pc-control/status` | `PcControlStatusWrite` | `true` |
| `pc_control_actions` | `GET` | `/api/pc-control/actions` | `PcControlActionRead` | `false` |

Coverage note:

- the table highlights the historically risky or non-trivial routes
- the full protected surface remains enumerated in [AUTHZ_ACTION_INVENTORY.md](./AUTHZ_ACTION_INVENTORY.md)
- route mounting now fails closed because `route_sets.rs` resolves every protected route through `route_spec_for(...)`
