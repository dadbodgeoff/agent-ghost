# ADE Security Remediation Package

Status: draft for implementation handoff on March 11, 2026.

This package turns the ADE Security review into an executable remediation
program. It is not a brainstorming set. It is a build package intended to let a
separate agent implement the work start to finish without inventing scope,
contracts, or release criteria.

## Package Goal

Produce a Security surface that is:

- contract-correct across gateway, SDK, dashboard, websocket, and export flows
- permission-aware across the full ADE shell, not just the `/security` route
- live and operationally trustworthy under success, denial, and degraded states
- verifiable through automated tests and explicit release gates

## Reading Order

Read in this order:

1. `01_PROBLEM_STATEMENT.md`
2. `02_TARGET_ARCHITECTURE.md`
3. `03_CONTRACT_MATRIX.md`
4. `04_IMPLEMENTATION_PLAN.md`
5. `05_VERIFICATION_AND_RELEASE_GATES.md`
6. `06_AGENT_HANDOFF.md`
7. `07_EXECUTION_CONTROL_PLAN.md`
8. `08_WORKSTREAM_PLAN.md`
9. `09_EXECUTION_TRACKER.md`
10. `10_TRACEABILITY_MATRIX.md`
11. `11_DRIFT_CONTROL_PROTOCOL.md`
12. `12_SIGNOFF_PACKET.md`
13. `13_VERIFICATION_COMMAND_RUNBOOK.md`
14. `14_INDEPENDENT_REVIEW_CHECKLIST.md`

## Document Roles

- `01_PROBLEM_STATEMENT.md`
  Defines the audited failures, remediation goals, non-goals, and engineering
  rules.
- `02_TARGET_ARCHITECTURE.md`
  Defines the end-state system shape and the required UI/API/WS relationships.
- `03_CONTRACT_MATRIX.md`
  Freezes the wire contracts, action gating, and canonical vocabularies.
- `04_IMPLEMENTATION_PLAN.md`
  Breaks the work into buildable phases with exact file-level scope.
- `05_VERIFICATION_AND_RELEASE_GATES.md`
  Defines the required tests, manual checks, and cutover criteria.
- `06_AGENT_HANDOFF.md`
  The final execution brief to hand to an implementation agent.
- `07_EXECUTION_CONTROL_PLAN.md`
  Defines how the remediation is run, governed, and advanced without drift.
- `08_WORKSTREAM_PLAN.md`
  Breaks the remediation into orchestrated workstreams with dependencies and
  evidence outputs.
- `09_EXECUTION_TRACKER.md`
  Provides work-package level execution tracking and acceptance checkpoints.
- `10_TRACEABILITY_MATRIX.md`
  Maps findings to requirements, code scope, tests, and closeout evidence.
- `11_DRIFT_CONTROL_PROTOCOL.md`
  Defines the mandatory pre-change and post-change drift checks.
- `12_SIGNOFF_PACKET.md`
  Defines the final evidence bundle required for engineering signoff.
- `13_VERIFICATION_COMMAND_RUNBOOK.md`
  Provides the exact repo commands for drift checks, tests, and final
  verification runs.
- `14_INDEPENDENT_REVIEW_CHECKLIST.md`
  Provides the checklist an outside engineer can use to sign off the work.

## Authoritative Intent

If there is a conflict:

- the contract matrix owns wire semantics
- the implementation plan owns sequencing and file scope
- the verification plan owns definition of done
- the agent handoff owns execution behavior for the implementing agent

## Inputs To This Package

This package is grounded in the reviewed code surface, especially:

- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/components/FilterBar.svelte`
- `dashboard/src/components/AuditTimeline.svelte`
- `packages/sdk/src/safety.ts`
- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-gateway/src/api/authz_policy.rs`
- `crates/ghost-audit/src/query_engine.rs`
- `crates/ghost-gateway/src/sandbox_reviews.rs`

## Outcome

When this package is complete and executed, a delivery agent should be able to:

- fix the audited failures
- close the known contract drift
- align security actions with authorization state everywhere they surface
- add the required tests
- prove the Security experience is coherent across the ADE
- produce a signoff-grade evidence packet for final review
