# Search Findings Audit

Status: March 11, 2026

Purpose: record the confirmed state of ADE search based on the live codebase, not on intent.

This document covers:

- the global search page
- the command palette search entry point
- the unified backend search endpoint
- the memory, security, goals, and sessions surfaces that search must integrate with
- the validation currently in place

## Standard

Search is only considered complete if all of the following are true:

- every result type is discoverable
- every result type is navigable
- the clicked destination preserves why the result matched
- the same query yields materially consistent behavior across global search and domain search
- the API contract exposes true counts and stable semantics
- relevance is not a proxy for entity type
- tests cover routing, relevance, visibility rules, and deep linking

## Confirmed Findings

### F1. Memory and audit hits are not addressable results.

The global search page maps:

- `memory -> /memory`
- `audit -> /security`

The command palette duplicates the same behavior.

Implication:

- clicking a memory hit does not focus the memory
- clicking an audit hit does not focus the audit entry
- the search surface returns matches it cannot actually deliver as resolved user intent

Evidence:

- `dashboard/src/routes/search/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`
- `dashboard/src/routes/memory/+page.svelte`
- `dashboard/src/routes/security/+page.svelte`

### F2. The search page does not own canonical URL state.

The page reads `?q=` from the URL, but submit only invokes `doSearch()` and does not update navigation state.

Implication:

- a user can search without producing a shareable URL
- back and forward behavior is inconsistent
- the command palette handoff and direct page usage do not share the same source of truth

Evidence:

- `dashboard/src/routes/search/+page.svelte`

### F3. Unified search bypasses the canonical memory search implementation.

The memory surface uses `/api/memory/search`, which supports:

- FTS5 candidate retrieval
- BM25 ranking
- embedding-based reranking
- post-retrieval filtering
- archived-memory exclusion by default

The unified `/api/search` endpoint instead issues its own direct LIKE query against `memory_snapshots`.

Implication:

- the same memory query can behave differently between the Memory page and global search
- global search can leak archived memories that the domain surface suppresses
- relevance quality is worse in the path users expect to be most powerful

Evidence:

- `crates/ghost-gateway/src/api/search.rs`
- `crates/ghost-gateway/src/api/memory.rs`
- `crates/cortex/cortex-storage/src/queries/fts_queries.rs`

### F4. Global search coverage is materially incomplete for sessions.

Sessions are searched only by:

- `session_id`
- `sender`

They are not searched by:

- event content
- event attributes
- likely user-visible session semantics

Implication:

- a session can be clearly relevant and still not appear in global search
- global search cannot serve as an ADE-wide investigation surface

Evidence:

- `crates/ghost-gateway/src/api/search.rs`
- `dashboard/src/routes/sessions/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`

### F5. Global search coverage is materially incomplete for proposals.

Proposals are searched only by:

- `id`
- `operation`

They are not searched by:

- agent id
- target type
- proposal content
- current state semantics beyond a display snippet

Implication:

- proposal search behaves like a thin lookup, not a domain search
- cross-domain investigation flows are incomplete

Evidence:

- `crates/ghost-gateway/src/api/search.rs`
- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/goals/[id]/+page.svelte`

### F6. Ranking is hardcoded by entity type, not by relevance.

Current scores are constants:

- agent: `1.0`
- memory: `0.9`
- session: `0.8`
- proposal: `0.7`
- audit: `0.6`

Implication:

- exact audit or session matches can be pushed below weak agent or memory matches
- the response is ordered by implementation bias rather than query quality
- the result list is not defensible as a relevance-ranked search experience

Evidence:

- `crates/ghost-gateway/src/api/search.rs`

### F7. `total` is not a true total.

The endpoint truncates the merged result set and then sets `total = results.len()`.

Implication:

- `total` means "returned rows after truncation", not "total matches"
- the API contract is misleading
- the UI cannot reliably present counts or pagination

Evidence:

- `crates/ghost-gateway/src/api/search.rs`
- `packages/sdk/src/search.ts`
- `dashboard/src/routes/search/+page.svelte`

### F8. Result routing is duplicated and already drifting.

The search page and command palette each maintain their own `TYPE_LINKS`.

Implication:

- search routing logic can fork
- new result types can be wired in one surface and broken in another
- there is no single owner for result navigation policy

Evidence:

- `dashboard/src/routes/search/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`

### F9. Validation proves the happy path only.

Current coverage proves:

- a memory marker can appear in search
- a seeded session id can appear in search
- the search page loads and shows a marker

Current coverage does not prove:

- click-through correctness
- URL state behavior
- archived-memory suppression
- per-type routing
- ranking quality
- cross-surface consistency
- error handling under partial result failures

Evidence:

- `crates/ghost-gateway/src/api/search.rs`
- `dashboard/scripts/live_knowledge_audit.mjs`

## Root Cause Summary

The search surface is implemented as a thin merged query and a thin page, not as a product system with domain ownership, routing guarantees, and shared semantics.

The main failure pattern is duplication:

- duplicated memory semantics
- duplicated routing tables
- duplicated notions of what a useful result is

The second failure pattern is contract weakness:

- misleading `total`
- no canonical per-type destination contract
- no requirement that global search preserve domain visibility rules

## Required Outcome

The remediation must produce:

- one authoritative search contract
- one authoritative result-routing policy
- one canonical domain-search owner per entity type
- one verification program that proves end-to-end behavior, not page presence
