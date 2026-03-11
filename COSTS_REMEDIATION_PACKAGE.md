# Costs Remediation Package

Status: March 11, 2026

Purpose: define, sequence, and operationalize the full remediation of ADE cost tracking so it can be implemented end to end without guesswork, hidden drift, or local optimization at the expense of system truth.

This package is intentionally split into five documents:

1. `COSTS_REMEDIATION_PACKAGE.md`
2. `COSTS_MASTER_REMEDIATION_SPEC.md`
3. `COSTS_REMEDIATION_IMPLEMENTATION.md`
4. `COSTS_REMEDIATION_TASKS.md`
5. `COSTS_AGENT_HANDOFF.md`

Reading order:

1. Read this package index.
2. Read `COSTS_MASTER_REMEDIATION_SPEC.md` as the authority.
3. Read `COSTS_REMEDIATION_IMPLEMENTATION.md` for exact build shape.
4. Read `COSTS_REMEDIATION_TASKS.md` for dependency order and completion tracking.
5. Hand `COSTS_AGENT_HANDOFF.md` to the execution agent.

The package exists because the current ADE cost surface is not one system. It is a cluster of partially-related implementations:

- the gateway cost tracker
- spending-cap enforcement
- autonomy budget checks
- session cost reporting
- the `/costs` page
- the agent detail cost card
- the session detail cost readout
- websocket refresh behavior
- service-worker cache policy

Those surfaces currently disagree on at least one of:

- source of truth
- freshness model
- daily boundary semantics
- pricing semantics
- contract ownership
- live update behavior

This package defines one canonical remediation path.

## Non-Negotiable Outcomes

- No ADE surface may display cost derived from a different formula than the backend ledger.
- No spending-cap decision may use less information than the UI shows.
- No "daily" metric may persist across UTC day rollover without an explicit exception.
- No session cost may depend on pagination window size.
- No dashboard surface may maintain a duplicate cost model when the SDK type already exists.
- No live cost surface may require a full page reload to become truthful.
- No completion claim is valid without backend tests, SDK tests, and dashboard behavior checks.

## Final Artifact

The terminal document in this package is `COSTS_AGENT_HANDOFF.md`.

That handoff document is written to be directly executable by an implementation agent, but it is not authoritative on its own. The authority order is:

1. `COSTS_MASTER_REMEDIATION_SPEC.md`
2. `COSTS_REMEDIATION_IMPLEMENTATION.md`
3. `COSTS_REMEDIATION_TASKS.md`
4. `COSTS_AGENT_HANDOFF.md`

If the handoff document conflicts with the master spec, the master spec wins.
