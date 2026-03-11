# ADE Agent Surface Verification Plan

Status: March 11, 2026

Authoritative design: `ADE_AGENT_SURFACE_REMEDIATION_SPEC.md`
Implementation plan: `ADE_AGENT_SURFACE_IMPLEMENTATION_PLAN.md`

## Verification Standard

This remediation is only complete when correctness is proven across four layers:

1. gateway contract correctness
2. SDK contract correctness
3. dashboard behavior correctness
4. degraded-mode truthfulness

Passing type-check alone is not meaningful for this work.

## Test Matrix

### A. Gateway Contract Tests

Target location:

- `crates/ghost-gateway/tests/agents_api_tests.rs` or equivalent focused test file

Required cases:

1. `GET /api/agents` returns canonical lifecycle, safety, and effective state fields.
2. `GET /api/agents` returns `ready` effective state for healthy agents rather than ambiguous legacy values.
3. paused agents appear as paused in the agent summary route.
4. quarantined agents appear as quarantined in the agent summary route.
5. platform kill state overrides effective state appropriately.
6. `GET /api/agents/:id` returns action policy that matches backend legality.
7. `GET /api/agents/:id/overview` returns:
   - agent
   - convergence
   - cost
   - recent_sessions
   - recent_audit_entries
   - crdt_summary
   - integrity_summary
   - panel_health
8. sessions queried for a given agent include only sessions in which that agent participated.
9. pause emits operational status event.
10. quarantine emits operational status event.
11. resume emits operational status event.
12. delete emits operational status event.

### B. SDK Tests

Target location:

- `packages/sdk/src/__tests__/client.test.ts`

Required cases:

1. `client.agents.list()` matches the canonical summary shape.
2. `client.agents.get(id)` parses the agent detail contract.
3. `client.agents.getOverview(id)` parses the overview contract.
4. runtime sessions agent filter serializes correctly if added to the shared sessions route.
5. WebSocket typed events include the operational-status event variant.

### C. Dashboard Component and Route Tests

Target location:

- `dashboard/tests/agents.spec.ts`

Required cases:

1. agents list renders ready agents correctly.
2. agents list updates after pause without full-page reload.
3. agents list updates after quarantine without full-page reload.
4. agents list updates after resume without full-page reload.
5. agent detail loads from overview route and renders all expected panels.
6. recent sessions shown on detail page belong only to the viewed agent.
7. panel failure renders explicit unavailable/error messaging.
8. paused agent resume path works.
9. quarantined agent shows gated resume flow instead of a generic resume action.
10. quarantine resume sends required acknowledgments.

### D. Manual Integration Checks

These should be run even if automated coverage passes.

1. Start gateway and dashboard.
2. Create or seed at least two agents.
3. Confirm `/agents` reflects both agents correctly.
4. Pause one agent and verify:
   - list page updates
   - detail page updates
   - effective state remains correct after reload
5. Quarantine one agent and verify:
   - list page updates
   - detail page updates
   - access pullback side effects do not corrupt displayed status
6. Attempt resume from quarantine without required confirmations and confirm the UI blocks that path.
7. Complete the gated quarantine resume flow and verify:
   - action succeeds
   - UI reflects resumed state
   - monitoring notice is shown
8. Simulate one failed overview dependency and confirm the relevant panel shows unavailable rather than empty.

## Release Gates

All of the following must pass before the remediation is considered complete:

- gateway tests covering status derivation and event emission
- SDK tests covering the new contract
- dashboard e2e tests covering list/detail/safety flows
- `pnpm -C dashboard check`
- relevant Rust test suite for touched gateway tests
- generated types refreshed with no parity drift

## Failure Modes To Explicitly Test

These are the historical fault lines. They must be tested directly:

1. `Ready` vs `Running` drift
2. pause/quarantine not reflected in `/api/agents`
3. list page not reacting to safety changes
4. global sessions appearing in agent detail
5. quarantine resume exposed but impossible
6. missing panel data incorrectly rendered as "no data"
7. shared store using dead status labels

## Acceptance Checklist

The final change set is acceptable only if each answer is "yes":

- Does the backend own operational truth?
- Does the SDK encode that truth without local reinterpretation?
- Does the dashboard render that truth consistently in list and detail pages?
- Are all safety transitions visible without manual reload?
- Is every agent-specific panel actually agent-scoped?
- Can the UI complete every action it exposes?
- Do degraded dependencies surface as degraded?
- Do tests pin these guarantees down?
