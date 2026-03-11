# Sessions Remediation Dossier

Purpose: define the full document set required to rebuild the ADE `Sessions` surface as a coherent, production-grade subsystem.

This package is intentionally small and ordered. It is meant to be read in sequence and then handed to an implementation agent.

## Reading Order

1. `design.md`
2. `contracts.md`
3. `plan.md`
4. `validation.md`
5. `task.md`

## Document Roles

- `design.md`
  - authoritative architecture and subsystem boundaries
- `contracts.md`
  - authoritative payload, cursor, mutation, and UI-state semantics
- `plan.md`
  - execution phases, dependency order, and cut lines
- `validation.md`
  - ship gate, adversarial checks, and completion criteria
- `task.md`
  - direct handoff document for an implementation agent

## Precedence

If these documents conflict:

1. `contracts.md` wins for public behavior and payload shape.
2. `design.md` wins for architecture and ownership boundaries.
3. `validation.md` wins for what counts as done.
4. `task.md` wins for execution order and deliverables.
5. `plan.md` is sequencing guidance, not contract authority.

## Standard

This package assumes the following bar:

- no silent truncation
- no ambiguous session identity semantics
- no optimistic UI that lies about persistence
- no cross-surface drift between Sessions, Agents, Observability, and Replay
- no mutation path without ownership validation, audit lineage, and negative-path tests
- no typed SDK or dashboard wrapper that forks from the gateway contract without an explicit exception record

## Primary Code Surfaces

- `dashboard/src/routes/sessions/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`
- `dashboard/src/routes/sessions/[id]/replay/+page.svelte`
- `dashboard/src/lib/stores/sessions.svelte.ts`
- `dashboard/src/routes/observability/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `packages/sdk/src/runtime-sessions.ts`
- `packages/sdk/src/generated-types.ts`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/tests/operation_journal_tests.rs`
- `dashboard/tests`
