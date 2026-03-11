# Search Remediation Package

Status: March 11, 2026

Purpose: define the documentation package required to rebuild ADE search into a cohesive, authoritative, end-to-end product surface with one canonical contract, one routing model, one relevance model, and one verification standard.

This package is based on the live code. If this package conflicts with older search notes or aspirational architecture text, this package wins for the search surface.

## Package Contents

1. `SEARCH_FINDINGS_AUDIT.md`
   - Confirmed defects, wiring gaps, and contract weaknesses in the current implementation.
   - Use this first to understand why the current surface is not production-grade.

2. `SEARCH_TARGET_ARCHITECTURE.md`
   - The target operating model for search across backend, SDK, dashboard, and command palette.
   - Defines canonical routing, contracts, ranking, and ownership.

3. `SEARCH_EXECUTION_PLAN.md`
   - The implementation program broken into phases, workstreams, sequence, and exit gates.
   - Use this to plan and stage the actual build.

4. `SEARCH_VALIDATION_PLAN.md`
   - The required automated and manual checks that must pass before search is considered complete.
   - This is the test doctrine for the remediation.

5. `SEARCH_AGENT_HANDOFF_SPEC.md`
   - The authoritative handoff document for an implementation agent.
   - This is the single execution brief that should be handed to the build agent after the supporting documents are reviewed.

## Reading Order

Read in this order:

1. `SEARCH_FINDINGS_AUDIT.md`
2. `SEARCH_TARGET_ARCHITECTURE.md`
3. `SEARCH_EXECUTION_PLAN.md`
4. `SEARCH_VALIDATION_PLAN.md`
5. `SEARCH_AGENT_HANDOFF_SPEC.md`

## Non-Negotiable Bar

This package assumes the following engineering standard:

- No search result type without an addressable destination.
- No duplicated search semantics for the same domain.
- No global search path that disagrees with the corresponding domain page.
- No result count that silently means "returned count" while being labeled "total".
- No ranking model that is only entity-type bias disguised as relevance.
- No command palette or search page that loses user intent across navigation.
- No live audit that proves only page load while missing click-through, deep link, and contract correctness.

## Primary Sources

- `dashboard/src/routes/search/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`
- `dashboard/src/routes/memory/+page.svelte`
- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/routes/sessions/+page.svelte`
- `dashboard/src/routes/goals/+page.svelte`
- `crates/ghost-gateway/src/api/search.rs`
- `crates/ghost-gateway/src/api/memory.rs`
- `crates/cortex/cortex-storage/src/queries/fts_queries.rs`
- `packages/sdk/src/search.ts`
- `dashboard/scripts/live_knowledge_audit.mjs`
