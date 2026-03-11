# Search Agent Handoff Spec

Status: March 11, 2026

Purpose: provide the authoritative implementation brief for an agent to rebuild ADE search from its current fragmented state into a cohesive, production-grade surface.

This document is the execution contract. The supporting documents in this package provide rationale and detailed constraints. If there is any ambiguity during implementation, resolve it in favor of this spec and the supporting package, not older architecture prose.

## Mission

Build ADE search into one coherent product surface across:

- global search page
- command palette
- gateway unified search API
- domain search surfaces and destinations
- SDK search contract
- automated verification

The finished system must behave as one product, not as a page that happens to call a merged endpoint.

## Mandatory Inputs

The implementing agent must read these before coding:

1. `docs/search-remediation/SEARCH_FINDINGS_AUDIT.md`
2. `docs/search-remediation/SEARCH_TARGET_ARCHITECTURE.md`
3. `docs/search-remediation/SEARCH_EXECUTION_PLAN.md`
4. `docs/search-remediation/SEARCH_VALIDATION_PLAN.md`

## In-Scope Code

Primary code surfaces:

- `crates/ghost-gateway/src/api/search.rs`
- `crates/ghost-gateway/src/api/memory.rs`
- `crates/cortex/cortex-storage/src/queries/fts_queries.rs`
- `packages/sdk/src/search.ts`
- `dashboard/src/routes/search/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`
- `dashboard/src/routes/memory/+page.svelte`
- `dashboard/src/routes/security/+page.svelte`
- `dashboard/scripts/live_knowledge_audit.mjs`

Additional files may be required if the implementation extracts shared helpers or adds dedicated search modules.

## Hard Requirements

### R1. Global search must become an orchestrator.

Do not leave bespoke per-domain SQL in unified search where canonical domain search behavior already exists or should exist.

Minimum required outcome:

- memory search semantics are shared with the canonical memory search path
- sessions and proposals have canonical search semantics, not thin lookup behavior

### R2. Every result must be addressable.

It is unacceptable to return a result that routes to a collection page which cannot restore or focus the matched entity.

Required outcome by type:

- agent: direct detail route
- session: direct detail route
- proposal: direct detail route
- memory: direct detail route or focused collection route with restored query context
- audit: focused security route with restored query context

### R3. Search URL state must be canonical.

The `/search` page must:

- hydrate from URL state
- write query state back to the URL on submit
- support reload and share behavior

### R4. Routing policy must be shared.

The command palette and search page must not maintain separate per-type routing tables.

Required outcome:

- one shared dashboard helper or equivalent single-owner module drives result navigation

### R5. Totals must be truthful.

The API must not label truncated returned rows as total matches.

Required outcome:

- returned count and total matches are distinct
- if by-type totals are exposed, they must also be truthful

### R6. Relevance must be normalized, not hardcoded by entity type.

Removing fixed score constants as the primary rank source is mandatory.

### R7. Degraded search must be explicit.

If one domain search fails, the response and UI must indicate degradation instead of silently dropping that domain.

## Prohibited Shortcuts

The implementing agent must not:

- hardcode new `TYPE_LINKS` in multiple places
- add URL parameters without wiring the destination pages to honor them
- leave memory search split between two different semantic paths
- claim completion on the basis of type-checking plus page-load tests
- keep misleading `total` behavior for compatibility convenience

## Required Deliverables

### D1. Backend

- revised unified search contract
- canonical domain query reuse or extraction
- accurate totals
- normalized ranking
- degraded-state metadata
- tests covering the required scenarios

### D2. SDK

- updated typed search client aligned with generated contract
- no silent fork from OpenAPI-generated types

### D3. Dashboard

- URL-canonical search page
- shared result routing helper
- addressable memory and audit result flows
- command palette parity with global search routing
- degraded-state handling

### D4. Verification

- updated gateway tests
- updated or new dashboard live audit coverage
- evidence in the final change summary of which commands were run and what they proved

## Sequence

Implement in this order:

1. contract and routing foundation
2. canonical domain query extraction
3. addressable destination behavior
4. ranking and totals
5. degraded-state semantics
6. verification hardening

Do not reverse this order unless blocked by concrete code dependencies.

## Acceptance Criteria

The work is complete only if all of the following are true:

1. Searching from `/search` writes the query into the URL and survives reload.
2. Searching from the command palette and clicking a result lands in the same destination as `/search`.
3. Memory hits preserve match context after navigation.
4. Audit hits preserve match context after navigation.
5. Archived memories are not surfaced by default in global search if the memory domain hides them by default.
6. Session and proposal searches cover materially useful fields beyond ids.
7. Result ordering is not driven primarily by hardcoded entity-type weights.
8. The API exposes truthful totals.
9. Automated coverage proves click-through and deep-link behavior.

## Final Output Expected From The Implementing Agent

When the implementing agent finishes, its final report must include:

- what changed at a high level
- which contracts changed
- which routes now consume canonical search state
- which tests and audits ran
- any residual limitations that were intentionally deferred

If any requirement in this spec cannot be met without a larger product decision, the agent must stop, document the blocker precisely, and identify the narrowest decision needed from a human.
