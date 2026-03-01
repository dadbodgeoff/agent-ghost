# Ghost-Gateway Full-Stack Connectivity Audit

**Date:** 2026-02-28
**Scope:** ghost-gateway routes, handlers, backing stores, schemas, write paths, state fields, crate deps, config fields, WS events, error paths, cross-crate types

## Findings

| # | File | Line | Finding | Severity | Category |
|---|------|------|---------|----------|----------|
| 1 | `convergence-monitor/src/monitor.rs` | 742 | **Schema mismatch: convergence_scores INSERT uses columns `(agent_id, score, level, recorded_at)` but v017 migration defines `(id TEXT PK, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at, event_hash BLOB NOT NULL, previous_hash BLOB NOT NULL)`.** Missing `id` (PK), `composite_score` vs `score`, missing `signal_scores` (NOT NULL), missing `event_hash`/`previous_hash` (NOT NULL). This INSERT will fail at runtime. | S0 | 3 |
| 2 | `convergence-monitor/src/monitor.rs` | 716 | **Schema mismatch: itp_events INSERT uses columns `(session_id, agent_id, event_type, payload, timestamp, event_hash, previous_hash)` but v017 migration defines `(id TEXT PK, session_id, event_type, sender, timestamp, sequence_number, content_hash, content_length, privacy_level, latency_ms, token_count, event_hash, previous_hash, attributes, created_at)`.** Column `agent_id` doesn't exist (it's `sender`), column `payload` doesn't exist, missing `id` (PK, NOT NULL). This INSERT will fail at runtime. | S0 | 3,10 |
| 3 | `ghost-gateway/src/api/oauth_routes.rs` | 28–42 | **`list_providers` uses `State(_state)` — state parameter is unused (prefixed with `_`).** Returns hardcoded JSON array. Not wired to `ghost-oauth` crate or any backing store. | S1 | 1,2 |
| 4 | `ghost-gateway/src/api/oauth_routes.rs` | 60–75 | **`connect` uses `State(_state)` — state parameter is unused.** Returns hardcoded placeholder auth URL. Not wired to `ghost-oauth` OAuthBroker. | S1 | 1,2 |
| 5 | `ghost-gateway/src/api/oauth_routes.rs` | 93–108 | **`callback` uses `State(_state)` — state parameter is unused.** Returns hardcoded success stub. No token exchange, no CSRF validation against stored state. | S1 | 1,2 |
| 6 | `ghost-gateway/src/api/oauth_routes.rs` | 111–115 | **`list_connections` uses `State(_state)` — state parameter is unused.** Returns hardcoded empty array `[]`. | S1 | 1,2 |
| 7 | `ghost-gateway/src/api/oauth_routes.rs` | 118–127 | **`disconnect` uses `State(_state)` — state parameter is unused.** Returns hardcoded success. No actual revocation. | S1 | 1,2 |
| 8 | `ghost-gateway/src/api/memory.rs` | 30–170 | **Read-only endpoint over empty table.** `memory_snapshots` table is created in v016 migration but nothing in the codebase writes to it (`INSERT INTO memory_snapshots` has zero matches). `/api/memory` and `/api/memory/:id` will always return empty results. | S1 | 4 |
| 9 | `ghost-gateway/src/api/costs.rs` | 35–60 | **Read-only endpoint over empty in-memory store.** `CostTracker.record()` is never called anywhere in the gateway codebase. `/api/costs` will always return `daily_total: 0.0, compaction_cost: 0.0` for every agent. | S1 | 4 |
| 10 | `ghost-gateway/src/api/sessions.rs` | 20–55 | **Read-only endpoint over empty table (within gateway).** `itp_events` is only written by `convergence-monitor` and `cortex-storage` queries, but the monitor's INSERT has a schema mismatch (Finding #2). Even if fixed, the gateway itself never writes ITP events. Sessions endpoint depends entirely on external writer. | S1 | 4 |
| 11 | `ghost-gateway/src/api/convergence.rs` | 35–75 | **Read-only endpoint over empty table (within gateway).** `convergence_scores` is only written by `convergence-monitor`, but that INSERT has a schema mismatch (Finding #1). Even if fixed, the gateway itself never writes scores. | S1 | 4 |
| 12 | `ghost-gateway/src/api/goals.rs` | 20–170 | **Read-only endpoint over empty table (within gateway).** `goal_proposals` is only written via `cortex_storage::queries::goal_proposal_queries::insert_proposal()`, which is only called from tests. No production code path inserts proposals. `/api/goals` will always return empty. | S1 | 4 |
| 13 | `ghost-gateway/src/api/websocket.rs` | 27–31 | **WsEvent::ScoreUpdate is never sent.** Defined as a variant but no code anywhere calls `event_tx.send(WsEvent::ScoreUpdate { .. })`. Dead variant. | S2 | 8 |
| 14 | `ghost-gateway/src/api/websocket.rs` | 33–37 | **WsEvent::InterventionChange is never sent.** Defined as a variant but no code anywhere calls `event_tx.send(WsEvent::InterventionChange { .. })`. Dead variant. | S2 | 8 |
| 15 | `ghost-gateway/src/bootstrap.rs` | 267 | **Agent capabilities from config are silently dropped.** `step4_init_agents_channels` creates `RegisteredAgent` with `capabilities: Vec::new()` instead of `capabilities: agent.capabilities.clone()`. Config field is loaded but never propagated. | S1 | 7 |
| 16 | `ghost-gateway/src/config.rs` | 78 | **`AgentConfig.template` field is loaded but never consumed.** No code reads `agent.template` during bootstrap or runtime. | S2 | 7 |
| 17 | `ghost-gateway/src/config.rs` | 215–220 | **`SecurityConfig.soul_drift_threshold` is loaded but never consumed.** No code reads this field at runtime. | S2 | 7 |
| 18 | `ghost-gateway/src/config.rs` | 183–192 | **`ConvergenceGatewayConfig.profile` is loaded but never consumed.** No code reads `config.convergence.profile` at runtime. | S2 | 7 |
| 19 | `ghost-gateway/src/config.rs` | 223–234 | **`ModelsConfig` (providers, api_key_env) is loaded but never consumed.** No code reads `config.models` at runtime. The CLI chat module discovers providers from env vars directly, ignoring this config. | S2 | 7 |
| 20 | `ghost-gateway/src/config.rs` | 307–310 | **`MeshConfig.min_trust_for_delegation` and `max_delegation_depth` are loaded but never consumed.** `step4c_init_mesh` reads `mesh.enabled` and `mesh.known_agents` but ignores these two fields. | S2 | 7 |
| 21 | `ghost-gateway/src/bootstrap.rs` | 112–117 | **Mesh router is built but never merged into the main router.** `step4c_init_mesh` returns `Option<axum::Router>` and `mesh_router` is stored in a local variable, but `build_router()` never merges it. The `/.well-known/agent.json` and `/a2a` endpoints are unreachable. | S1 | 1 |
| 22 | `ghost-gateway/src/api/push_routes.rs` | 1–100 | **Push routes are never mounted.** `push_router()` builds a standalone router but it's never called from `build_router()` or `main.rs`. `/api/push/*` endpoints are unreachable. | S2 | 1 |
| 23 | `ghost-gateway/Cargo.toml` | dep | **`cortex-temporal` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 24 | `ghost-gateway/Cargo.toml` | dep | **`cortex-convergence` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 25 | `ghost-gateway/Cargo.toml` | dep | **`cortex-validation` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 26 | `ghost-gateway/Cargo.toml` | dep | **`simulation-boundary` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 27 | `ghost-gateway/Cargo.toml` | dep | **`read-only-pipeline` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 28 | `ghost-gateway/Cargo.toml` | dep | **`ghost-channels` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 29 | `ghost-gateway/Cargo.toml` | dep | **`ghost-skills` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 30 | `ghost-gateway/Cargo.toml` | dep | **`ghost-heartbeat` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 31 | `ghost-gateway/Cargo.toml` | dep | **`ghost-policy` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 32 | `ghost-gateway/Cargo.toml` | dep | **`ghost-oauth` is a dependency but never imported or used in any gateway source file.** OAuth routes are stubs that don't call into the crate. | S2 | 6 |
| 33 | `ghost-gateway/Cargo.toml` | dep | **`itp-protocol` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 34 | `ghost-gateway/Cargo.toml` | dep | **`ed25519-dalek` is a dependency but never imported directly.** Signing goes through `ghost-signing` wrapper. May be needed transitively but is not directly used. | S3 | 6 |
| 35 | `ghost-gateway/Cargo.toml` | dep | **`async-trait` is a dependency but never imported or used in any gateway source file.** | S3 | 6 |
| 36 | `ghost-gateway/src/api/sessions.rs` | 35 | **DB errors silently return empty success.** If `db.prepare()` or `stmt.query_map()` fails, the error is swallowed and an empty `Json(Vec::new())` is returned with 200 OK. Should return 500. | S2 | 9 |
| 37 | `ghost-gateway/src/api/convergence.rs` | 42 | **DB lock failure silently returns empty success.** If `state.db.lock()` fails (poisoned), returns `Json(Vec::new())` with 200 OK instead of 500. | S2 | 9 |
| 38 | `ghost-gateway/src/api/convergence.rs` | 55 | **DB query error silently returns default score.** If `latest_by_agent()` returns `Err`, the catch-all `_` arm returns a default `ConvergenceScoreResponse` with `score: 0.0` and 200 OK. DB errors are indistinguishable from "no score computed yet". | S2 | 9 |
| 39 | `ghost-gateway/src/api/memory.rs` | 60–100 | **DB errors silently return empty results.** Multiple `if let Ok(...)` patterns swallow SQL errors and return empty results with 200 OK. | S2 | 9 |
| 40 | `ghost-gateway/src/api/safety.rs` | 100 | **`pause_agent` doesn't broadcast WsEvent.** `kill_all` and `quarantine_agent` both send `WsEvent::KillSwitchActivation`, but `pause_agent` does not. Inconsistent event coverage. | S2 | 8 |
| 41 | `ghost-gateway/src/api/safety.rs` | 130–175 | **`resume_agent` doesn't broadcast WsEvent.** No event is sent when an agent is resumed, so WebSocket clients won't know about state changes. | S2 | 8 |
| 42 | `ghost-gateway/src/bootstrap.rs` | 82 | **`_secret_provider` is built but immediately dropped.** The SecretProvider is constructed in step 1b but stored in a local `_` prefixed variable. It's never passed to AppState or any handler. | S1 | 5,7 |
| 43 | `ghost-gateway/src/api/memory.rs` | 46–49 | **Cross-table JOIN assumes `memory_events.actor_id` matches agent UUID string.** The `actor_id` column in `memory_events` is `TEXT NOT NULL DEFAULT 'system'` (v016 migration). The API passes `agent_id` query param (a UUID string) to match against `actor_id`. If the writer uses a different format (e.g., agent name vs UUID), the JOIN will silently return zero rows. | S2 | 10 |
| 44 | `ghost-gateway/src/api/sessions.rs` | 37 | **Sessions query references `sender` column but convergence-monitor writes `agent_id`.** The v017 migration defines `sender TEXT` on `itp_events`. The sessions handler uses `GROUP_CONCAT(DISTINCT sender)`. But the convergence-monitor INSERT (Finding #2) uses `agent_id` which doesn't exist in the schema. If the monitor's INSERT were fixed to use `sender`, this would work. Currently both are broken. | S1 | 3,10 |
| 45 | `ghost-gateway/src/bootstrap.rs` | 265 | **Agent `channel_bindings` from config are never populated.** `RegisteredAgent` is created with `channel_bindings: Vec::new()` even though `config.channels` is iterated and logged. The channel→agent binding is never stored. | S2 | 7 |


## Summary by Severity

| Severity | Count | Description |
|----------|-------|-------------|
| S0 (Critical) | 2 | Schema mismatches that cause runtime INSERT failures |
| S1 (High) | 10 | Unused state params, empty tables with no write path, dropped values, unreachable routes |
| S2 (Medium) | 18 | Silent error swallowing, dead WS variants, unused config fields, unused crate deps with semantic impact |
| S3 (Low) | 13 | Unused Cargo.toml dependencies (compile-time bloat only) |

---

## Implementation Phases (Ordered by Dependency Chain)

### Phase 1: Fix Critical Schema Mismatches (Findings #1, #2)

These are S0 blockers. The convergence-monitor's SQL INSERTs don't match the v017 migration schema. Until fixed, no convergence scores or ITP events can be persisted, which means findings #10, #11, #44 are also blocked.

**Dependencies:** None. This is the root cause.

**Work:**
- Fix `convergence-monitor/src/monitor.rs` `persist_convergence_score()` to INSERT with all required NOT NULL columns matching v017 schema (`id`, `composite_score`, `signal_scores`, `event_hash`, `previous_hash`, etc.)
- Fix `convergence-monitor/src/monitor.rs` ITP event INSERT to use `id` (PK), `sender` (not `agent_id`), remove `payload` column, add required NOT NULL columns

### Phase 2: Wire Write Paths for Read-Only Endpoints (Findings #8, #9, #10, #11, #12)

After Phase 1 fixes the schema, ITP events and convergence scores will flow. But memory_snapshots and goal_proposals still have no production writer, and CostTracker.record() is never called.

**Dependencies:** Phase 1 (for itp_events and convergence_scores)

**Work:**
- Implement a memory snapshot writer (likely in cortex-storage or the agent loop) that INSERTs into `memory_snapshots`
- Wire `CostTracker.record()` into the LLM call path (likely in `ghost-agent-loop` after each LLM response)
- Wire `insert_proposal()` into the proposal creation flow (likely in the agent loop's goal/proposal pipeline)

### Phase 3: Wire OAuth Routes to ghost-oauth Crate (Findings #3–7, #32)

All 5 OAuth endpoints are stubs returning hardcoded data. The `ghost-oauth` crate is a dependency but never imported.

**Dependencies:** None (independent of Phase 1–2)

**Work:**
- Replace stub handlers with calls to `ghost_oauth::OAuthBroker`
- Ensure `State(state)` is actually used (not `_state`)
- Wire OAuthBroker into AppState or build a separate state

### Phase 4: Fix Dropped Bootstrap Values (Findings #15, #42, #45)

Config values are loaded but silently dropped during bootstrap.

**Dependencies:** None (independent)

**Work:**
- Pass `agent.capabilities.clone()` instead of `Vec::new()` in `step4_init_agents_channels` (Finding #15)
- Store `_secret_provider` in AppState or pass it to components that need it (Finding #42)
- Populate `channel_bindings` from `config.channels` during agent registration (Finding #45)

### Phase 5: Mount Unreachable Routes (Findings #21, #22)

The mesh router and push router are built but never merged into the main axum router.

**Dependencies:** None (independent)

**Work:**
- In `main.rs` or `build_router()`, merge the mesh router returned by `step4c_init_mesh` into the main router (e.g., `app.merge(mesh_router)`)
- Call `push_routes::push_router()` and merge it into the main router in `build_router()`

### Phase 6: Fix Silent Error Swallowing (Findings #36, #37, #38, #39)

Multiple handlers return 200 OK with empty data when DB errors occur.

**Dependencies:** None (independent)

**Work:**
- `sessions.rs`: Return `StatusCode::INTERNAL_SERVER_ERROR` on DB lock poison or query failure instead of empty `Vec`
- `convergence.rs`: Return 500 on DB lock poison instead of empty `Vec`; distinguish DB errors from "no score yet" in the `latest_by_agent` match
- `memory.rs`: Propagate SQL errors as 500 instead of swallowing with `if let Ok(...)`

### Phase 7: Wire Missing WebSocket Events (Findings #13, #14, #40, #41)

Two WsEvent variants are never sent, and two safety actions don't broadcast.

**Dependencies:** Phase 1 (ScoreUpdate needs convergence scores to flow)

**Work:**
- Send `WsEvent::ScoreUpdate` when convergence scores are computed/received
- Send `WsEvent::InterventionChange` when intervention levels change
- Add `event_tx.send(WsEvent::KillSwitchActivation { level: "PAUSE", .. })` to `pause_agent`
- Add an `AgentStateChange` or new event to `resume_agent`

### Phase 8: Consume Orphan Config Fields (Findings #16, #17, #18, #19, #20)

Five config sections are parsed but never read at runtime.

**Dependencies:** None (independent)

**Work:**
- `agent.template`: Wire to agent template system or remove from config
- `security.soul_drift_threshold`: Pass to convergence scoring or remove
- `convergence.profile`: Pass to convergence monitor or scoring engine
- `models.providers`: Wire to LLM provider initialization or remove
- `mesh.min_trust_for_delegation` / `max_delegation_depth`: Pass to mesh delegation logic

### Phase 9: Remove Unused Cargo Dependencies (Findings #23–31, #33–35)

13 crate dependencies are never imported. Pure compile-time bloat.

**Dependencies:** None (independent, lowest priority)

**Work:**
- Remove from `[dependencies]` in `ghost-gateway/Cargo.toml`: `cortex-temporal`, `cortex-convergence`, `cortex-validation`, `simulation-boundary`, `read-only-pipeline`, `ghost-channels`, `ghost-skills`, `ghost-heartbeat`, `ghost-policy`, `itp-protocol`, `ed25519-dalek`, `async-trait`
- Verify no transitive dependency breakage after removal
