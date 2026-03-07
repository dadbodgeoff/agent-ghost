# Runtime Safety Remediation Tasks

## Objective

Execute the remediation defined in [RUNTIME_SAFETY_REMEDIATION_DESIGN.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/RUNTIME_SAFETY_REMEDIATION_DESIGN.md) without a flag day rewrite.

Target end state:

- every live execution path uses a durable agent identity
- authoritative kill state is enforced before and during execution
- policy evaluation is mandatory before tool dispatch
- dangerous tools fail closed by default
- degraded convergence operation is explicit and observable
- distributed kill is disabled/feature-gated in this milestone unless a separate fully planned milestone is explicitly approved

## Non-Negotiable Rules

These rules apply to every PR in this remediation:

- do not add new `AgentRunner` construction paths that bypass gateway-owned runtime safety wiring
- do not add new tool dispatch paths that bypass policy evaluation
- do not keep permissive defaults for shell, filesystem writes, or external HTTP in the name of convenience
- do not rely on route-local UUID generation for durable agent identity
- do not count happy-path test volume as safety evidence
- do not land mock, placeholder, or partially wired distributed-kill behavior
- do not accept TODO-backed implementations for safety-critical control paths
- keep `main` releasable after every merged PR

## Critical Path

There is one correct order:

1. freeze the unsafe seams
2. define runtime safety ownership
3. fix durable identity
4. wire authoritative kill-state enforcement
5. wire mandatory policy enforcement
6. harden dangerous tool defaults
7. formalize degraded convergence behavior
8. gate distributed kill and narrow claims
9. update public claims and guardrails

Do not start with README edits or broad route cleanup. The first work must be at the execution seam.

## Merge Gates

Every remediation PR must satisfy all of:

- `cargo test --workspace`
- any new safety-critical tests added in the PR pass reliably
- no new runner construction path bypasses the gateway-owned runtime context
- no new direct tool dispatch path bypasses policy
- no new permissive dangerous-tool default is introduced
- route/API status behavior matches actual runtime enforcement for the scope touched
- no safety-critical PR ships with placeholder behavior, mocks in production paths, or unfinished contracts

## Test Standard

This remediation is governed by a production-grade test bar:

- prefer a small number of high-signal invariant tests
- test failure, stale state, missing state, mid-run state changes, and adversarial inputs
- reject coverage theater and repetitive endpoint success tests
- every major task below must identify the exact tests that would catch regression

The target is not "many tests." The target is "tests that would have caught the bug."

## Task List

## T0. Freeze the unsafe seams

Purpose:

- stop runtime safety drift while remediation is in progress

Actions:

- document the runtime safety remediation path in repo docs
- avoid new route-local `AgentRunner` wiring for safety-critical dependencies
- avoid new tool execution surfaces that do not pass through the canonical executor path

Done when:

- the remediation path is documented
- reviewers have a clear basis to reject new hot-path bypasses

Verification:

- `rg -n "AgentRunner::new" crates/ghost-gateway/src`
- `rg -n "execute\\(" crates/ghost-agent-loop/src crates/ghost-gateway/src`

## T1. Define one runtime safety context

Purpose:

- establish one authoritative gateway-owned object for live-run safety wiring

Actions:

- define the runtime safety context/builder described in the design doc
- include durable agent identity, session identity, kill-state authority, distributed gate handle, policy evaluation facade, convergence expectations, and capability scope
- make route handlers consume this builder instead of assembling runner state ad hoc

Constraints:

- route handlers must not wire kill state, policy, and identity independently
- the canonical runner construction path must be obvious and reusable

Done when:

- there is one accepted path for building a live runner from gateway state
- safety-critical dependencies are no longer optional per route

Verification:

- a code search can identify the canonical builder and a shrinking set of direct `AgentRunner::new` callsites

Required tests:

- `runner_construction_requires_runtime_safety_context`
- `route_handler_cannot_omit_authoritative_kill_state`

## T2. Fix durable agent identity

Purpose:

- make supervision, convergence, kill state, and audit continuity meaningful across runs

Actions:

- define how durable agents are resolved for API chat, studio, and CLI modes
- separate durable `agent_id` from `session_id`, `run_id`, and `message_id`
- route convergence lookup, kill checks, and cost/audit correlation through the durable identity
- preserve compatibility for existing session data where needed

Constraints:

- new per-turn UUIDs may exist for runs or messages, not for the persistent agent identity
- studio and API paths must not mint fresh durable agent IDs on each request

Done when:

- all live execution paths use stable durable identity semantics
- convergence and per-agent control surfaces refer to the same entity

Verification:

- `rg -n "let agent_id = Uuid::now_v7\\(\\)" crates/ghost-gateway/src`

Required tests:

- `durable_identity_required_for_live_run`
- `same_agent_across_turns_reuses_convergence_state`
- `pause_applies_to_future_turns_of_same_durable_agent`

## T3. Wire authoritative kill-state enforcement

Purpose:

- make control-plane safety actions block execution in reality

Actions:

- bind live runner kill checks to gateway-owned kill-state authority
- ensure pre-loop rejects paused, quarantined, or killed agents
- ensure per-iteration checks halt active runs after state changes
- bind distributed gate state into the same enforcement path when enabled
- define exact semantics for `pause`, `quarantine`, and `kill_all`

Constraints:

- runtime behavior and safety API behavior must agree
- a successful pause/quarantine/kill response is invalid if the runner still executes

Done when:

- kill-state APIs and live execution agree on blocked/allowed state
- active runs halt on the next safe check boundary after kill-state escalation

Verification:

- code search finds real usage of authoritative kill-state checks on the live execution path

Required tests:

- `pause_blocks_agent_chat_execution`
- `pause_blocks_studio_execution`
- `kill_all_during_active_run_halts_on_next_iteration`
- `status_surface_matches_actual_enforcement_state`

## T4. Move policy into mandatory tool dispatch

Purpose:

- turn policy from documentation and diagnostics into hard enforcement

Actions:

- place policy evaluation on the canonical path between tool selection and dispatch
- resolve capability grants from durable agent configuration
- apply convergence-tightening using current runtime context
- return structured denial feedback to the conversation loop without side effects
- audit denials and threshold-based escalation triggers

Constraints:

- plan validation remains additive, not a replacement
- no tool call executes before a policy decision exists

Done when:

- every dispatched tool has an associated policy permit decision
- denied tools cannot reach side-effecting dispatch code

Verification:

- non-CLI runtime code contains real policy-engine integration
- CLI-only `policy check` is no longer the sole runtime use of `ghost-policy`

Required tests:

- `policy_denied_tool_never_reaches_dispatch`
- `capability_grant_required_for_tool_execution`
- `convergence_policy_tightening_blocks_expected_tools`
- `new_tool_without_policy_mapping_fails_closed`

## T5. Harden dangerous tool defaults

Purpose:

- reduce blast radius even if upstream enforcement regresses

Actions:

- make `shell` deny-all unless explicit prefixes are configured
- normalize filesystem write paths before containment checks
- verify external HTTP tools stay deny-by-default unless explicitly allowed
- document required opt-ins in config/docs

Constraints:

- missing config must never widen permissions
- workspace containment must hold for non-existent and newly created paths

Done when:

- dangerous tools fail closed by default
- filesystem writes cannot escape the workspace

Verification:

- inspect tool default constructors and config application for permissive fallbacks

Required tests:

- `shell_denied_when_unconfigured`
- `write_file_rejects_nonexistent_path_traversal`
- `write_file_allows_valid_in_workspace_create`
- `http_request_denied_when_domain_not_explicitly_allowed`

## T6. Formalize degraded convergence operation

Purpose:

- make monitor absence, stale state, and parse failures visible and governable

Actions:

- define runtime monitor health states
- distinguish missing, stale, and corrupted convergence state
- surface degraded state in health/status APIs and logs
- define deployment-mode behavior for degraded operation

Constraints:

- degraded operation must not be indistinguishable from healthy operation
- stricter deployments must be able to block execution when convergence protection is absent

Done when:

- operators can see the difference between healthy and degraded convergence protection
- deployment mode determines whether degraded execution is allowed

Verification:

- health/status surfaces expose monitor/convergence health explicitly

Required tests:

- `missing_convergence_state_enters_degraded_or_blocked_mode_per_profile`
- `stale_convergence_state_is_not_treated_as_healthy`
- `corrupted_convergence_state_is_visible_and_handled`

## T7. Gate distributed kill and define the bar for any future dedicated milestone

Purpose:

- eliminate ambiguity between declared distributed safety and actual runtime behavior without shipping partial cluster-safety logic

Actions:

- disable or hard-feature-gate partial distributed kill behavior in this milestone
- narrow claims in config, health/status, and docs so distributed kill is not implied as complete
- define the required acceptance bar for a future dedicated distributed-kill milestone
- ensure no production path treats distributed kill as authoritative while gated

Constraints:

- distributed safety cannot remain partly implemented and publicly implied as complete
- no mock transport, placeholder receiver, or partial authoritative-state application is acceptable
- if a future distributed-kill milestone is approved, it must be fully planned before implementation begins

Done when:

- distributed kill is disabled/feature-gated everywhere relevant in this milestone
- runtime, health/status surfaces, config, and docs all agree on that fact

Verification:

- code and docs do not imply authoritative distributed kill while the feature is gated
- no partial propagation path remains active by default

Required tests:

- `distributed_kill_disabled_by_default_when_not_fully_implemented`
- `distributed_kill_status_surface_honest_when_gated`
- `partial_distributed_kill_path_not_active_in_production_mode`

## T8. Add guardrails against regression

Purpose:

- stop future drift after the remediation lands

Actions:

- add code-review rules or CI checks for new raw runner construction drift if needed
- add targeted documentation on runtime safety invariants
- update developer guidance around durable identity, policy integration, and dangerous tool defaults

Done when:

- the repo has explicit enforcement against reintroducing the same class of runtime bypasses

Verification:

- CI or repo guidance clearly flags prohibited patterns

Required tests:

- none beyond the invariant tests above, unless a guardrail itself introduces logic worth testing

## T9. Update public claims

Purpose:

- align OSS-facing claims with implemented runtime behavior

Actions:

- update README safety wording if needed
- narrow any distributed-safety or monitor-enforcement claims until implementation is complete
- describe dangerous-tool defaults accurately
- document degraded-mode behavior plainly

Constraints:

- do not overclaim during remediation

Done when:

- public docs match actual runtime semantics

Verification:

- README and safety docs no longer imply guarantees that the runtime does not enforce

## Minimal Production Test Set

This remediation is not complete until the following tests exist and pass for the right reasons:

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

If these ten are not in place, the remediation is incomplete regardless of total test count.

## Suggested PR Breakdown

Recommended merge sequence:

1. PR1: T0 + T1
2. PR2: T2
3. PR3: T3
4. PR4: T4
5. PR5: T5
6. PR6: T6
7. PR7: T7
8. PR8: T8 + T9

Do not combine T3, T4, and T5 into one giant PR. Those are separable, high-risk changes and should be reviewable independently.

## Exit Criteria

This remediation milestone is done only when:

- durable identity is used on every live execution path
- authoritative kill state blocks execution in practice
- policy is mandatory before tool dispatch
- dangerous tools fail closed by default
- degraded convergence behavior is explicit
- distributed kill is gated/disabled honestly in this milestone unless a separate fully planned follow-on milestone supersedes this decision
- the minimal production test set exists and passes
- public claims match runtime truth
