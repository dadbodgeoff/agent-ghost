# PC Control Cohesion Package

**Version:** 1.0  
**Date:** March 11, 2026  
**Status:** Execution Package

## Purpose

This package defines the work required to turn the current PC Control surface
into a coherent ADE subsystem with aligned runtime behavior, contracts, UI,
telemetry, and verification.

It is intentionally split into authority layers so an implementation agent can
work from a stable decision stack instead of improvising semantics mid-build.

## Document Graph

Read in this order:

1. `CURRENT_STATE_AUDIT.md`
2. `TARGET_ARCHITECTURE.md`
3. `CONTRACT_SPEC.md`
4. `IMPLEMENTATION_PLAN.md`
5. `VERIFICATION_PLAN.md`
6. `AGENT_BUILD_SPEC.md`

## Authority Model

Use these authority levels:

1. `AGENT_BUILD_SPEC.md`
   - execution entrypoint for the implementation agent
   - summarizes scope, order, and acceptance criteria
   - may reference other docs but must not override them silently

2. `CONTRACT_SPEC.md`
   - canonical behavioral contract for API, SDK, websocket, UI state, and
     runtime semantics
   - if code disagrees with this document, code is wrong unless the document is
     formally updated first

3. `TARGET_ARCHITECTURE.md`
   - canonical structural design
   - defines ownership boundaries and data flow

4. `IMPLEMENTATION_PLAN.md`
   - canonical work breakdown
   - defines phases, file touch points, and dependency ordering

5. `VERIFICATION_PLAN.md`
   - canonical release gate and evidence model

6. `CURRENT_STATE_AUDIT.md`
   - evidence base for why the remediation exists
   - descriptive, not normative once the target design is approved

## Non-Negotiable Principles

- No dashboard control may claim runtime effect unless live execution semantics
  are actually updated.
- No duplicated semantic field may exist without a declared source of truth.
- No websocket event may be emitted unless a consumer contract exists.
- No UI affordance may represent unsupported multiplicity, mutability, or
  durability.
- No safety-critical mutation may rely on restart to become real.
- No release may be declared complete without live-path verification.

## Scope Boundary

In scope:

- gateway PC Control APIs
- dashboard PC Control page
- SDK PC Control client/types
- skill catalog/runtime integration
- config reload and runtime reconciliation
- telemetry, audits, websocket refresh behavior
- automated tests and live audit coverage

Out of scope unless required by the contract:

- redesign of unrelated ADE feature surfaces
- new PC Control skills beyond the existing set
- cross-platform backend expansion beyond current supported platforms

## Deliverable

The final handoff artifact for implementation is:

- `AGENT_BUILD_SPEC.md`

The other documents exist to prevent ambiguity during implementation.
