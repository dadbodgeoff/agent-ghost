# Autonomy Boundary Inventory

Status: live inventory for Gate 1 on March 10, 2026

Purpose: identify every autonomy-adjacent seam in the repo, classify its
ownership status, and freeze new drift until the autonomy control plane owns
durable scheduling, retries, and proactive execution.

## Classification Rules

- `canonical`: the long-term owner for the behavior named here.
- `transitional`: a live surface that remains during cutover but is not the
  final authority.
- `drifted`: a live or shipped surface that implies ownership or behavior the
  runtime does not actually have.
- `dead`: code or contract surface that exists on disk but is not wired into
  the live gateway runtime.

## Runtime Owners

| Surface | File | Current status | Notes |
| --- | --- | --- | --- |
| Gateway bootstrap background task owner | [crates/ghost-gateway/src/bootstrap.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs) | `canonical` | The only live bootstrap authority for long-running gateway tasks. |
| Gateway runtime lifecycle owner | [crates/ghost-gateway/src/runtime.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/runtime.rs) | `canonical` | Owns tracked task startup, shutdown, and cancellation. |
| Periodic task scheduler | [crates/ghost-gateway/src/periodic.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/periodic.rs) | `transitional` | Live helper for non-autonomy maintenance work only. Must not become a second autonomy owner. |
| Backup scheduler loop | [crates/ghost-gateway/src/backup_scheduler.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/backup_scheduler.rs) | `canonical` | Canonical only for backup maintenance. Not an autonomy execution path. |
| Workflow execution runtime | [crates/ghost-gateway/src/api/workflows.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/workflows.rs) | `transitional` | Canonical for workflow graph execution once invoked, but not a schedule owner. |
| Operation lease heartbeat | [crates/ghost-gateway/src/api/idempotency.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/idempotency.rs) | `canonical` | Canonical only for request mutation lease renewal, not autonomous heartbeat behavior. |

## Timer Loops And Pollers

| Surface | File | Current status | Notes |
| --- | --- | --- | --- |
| Periodic task loop | [crates/ghost-gateway/src/periodic.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/periodic.rs) | `transitional` | Live timer loop; freeze scope to maintenance tasks. |
| Backup scheduler loop | [crates/ghost-gateway/src/backup_scheduler.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/backup_scheduler.rs) | `canonical` | Not an agent/autonomy path. |
| Operation lease renewal timer | [crates/ghost-gateway/src/api/idempotency.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/idempotency.rs) | `canonical` | Request-lifecycle only. |
| `ghost-heartbeat` heartbeat engine | [crates/ghost-heartbeat/src/heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/heartbeat.rs) | `dead` | Not started by live gateway bootstrap. |
| `ghost-heartbeat` cron engine | [crates/ghost-heartbeat/src/cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/cron.rs) | `dead` | Not started by live gateway bootstrap. |
| Client session heartbeat keepalive | [crates/ghost-gateway/src/api/sessions.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/sessions.rs) | `canonical` | Canonical only for frontend/runtime session liveness. |

## Schedule-Like APIs And Models

| Surface | File | Current status | Notes |
| --- | --- | --- | --- |
| Workflow CRUD API | [crates/ghost-gateway/src/api/workflows.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/workflows.rs) | `canonical` | Canonical for workflow definitions and explicit execution requests. |
| Workflow execution history API | [crates/ghost-gateway/src/api/workflows.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/workflows.rs) | `canonical` | Canonical for historical workflow execution rows. |
| Workflow schedule fields implied by CLI | [crates/ghost-gateway/src/cli/cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/cron.rs) | `drifted` | The workflow API does not own `schedule`, `last_run`, or `next_run` fields today. |
| Agent template heartbeat interval | [crates/ghost-gateway/src/agents/templates.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/agents/templates.rs) | `transitional` | Usable as policy input for future autonomy bootstrap, but not a live runtime scheduler. |
| `ghost-heartbeat` cron YAML model | [crates/ghost-heartbeat/src/cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/cron.rs) | `dead` | Exists on disk; no live owner. |

## Heartbeat-Like Surfaces

| Surface | File | Current status | Notes |
| --- | --- | --- | --- |
| Session heartbeat route `/api/sessions/:id/heartbeat` | [crates/ghost-gateway/src/api/sessions.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/sessions.rs) | `canonical` | Canonical client keepalive, not agent autonomy. |
| `ghost heartbeat status` CLI | [crates/ghost-gateway/src/cli/heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/heartbeat.rs) | `drifted` | Reads nonexistent `/api/health` fields. |
| `ghost-heartbeat` engine fire path | [crates/ghost-heartbeat/src/heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/heartbeat.rs) | `dead` | Not bootstrapped live; its tier claims are not runtime truth. |
| `HEARTBEAT.md` runtime contract | [crates/ghost-heartbeat/src/heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/heartbeat.rs) | `drifted` | Referenced by message text but not present as a live enforced contract. |

## CLI, API, SDK, And Docs Status Surfaces

| Surface | File | Current status | Notes |
| --- | --- | --- | --- |
| `/api/health` | [crates/ghost-gateway/src/api/health.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/health.rs) | `canonical` | Canonical health endpoint, but it currently lacks a truthful autonomy section. |
| OpenAPI session heartbeat name | [crates/ghost-gateway/src/api/openapi.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/openapi.rs) | `canonical` | Correctly documents runtime session heartbeat, not autonomy heartbeat. |
| `ghost heartbeat status` help and output | [crates/ghost-gateway/src/cli/heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/heartbeat.rs) | `drifted` | Implies live engine state that the server does not expose. |
| `ghost cron list/history` | [crates/ghost-gateway/src/cli/cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/cron.rs) | `drifted` | Infers cron ownership from unrelated workflow and audit fields. |
| README heartbeat/cron claims | [README.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/README.md) | `drifted` | Describes tiered heartbeat ownership that is not live in gateway bootstrap. |
| Dashboard approvals page | [dashboard/src/routes/approvals/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/approvals/+page.svelte) | `canonical` | Canonical for goal proposals today; may be extended for autonomy approvals rather than replaced. |

## Ownership Freeze

Until the autonomy control plane is live:

1. No new background timer may directly invoke an agent turn, workflow
   execution, or proactive side effect outside the control-plane runtime.
2. `PeriodicTaskScheduler` may not take on autonomy work.
3. Workflow APIs may define graphs and explicit executions, but scheduling and
   retries must converge into the autonomy ledger.
4. CLI, API, SDK, and dashboard surfaces must either read live autonomy state
   or fail closed; they may not infer placeholder scheduler state.
5. `ghost-heartbeat` remains a policy/observation library candidate, not a live
   second runtime owner.

## Immediate Corrections

- Treat `ghost heartbeat status` and `ghost cron` as drifted until they read a
  real autonomy status surface.
- Treat `/api/sessions/:id/heartbeat` as client liveness only in code, docs, and
  operator language.
- Treat `HEARTBEAT.md` as non-authoritative until the runtime either generates
  it or removes it from executable assumptions.
