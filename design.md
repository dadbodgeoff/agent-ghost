# Autonomy Control Plane Design

Status: proposed on March 10, 2026
Scope: next major system after gateway/auth/runtime hardening
Supersedes: the previous top-level "remaining hardening phase" design as the primary repo-level design document
Execution tracker: [AUTONOMY_CONTROL_PLANE_TASKS.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/AUTONOMY_CONTROL_PLANE_TASKS.md)

## Document Intent

This document defines the next system to build after the gateway, auth,
capability, runtime-safety, and quarantine foundations are in place.

The goal is not "more automation."

The goal is one trustworthy autonomy control plane that can run:

- ambient heartbeats
- precise schedules
- workflow triggers
- retries
- escalations
- deferred follow-ups
- user-visible proactive actions

without creating a second unsafe product inside the first one.

This document is intentionally grounded in the live repo, not only the
aspirational architecture documents.

## Implementation Refinements

As implemented in the current cut:

- the live autonomy deployment mode is `single_gateway_leased`
- quiet-hours enforcement supports `UTC` and fixed UTC offsets such as `-05:00`
- approval is per-run, TTL-bounded, and revalidated at dispatch time

## Executive Thesis

The product should not grow more independent background loops.

It should grow one durable autonomy kernel with:

1. one persisted job ledger
2. one execution state machine
3. one policy and budget model
4. one user-visible audit trail for "why did this run?"
5. one set of pause, quarantine, rollback, and suppression controls

If the platform gets this right, the system becomes meaningfully better than
most agent products:

- more proactive than chat-only agents
- more understandable than black-box automation
- more controllable than DIY cron plus prompts
- safer than agents that can freely schedule and message on their own

That is the actual novelty target.

## Why This Work Is Timely Now

The sequencing is finally correct.

Recent work has already pushed the substrate in the right direction:

- auth and gateway contracts are tighter
- capability grants are explicit
- tool and skill pullback exists
- quarantine and kill paths exist
- Studio cancellation and failure handling are materially better

That means the next leverage point is autonomy.

If autonomy had been built before those controls, it would have amplified unsafe
behavior. Built now, it can amplify usefulness without losing operator control.

## Current-State Audit

This section records what is live today versus what only exists as scaffolding.

### Live Facts From The Repo

1. The gateway does not start an agent heartbeat engine or cron engine during
   bootstrap. The tracked background tasks today are WAL checkpointing,
   convergence watching, config watching, backup scheduling, auto triggers, and
   cost persistence in
   [bootstrap.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs#L398).

2. The `ghost-heartbeat` crate exists and has meaningful code, but it is not
   wired into the live gateway runtime. The crate exposes `HeartbeatEngine`,
   `CronEngine`, and tier logic in:
   [heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/heartbeat.rs),
   [cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/cron.rs),
   [tiers.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/tiers.rs).

3. The heartbeat tier model is not actually active in execution. The engine's
   `fire()` path still always runs a full agent turn in
   [heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/heartbeat.rs#L224).
   The tier selector exists, but is not used to choose an execution mode.

4. The heartbeat configuration surface is only partially real. Fields such as
   `active_hours_start`, `active_hours_end`, and `timezone_offset_hours` are
   defined in
   [heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/heartbeat.rs#L32),
   but there is no live runtime consumer using them.

5. The cron engine is not timezone-aware in practice despite carrying a
   timezone field. It evaluates jobs against `Utc::now()` and matches cron
   fields directly in
   [cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/cron.rs#L99)
   and
   [cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/cron.rs#L127).

6. The cron syntax is intentionally incomplete. It supports `*` and numeric
   fields only. It does not support ranges, steps, or named weekdays in
   [cron.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-heartbeat/src/cron.rs#L127).

7. The current heartbeat CLI is not a truthful engine status view. It reads
   `heartbeat_frequency`, `convergence_tier`, `last_heartbeat`, and
   `agents_count` from `/api/health` in
   [cli/heartbeat.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/cli/heartbeat.rs#L36),
   but the actual health endpoint does not return those fields in
   [health.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/health.rs#L23).

8. The current `/api/sessions/:id/heartbeat` route is a frontend session
   keepalive, not an autonomous agent heartbeat. That route exists in
   [route_sets.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/route_sets.rs#L482)
   and is implemented in
   [sessions.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/sessions.rs#L765).

9. There is no real `HEARTBEAT.md` file in the repo or under `~/.ghost` for
   the running system to consume. The current heartbeat message refers to a file
   that does not exist in the active environment.

10. The product already has a second candidate scheduling authority in the
    workflow system. Workflow CRUD and execution are live in
    [workflows.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/workflows.rs),
    which means autonomy work must avoid creating permanent split ownership
    between workflow schedules and `ghost-heartbeat` cron jobs.

### Diagnosis

The repo currently has the parts of an autonomy system, but not one coherent
autonomy runtime.

The danger is not that the repo lacks ideas.

The danger is that it has multiple half-authorities:

- a heartbeat crate
- a cron crate
- a generic periodic scheduler
- workflow execution
- client session heartbeats
- CLI commands that imply runtime behavior not actually present

That is exactly the kind of drift this design must remove.

## Core Problem Statement

Users want an agent that is persistent and proactive.

Users also want:

- low surprise
- low cost
- reversibility
- clear blame when something runs
- no spam
- no hidden background activity
- immediate pullback when the system is flagged

Those goals conflict.

The system therefore needs an autonomy control plane that treats initiative as a
governed resource, not a free side effect of scheduling.

## Design Goals

### Primary Goals

1. One authority for all autonomous work.
2. Durable state for all scheduled or deferred execution.
3. Clear user-visible "why now?" reasoning for every autonomous action.
4. Safe proactive behavior that degrades before it surprises.
5. Immediate pullback when policy, safety, or trust thresholds are crossed.
6. No silent loss of due work and no silent duplicate execution.
7. Honest health and operator tooling.
8. Safe migration and cutover without hidden duplicate execution or status
   regressions.

### Secondary Goals

1. Reuse existing gateway, workflow, and policy primitives instead of inventing
   a parallel runtime.
2. Keep the system local-first and vendor-agnostic.
3. Preserve deterministic auditing and postmortem analysis.

### Non-Goals

1. Replacing the existing workflow DAG engine.
2. Building a generic distributed scheduler for arbitrary cluster scale.
3. Introducing a third independent task model beside jobs and workflows.
4. Shipping unrestricted autonomous messaging.

## Non-Negotiable Invariants

These are release-blocking for autonomy.

### Invariant 1: One Autonomy Ledger

Every autonomous action must be represented by a persisted job or run record.
No hidden timers may directly invoke the agent loop.

### Invariant 2: One Execution State Machine

All background work must use the same lifecycle:

- queued
- leased
- running
- waiting
- succeeded
- failed
- paused
- quarantined
- aborted

### Invariant 3: No Unexplained Initiative

Every agent-initiated run must store a machine-readable explanation of:

- trigger source
- why it is due now
- what changed since the previous run
- why user interruption is or is not justified

### Invariant 4: Policy Before Execution

Pause, quarantine, kill switches, capability pullback, cost ceilings, and
initiative budgets are evaluated before the agent loop is entered.

### Invariant 5: Reversible By Default

Any non-trivial autonomous action must be cancelable, suppressible, and where
possible reversible.

### Invariant 6: Honest Health Surfaces

CLI and API status endpoints may only report fields backed by live runtime
state. Placeholder or inferred health is considered broken.

### Invariant 7: Explicit Overlap And Catch-Up Semantics

Every job type must define:

- overlap policy
- missed-run policy
- retry policy
- idempotency key

No implicit scheduler behavior is allowed.

### Invariant 8: Migration Before Deletion

No drifted autonomy seam may be deleted or demoted until the replacement path
is live, observable, and covered by rollback criteria.

### Invariant 9: Measurable Runtime Health

The control plane must define explicit service-level indicators for queue lag,
lease recovery, duplicate execution, and proactive action quality before old
paths are retired.

### Invariant 10: Sensitive Autonomy Data Has A Lifecycle

`why_now`, suppressions, approval records, and proactive audit artifacts must
have explicit retention, redaction, and export semantics.

### Invariant 11: No Exactly-Once Fiction

The control plane must explicitly model at-least-once dispatch with durable
idempotency and side-effect correlation. It may not imply exactly-once
execution where the underlying system cannot prove it.

## Proposed Architecture

### 1. System Shape

The target system is a single autonomy control plane inside the gateway.

It has six layers:

1. Trigger intake
2. Durable job ledger
3. Policy and budget evaluator
4. Lease and dispatch runtime
5. Executor adapters
6. Audit and user-facing explanation surfaces

### Trigger Intake

Trigger intake normalizes all reasons work might need to happen:

- reactive follow-up from a user conversation
- schedule fire
- ambient heartbeat observation
- retry after failure
- workflow continuation
- safety-driven escalation
- external webhook or channel signal

Each trigger becomes a normalized job request.

### Durable Job Ledger

This is the core source of truth. It replaces scattered timer state.

Proposed canonical tables:

- `autonomy_jobs`
- `autonomy_runs`
- `autonomy_leases`
- `autonomy_policies`
- `autonomy_suppressions`
- `autonomy_notifications`

The ledger belongs in `cortex-storage` so state survives gateway restart and can
be audited alongside the rest of the platform.

### Policy And Budget Evaluator

This layer decides whether a queued job is allowed to become executable.

It consults:

- kill switch state
- pause/quarantine state
- capability pullback state
- convergence state
- cost tracker
- initiative budgets
- user policy
- quiet hours

This is where the product becomes novel: initiative is budgeted, not assumed.

### Lease And Dispatch Runtime

The runtime owns polling due jobs, leasing them, enforcing overlap policy,
running them once, and writing the outcome back to the ledger.

It must be the only owner of autonomous execution.

### Executor Adapters

Executors translate jobs into real work:

- heartbeat observer adapter
- workflow executor adapter
- agent turn adapter
- notification adapter
- retry adapter

Executors do not own schedule semantics.

### Audit And Explanation Surfaces

Every autonomous run produces:

- a structured audit event
- a user-visible explanation
- operator-visible runtime state

If the system cannot explain why something ran, the run model is incomplete.

### 2. Canonical Job Model

All autonomous work should map into a single job model.

Proposed fields:

- `id`
- `job_type`
- `owner_agent_id`
- `source`
- `intent`
- `payload`
- `payload_schema_version`
- `trigger_kind`
- `schedule_spec`
- `priority`
- `cost_budget`
- `initiative_budget`
- `trust_budget`
- `quiet_hours_policy`
- `overlap_policy`
- `missed_run_policy`
- `retry_policy`
- `idempotency_scope`
- `side_effect_key`
- `status`
- `next_run_at`
- `last_run_at`
- `last_success_at`
- `last_failure_at`
- `last_result_summary`
- `last_why_now`
- `created_at`
- `updated_at`

### Job Types

Initial canonical job types:

- `heartbeat_observe`
- `schedule_prompt`
- `workflow_run`
- `retry_run`
- `deferred_followup`
- `safety_escalation`
- `notification_delivery`

`heartbeat_observe` is intentionally not the same thing as "run a full agent
turn." It begins as observation and only escalates when justified.

#### Delivery Semantics And Side-Effect Contract

The control plane should target durable at-least-once execution with explicit
idempotency boundaries.

That means:

- the scheduler may dispatch again after lease loss or crash recovery
- handlers must be idempotent within a declared scope
- external side effects must carry a precomputed correlation key derived from
  logical job identity plus the relevant scheduled fire or run identity
- notification delivery should be backed by the same durable ledger or an
  explicit outbox-style equivalent, not an in-memory "send and hope" path

Exhausted retries must not simply vanish into a generic failed state. Jobs that
cannot safely auto-retry should move to an operator-visible terminal or
manual-review disposition.

#### Backpressure And Fairness

The control plane must define:

- max global dispatch concurrency
- per-agent or per-tenant concurrency ceilings
- a starvation/fairness rule so one noisy agent cannot monopolize dispatch
- what happens when the dispatcher is saturated

Runtime overload may degrade throughput, but it may not silently disable the
control plane or silently drop due work.

### 3. Initiative Budget Model

This is the central product differentiator.

Most systems budget only tokens or money. This system also budgets initiative.

Every agent receives multiple budgets:

- cost budget
- risk budget
- interruption budget
- novelty budget
- trust budget

### Cost Budget

The hard dollar/token envelope for autonomous behavior.

### Risk Budget

How much non-reversible or externally visible action is allowed.

### Interruption Budget

How many times the agent is allowed to proactively interrupt the user in a
window, weighted by severity.

### Novelty Budget

How much the system may do something behaviorally new versus something the user
has already accepted.

### Trust Budget

How much initiative the agent is allowed based on recent accuracy, reversals,
user dismissals, suppressions, and policy hits.

### Operational Rule

If a job exceeds available initiative budget, the system must degrade it before
execution:

- from notify to draft
- from act to propose
- from propose to observe
- from observe to suppress

This turns autonomy from a binary "allowed or denied" system into a graded and
trustworthy one.

### 4. Heartbeat Redesign

The current heartbeat concept must be split into two pieces:

1. observation
2. intervention

### Problem In Current Design

The current crate claims multiple heartbeat tiers, but the only real fire path
still runs a full agent turn. That defeats the point of tiering and will not
scale safely.

### New Heartbeat Model

#### Tier H0: Liveness

- zero-token process liveness or session freshness signal
- updates internal state only
- never invokes the agent loop

#### Tier H1: State Diff

- reads convergence, task backlog, failed runs, pending approvals
- computes a delta against previous state
- persists the delta
- still does not invoke the agent loop

#### Tier H2: Reasoning Snapshot

- uses a compact deterministic evaluator
- decides whether anything justifies attention
- may create a deferred follow-up job or draft notification

#### Tier H3: Full Agent Turn

- only entered when lower tiers conclude that model reasoning is justified
- must consume initiative budget
- must produce a structured "why now?"

### `HEARTBEAT.md`

`HEARTBEAT.md` should not be the runtime authority.

If retained, it becomes a generated or operator-authored explanatory view of the
typed heartbeat policy, not the executable source of truth.

The runtime authority should be typed policy stored in the ledger or config.

### 5. Scheduling Redesign

The product needs real schedule semantics, not best-effort timer loops.

### Canonical Schedule Fields

Each scheduled job must define:

- timezone
- next fire time
- overlap policy
- missed-run policy
- retry policy
- jitter policy
- max runtime

### Required Overlap Policies

- `allow`
- `forbid`
- `replace`
- `queue_one`

### Required Missed-Run Policies

- `skip`
- `catch_up_one`
- `catch_up_all_with_cap`
- `reschedule_from_now`

### Required Retry Policies

- max attempts
- max retry duration
- exponential backoff
- min backoff
- max backoff
- retryable failure classes

### Why This Is Required

This aligns with mature scheduler practice:

- Kubernetes CronJobs explicitly model `timeZone`,
  `concurrencyPolicy`, and `startingDeadlineSeconds`
  ([docs](https://kubernetes.io/docs/concepts/workloads/controllers/cron-jobs/)).
- Cloud Scheduler exposes retry count and backoff controls
  ([docs](https://cloud.google.com/scheduler/docs/configuring/retry-jobs)).
- Google SRE guidance for periodic schedulers emphasizes durable state and
  idempotent job semantics
  ([Distributed Periodic Scheduling with Cron Service](https://sre.google/sre-book/distributed-periodic-scheduling/)).

The current cron crate is not close to this bar and should not be promoted as
production-grade until these semantics exist.

### 6. Workflow Integration

The workflow system is already a real product surface. The autonomy design must
not compete with it.

### Rule

Workflows remain the graph-of-steps execution model.

The autonomy control plane becomes the scheduling, retry, and triggering
authority that can enqueue workflow runs.

That means:

- workflow definitions stay in `api/workflows.rs`
- workflow execution state remains durable
- schedules for workflows move into the autonomy ledger
- cron-as-a-separate-world is retired or reduced to a compatibility adapter

### 7. Migration, Shadow Mode, And Cutover

The control plane should not replace drifted autonomy seams with a flag day and
hope.

### Required Migration Inputs

Before cutover, the system must inventory and classify:

- existing workflow schedule records
- old heartbeat or cron runtime state
- fake or placeholder autonomy status fields
- any persistent retry metadata

### Backfill Rule

If an old surface has durable state that affects due work, that state must be
mapped into the ledger or explicitly retired with operator-visible reasoning.
Silent dropping of due or pending work is not allowed.

### Shadow Mode

Before the old seams are removed, the new control plane should run in shadow
mode where practical and record:

- due-job selection differences
- next-fire computation differences
- status-surface differences
- duplicate-dispatch risk signals

Shadow mode is not permanent dual execution. It is comparison without
duplicate side effects.

### Cutover Requirements

Cutover must define:

- the activation flag or migration boundary
- the exact old surfaces being retired
- rollback criteria
- rollback procedure
- operator-visible status proving which authority is live

### Rollback Triggers

At minimum, cutover rollback criteria should include:

- duplicate execution above threshold
- queue lag or overdue age above threshold
- stuck leases beyond recovery budget
- false health or status reporting
- unexpected proactive notification spikes
- cost spikes attributable to the new control plane

### 8. Multi-Process And Lease Semantics

The design must be explicit about whether one gateway process or many may lease
jobs concurrently.

### Current Deployment Assumption

If the runtime is initially single-gateway only, that must be documented as a
hard invariant and defended with tests so the system is not accidentally scaled
into duplicate execution.

### Lease Contract

The ledger and runtime must define:

- lease owner identity
- lease duration
- renewal cadence
- expiry recovery semantics
- what happens when a process crashes after leasing but before completion write
- what happens when a process crashes after a side effect but before run
  finalization

### Multi-Process Readiness

Even if the initial implementation remains single-process, the lease algorithm
must not make future multi-process hardening impossible. Lease ownership,
idempotency keys, and recovery semantics should be explicit from day one.

### 9. User Trust Model

Autonomy must be legible to users, not only safe to operators.

Every autonomous action should expose:

- why this ran
- why it ran now
- what changed
- what it plans to do
- what it actually did
- how to stop similar future actions

### User Controls

Required user-facing controls:

- pause all autonomy
- pause one agent
- suppress one class of proactive behavior
- quiet hours
- require approval for external notifications
- dry-run mode
- rollback or undo where action is reversible

### Behavioral Rule

If the user repeatedly suppresses a behavior, the trust budget for that behavior
class should decay automatically.

### 10. Data Retention And Privacy

Autonomy artifacts are not ordinary logs. They encode behavioral and preference
signals about the user and the agent.

### Required Policy Surface

The system must define retention and redaction rules for:

- `why_now` reasoning
- suppression records
- approval history
- proactive notification audit trails
- lease and run failure diagnostics when they contain user context

### Required Behaviors

- retention windows must be explicit
- export behavior must be explicit
- deletion or redaction behavior must be explicit
- sensitive artifacts must not silently persist forever because they were
  convenient for debugging

### 11. Human Approval Semantics

Approval-required mode must be a real execution contract, not just a UI flag.

### Required Contract

Approval-gated jobs must define:

- what work may be prepared before approval
- what side effects are blocked before approval
- how long approval remains valid
- whether approval is scoped to one run, one behavior class, or one agent
- what happens if policy, budget, or context changes before approval arrives

### Revalidation Rule

When approval is delayed, the runtime must re-evaluate policy, budget, and
context before execution. Old approval may authorize reconsideration, but it
must not force stale execution.

### 12. Safety And Policy Integration

The autonomy control plane must reuse the safety work already built.

### Required Inputs

- kill switch
- quarantine state
- capability pullback
- convergence protection level
- policy denial counts
- cost tracking

### Required Outputs

- structured safety events for autonomous runs
- automatic downgrade from action to draft/proposal
- automatic pause after repeated denials or reversals
- quarantine-aware suppression of future jobs

### Key Rule

Autonomy is not allowed to bypass the same constraints that user-triggered turns
obey.

If a capability is pulled back, autonomous jobs lose it too.

### 13. Honest Health And Operator Surfaces

The current health and CLI surfaces drift from reality. The redesign must fix
that before autonomy is considered production-ready.

### `/api/health`

Should report actual autonomy runtime state, not implied state:

- control plane enabled
- scheduler loop state
- dispatcher loop state
- runtime mode or deployment assumption when autonomy depends on single-gateway
  ownership
- due jobs
- leased jobs
- failed jobs
- terminal/manual-review jobs
- oldest overdue job
- last successful heartbeat observation
- queue lag
- dispatcher backpressure or saturation state

### CLI

`ghost heartbeat status` and `ghost cron` should become honest views over the
new ledger and runtime state. They must not derive nonexistent fields from
generic health responses.

### 14. Operational SLOs And Rollback

The control plane should not ship as a black box with "works in testing" as the
only operating standard.

### Required Initial SLOs

The first production-grade cut should define at least:

- max due-job selection lag
- max oldest overdue job age
- max lease recovery window
- duplicate-run rate
- false-positive proactive notification rate
- autonomy cost per day or billing window

### Rollback Contract

Every rollout phase that replaces live behavior must include:

- rollback trigger thresholds
- rollback operator steps
- proof that old status surfaces are not left lying during rollback
- post-rollback validation steps

### 15. Implementation Shape In This Repo

This design should land as a refactor and unification effort, not a bolt-on.

### Primary Crates To Touch

- `crates/ghost-gateway`
- `crates/ghost-heartbeat`
- `crates/ghost-agent-loop`
- `crates/cortex/cortex-storage`
- `packages/sdk`
- `dashboard`

### Planned Ownership

### Repo Alignment Rule

The control plane should reuse existing durable ownership patterns already live
in the repo unless a written incompatibility requires deviation.

Specifically, the current `operation_journal` and `workflow_executions`
contracts already use `owner_token`, `lease_epoch`, durable lease renewal, and
versioned state semantics. The autonomy ledger should extend or align with
those patterns rather than inventing a conflicting lease dialect.

Likewise, any new autonomy API surfaces must update gateway route contracts,
OpenAPI exposure, SDK types, and dashboard consumers together.

#### `crates/cortex/cortex-storage`

Add durable tables and queries for:

- autonomy jobs
- autonomy runs
- leases
- suppressions
- policies
- notifications

Also extend schema-contract validation so the autonomy ledger is part of the
same storage contract discipline as workflow and operation-journal state.

#### `crates/ghost-gateway`

Own:

- bootstrap wiring
- scheduler runtime
- dispatcher runtime
- policy and budget evaluation
- API and CLI status surfaces
- workflow integration

Likely files:

- `src/bootstrap.rs`
- `src/periodic.rs` or a new `src/autonomy/`
- `src/api/health.rs`
- `src/api/workflows.rs`
- `src/cli/heartbeat.rs`
- `src/cli/cron.rs`
- `src/state.rs`

#### `crates/ghost-heartbeat`

Either:

1. become a thin library of heartbeat-specific policy and observation logic, or
2. be subsumed into a new `autonomy` module inside the gateway

The current state argues for option 1 as a migration bridge, then possible
consolidation later.

#### `crates/ghost-agent-loop`

Stay responsible for executing a turn once the autonomy runtime decides a full
agent turn is justified.

Do not let it become a scheduler.

#### `packages/sdk` and `dashboard`

Expose:

- autonomy status
- run explanations
- suppression controls
- pause and rollback controls
- due and overdue job visibility

### 16. Phased Rollout

This should be built in five phases.

### Phase 1: Ledger And Honest Status

Deliverables:

- durable autonomy tables
- truthful `/api/health` autonomy section
- truthful CLI surfaces
- bootstrap starts one control-plane runtime

Exit criteria:

- no heartbeat or cron status field is placeholder-derived
- runtime survives restart with durable state
- due jobs and overdue jobs are inspectable

### Phase 2: Migration, Backfill, And Shadow

Deliverables:

- legacy schedule and retry state inventory
- durable backfill rules for any migrated state
- shadow comparison for due-job selection and status reporting
- cutover and rollback playbook

Exit criteria:

- migrated state does not silently lose due work
- shadow results are explained and accepted
- rollback triggers and procedures are documented and testable

### Phase 3: Workflow And Schedule Unification

Deliverables:

- workflow runs can be scheduled through the ledger
- overlap and missed-run policies exist
- timezone-aware scheduling exists
- retry policy is durable and explicit

Exit criteria:

- no second schedule authority exists
- scheduled workflow execution has deterministic replay semantics

### Phase 4: Heartbeat Observation Tiers

Deliverables:

- H0/H1/H2/H3 tier execution path
- no default full-turn heartbeat
- typed heartbeat policy
- generated or demoted `HEARTBEAT.md`

Exit criteria:

- most heartbeats do not invoke the model
- initiative budget is enforced for H3

### Phase 5: User Trust Controls

Deliverables:

- why-now explanations
- suppressions
- quiet hours
- draft-only and approval-required modes
- rollback where possible
- explicit retention and redaction semantics for autonomy artifacts

Exit criteria:

- proactive actions are user-legible and reversible
- suppression behavior feeds trust-budget decay
- approval-gated actions revalidate before execution

### 17. Test Strategy

This system should not ship without explicit cross-layer tests.

### Storage Tests

- due job selection
- lease acquisition and lease expiry
- overlap policy correctness
- missed-run policy correctness
- retry backoff correctness

### Gateway Tests

- bootstrap starts autonomy runtime exactly once
- pause/quarantine blocks queued jobs
- kill switch drains execution safely
- health endpoint reflects actual runtime state
- dispatcher saturation is visible and does not silently disable the runtime
- terminal/manual-review jobs remain visible until explicitly resolved

### Workflow Tests

- scheduled workflow enqueue
- retry and resume after crash
- no double execution under overlap policies

### Heartbeat Tests

- H0/H1/H2 do not invoke full agent turns
- H3 does invoke the agent loop only when justified
- initiative budget downgrades notify to draft/proposal/observe

### UI And SDK Tests

- user can see why a run happened
- user can suppress future similar runs
- user can pause and resume autonomy

### Adversarial Tests

- repeated failures disable or downgrade noisy jobs
- stale leases recover safely
- cost ceiling stops future proactive runs
- capability pullback immediately constrains autonomous jobs
- DB write failure after lease acquisition is recoverable
- crash after side effect but before completion write does not create silent
  ambiguity
- clock skew or delayed timer wakeups do not violate missed-run policy
- delayed approvals and stale suppressions force re-evaluation before execution
- concurrent lease contenders do not double-dispatch the same logical work
- exhausted retries move to visible terminal/manual-review disposition
- at-least-once recovery does not create duplicate external side effects inside
  the declared idempotency scope

### 18. Migration Rules

To avoid another round of drift, migration must follow these rules:

1. Do not add new autonomous timers outside the control plane.
2. Do not expose CLI or API status for behavior that is not live.
3. Do not let workflow scheduling and cron scheduling diverge semantically.
4. Do not market `HEARTBEAT.md` as the authority unless the runtime actually
   loads it.
5. Do not ship tier language unless the runtime really executes by tier.
6. Do not delete or demote old seams until replacement status surfaces are live
   and rollback notes exist.
7. Do not treat cutover or backfill as operator folklore. They must be written,
   tested, and visible.
8. Do not imply exactly-once semantics where the system actually depends on
   durable idempotency.
9. Do not add new autonomy payloads or schedule specs without explicit schema
   versioning.

### 19. Open Questions

These questions are real, but they should not block Phase 1.

1. Should the autonomy ledger live fully inside `cortex-storage`, or should
   some fast-changing lease state remain in gateway memory with durable write
   through?
2. Should typed autonomy policy live in config, DB, or both?
3. Should workflow schedules be stored as first-class autonomy jobs or as
   workflow-owned records projected into the autonomy queue?
4. Should notification delivery be part of the same state machine or a child
   action with its own ledger?

### 20. Final Position

The next truly important system in this repo is not another skill, workflow
node, or dashboard page.

It is a trustworthy autonomy control plane.

That is the piece that turns the project from:

- a secure agent runtime

into:

- a secure, persistent, understandable, user-trustworthy agent platform

The repo is ready for this now.

It is not ready for more scattered autonomy.

## References

These sources inform the scheduling and durability parts of the design:

- Kubernetes CronJob concepts:
  [https://kubernetes.io/docs/concepts/workloads/controllers/cron-jobs/](https://kubernetes.io/docs/concepts/workloads/controllers/cron-jobs/)
- Google Cloud Scheduler retry behavior:
  [https://cloud.google.com/scheduler/docs/configuring/retry-jobs](https://cloud.google.com/scheduler/docs/configuring/retry-jobs)
- Google SRE, distributed periodic scheduling:
  [https://sre.google/sre-book/distributed-periodic-scheduling/](https://sre.google/sre-book/distributed-periodic-scheduling/)
- Tokio missed tick behavior:
  [https://docs.rs/tokio/latest/tokio/time/enum.MissedTickBehavior.html](https://docs.rs/tokio/latest/tokio/time/enum.MissedTickBehavior.html)
