# ITP Remediation Package

Status: March 11, 2026

Purpose: define the authoritative documentation package for rebuilding the ADE `ITP Events` surface and its upstream capture/ingest/streaming path to a production engineering bar.

This package is based on the live repository state, not on older aspirational architecture material. If this package conflicts with older notes, this package wins.

## Read Order

1. `01_MASTER_SPEC.md`
2. `02_IMPLEMENTATION_PLAN.md`
3. `03_VERIFICATION_AND_RELEASE_GATES.md`
4. `04_AGENT_HANDOFF_BRIEF.md`

## Package Roles

- `01_MASTER_SPEC.md`
  - defines the problem, scope, target architecture, invariants, ownership boundaries, and non-negotiable standards
- `02_IMPLEMENTATION_PLAN.md`
  - translates the master spec into atomic workstreams, file touch points, dependencies, and acceptance criteria
- `03_VERIFICATION_AND_RELEASE_GATES.md`
  - defines the tests, audits, parity checks, and release gates required to prevent recurrence
- `04_AGENT_HANDOFF_BRIEF.md`
  - the final implementation brief that can be handed to a build agent with minimal additional context

## Standard

This package assumes the following bar:

- No ambiguous ownership of a public contract.
- No UI field whose label overstates what the backend actually knows.
- No “live” surface without real live wiring and reconnect semantics.
- No duplicated critical-path producer implementation with divergent event shapes.
- No implementation accepted without contract tests, integration tests, and operator-visible verification.

## Current Repository Reality

The current ADE has:

- a snapshot-only `ITP Events` route in `dashboard/src/routes/itp/+page.svelte`
- a thin `/api/itp/events` route in `crates/ghost-gateway/src/api/itp.rs`
- richer session-level event/replay surfaces already implemented elsewhere
- websocket infrastructure already capable of real-time event fan-out
- multiple extension-side ITP implementations with divergent payload semantics

That means this work is not a greenfield page build. It is a contract and pipeline consolidation effort.
