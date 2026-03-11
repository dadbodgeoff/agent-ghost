# ADE Security Remediation: Agent Handoff

Status: executable handoff brief on March 11, 2026.

This is the final document to hand to an implementation agent.

## Objective

Implement the ADE Security remediation start to finish so the Security surface
is contract-correct, permission-aware, live, and release-ready.

You are not patching one page. You are aligning the gateway, SDK, shell, and
dashboard security surface into one coherent system.

## Required Reading

Read these before editing code:

1. `docs/ade-security/01_PROBLEM_STATEMENT.md`
2. `docs/ade-security/02_TARGET_ARCHITECTURE.md`
3. `docs/ade-security/03_CONTRACT_MATRIX.md`
4. `docs/ade-security/04_IMPLEMENTATION_PLAN.md`
5. `docs/ade-security/05_VERIFICATION_AND_RELEASE_GATES.md`
6. `docs/ade-security/07_EXECUTION_CONTROL_PLAN.md`
7. `docs/ade-security/08_WORKSTREAM_PLAN.md`
8. `docs/ade-security/09_EXECUTION_TRACKER.md`
9. `docs/ade-security/10_TRACEABILITY_MATRIX.md`
10. `docs/ade-security/11_DRIFT_CONTROL_PROTOCOL.md`
11. `docs/ade-security/13_VERIFICATION_COMMAND_RUNBOOK.md`
12. `docs/ade-security/14_INDEPENDENT_REVIEW_CHECKLIST.md`

## Non-Negotiable Rules

- Do not hide failure as empty state.
- Do not surface privileged actions without permission-aware gating.
- Do not invent new frontend-only filter vocabularies.
- Do not leave shell shortcuts inconsistent with page authorization behavior.
- Do not ship contract changes without SDK alignment.
- Do not stop at implementation without tests.

## Execution Sequence

Execute in this order.

Update `09_EXECUTION_TRACKER.md` as work packages move from `Ready` to
`Accepted`.

### Step 1: Freeze and inspect contracts

- inspect `crates/ghost-gateway/src/api/safety.rs`
- inspect `packages/sdk/src/safety.ts`
- inspect `crates/ghost-audit/src/query_engine.rs`
- inspect `packages/sdk/src/audit.ts`

Deliverable:

- decide and implement canonical safety-status fields
- decide and implement canonical audit filter semantics

### Step 2: Repair backend and SDK contract drift

Files likely in scope:

- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-audit/src/query_engine.rs`
- `packages/sdk/src/safety.ts`
- `packages/sdk/src/audit.ts`
- generated types if applicable

Deliverable:

- dashboard-consumable safety contract
- audit/export contract parity

### Step 3: Build the security orchestration layer

Files likely in scope:

- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/lib/stores/safety.svelte.ts`
- `dashboard/src/lib/stores/audit.svelte.ts`
- optional new store for consolidated security state

Deliverable:

- section-level states
- live refresh behavior
- per-agent intervention surface
- no swallowed subsystem failures

### Step 4: Gate privileged affordances in the full ADE shell

Files likely in scope:

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/components/CommandPalette.svelte`
- `dashboard/src/routes/security/+page.svelte`
- auth-session store if needed

Deliverable:

- `kill-all` affordances only for superadmin
- review decision affordances only for authorized principals
- no unauthorized shortcut registration

### Step 5: Fix filters, rendering, and export coherence

Files likely in scope:

- `dashboard/src/components/FilterBar.svelte`
- `dashboard/src/components/AuditTimeline.svelte`
- `dashboard/src/routes/security/+page.svelte`

Deliverable:

- only canonical filter values are offered
- canonical severities render correctly
- export uses active filters

### Step 6: Add and run tests

Must include:

- gateway contract tests
- SDK tests
- Playwright tests for `/security`
- the verification sequence from `13_VERIFICATION_COMMAND_RUNBOOK.md`

Deliverable:

- passing automated coverage for normal, denied, and degraded cases

## Minimum Definition Of Done

You are done only when all of the following are true.

- Security overview renders the real kill state correctly.
- Security actions are permission-aware before invocation.
- Security audit filters return real, contract-valid results.
- Security export matches on-screen filters.
- Security audit evidence refreshes after relevant websocket events.
- Shell shortcuts and page controls are aligned with backend authz.
- Verification gates in `05_VERIFICATION_AND_RELEASE_GATES.md` pass.

## Closeout Format

When work is finished, report:

1. files changed
2. contracts changed
3. tests added or updated
4. release gates passed
5. residual risks, if any

Also populate `12_SIGNOFF_PACKET.md`.
Ensure the completed work can pass `14_INDEPENDENT_REVIEW_CHECKLIST.md`
without implementation-only context.

## Explicit Warning

Do not declare success because the page compiles.

Success requires:

- contract parity
- authorization parity
- live refresh parity
- evidence/export parity
- automated proof
