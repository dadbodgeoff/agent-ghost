# ADE Channels Remediation Package

Purpose: provide an implementation-grade document set for rebuilding the ADE channels surface and runtime path into one coherent, production-grade system.

This package is intentionally opinionated. It makes the architectural decisions that an implementation agent would otherwise be forced to guess.

## Reading Order

1. `MASTER_SPEC.md`
2. `ARCHITECTURE_AND_CONTRACT.md`
3. `MIGRATION_AND_CUTOVER.md`
4. `TEST_DOCTRINE.md`
5. `IMPLEMENTATION_TASKS.md`
6. `AGENT_HANDOFF.md`

## Document Roles

- `MASTER_SPEC.md`: authoritative scope, failures, requirements, and acceptance bar.
- `ARCHITECTURE_AND_CONTRACT.md`: source-of-truth model, service ownership, REST/WS/SDK/UI contract, and runtime flows.
- `MIGRATION_AND_CUTOVER.md`: how to move from the current split config/runtime/UI state to the new authoritative model without drift.
- `TEST_DOCTRINE.md`: the invariants and test suite required to prevent regression.
- `IMPLEMENTATION_TASKS.md`: ordered execution plan with task-level verification gates.
- `AGENT_HANDOFF.md`: the final build brief to hand to an implementation agent.

## Authority

If any downstream doc conflicts with `MASTER_SPEC.md`, `MASTER_SPEC.md` wins.

If `AGENT_HANDOFF.md` conflicts with the supporting documents, the supporting documents win and the handoff must be updated before implementation proceeds.
