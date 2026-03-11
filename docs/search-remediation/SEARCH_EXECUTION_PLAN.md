# Search Execution Plan

Status: March 11, 2026

Purpose: define the implementation sequence required to move from the current search surface to the target architecture.

## Program Rules

- Do not add new UX on top of broken routing.
- Do not improve ranking before canonical domain ownership is established.
- Do not ship a new result type until its destination is addressable.
- Do not declare completion until automated end-to-end verification passes.

## Workstreams

### W1. Contract and Routing Foundation

Deliverables:

- authoritative search result schema
- shared dashboard result-routing helper
- canonical URL-state behavior on the search page

Tasks:

1. Define the new search response contract in gateway and OpenAPI.
2. regenerate SDK types and align `packages/sdk/src/search.ts`.
3. add a shared dashboard helper for search result navigation.
4. replace local `TYPE_LINKS` in:
   - `dashboard/src/routes/search/+page.svelte`
   - `dashboard/src/components/CommandPalette.svelte`
5. make `/search` submit update URL state.

Exit gate:

- no search route owns local per-type routing policy
- search page round-trips through URL state

### W2. Canonical Domain Query Extraction

Deliverables:

- shared query functions or shared internal services for memory, sessions, proposals, and audit

Tasks:

1. factor memory search so unified search uses the same retrieval path as `/api/memory/search`.
2. implement a canonical session search query over session metadata plus searchable event content.
3. implement a canonical proposal search query with wider searchable fields.
4. implement or extract canonical audit search behavior compatible with the security surface.

Exit gate:

- global search no longer uses bespoke direct SQL for domains that already have or need canonical query paths

### W3. Addressable Destinations

Deliverables:

- result destinations that preserve match context for every result type

Tasks:

1. decide whether Memory gets a dedicated detail page or a focused collection pattern.
2. implement memory focus and query restoration.
3. implement security focus and query restoration for audit hits.
4. verify session and proposal detail navigation remain direct.

Exit gate:

- every search result type lands in a destination that shows why it matched

### W4. Relevance and Totals

Deliverables:

- normalized ranking
- accurate total metadata
- optional by-type totals

Tasks:

1. define normalization strategy for domain scores.
2. remove fixed entity-type score constants as the primary rank source.
3. compute real total matches.
4. expose returned count separately from total matches.

Exit gate:

- API contract no longer mislabels returned rows as total matches
- ranking is defensible for mixed-type result sets

### W5. Degradation and Error Semantics

Deliverables:

- partial failure model
- UI messaging for degraded search

Tasks:

1. define per-domain error reporting in the unified response.
2. plumb degraded-state handling into the search page.
3. ensure command palette does not silently hide backend degradation.

Exit gate:

- failed subqueries are visible, not silently omitted

### W6. Verification and Release Gate

Deliverables:

- unit coverage
- integration coverage
- live audit coverage

Tasks:

1. extend gateway tests for totals, archived suppression, per-type routing metadata, and multi-domain ranking.
2. extend dashboard live audit to click through results and verify focus restoration.
3. add cases for:
   - memory result click-through
   - audit result click-through
   - URL share/restore
   - partial degradation
   - archived memory exclusion

Exit gate:

- all required checks in `SEARCH_VALIDATION_PLAN.md` pass

## Recommended Implementation Sequence

1. W1 Contract and Routing Foundation
2. W2 Canonical Domain Query Extraction
3. W3 Addressable Destinations
4. W4 Relevance and Totals
5. W5 Degradation and Error Semantics
6. W6 Verification and Release Gate

This sequence prevents cosmetic fixes from masking foundational inconsistency.

## Explicit Non-Goals for This Program

- redesigning the ADE visual language
- adding unrelated search result types outside the current five domains
- implementing vector search for every domain in one pass if canonical text search remains incomplete

## Definition of Done

Search remediation is done only when:

- the search page is URL-canonical
- command palette and search page share routing policy
- memory semantics are canonical across global and domain search
- all result types are addressable
- totals are accurate
- ranking is no longer entity-type hardcoded
- live audit proves click-through and deep-link behavior
