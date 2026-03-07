# Runtime Safety Remediation Design

## Summary

This document defines the remediation plan for the highest-risk runtime safety gaps in GHOST discovered during a control-plane audit.

The repo already contains strong safety concepts:

- kill states
- convergence monitoring
- policy evaluation
- distributed kill coordination
- auditability
- bounded tooling

The current problem is not lack of safety subsystems. The problem is that several of the strongest controls are not yet binding on the primary execution path.

This plan closes that gap.

The target state is:

- every live agent run uses a stable agent identity
- kill, pause, quarantine, and distributed gate state are enforced on the hot path
- policy is enforced before every tool execution
- tool defaults are fail-closed
- convergence state is durable and meaningful across sessions
- safety claims in the README are backed by runtime invariants rather than adjacent components

## Audit Scope

This design is scoped to the runtime execution seam between:

- `crates/ghost-gateway`
- `crates/ghost-agent-loop`
- `crates/ghost-policy`
- `crates/ghost-kill-gates`
- `crates/convergence-monitor`

The audited paths were:

- HTTP agent chat
- streaming agent chat
- studio session execution
- CLI interactive chat
- tool execution dispatch
- kill switch and distributed kill propagation
- convergence state loading during pre-loop

## Why Change

The current runtime has six critical issues.

### 1. Kill state is not authoritative on the hot path

The gateway owns a real `KillSwitch`, but `AgentRunner` instances used by API and studio routes are created with their own local kill state and do not appear to consult `AppState.kill_switch` before executing.

Impact:

- `pause`
- `quarantine`
- `kill_all`

can appear successful in control APIs while live agent execution continues.

### 2. Agent identity is ephemeral

The primary execution endpoints mint fresh `agent_id` values per request. Convergence state, per-agent kill state, audit continuity, cost continuity, and long-lived supervision all depend on stable identity.

Impact:

- convergence state files are effectively cold every turn
- per-agent kill operations lose meaning
- longitudinal supervision is weakened
- the system behaves more like stateless request handling than persistent paired autonomy

### 3. Policy exists but is not binding

`ghost-policy` contains the strongest allow/deny logic, but runtime tool execution currently flows through `ToolExecutor` without a mandatory `PolicyEngine` check.

Impact:

- CORP policy is advisory in the main path
- capability grants are not acting as a hard boundary
- convergence-tightening rules are not guaranteed to block tools

### 4. Built-in tool defaults are permissive

The shell tool allows any command when no prefix allowlist is configured. The filesystem write path can escape the workspace for non-existent paths because resolution is not normalized before boundary checks.

Impact:

- fail-open execution for the most dangerous tools
- elevated blast radius if policy is bypassed or missing

### 5. Distributed kill coordination is incomplete

The distributed gate structure exists, but relay/fanout behavior is not fully connected into a real inbound and outbound propagation path.

Impact:

- split-brain safety behavior across peers
- local safety confidence that does not survive cluster operation

### 6. Degraded convergence mode is too silent

When convergence state is missing or unreadable, the system currently defaults to level 0 behavior. That is acceptable for bootstrap or recovery, but not as the normal state for long-running paired operation.

Impact:

- monitor outages quietly weaken safety posture
- operators may believe convergence controls are active when they are not

## Goals

- make safety controls binding on every production execution path
- preserve the existing architecture rather than replacing it
- converge on one authoritative source of truth for agent identity and kill state
- enforce policy before tool execution, not after
- move dangerous tools to fail-closed defaults
- define explicit degraded-mode behavior instead of silent weakening
- produce a design that can be implemented incrementally while keeping `main` releasable

## Non-Goals

- redesigning the full product surface
- changing the conceptual safety model
- replacing the convergence monitor
- rewriting all tools or all route handlers at once
- shipping new end-user features during this remediation

## Engineering Bar

This plan is only acceptable if the final system satisfies all of:

- one stable agent identity per durable agent
- one authoritative kill-state source used by all runners
- one mandatory policy check before any tool executes
- dangerous tools fail closed by default
- degraded monitor states are observable and explicit
- every phase has a measurable exit condition

This plan is also only acceptable if verification follows a production-first standard:

- do not rely on large happy-path test volume as evidence of safety
- prefer a small number of high-signal, failure-oriented tests that prove runtime invariants
- test the control plane under state changes, stale state, missing state, partial failure, and adversarial input
- require tests that would have caught the exact classes of failures described in this document

## Architectural Invariants

These are hard rules.

### Invariant 1: agent identity is durable

- request handlers do not generate ad hoc runtime agent identities for persistent agents
- the same durable agent resolves to the same runtime identity across sessions and turns

### Invariant 2: kill state is authoritative

- all execution paths consult the same kill-state authority
- `pause`, `quarantine`, and `kill_all` are blocking states, not advisory metadata

### Invariant 3: policy is in the execution path

- no tool dispatch occurs without policy evaluation
- plan validation is additive, not a substitute for policy

### Invariant 4: dangerous tools fail closed

- shell execution is denied when not explicitly configured
- filesystem writes cannot escape the workspace
- external HTTP is deny-by-default unless explicitly allowed

### Invariant 5: degraded safety is visible

- missing monitor state must be observable in logs, health, and status APIs
- degraded operation must not be indistinguishable from healthy operation

### Invariant 6: distributed safety cannot be aspirational

- if mesh safety is enabled, propagation paths must be implemented end to end
- if they are not implemented, the system must say so clearly and avoid overclaiming

## Current State

### Runtime ownership

The gateway owns application state, including:

- `KillSwitch`
- optional distributed kill gate bridge
- convergence profile
- tool config
- skill registry

But execution handlers create fresh `AgentRunner` values and wire only subsets of that state into the runner.

### Execution ownership

`AgentRunner` currently owns:

- local kill atomic
- optional local distributed gate handle
- local tool registry
- local tool executor
- pre-loop safety checks

The design intent is good, but the runner is not consistently hydrated from gateway state before use.

### Policy ownership

`ghost-policy` is structurally separated and internally coherent, but it is not yet the non-optional gate in live tool execution.

### Convergence ownership

The monitor writes shared state files by `agent_id`. This only works if the execution side uses stable identities and treats missing state as exceptional when the system is expected to be supervised.

## Target Architecture

## Decision 1: introduce a runtime safety context

Add a single runtime wiring concept owned by the gateway and passed into `AgentRunner` construction.

This context should define the authoritative runtime safety dependencies required for any live run:

- durable agent identity
- session identity
- kill-state authority
- distributed kill gate handle
- policy engine or policy evaluator facade
- convergence expectations
- tool capability scope
- spending/cost authority

The key design rule is that route handlers should not manually wire safety-critical pieces one by one. They should build or request one runtime context and hand it to the runner builder.

### Why

The current failures are mostly hydration failures. A dedicated runtime context removes the ability for individual routes to accidentally omit kill state, policy, or stable identity.

## Decision 2: separate durable agent identity from request identity

Each execution path must resolve a durable agent record before constructing the runner.

Required distinction:

- durable `agent_id`: identity of the supervised agent
- `session_id`: identity of a conversation or studio session
- `message_id` or `run_id`: identity of a single request/turn

The runner must use the durable `agent_id` for:

- convergence state lookup
- kill-state checks
- cost tracking
- audit correlation
- policy capability resolution

### Implication

Studio sessions and API chat may still create new run or message identifiers, but not new durable agent identities per turn.

## Decision 3: make policy evaluation mandatory inside tool execution

Policy must be a required step between:

- tool selection by the LLM
- tool dispatch by `ToolExecutor`

The required order becomes:

1. gate checks
2. model response
3. plan validation
4. per-tool policy evaluation
5. tool execution
6. audit and feedback

### Why this location

Policy should not live only in route handlers because:

- it must apply to every execution surface
- route-local enforcement invites drift
- the executor is the last trustworthy choke point before side effects

## Decision 4: fail closed on dangerous tool defaults

Dangerous built-ins must require explicit opt-in.

Required target behavior:

- `shell`: denied unless allowed prefixes are configured
- `write_file`: normalized path resolution must prove containment before create/write
- `http_request`: deny external domains unless explicitly allowed
- `web_fetch`: retain restrictive defaults and document them

### Why

For a system claiming safe long-running autonomy, permissive defaults are the wrong default failure mode.

## Decision 5: define degraded convergence operation explicitly

The system should support degraded operation, but degraded operation must become a named runtime mode.

Target modes:

- `Healthy`: convergence state present and current
- `Degraded`: monitor unavailable, stale, or unreadable
- `Unsafe`: operator policy forbids execution without monitor health

The chosen mode should be configurable by deployment profile.

Suggested profile behavior:

- local development: degraded allowed with loud warnings
- single-user alpha: degraded allowed with UI/API status surfacing
- supervised production: degraded blocks high-risk execution

## Decision 6: close the distributed kill story or narrow the claim

This is the riskiest part of the remediation to scope poorly.

Therefore this plan adopts a hard rule:

- no partial distributed-kill implementation ships as part of this remediation
- no mock transport, placeholder receiver, or "temporary" propagation path is acceptable
- no TODO-backed cluster safety claim is acceptable

For this remediation milestone, there are only two acceptable outcomes:

1. distributed kill is fully implemented end to end in a dedicated milestone with explicit contracts, tests, and rollout controls
2. distributed kill is hard-disabled or feature-gated, and all public/runtime claims are narrowed accordingly

The default planning assumption for this remediation is outcome 2.

If a separate dedicated distributed-kill milestone is approved later, that work must not begin as opportunistic partial implementation inside the core runtime remediation.

If mesh/distributed safety is enabled in a future dedicated milestone, the system must implement:

- outbound propagation
- inbound authenticated receipt
- local state application on receipt
- audit on both sender and receiver
- health/status visibility for propagation failures

If that future dedicated milestone is not executed, distributed kill remains disabled or explicitly unsupported.

## Phased Plan

## Phase 0: freeze the unsafe seams

Purpose:

- stop safety drift while remediation is in progress

Actions:

- document the current gaps
- prohibit new runner construction paths that bypass shared runtime wiring
- prohibit new direct tool dispatch paths that bypass policy

Exit condition:

- there is one documented remediation path and no new hot paths are introduced during the work

## Phase 1: unify runtime context and durable identity

Purpose:

- make every live execution path start from the same supervised identity model

Actions:

- define a runner-construction API owned by the gateway
- resolve durable agent identity before building the runner
- separate agent, session, run, and message identifiers
- ensure API chat, streaming chat, studio, and CLI all use the same construction model where applicable

Exit condition:

- no live route creates a fresh durable `agent_id` per request
- convergence and kill state lookups use the durable agent identity

## Phase 2: wire authoritative kill-state enforcement

Purpose:

- make control-plane safety actions block execution in reality

Actions:

- bind runner kill checks to gateway-owned kill state
- bind distributed gate state where enabled
- ensure `pause`, `quarantine`, and `kill_all` are all enforced before and during runs
- define exact behavior when state changes mid-turn

Required behavior:

- pre-loop must reject blocked agents
- per-iteration checks must halt runs after activation
- status endpoints and execution behavior must agree

Exit condition:

- a live run cannot continue after authoritative kill activation

## Phase 3: move policy into mandatory tool dispatch

Purpose:

- turn policy from guidance into enforcement

Actions:

- define a policy evaluation adapter available to the executor
- ensure capability grants resolve from durable agent configuration
- apply convergence-tightening rules using current runtime context
- feed denials back to the model in a structured way without executing side effects
- audit denials and escalation triggers

Exit condition:

- every tool call has a recorded policy decision before dispatch
- no tool can execute without passing policy

## Phase 4: harden tool boundaries

Purpose:

- reduce blast radius even if upstream controls regress

Actions:

- change shell default from allow-all to deny-all
- normalize and validate filesystem paths before create/write
- review HTTP and web tool defaults against deny-by-default expectations
- document explicit enablement requirements in config and docs

Exit condition:

- dangerous tools are fail-closed by default
- workspace boundaries are enforced for existing and non-existing paths

## Phase 5: formalize degraded convergence behavior

Purpose:

- make monitor dependence explicit and observable

Actions:

- define runtime monitor health states
- surface those states through health/status APIs and logs
- choose blocking behavior per deployment mode
- add stale-state thresholds instead of only file-exists parsing

Exit condition:

- operators can tell when convergence protections are healthy, degraded, or absent
- deployments can choose whether degraded mode is allowed

## Phase 6: gate distributed kill and narrow claims

Purpose:

- remove ambiguity between designed and actually working cluster safety during the core remediation milestone

Actions:

- disable or hard-feature-gate partial distributed kill behavior
- remove or narrow any claims that imply complete distributed kill semantics
- ensure configuration, health/status surfaces, and docs all agree on the feature state
- define the acceptance bar for any future dedicated distributed-kill milestone

Explicit non-actions for this milestone:

- do not land placeholder inbound handlers
- do not land partial propagation that does not affect authoritative runtime state
- do not land mocked quorum behavior
- do not leave "temporarily enabled" distributed kill semantics in production code

Exit condition:

- distributed kill is either disabled/feature-gated everywhere relevant, or removed from current safety claims entirely

## Verification Strategy

This design is not complete without adversarial validation.

The testing goal is not "more tests." The goal is "proof that the runtime fails safely under the conditions that matter."

This remediation should reject the common anti-pattern of shipping thousands of low-signal tests that mostly validate nominal behavior. For a safety-first control plane, the highest-value tests are the ones that attack assumptions:

- state missing when it should exist
- state changing mid-run
- stale state being mistaken for current truth
- policy omitted by wiring drift
- dangerous tools enabled by default or config absence
- distributed coordination partially succeeding
- operator actions saying one thing while runtime behavior does another

The right test suite here is small, sharp, and hostile.

## Test Philosophy

### Principle 1: prove invariants, do not narrate flows

Each test should prove one hard property, for example:

- a paused agent cannot execute
- a policy-denied tool cannot dispatch
- a runner using a non-durable identity is rejected
- degraded convergence state is surfaced and handled according to policy

Tests that merely replay the intended flow without trying to break it are lower value.

### Principle 2: prefer failure injection over coverage theater

If choosing between:

- ten happy-path route tests
- one test that flips kill state during execution and proves the runner halts

the second is the better engineering test.

### Principle 3: test at the seam where reality breaks

Most of the issues in this audit came from integration seams, not isolated functions.

Therefore, the highest-priority tests should execute across:

- gateway -> runner construction
- runner -> policy evaluation
- runner -> tool dispatch
- monitor state -> pre-loop enforcement
- safety API -> actual agent halt behavior
- mesh propagation -> local runtime state application

### Principle 4: no false confidence from broad counts

A large suite is acceptable only if it is mostly invariant-focused. Test count is not a quality metric.

Success is:

- a reviewer can point to a small number of tests and say "these would have caught the safety regression"

Failure is:

- a large suite passes while the runtime still violates its own safety claims

## Required test classes

### 1. identity continuity tests

- same agent across turns resolves the same durable identity
- convergence state and kill state survive across runs
- route handlers cannot silently substitute fresh agent identities for durable ones

### 2. kill-path tests

- pause blocks start
- pause during run halts next iteration
- quarantine blocks tool execution
- kill_all blocks all agents
- status API and runner state match
- safety API success without runtime halt is treated as a test failure
- local and distributed kill state cannot diverge silently

### 3. policy-path tests

- denied tools never dispatch
- capability grants are required
- convergence-tightening denies the expected tools
- plan validation and policy both run in the correct order
- missing policy wiring causes a hard test failure, not a skipped behavior
- unknown or newly added tool paths must fail closed until policy coverage exists

### 4. tool-boundary tests

- shell is denied with empty config
- non-existent traversal writes are rejected
- allowed writes succeed inside the workspace
- external HTTP is blocked when unconfigured
- missing config must never widen permissions
- tool registry additions must not inherit permissive defaults accidentally

### 5. degraded-mode tests

- stale/missing monitor state is surfaced
- configured strict deployments refuse execution in unsafe monitor state
- stale state and absent state are tested separately
- corrupted state is handled explicitly and observably

### 6. distributed-safety tests

- partial distributed-kill code paths are not silently active when the feature is gated
- propagation-related status surfaces are honest about feature state
- receiver-side authentication and state application are tested only if a full dedicated milestone is approved

## Minimal Production Test Set

The initial remediation should target a compact set of production-grade tests rather than a huge suite expansion.

Recommended minimum set:

1. `durable_identity_required_for_live_run`
2. `pause_blocks_agent_chat_and_studio_execution`
3. `kill_all_during_active_run_halts_on_next_iteration`
4. `policy_denied_tool_never_reaches_dispatch`
5. `shell_denied_when_unconfigured`
6. `write_file_rejects_nonexistent_path_traversal`
7. `missing_convergence_state_enters_degraded_or_blocked_mode_per_profile`
8. `distributed_kill_disabled_by_default_when_not_fully_implemented`
9. `status_surface_matches_actual_enforcement_state`
10. `new_tool_without_policy_mapping_fails_closed`

These tests are intentionally narrow and severe. If these ten pass for the right reasons, they provide more confidence than a large set of route-level happy-path tests.

If a future dedicated distributed-kill milestone is approved, it must replace test 8 with a full end-to-end cluster-runtime invariant test set rather than layering partial tests on top of the gated state.

## Anti-Goals For Testing

Do not spend this remediation cycle optimizing for:

- broad route snapshot coverage
- repetitive API success-case tests
- "one test per endpoint" checklists
- inflated total test count
- mocks so deep that the real control seam is no longer exercised

The control-plane problem here is not lack of nominal behavior coverage. It is lack of proof that the system stays safe when reality becomes inconvenient.

## Operational Readiness Gates

Before broad OSS promotion as a safety-first platform, the following must be true:

- the six issues in this document are closed or explicitly feature-gated
- safety regressions are covered by automated tests
- degraded monitor behavior is documented in public docs
- distributed safety claims match implemented behavior
- dangerous tools are opt-in

## Rollout Strategy

The correct order is:

1. fix runtime identity and kill-state wiring
2. fix mandatory policy enforcement
3. fix dangerous tool defaults
4. formalize degraded monitor behavior
5. complete or narrow distributed kill claims
6. update README and public safety claims

Do not widen distribution before the first three are done. Those are the minimum needed to align behavior with the project’s core safety thesis.

## Risks

### Risk 1: route-local drift continues

Mitigation:

- central runner construction
- one runtime safety context
- CI checks against direct runner construction in route handlers if needed

### Risk 2: identity migration breaks existing sessions

Mitigation:

- introduce durable identity mapping carefully
- preserve session IDs as separate concepts
- add compatibility paths for existing studio data

### Risk 3: policy enforcement breaks current flows

Mitigation:

- ship with detailed denial telemetry
- make missing capability failures explicit and debuggable
- stage rollout by surface if necessary, but do not leave permanent bypasses

### Risk 4: stricter defaults reduce developer convenience

Mitigation:

- allow explicit local-dev opt-in in config
- keep production defaults safe

### Risk 5: distributed kill takes longer than expected

Mitigation:

- do not treat distributed kill as part of the core remediation milestone
- keep it gated unless a separate fully planned milestone is explicitly approved

## Open Questions

- what is the canonical durable agent record for API chat when the request does not name an agent explicitly
- should monitor degradation block all execution or only high-risk execution
- should CLI interactive chat share the same durable identity model as API/studio or remain a distinct local-dev mode
- where should policy capability grants be sourced for dynamically installed skills
- whether a separate dedicated distributed-kill milestone should be planned after the core runtime remediation is done

## Recommendation

Implement this as a dedicated remediation milestone, not as opportunistic cleanup.

This is not polish work. It is the work required to make GHOST’s strongest safety claims true in runtime behavior.

For distributed kill specifically, the recommendation is stricter:

- gate it in this milestone
- remove partial behavior and partial claims
- plan the full cluster-safety implementation separately, or not at all
