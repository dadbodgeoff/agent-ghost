# OpenAPI Parity Audit

## Purpose

This document is the first-pass audit artifact for `DR-001`: router/schema
truthfulness.

It records the current measured drift between:

- mounted routes in [bootstrap.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs)
- documented OpenAPI paths in [openapi.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/openapi.rs)

The measurement command is:

```bash
python3 scripts/check_openapi_parity.py
```

Strict mode for CI later:

```bash
python3 scripts/check_openapi_parity.py --fail-on-drift
```

## Current Snapshot

Current measured counts:

- mounted routes: 120
- documented OpenAPI paths: 116
- intentional router exclusions currently declared: 4
- undocumented mounted routes after exclusions: 0
- stale documented paths after exclusions: 0
- listed OpenAPI helpers missing definitions: 0
- defined OpenAPI helpers omitted from `ApiDoc`: 0

The mounted application route surface is now fully accounted for by either:

- an OpenAPI path definition, or
- an explicit policy exclusion

## Intentional Exclusions

These are the only exclusions currently accepted:

- `/.well-known/agent.json`
  - Mesh discovery metadata is intentionally outside the OpenAPI application
    route surface.
- `/a2a`
  - The mesh transport ingress path is intentionally excluded from the
    application OpenAPI surface.
- `/api/openapi.json`
  - The schema endpoint itself is not modeled as an application data route.
- `/api/ws`
  - The websocket upgrade endpoint is not represented in the current OpenAPI
    document.

If more exclusions are needed, they must be added to
[openapi_parity_policy.json](/Users/geoffreyfernald/Documents/New project/agent-ghost/schemas/openapi_parity_policy.json)
with a concrete rationale.

## Current Route-Only Drift

Current undocumented mounted routes after exclusions:

- none

There is currently no route-only drift.

## Risk Assessment

### Release Risk

Low, provided parity stays enforced.

The current risk is regression, not known present drift.

### OSS Risk

Moderate.

External consumers are protected as long as parity remains enforced and the
exclusion policy stays explicit. The primary remaining risk is future drift.

### Contributor Risk

Moderate.

Contributors can still update only the router or only the schema if parity is
not checked in CI.

## Recommended Closure Plan

## Phase 1: Policy and Inventory

- keep the parity checker and policy file in repo
- require explicit rationale for every exclusion
- inventory the route-only drift by domain owner

## Phase 2: Dashboard-Critical Coverage

Bring the schema into parity for all dashboard-used mounted routes first:

- auth/session
- channels
- costs
- ITP
- OAuth
- profiles
- search
- studio
- traces
- workflows

Status:

- complete for the current dashboard-facing route surface
- parity checker now also validates that `ApiDoc` helper names map to real
  `#[utoipa::path]` definitions and that defined helpers are actually included
  in `paths(...)`

## Phase 3: Remaining Route Groups

Status:

- complete for admin backup/export/restore, agent chat, integrity/state,
  memory archival, skill execution, and marketplace
- current parity command:
  - `python3 scripts/check_openapi_parity.py --fail-on-drift`
  - passes with `0` undocumented mounted routes and `0` stale documented paths

## Phase 4: Public Contract Decision

Choose one of:

1. Full route-complete schema for the supported public API
2. Narrow public schema with explicit internal-route exclusion policy

What is not acceptable:

- partial schema plus public generated export without policy

Current decision:

- generated OpenAPI types are no longer re-exported from the public SDK surface
  while schema/codegen compatibility is governed separately

## Phase 5: CI Enforcement

After policy and initial parity work:

- wire `python3 scripts/check_openapi_parity.py --fail-on-drift` into CI
- fail on any new undocumented route unless explicitly excluded with reason

## Exit Criteria

`DR-001` is only closed when all are true:

- every mounted route is either documented or explicitly excluded by policy
- every exclusion is reviewed and justified
- generated type export policy matches actual schema truth
- CI fails on future router/schema drift
