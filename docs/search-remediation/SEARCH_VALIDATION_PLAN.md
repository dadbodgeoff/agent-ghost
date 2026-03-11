# Search Validation Plan

Status: March 11, 2026

Purpose: define the test and verification bar required before search remediation can be considered complete.

## Validation Doctrine

Search validation must prove:

- contract correctness
- cross-surface consistency
- navigability
- visibility-rule correctness
- degraded-state correctness

Page-load-only checks are insufficient.

## Required Automated Coverage

### A1. Gateway Unit and Integration Tests

Add tests for:

1. true total semantics
   - returned count differs from total matches when truncation occurs
2. archived-memory suppression
   - archived memory hidden by default in global search
3. canonical memory parity
   - equivalent memory query through global search and memory search returns the same memory ids under default rules
4. session content search
   - session appears when query matches event content, not only session id
5. proposal content search
   - proposal appears when query matches searchable proposal body or target metadata
6. normalized ranking
   - exact lower-domain match beats weak higher-domain match when appropriate
7. invalid type handling
   - unknown type values fail clearly or are explicitly ignored according to contract
8. partial degradation
   - one failing domain returns degradation metadata instead of silent omission

### A2. SDK Contract Checks

Add or extend checks so that:

- generated types match the live OpenAPI contract
- `packages/sdk/src/search.ts` does not fork the contract silently
- navigation metadata fields are typed and consumed correctly

### A3. Dashboard Behavioral Tests

Add E2E or live audit checks for:

1. `/search?q=...` restores the query and results
2. submitting on `/search` updates the URL
3. clicking a memory result lands on a memory surface that shows the matching item
4. clicking an audit result lands on a security surface that shows the matching entry
5. command palette result click uses the same destination behavior as the search page
6. no-result state renders correctly
7. degraded-state messaging renders when one domain search fails

## Required Live Audit Extensions

Extend `dashboard/scripts/live_knowledge_audit.mjs` or add a new dedicated search audit to prove:

- memory marker result click-through
- audit marker result click-through
- search URL restore after page reload
- archived memory exclusion from global search
- command palette navigation parity with `/search`

## Manual Verification Checklist

Manual verification is still required for:

1. mixed-type relevance sanity
2. keyboard interaction in command palette and search page
3. browser back and forward behavior after multiple searches
4. direct deep link opening from a cold session
5. readable degraded-state UX

## Release Gate

Search remediation cannot ship unless:

- gateway tests pass
- dashboard type checks pass
- search live audit passes
- no known non-addressable result type remains
- no known domain-semantic fork remains between global and domain search

## Suggested Commands

At minimum, the implementing agent should leave behind a runnable sequence equivalent to:

```bash
cargo test -p ghost-gateway search::
pnpm --dir dashboard check
pnpm --dir dashboard audit:knowledge-live -- --mode preview
```

If a dedicated search live audit is added, that command becomes mandatory for signoff.
