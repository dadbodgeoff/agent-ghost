# Search Target Architecture

Status: March 11, 2026

Purpose: define the target-state architecture for ADE search across gateway, SDK, dashboard, and command palette.

## Architectural Principles

### P1. Global search is an orchestrator, not a parallel product.

Global search may compose results across domains, but it must not invent domain-specific semantics that already exist elsewhere.

Corollary:

- memory search semantics belong to the memory search pipeline
- audit search semantics belong to the audit query pipeline
- search routing policy belongs to one shared dashboard module

### P2. Every result must be resolvable.

A search result is only valid if it includes enough information to navigate to a destination that preserves the context of the match.

### P3. Query state is canonical in the URL.

If a query can be reproduced, it must be representable in URL state.

### P4. Relevance must be query-driven.

Cross-domain ranking can normalize domain-specific scores, but it cannot use entity type as the primary ranker.

### P5. Visibility rules must be consistent.

If a domain excludes archived, deleted, or restricted entities by default, global search must respect the same rule unless an explicit override is present.

## Target Backend Model

### Unified Search Endpoint Role

`GET /api/search` becomes an aggregation endpoint with the following responsibilities:

- parse global query and type filters
- call canonical per-domain query functions
- normalize returned scores
- merge and sort results
- return true total metadata
- return stable navigation metadata

It must not:

- implement custom search semantics for domains that already own a search path
- fabricate totals
- expose non-addressable results

### Result Contract

Each search result must carry:

- `result_type`
- `id`
- `title`
- `snippet`
- `score`
- `navigation`
- `match_context`

Recommended structure:

```ts
type SearchResult = {
  result_type: 'agent' | 'session' | 'memory' | 'proposal' | 'audit';
  id: string;
  title: string;
  snippet: string;
  score: number;
  navigation: {
    href: string;
    route_kind: 'detail' | 'collection';
    focus_id?: string;
    query?: string;
  };
  match_context: {
    matched_fields: string[];
    highlight_terms?: string[];
  };
};
```

Notes:

- `navigation.href` must be computed server-side or by one shared client helper, not by multiple pages.
- `route_kind=collection` is allowed only if the collection page supports `focus_id` or equivalent query restoration.

### Per-Type Ownership

- agents: direct query in unified search is acceptable if no richer domain search exists
- sessions: introduce a canonical session search query that can search session id, sender, and searchable event content
- memories: reuse `/api/memory/search` internals or factor them into a shared query module
- proposals: introduce a canonical proposal search query covering operation, agent, target type, and searchable content
- audit: either reuse audit query internals or factor a shared audit search query

### Count Semantics

The response must distinguish:

- `returned_count`
- `total_matches`
- optional `total_matches_by_type`

The UI must never infer one from another.

## Target Dashboard Model

### Shared Search Routing Module

Create one dashboard utility that owns search navigation policy for all surfaces:

- global search page
- command palette
- future omniboxes

Responsibilities:

- map result payloads to URLs
- preserve `q`, `focus`, `type`, and other state when needed
- prevent route drift between search entry points

### Search Page Behavior

The search page must:

- derive query state from the URL
- write query state back to the URL on submit and filter changes
- support direct deep links
- group by type without hiding cross-type rank order if a blended mode is desired
- render empty, partial-failure, and degraded states explicitly

### Destination Behavior

#### Memory

If a dedicated memory detail page does not exist, `/memory` must accept:

- `q`
- `focus`

and restore the matching list state while visibly focusing the target memory.

#### Security

`/security` must accept:

- `search`
- `focus`

and restore the audit filter state while focusing the target entry if present.

#### Sessions and Goals

Detail pages already exist and should remain the primary target.

## Target Command Palette Model

The command palette is not a separate search product.

It must:

- use the same result contract as the search page
- use the same shared routing helper
- open the same destinations with the same context preservation
- optionally bias commands over results in UI, but not fork navigation semantics

## Failure Model

The unified endpoint should support partial degradation.

Example:

- memory query succeeds
- audit query succeeds
- proposal query fails

Expected behavior:

- return successful result groups
- return degradation metadata for failed domains
- show degraded-state messaging in the UI

Silent omission is not acceptable.

## Ownership Model

- backend unified search contract owner: `crates/ghost-gateway/src/api/search.rs`
- domain query owners: domain APIs or extracted shared query modules
- dashboard navigation owner: one shared helper under `dashboard/src/lib`
- validation owner: gateway tests plus dashboard live audit coverage
