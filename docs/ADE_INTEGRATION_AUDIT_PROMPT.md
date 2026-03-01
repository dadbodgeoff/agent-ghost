# ADE Integration Audit Prompt

> Copy everything below the line into a new agent session.

---

## Task: GHOST ↔ ADE Integration Verification Audit

You are an integration auditor. Your job is to verify that the GHOST backend platform
(37 Rust crates) and the ADE frontend (Svelte 5 dashboard + browser extension) are
fully mapped, contract-aligned, and ready to connect. Produce a single audit document
at `docs/ADE_INTEGRATION_AUDIT.md` with your findings.

### Context Files

Read these first to understand the system:
- `docs/ADE_DESIGN_PLAN.md` — The ADE design plan with all planned endpoints, routes, components, and data flows
- `README.md` — Platform overview and architecture
- `Cargo.toml` — Workspace members (all 37 crates)
- `FILE_MAPPING.md` — Existing file mapping documentation
- `AGENT_ARCHITECTURE.md` — Agent architecture documentation

### Phase 1: Backend API Surface Audit

For every REST endpoint in `crates/ghost-gateway/src/api/`, do the following:

1. Read each file in `crates/ghost-gateway/src/api/` (agents.rs, audit.rs, convergence.rs, costs.rs, goals.rs, health.rs, memory.rs, mesh_routes.rs, oauth_routes.rs, push_routes.rs, safety.rs, sessions.rs, websocket.rs)
2. Read the router setup in `crates/ghost-gateway/src/` to find where routes are mounted (look for `Router::new()` and `.route()` calls)
3. For EACH endpoint, document:
   - HTTP method and path (e.g., `GET /api/agents`)
   - Request type (query params, path params, JSON body) — extract the actual Rust struct
   - Response type — extract the actual Rust serialization struct
   - Which crate it delegates to (e.g., `cortex_storage::queries::...`, `ghost_audit::...`)
   - Whether the endpoint is actually wired in the router (not just defined but never mounted)

4. Cross-reference against `docs/ADE_DESIGN_PLAN.md` Appendix A (22 planned endpoints):
   - Which planned endpoints ALREADY EXIST in the gateway code?
   - Which planned endpoints are MISSING and need to be built?
   - Are there existing endpoints NOT mentioned in the design plan that the ADE should consume?

### Phase 2: WebSocket Event Contract Audit

1. Read `crates/ghost-gateway/src/api/websocket.rs` — document every `WsEvent` variant with its exact field names and types
2. Read `dashboard/src/lib/api.ts` — document how the dashboard connects to WebSocket
3. Read all files in `dashboard/src/lib/stores/` — document which stores exist and what WebSocket events they consume (if any)
4. Cross-reference against `docs/ADE_DESIGN_PLAN.md` Appendix C (11 event types):
   - Which events are defined in Rust but NOT consumed by any dashboard store?
   - Which planned events don't exist in the Rust WsEvent enum yet?
   - Are the field names in the Rust structs consistent with what the JS/TS code expects?

### Phase 3: Data Model Contract Audit

For each data type that crosses the backend→frontend boundary:

1. Find the Rust struct (with `#[derive(Serialize)]`) in the gateway API handlers
2. Find the corresponding TypeScript type or interface in the dashboard code (if it exists)
3. Verify field name alignment — Rust uses snake_case, check if serde renames are applied or if the frontend expects camelCase
4. Check for mismatches:
   - Fields present in Rust but not consumed in TS
   - Fields expected in TS but not present in Rust response
   - Type mismatches (e.g., Rust `i64` vs TS `number`, Rust `Option<String>` vs TS nullable)
   - Enum serialization format (does Rust use `#[serde(tag = "type")]` and does TS handle it?)

Key data models to audit:
- `AgentInfo` (agents.rs) ↔ agent store
- `ConvergenceScoreResponse` (convergence.rs) ↔ convergence store
- `AgentCostInfo` (costs.rs) ↔ costs (no store yet?)
- `WsEvent` variants (websocket.rs) ↔ WebSocket message parsing
- Audit entries (audit.rs) ↔ audit display
- Session data (sessions.rs) ↔ session store
- Memory snapshots (memory.rs) ↔ memory display
- Safety/kill switch status (safety.rs) ↔ security display

### Phase 4: Database Schema ↔ API Contract Audit

1. Read the migration files in `crates/cortex/cortex-storage/src/migrations/` to understand the actual SQLite schema
2. Read the query modules in `crates/cortex/cortex-storage/src/` (look for `queries/` or query functions)
3. For each API endpoint that queries the database, verify:
   - The SQL column names match what the Rust structs expect
   - The table exists (migration was applied)
   - NULL handling is consistent (SQL nullable columns map to `Option<T>` in Rust)
   - Any JOIN or GROUP BY queries reference columns that actually exist

Cross-reference with `SQL_COLUMN_MISMATCH_AUDIT.md` for known issues.

### Phase 5: Dashboard Route ↔ API Dependency Map

For each route in `dashboard/src/routes/`:

1. Read the `+page.svelte` file
2. Document which API endpoints it calls (look for `api.get()`, `api.post()`, `fetch()`)
3. Document which stores it uses
4. Document which components it renders
5. Verify the API endpoints it calls actually exist and return the data shape it expects

Cross-reference against `docs/ADE_DESIGN_PLAN.md` Appendix B (23 planned routes):
- Which routes exist but are stubs (no real data fetching)?
- Which routes are planned but don't exist yet?
- Which routes call endpoints that don't exist yet?

### Phase 6: Component ↔ Data Contract Audit

For each component in `dashboard/src/components/`:

1. Read the component file
2. Document its props (exported variables / `export let` / `$props()`)
3. Document what data shape each prop expects
4. Check if any route actually passes data to this component
5. Flag components that are built but never used (orphaned)

### Phase 7: Crate Dependency Chain Audit

For the ADE to work, certain crate functions must be accessible from the gateway. Verify:

1. Read `crates/ghost-gateway/Cargo.toml` — what crates does the gateway depend on?
2. For each planned ADE endpoint in the design plan, trace the dependency:
   - Endpoint → gateway handler → which crate function → is that crate a dependency?
   - Example: `/api/memory/search` needs `cortex_retrieval::hybrid_search()` — is `cortex-retrieval` in the gateway's dependencies?
3. Flag any planned endpoints that would require adding new crate dependencies to the gateway

### Phase 8: Authentication & Authorization Audit

1. Read the auth middleware in `crates/ghost-gateway/src/auth/` or wherever token validation lives
2. Document which endpoints require authentication and which are public
3. Check if the dashboard sends the auth token correctly for every API call
4. Verify WebSocket authentication (token in query param) matches what the dashboard sends
5. Check if CORS is configured to allow the dashboard origin

### Phase 9: Missing Contracts Summary

Produce a final summary table:

```
| Gap Type | Description | Backend File | Frontend File | Severity |
|----------|-------------|--------------|---------------|----------|
| Missing endpoint | /api/memory/search not implemented | — | — | High |
| Field mismatch | AgentInfo.status is String, store expects enum | agents.rs | agents store | Medium |
| Orphaned component | CausalGraph.svelte never rendered | — | CausalGraph.svelte | Low |
| Missing dependency | cortex-retrieval not in gateway Cargo.toml | Cargo.toml | — | High |
| ... | ... | ... | ... | ... |
```

Categorize each gap as:
- **Critical**: Blocks Phase 1 of the ADE plan (real-time foundation)
- **High**: Blocks Phase 2 (core ADE features)
- **Medium**: Blocks Phase 3-4 or causes data inconsistency
- **Low**: Cosmetic, orphaned code, or documentation gap

### Output Format

Write the full audit to `docs/ADE_INTEGRATION_AUDIT.md` with:
1. An executive summary (what percentage of contracts are aligned)
2. Each phase as a section with detailed findings
3. The missing contracts summary table
4. A prioritized action list: what to fix first to unblock Phase 1

Be thorough. Read every file. Don't assume — verify. If a function is called but you can't find its definition, flag it. If a field name looks like it might be wrong, check the actual Rust struct. This audit is the foundation for the entire ADE build.
