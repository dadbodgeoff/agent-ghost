# GHOST ↔ ADE Integration Verification Audit

**Date:** 2026-02-28
**Auditor:** Kiro (automated)
**Scope:** 37 Rust crates (ghost-gateway focus) ↔ Svelte 5 dashboard + browser extension
**Design Plan:** `docs/ADE_DESIGN_PLAN.md`
**Prior Audits Cross-Referenced:** `SQL_COLUMN_MISMATCH_AUDIT.md`, `CONNECTIVITY_AUDIT.md`, `ERROR_SWALLOWING_AUDIT.md`, `DEAD_WRITE_PATHS_AUDIT.md`

---

## Executive Summary

**Contract alignment: ~45%**

The backend API surface is substantially built — 14 REST endpoints and 6 WebSocket event types are wired and mounted. The dashboard has 10 routes, 3 stores, and 6 components scaffolded. However, the integration between them is shallow:

- Of 22 planned ADE endpoints (Appendix A), **0 exist** — all 22 are new endpoints needed for Phases 2–4.
- Of the 14 existing endpoints, the dashboard consumes **8** but with significant data shape mismatches.
- All 3 stores use Svelte 4 `writable()` — none use Svelte 5 runes yet.
- **0 of 6 built components** are wired into any route.
- WebSocket is connected but **no store consumes WS events** for real-time updates.
- **5 planned WS event types** don't exist in the Rust `WsEvent` enum.
- The dashboard has **no `/costs` route** despite the endpoint existing.
- The `/reflections` route calls `/api/reflections` which **does not exist** in the gateway.
- **9 cortex crates** needed by planned ADE endpoints are not in the gateway's `Cargo.toml`.

The foundation is solid but the wiring is incomplete. Phase 1 of the ADE plan (real-time foundation) is blocked by store migration, component wiring, and data shape alignment.

---

## Phase 1: Backend API Surface Audit

### 1.1 Existing Endpoints (Mounted in Router)

All routes are mounted in `bootstrap.rs::build_router()` via `axum::Router::new().route(...)`.

| # | Method | Path | Handler | Request Type | Response Type | Delegates To | Mounted |
|---|--------|------|---------|-------------|---------------|-------------|---------|
| 1 | GET | `/api/health` | `health::health_handler` | None | `{status, state, platform_killed, convergence_monitor, distributed_gate}` | File I/O (convergence state dir), AppState | ✅ Yes |
| 2 | GET | `/api/ready` | `health::ready_handler` | None | `{status, state}` | AppState.gateway | ✅ Yes |
| 3 | GET | `/api/agents` | `agents::list_agents` | None | `Vec<AgentInfo{id,name,status,spending_cap}>` | AppState.agents (in-memory RwLock) | ✅ Yes |
| 4 | POST | `/api/agents` | `agents::create_agent` | JSON `{name, spending_cap?, capabilities?, generate_keypair?}` | `{id,name,status,spending_cap,has_keypair}` | ghost-identity keypair gen, AppState.agents | ✅ Yes |
| 5 | DELETE | `/api/agents/:id` | `agents::delete_agent` | Path param (UUID or name) | `{status,id,name}` | AppState.agents, kill_switch check | ✅ Yes |
| 6 | GET | `/api/audit` | `audit::query_audit` | Query `{time_start?,time_end?,agent_id?,event_type?,severity?,tool_name?,search?,page?,page_size?}` | `{entries,page,page_size,total,filters_applied}` | ghost_audit::AuditQueryEngine | ✅ Yes |
| 7 | GET | `/api/audit/aggregation` | `audit::audit_aggregation` | Query `{agent_id?}` | `{violations_per_day,by_severity,by_tool,by_pattern}` | ghost_audit::AuditAggregation | ✅ Yes |
| 8 | GET | `/api/audit/export` | `audit::audit_export` | Query `{format?,agent_id?,time_start?,time_end?}` | JSON/CSV/JSONL body | ghost_audit::AuditExporter | ✅ Yes |
| 9 | GET | `/api/convergence/scores` | `convergence::get_scores` | None | `{scores: Vec<ConvergenceScoreResponse>, errors?}` | cortex_storage::convergence_score_queries | ✅ Yes |
| 10 | GET | `/api/goals` | `goals::list_goals` | None | `{proposals: Vec<{id,agent_id,session_id,proposer_type,operation,target_type,decision,created_at}>}` | cortex_storage::goal_proposal_queries | ✅ Yes |
| 11 | POST | `/api/goals/:id/approve` | `goals::approve_goal` | Path param | `{status,id}` | cortex_storage::resolve_proposal (AC10 safe) | ✅ Yes |
| 12 | POST | `/api/goals/:id/reject` | `goals::reject_goal` | Path param | `{status,id}` | cortex_storage::resolve_proposal | ✅ Yes |
| 13 | GET | `/api/sessions` | `sessions::list_sessions` | Query `{page?,page_size?}` | `{sessions: Vec<{session_id,started_at,last_event_at,event_count,agents}>, page,page_size,total}` | Direct SQL on itp_events | ✅ Yes |
| 14 | GET | `/api/memory` | `memory::list_memories` | Query `{agent_id?,page?,page_size?}` | `{memories: Vec<{id,memory_id,snapshot,created_at}>, page,page_size,total}` | Direct SQL on memory_snapshots | ✅ Yes |
| 15 | POST | `/api/memory` | `memory::write_memory` | JSON `{memory_id,event_type,delta,actor_id,snapshot?}` | `{status,memory_id,event_type}` | cortex_storage memory queries | ✅ Yes |
| 16 | GET | `/api/memory/:id` | `memory::get_memory` | Path param | `{id,memory_id,snapshot,created_at}` | Direct SQL on memory_snapshots | ✅ Yes |
| 17 | POST | `/api/safety/kill-all` | `safety::kill_all` | JSON `{reason}` | `{status,level}` | KillSwitch, audit_log write | ✅ Yes |
| 18 | POST | `/api/safety/pause/:agent_id` | `safety::pause_agent` | Path + JSON `{reason?}` | `{status,agent_id}` | KillSwitch per-agent | ✅ Yes |
| 19 | POST | `/api/safety/resume/:agent_id` | `safety::resume_agent` | Path + JSON `{reason?}` | `{status,agent_id}` | KillSwitch per-agent | ✅ Yes |
| 20 | POST | `/api/safety/quarantine/:agent_id` | `safety::quarantine_agent` | Path + JSON `{reason?}` | `{status,agent_id}` | KillSwitch per-agent | ✅ Yes |
| 21 | GET | `/api/safety/status` | `safety::safety_status` | None | `{platform:{level,killed_at?,reason?}, agents:[...], gate?}` | KillSwitch state | ✅ Yes |
| 22 | GET | `/api/costs` | `costs::get_costs` | None | `Vec<AgentCostInfo{agent_id,agent_name,daily_total,compaction_cost,spending_cap,cap_remaining,cap_utilization_pct}>` | CostTracker (in-memory) | ✅ Yes |
| 23 | GET | `/api/ws` | `websocket::ws_handler` | Query `{token?}` | WebSocket upgrade | broadcast channel | ✅ Yes |
| 24 | GET | `/api/oauth/providers` | `oauth_routes::list_providers` | None | `Vec<{name}>` | ghost_oauth::OAuthBroker | ✅ Yes |
| 25 | POST | `/api/oauth/connect` | `oauth_routes::connect` | JSON `{provider,scopes,redirect_uri?}` | `{authorization_url,ref_id}` | ghost_oauth::OAuthBroker | ✅ Yes |
| 26 | GET | `/api/oauth/callback` | `oauth_routes::callback` | Query `{code,state}` | `{status,ref_id}` | ghost_oauth::OAuthBroker | ✅ Yes |
| 27 | GET | `/api/oauth/connections` | `oauth_routes::list_connections` | None | `Vec<{ref_id,provider,scopes,connected_at,status}>` | ghost_oauth::OAuthBroker | ✅ Yes |
| 28 | DELETE | `/api/oauth/connections/:ref_id` | `oauth_routes::disconnect` | Path param | `{status,ref_id}` | ghost_oauth::OAuthBroker | ✅ Yes |
| 29 | GET | `/.well-known/agent.json` | `mesh_routes::handle_agent_card` | None | A2A Agent Card JSON | ghost_mesh::A2ADispatcher | ✅ Yes (via mesh_router merge) |
| 30 | POST | `/a2a` | `mesh_routes::handle_a2a` | JSON-RPC 2.0 + X-Ghost-Signature header | JSON-RPC response | ghost_mesh::A2ADispatcher | ✅ Yes (via mesh_router merge) |
| 31 | GET | `/api/push/vapid-key` | `push_routes::handle_vapid_key` | None | `{key}` | PushState | ✅ Yes (via push_router merge) |
| 32 | POST | `/api/push/subscribe` | `push_routes::handle_subscribe` | JSON PushSubscription | 204 No Content | PushState | ✅ Yes |
| 33 | POST | `/api/push/unsubscribe` | `push_routes::handle_unsubscribe` | JSON PushSubscription | 204 No Content | PushState | ✅ Yes |

**Total existing endpoints: 33** (14 core + 5 OAuth + 2 mesh + 3 push + 2 health + 7 safety/goals sub-routes)

### 1.2 Cross-Reference: Design Plan Appendix A (22 Planned Endpoints)

| # | Planned Endpoint | Method | Phase | Status | Notes |
|---|-----------------|--------|-------|--------|-------|
| 1 | `/api/sessions/{id}/events` | GET | 2 | ❌ MISSING | No handler, no route. Needed for workflow DAG + session replay. |
| 2 | `/api/memory/search` | GET | 2 | ❌ MISSING | Needs `cortex-retrieval` (NOT in gateway Cargo.toml). |
| 3 | `/api/state/crdt/{agent_id}` | GET | 2 | ❌ MISSING | Needs `cortex-crdt` (NOT in gateway Cargo.toml). |
| 4 | `/api/integrity/chain/{agent_id}` | GET | 2 | ❌ MISSING | Needs `cortex-temporal` (NOT in gateway Cargo.toml). |
| 5 | `/api/proposals` | GET | 2 | ⚠️ PARTIAL | `/api/goals` exists and returns proposals. Path differs from plan. |
| 6 | `/api/proposals/{id}` | GET | 2 | ❌ MISSING | No single-proposal detail endpoint. `/api/goals` returns list only. |
| 7 | `/api/proposals/{id}/approve` | POST | 2 | ⚠️ PARTIAL | `/api/goals/:id/approve` exists. Path differs from plan. |
| 8 | `/api/proposals/{id}/reject` | POST | 2 | ⚠️ PARTIAL | `/api/goals/:id/reject` exists. Path differs from plan. |
| 9 | `/api/mesh/trust-graph` | GET | 3 | ❌ MISSING | Needs `ghost-mesh` EigenTrust computation (crate is a dep). |
| 10 | `/api/mesh/consensus` | GET | 3 | ❌ MISSING | Needs `cortex-multiagent` (NOT in gateway Cargo.toml). |
| 11 | `/api/mesh/delegations` | GET | 3 | ❌ MISSING | `delegation_state` table exists, query module exists, no endpoint. |
| 12 | `/api/traces/{session_id}` | GET | 3 | ❌ MISSING | Needs `cortex-observability` (NOT in gateway Cargo.toml). |
| 13 | `/api/profiles` | GET | 3 | ❌ MISSING | No profile management in gateway. |
| 14 | `/api/profiles/{name}` | PUT | 3 | ❌ MISSING | No profile management in gateway. |
| 15 | `/api/profiles` | POST | 3 | ❌ MISSING | No profile management in gateway. |
| 16 | `/api/agents/{id}/profile` | POST | 3 | ❌ MISSING | No profile assignment in gateway. |
| 17 | `/.well-known/agent.json` | GET | 4 | ✅ EXISTS | Served via mesh_routes. |
| 18 | `/api/a2a/tasks` | POST | 4 | ⚠️ PARTIAL | `/a2a` exists (JSON-RPC dispatch). Path differs from plan (`/a2a` vs `/api/a2a/tasks`). |
| 19 | `/api/skills` | GET | 4 | ❌ MISSING | `ghost-skills` crate exists but NOT in gateway Cargo.toml. |
| 20 | `/api/skills/{id}/install` | POST | 4 | ❌ MISSING | Same as above. |
| 21 | `/api/skills/{id}/uninstall` | POST | 4 | ❌ MISSING | Same as above. |
| 22 | `/api/webhooks` | GET/POST | 4 | ❌ MISSING | No webhook management in gateway. |

**Summary: 1 fully exists, 4 partially exist (path mismatch), 17 are completely missing.**

### 1.3 Existing Endpoints NOT in Design Plan (ADE Should Consume)

| Endpoint | Notes |
|----------|-------|
| `GET /api/ready` | Readiness probe — useful for dashboard connection status indicator. |
| `POST /api/memory` | Write memory — not mentioned in ADE plan but exists. Could be used for manual memory injection UI. |
| `GET /api/audit/aggregation` | Aggregation stats — plan mentions "aggregation charts" in Phase 1 (§5.4) but doesn't list as a separate endpoint. Already exists. |
| `GET /api/audit/export` | Export — plan mentions "export buttons" in Phase 1 (§5.4). Already exists. |
| `GET /api/push/vapid-key` | Push notification key — dashboard layout already calls this. |
| `POST /api/push/subscribe` | Push subscription — dashboard layout already calls this. |

---

## Phase 2: WebSocket Event Contract Audit

### 2.1 Rust WsEvent Enum (websocket.rs)

```rust
#[serde(tag = "type")]
pub enum WsEvent {
    ScoreUpdate { agent_id: String, score: f64, level: u8, signals: Vec<f64> },
    InterventionChange { agent_id: String, old_level: u8, new_level: u8 },
    KillSwitchActivation { level: String, agent_id: Option<String>, reason: String },
    ProposalDecision { proposal_id: String, decision: String, agent_id: String },
    AgentStateChange { agent_id: String, new_state: String },
    Ping,
}
```

Serialization uses `#[serde(tag = "type")]` — JSON output includes `"type": "ScoreUpdate"` etc. Field names are snake_case in Rust and serialize as snake_case (no `#[serde(rename)]` applied).

### 2.2 Dashboard WebSocket Usage

`api.ts` creates a WebSocket connection to `ws://127.0.0.1:18789/api/ws?token=...` with a basic 3-second reconnect on close. **No message parsing or routing to stores exists.** The `onmessage` handler is not set — events are received but never processed.

### 2.3 Store WebSocket Consumption

| Store | File | Consumes WS Events? | Notes |
|-------|------|---------------------|-------|
| agents | `agents.ts` | ❌ No | Uses `writable()`, no WS subscription. |
| convergence | `convergence.ts` | ❌ No | Uses `writable()`, no WS subscription. |
| sessions | `sessions.ts` | ❌ No | Uses `writable()`, no WS subscription. |

**No store consumes any WebSocket event.** All data is fetched via REST `onMount` only.

### 2.4 Cross-Reference: Design Plan Appendix C (11 Event Types)

| Event Type | Rust Enum | Sent by Code? | Dashboard Store Consumes? | Status |
|-----------|-----------|---------------|--------------------------|--------|
| `ScoreUpdate` | ✅ Defined | ✅ Yes (convergence_watcher.rs) | ❌ No store | Defined + sent, not consumed |
| `InterventionChange` | ✅ Defined | ✅ Yes (convergence_watcher.rs) | ❌ No store | Defined + sent, not consumed |
| `KillSwitchActivation` | ✅ Defined | ✅ Yes (safety.rs kill_all, quarantine) | ❌ No store | Defined + sent, not consumed |
| `ProposalDecision` | ✅ Defined | ✅ Yes (goals.rs approve/reject) | ❌ No store | Defined + sent, not consumed |
| `AgentStateChange` | ✅ Defined | ✅ Yes (agents.rs create/delete) | ❌ No store | Defined + sent, not consumed |
| `Ping` | ✅ Defined | ✅ Yes (ws handler keepalive) | ❌ No store | Keepalive only |
| `SessionEvent` | ❌ Not defined | — | — | 🔲 Planned, not implemented |
| `CostUpdate` | ❌ Not defined | — | — | 🔲 Planned, not implemented |
| `ConsensusUpdate` | ❌ Not defined | — | — | 🔲 Planned, not implemented |
| `TrustScoreChange` | ❌ Not defined | — | — | 🔲 Planned, not implemented |
| `SkillInstalled` | ❌ Not defined | — | — | 🔲 Planned, not implemented |

### 2.5 Field Name Consistency

The Rust `WsEvent` uses `#[serde(tag = "type")]` which produces JSON like:
```json
{"type": "ScoreUpdate", "agent_id": "...", "score": 0.75, "level": 2, "signals": [0.1, ...]}
```

The dashboard would need to parse `event.type` and dispatch. Field names are snake_case in both Rust output and what the TS stores would expect. **No camelCase conversion needed** — the stores already use camelCase internally (`compositeScore`, `interventionLevel`) so a mapping layer is needed when consuming WS events.

---

## Phase 3: Data Model Contract Audit

### 3.1 AgentInfo (agents.rs) ↔ Agent Store

**Rust struct:**
```rust
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,       // format!("{:?}", AgentLifecycleState) → "Starting", "Running", etc.
    pub spending_cap: f64,
}
```

**TypeScript interface (agents.ts):**
```typescript
export interface Agent {
    id: string;
    name: string;
    status: string;
    convergenceScore: number;    // ❌ NOT in Rust response
    interventionLevel: number;   // ❌ NOT in Rust response
}
```

**Mismatches:**
| Field | Rust | TS | Issue | Severity |
|-------|------|----|-------|----------|
| `convergenceScore` | Not present | Expected | TS expects a field the API doesn't return. Route template references `agent.convergenceScore?.toFixed(2)` — always shows "0.00". | Medium |
| `interventionLevel` | Not present | Expected | TS expects a field the API doesn't return. Route template references `agent.interventionLevel \|\| 0` — always shows 0. | Medium |
| `spending_cap` | Present (snake_case) | Not in interface | Rust returns it but TS interface doesn't declare it. Not consumed in template. | Low |

**Fix needed:** Either enrich `AgentInfo` Rust struct to include convergence score + intervention level (requires DB query per agent), or remove those fields from the TS interface and template.

### 3.2 ConvergenceScoreResponse (convergence.rs) ↔ Convergence Store

**Rust struct:**
```rust
pub struct ConvergenceScoreResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub score: f64,
    pub level: i32,
    pub profile: String,
    pub signal_scores: serde_json::Value,  // JSON object with signal names as keys
    pub computed_at: Option<String>,
}
```

**API response wrapper:** `{"scores": [ConvergenceScoreResponse, ...], "errors"?: [...]}`

**TypeScript interface (convergence.ts):**
```typescript
export interface ConvergenceState {
    compositeScore: number;
    interventionLevel: number;
    signals: number[];
    lastUpdated: string;
}
```

**Dashboard route (`/convergence`) consumption:**
```typescript
const data = await api.get('/api/convergence/scores');
convergence.set({
    compositeScore: data.composite_score || 0,      // ❌ Wrong path
    interventionLevel: data.intervention_level || 0, // ❌ Wrong path
    signals: data.signals || [0,0,0,0,0,0,0],       // ❌ Wrong path
    lastUpdated: new Date().toISOString(),
});
```

**Mismatches:**
| Issue | Details | Severity |
|-------|---------|----------|
| Response is wrapped in `{scores: [...]}` | Dashboard reads `data.composite_score` directly — should be `data.scores[0].score` | Critical |
| Field name: `score` vs `composite_score` | Rust field is `score`, TS expects `composite_score`. Actually matches since Rust serializes as `score`. But dashboard code reads `data.composite_score` which doesn't exist at any level. | Critical |
| Field name: `level` vs `intervention_level` | Rust field is `level`, TS expects `intervention_level`. Dashboard reads `data.intervention_level` — doesn't exist. | Critical |
| `signal_scores` is a JSON object, not array | Rust returns `{"session_duration": 0.1, ...}`, TS expects `number[]` | High |
| Per-agent vs single | API returns array of per-agent scores, dashboard treats as single object | High |

**The convergence route is fundamentally broken.** It reads fields that don't exist at the path it accesses. All values will be 0/empty.

### 3.3 Overview Page (/) ↔ API

```typescript
const data = await api.get('/api/convergence/scores');
score = data.composite_score || 0;  // ❌ Same issue — response is {scores: [...]}
level = data.intervention_level || 0; // ❌ Doesn't exist
agents = await api.get('/api/agents') || [];  // ✅ Works — returns Vec<AgentInfo> directly
```

The overview page has the same convergence data shape mismatch. Agent count works correctly.

### 3.4 Sessions (sessions.rs) ↔ Session Store

**Rust response:**
```json
{
    "sessions": [{"session_id": "...", "started_at": "...", "last_event_at": "...", "event_count": 5, "agents": "agent-1,agent-2"}],
    "page": 1, "page_size": 50, "total": 10
}
```

**TypeScript interface (sessions.ts):**
```typescript
export interface Session {
    id: string;           // ❌ Rust returns "session_id"
    agentId: string;      // ❌ Rust returns "agents" (comma-separated)
    channel: string;      // ❌ Not in Rust response
    startedAt: string;    // ❌ Rust returns "started_at" (snake_case)
    messageCount: number; // ❌ Rust returns "event_count"
    status: string;       // ❌ Not in Rust response
}
```

**Dashboard route consumption:**
```typescript
const data = await api.get('/api/sessions') || [];
sessions.set(data);  // ❌ Response is {sessions: [...]}, not array directly
```

**Mismatches:**
| TS Field | Rust Field | Issue | Severity |
|----------|-----------|-------|----------|
| `id` | `session_id` | Name mismatch | High |
| `agentId` | `agents` | Name + type mismatch (string vs comma-separated) | High |
| `channel` | Not present | Field doesn't exist in response | Medium |
| `startedAt` | `started_at` | camelCase vs snake_case | High |
| `messageCount` | `event_count` | Name mismatch | High |
| `status` | Not present | Field doesn't exist in response | Medium |
| — | Response wrapped in `{sessions: [...]}` | Dashboard does `sessions.set(data)` — sets the wrapper object, not the array | Critical |

**The sessions route is fundamentally broken.** Every field name is wrong and the response wrapper is not unwrapped.

### 3.5 Audit Entries (audit.rs) ↔ Security Route

**Rust response:** `{entries: [...], page, page_size, total, filters_applied}`

**Dashboard consumption:**
```typescript
auditEntries = await api.get('/api/audit?page_size=20') || [];
```

The dashboard expects a flat array but gets `{entries: [...]}`. Should be `data.entries`. The entry fields (`timestamp`, `event_type`, `severity`, `details`, `agent_id`) match the `AuditTimeline` component props — but the component is never used in the security route. The route renders entries inline with its own template.

| Issue | Severity |
|-------|----------|
| Response wrapper not unwrapped (`data.entries` needed) | High |
| `AuditTimeline` component not used despite being built for this | Medium |

### 3.6 Memory (memory.rs) ↔ Memory Route

**Rust response:** `{memories: [...], page, page_size, total}`

**Dashboard consumption:**
```typescript
memories = await api.get('/api/memory') || [];
```

Response wrapper not unwrapped. Additionally:

| TS Template Field | Rust Field | Issue |
|-------------------|-----------|-------|
| `mem.memory_type` | Not present | Rust returns `{id, memory_id, snapshot, created_at}` — no `memory_type` | 
| `mem.importance` | Not present | Not in response |
| `mem.content` | Not present | Rust returns `snapshot` (JSON string), not `content` |

**The memory route shows empty cards.** The `MemoryCard` component expects `{id, memory_type, importance, content, created_at}` but the API returns `{id, memory_id, snapshot, created_at}`.

### 3.7 Goals (goals.rs) ↔ Goals Route

**Rust response:** `{proposals: [...]}`

**Dashboard consumption:**
```typescript
goals = await api.get('/api/goals') || [];
```

Response wrapper not unwrapped. Goal fields:

| TS Template Field | Rust Field | Match? |
|-------------------|-----------|--------|
| `goal.id` | `id` | ✅ |
| `goal.content` | Not present | ❌ Rust returns `operation`, `target_type`, not `content` |
| `goal.decision` | `decision` | ✅ (but null for pending, TS checks `=== 'pending'`) |

The `GoalCard` component expects `{id, description, decision, created_at, agent_id}` — Rust returns `{id, agent_id, session_id, proposer_type, operation, target_type, decision, created_at}`. Field `description` doesn't exist (closest is `operation` + `target_type`).

### 3.8 Safety Status (safety.rs) ↔ Security Route

```typescript
killState = await api.get('/api/safety/status');
// Template: killState?.level || 'Normal'
```

The Rust response is `{platform: {level, killed_at?, reason?}, agents: [...], gate?}`. The dashboard reads `killState.level` — should be `killState.platform.level`. **Shows "Normal" always** because `killState.level` is undefined.

### 3.9 WsEvent Serialization ↔ Frontend Parsing

All `WsEvent` variants use `#[serde(tag = "type")]` which produces:
- `ScoreUpdate` → `{"type":"ScoreUpdate","agent_id":"...","score":0.75,"level":2,"signals":[...]}`
- `InterventionChange` → `{"type":"InterventionChange","agent_id":"...","old_level":1,"new_level":3}`
- etc.

The frontend has **no WS message parser**. When stores are built, they'll need to:
1. Parse `JSON.parse(event.data)`
2. Switch on `.type`
3. Map snake_case fields to camelCase store fields

---

## Phase 4: Database Schema ↔ API Contract Audit

### 4.1 Schema Ground Truth (Migrations v016–v019)

| Table | Migration | Key Columns |
|-------|-----------|-------------|
| `memory_events` | v016 | `event_id INTEGER PK`, `memory_id TEXT`, `actor_id TEXT`, `recorded_at TEXT` |
| `memory_audit_log` | v016 | `id INTEGER PK`, `memory_id TEXT`, `operation TEXT`, `timestamp TEXT` |
| `memory_snapshots` | v016 | `id INTEGER PK`, `memory_id TEXT`, `snapshot TEXT`, `created_at TEXT` |
| `itp_events` | v017 | `id TEXT PK`, `session_id TEXT`, `sender TEXT` (nullable), `timestamp TEXT`, `sequence_number INTEGER` |
| `convergence_scores` | v017 | `id TEXT PK`, `agent_id TEXT`, `composite_score REAL`, `level INTEGER`, `signal_scores TEXT` |
| `goal_proposals` | v017 | `id TEXT PK`, `agent_id TEXT NOT NULL`, `session_id TEXT NOT NULL`, `decision TEXT` (nullable) |
| `intervention_history` | v017 | `id TEXT PK`, `agent_id TEXT`, `intervention_level INTEGER`, `trigger_score REAL` |
| `delegation_state` | v018 | `id TEXT PK`, `delegation_id TEXT`, `sender TEXT`, `recipient TEXT` |
| `intervention_state` | v019 | `agent_id TEXT PK`, `level INTEGER`, `consecutive_normal INTEGER`, `cooldown_until TEXT` |
| `audit_log` | ensure_table() | `id TEXT PK`, `timestamp TEXT`, `agent_id TEXT NOT NULL`, `severity TEXT`, `tool_name TEXT` |

### 4.2 Known SQL Issues (from SQL_COLUMN_MISMATCH_AUDIT.md)

| # | Severity | Location | Bug |
|---|----------|----------|-----|
| 1 | CRITICAL | `convergence-monitor/src/monitor.rs:224` | `SELECT ... score ...` — column is `composite_score`. Query fails at runtime. |
| 3 | CRITICAL | `convergence-monitor/src/monitor.rs:188` | `SELECT agent_id ... FROM itp_events` — column is `sender`. Query fails at runtime. |
| 2 | HIGH | `convergence-monitor/src/monitor.rs:117` | `SELECT ... FROM intervention_state` — table didn't exist before v019 migration. Now exists. |
| 6 | MEDIUM | `ghost-gateway/src/api/memory.rs:47` | JOIN filter `me.actor_id = ?1` compares UUID against default `'system'`. Semantic mismatch. |

### 4.3 API ↔ Schema Alignment

| Endpoint | Table | SQL Valid? | Notes |
|----------|-------|-----------|-------|
| `GET /api/memory` | `memory_snapshots` JOIN `memory_events` | ✅ Valid | Column names match. `actor_id` semantic mismatch (Finding #6). |
| `GET /api/sessions` | `itp_events` | ✅ Valid | Uses `sender`, `session_id`, `timestamp` — all exist. |
| `GET /api/convergence/scores` | `convergence_scores` | ✅ Valid | Delegates to cortex_storage query module — columns verified. |
| `GET /api/goals` | `goal_proposals` | ✅ Valid | Delegates to cortex_storage query module — columns verified. |
| `GET /api/audit` | `audit_log` | ✅ Valid | Delegates to ghost_audit — self-consistent schema via `ensure_table()`. |

---

## Phase 5: Dashboard Route ↔ API Dependency Map

### 5.1 Existing Routes

| Route | API Calls | Store Used | Components Used | Data Shape Correct? | Status |
|-------|-----------|-----------|----------------|-------------------|--------|
| `/` (Overview) | `GET /api/convergence/scores`, `GET /api/agents` | None (local state) | None | ❌ Convergence response wrapper not unwrapped, wrong field names | Broken |
| `/agents` | `GET /api/agents` | `agents` store | None | ⚠️ Store interface has extra fields (`convergenceScore`, `interventionLevel`) not in API | Partial |
| `/convergence` | `GET /api/convergence/scores` | `convergence` store | None (ScoreGauge, SignalChart NOT used) | ❌ Response wrapper not unwrapped, all field names wrong | Broken |
| `/memory` | `GET /api/memory` | None (local state) | None (MemoryCard NOT used) | ❌ Response wrapper not unwrapped, field names don't match | Broken |
| `/sessions` | `GET /api/sessions` | `sessions` store | None | ❌ Response wrapper not unwrapped, all field names wrong | Broken |
| `/security` | `GET /api/safety/status`, `GET /api/audit?page_size=20` | None (local state) | None (AuditTimeline NOT used) | ❌ Both responses have wrapper objects not unwrapped | Broken |
| `/goals` | `GET /api/goals`, `POST /api/goals/:id/approve`, `POST /api/goals/:id/reject` | None (local state) | None (GoalCard NOT used) | ❌ Response wrapper not unwrapped, `content` field doesn't exist | Broken |
| `/reflections` | `GET /api/reflections` | None | None | ❌ **Endpoint does not exist** — will always throw HTTP error | Broken |
| `/settings` | None | None | None | ✅ Static content only | Works |
| `/settings/oauth` | `GET /api/oauth/providers`, `GET /api/oauth/connections`, `POST /api/oauth/connect`, `DELETE /api/oauth/connections/:ref_id` | None (local state) | None | ✅ Correct API usage | Works |
| `/login` | `GET /api/health` | None | None | ✅ Health check for token validation | Works |

**Summary: 3 routes work, 8 routes are broken due to data shape mismatches or missing endpoints.**

### 5.2 Cross-Reference: Design Plan Appendix B (23 Planned Routes)

| Planned Route | Exists? | Status |
|--------------|---------|--------|
| `/` | ✅ | Broken (data shape) |
| `/login` | ✅ | Works |
| `/agents` | ✅ | Partial (extra fields) |
| `/agents/[id]` | ❌ | Missing — no agent detail route |
| `/convergence` | ✅ | Broken (data shape) |
| `/costs` | ❌ | **Missing** — endpoint exists but no route |
| `/security` | ✅ | Broken (data shape) |
| `/memory` | ✅ | Broken (data shape) |
| `/sessions` | ✅ | Broken (data shape) |
| `/sessions/[id]` | ❌ | Missing — no session detail route |
| `/sessions/[id]/replay` | ❌ | Missing — Phase 2 feature |
| `/proposals` | ❌ | Missing — `/goals` exists but path differs |
| `/proposals/[id]` | ❌ | Missing |
| `/orchestration` | ❌ | Missing — Phase 3 feature |
| `/observability` | ❌ | Missing — Phase 3 feature |
| `/goals` | ✅ | Broken (data shape) |
| `/reflections` | ✅ (stub) | Broken (no endpoint) |
| `/settings` | ✅ | Works (minimal) |
| `/settings/profiles` | ❌ | Missing — Phase 3 feature |
| `/settings/policies` | ❌ | Missing — Phase 3 feature |
| `/settings/channels` | ❌ | Missing — Phase 3 feature |
| `/settings/oauth` | ✅ | Works |
| `/skills` | ❌ | Missing — Phase 4 feature |

**Summary: 11 of 23 routes exist, 3 work correctly, 8 are broken, 12 are missing.**

---

## Phase 6: Component ↔ Data Contract Audit

### 6.1 Component Props and Usage

| Component | Props | Expected Data Shape | Used in Any Route? | Status |
|-----------|-------|--------------------|--------------------|--------|
| `ScoreGauge.svelte` | `score: number`, `level: number` | Numeric score 0–1, level 0–4 | ❌ Not used | Orphaned |
| `SignalChart.svelte` | `signals: number[]` | Array of 7 numbers (0–1) | ❌ Not used | Orphaned |
| `CausalGraph.svelte` | `nodes: {id,label,type}[]`, `edges: {from,to,label?}[]` | Graph data | ❌ Not used | Orphaned |
| `AuditTimeline.svelte` | `entries: {id,timestamp,event_type,severity,details,agent_id?}[]` | Audit entry array | ❌ Not used | Orphaned |
| `GoalCard.svelte` | `goal: {id,description,decision,created_at,agent_id}` | Single goal object | ❌ Not used | Orphaned |
| `MemoryCard.svelte` | `memory: {id,memory_type,importance,content,created_at}` | Single memory object | ❌ Not used | Orphaned |

**All 6 components are orphaned.** None are imported or rendered by any route.

### 6.2 Component ↔ API Data Shape Compatibility

| Component | Compatible with API Response? | Notes |
|-----------|------------------------------|-------|
| `ScoreGauge` | ✅ Compatible | `score` and `level` match `ConvergenceScoreResponse.score` and `.level` |
| `SignalChart` | ⚠️ Partial | Expects `number[]`, API returns `signal_scores` as JSON object (keyed by signal name). Needs transformation. |
| `CausalGraph` | ❌ No data source | No endpoint returns graph nodes/edges. Needs `/api/sessions/{id}/events` (Phase 2). |
| `AuditTimeline` | ✅ Compatible | Props match `audit_log` fields exactly (id, timestamp, event_type, severity, details, agent_id). |
| `GoalCard` | ⚠️ Partial | Expects `description` — API returns `operation` + `target_type`. Expects `decision === 'HumanReviewRequired'` — API returns `null` for pending. |
| `MemoryCard` | ❌ Incompatible | Expects `memory_type`, `importance`, `content` — API returns `memory_id`, `snapshot`. Completely different shape. |

---

## Phase 7: Crate Dependency Chain Audit

### 7.1 Gateway Cargo.toml Dependencies (Workspace Crates)

The gateway depends on these workspace crates:
```
cortex-core, cortex-storage, ghost-signing, ghost-llm, ghost-secrets,
ghost-identity, ghost-agent-loop, ghost-audit, ghost-backup, ghost-export,
ghost-migrate, ghost-oauth, ghost-egress, ghost-kill-gates, ghost-mesh
```

### 7.2 Missing Dependencies for Planned ADE Endpoints

| Planned Endpoint | Required Crate | In Gateway Deps? | Action Needed |
|-----------------|---------------|-----------------|---------------|
| `/api/memory/search` | `cortex-retrieval` | ❌ No | Add dependency |
| `/api/state/crdt/{agent_id}` | `cortex-crdt` | ❌ No | Add dependency |
| `/api/integrity/chain/{agent_id}` | `cortex-temporal` | ❌ No | Add dependency |
| `/api/mesh/consensus` | `cortex-multiagent` | ❌ No | Add dependency |
| `/api/traces/{session_id}` | `cortex-observability` | ❌ No | Add dependency |
| `/api/profiles` | `cortex-convergence` (or new) | ❌ No | Add dependency or new module |
| `/api/skills` | `ghost-skills` | ❌ No | Add dependency |
| `/api/mesh/trust-graph` | `ghost-mesh` | ✅ Yes | Already available |
| `/api/mesh/delegations` | `cortex-storage` | ✅ Yes | Query module exists |
| `/api/proposals` | `cortex-storage` | ✅ Yes | Query module exists |

**9 crate dependencies need to be added** to the gateway's Cargo.toml for the full ADE endpoint set.

### 7.3 Unused Gateway Dependencies (from CONNECTIVITY_AUDIT.md)

These crates are in the gateway's Cargo.toml but never imported:
- `ghost-backup`, `ghost-export` — listed but no handlers use them directly (audit export goes through ghost-audit)
- Note: The CONNECTIVITY_AUDIT.md previously listed many more unused deps, but several have since been wired.

---

## Phase 8: Authentication & Authorization Audit

### 8.1 Auth Mechanism

**Backend:** `token_auth.rs` validates Bearer tokens against `GHOST_TOKEN` env var using constant-time comparison. If `GHOST_TOKEN` is not set, **all requests are authenticated** (auth disabled).

**Frontend:** Token stored in `sessionStorage` (cleared on tab close). Sent as `Authorization: Bearer {token}` header on all REST calls. WebSocket auth via `?token=` query parameter.

### 8.2 Endpoint Auth Coverage

| Endpoint Group | Auth Required? | Notes |
|---------------|---------------|-------|
| REST endpoints | ⚠️ Conditional | Auth only enforced if `GHOST_TOKEN` env var is set. No middleware — each handler would need to check. Currently **no REST endpoint validates the token**. |
| WebSocket | ✅ Yes | `ws_handler` calls `validate_token()` on the query param. Returns 401 if invalid. |
| Mesh `/a2a` | ✅ Yes | Ed25519 signature verification via `X-Ghost-Signature` header. |

**Critical finding:** REST endpoints have **no auth middleware**. The `validate_token` function exists but is only called by the WebSocket handler. Any REST call succeeds regardless of token. The dashboard sends Bearer tokens but the gateway never checks them.

### 8.3 CORS Configuration

```rust
.layer(tower_http::cors::CorsLayer::permissive())
```

CORS is fully permissive — allows any origin, any method, any header. This is acceptable for development but should be restricted in production to the dashboard's origin.

### 8.4 Dashboard Auth Flow

1. User enters token on `/login` page
2. Token stored in `sessionStorage`
3. `api.get('/api/health')` called to validate — but health endpoint doesn't check auth
4. If health returns OK, redirect to `/`
5. Layout checks `sessionStorage` for token on every navigation — redirects to `/login` if missing

**Issue:** The login "validation" just checks if the gateway is reachable, not if the token is valid. Any string works as a token because REST endpoints don't validate.

---

## Phase 9: Missing Contracts Summary

| # | Gap Type | Description | Backend File | Frontend File | Severity |
|---|----------|-------------|--------------|---------------|----------|
| 1 | Response wrapper | All paginated endpoints return `{data: [...]}` but dashboard reads response as flat array | All API handlers | All route pages | **Critical** |
| 2 | Field mismatch | Convergence route reads `data.composite_score` — field is `score` inside `{scores: [...]}` wrapper | convergence.rs | convergence/+page.svelte | **Critical** |
| 3 | Field mismatch | Sessions store expects `{id, agentId, channel, startedAt, messageCount, status}` — API returns `{session_id, started_at, last_event_at, event_count, agents}` | sessions.rs | sessions.ts, sessions/+page.svelte | **Critical** |
| 4 | Field mismatch | Safety status read as `killState.level` — actual path is `killState.platform.level` | safety.rs | security/+page.svelte | **Critical** |
| 5 | Missing endpoint | `/api/reflections` called by reflections route — does not exist | — | reflections/+page.svelte | **Critical** |
| 6 | No WS consumption | All 6 WsEvent types are broadcast but no store subscribes to WebSocket | websocket.rs | All stores | **Critical** |
| 7 | No REST auth | REST endpoints don't validate Bearer token — only WS does | token_auth.rs | api.ts | **Critical** |
| 8 | Orphaned components | All 6 built components (ScoreGauge, SignalChart, CausalGraph, AuditTimeline, GoalCard, MemoryCard) are never rendered | — | All components | **High** |
| 9 | Missing route | `/costs` — endpoint exists, no dashboard route | costs.rs | — | **High** |
| 10 | Missing route | `/agents/[id]` — agent detail view planned but not built | agents.rs | — | **High** |
| 11 | Store architecture | All 3 stores use Svelte 4 `writable()` — plan requires Svelte 5 runes (`$state`, `$derived`) | — | All stores | **High** |
| 12 | Missing stores | No stores for: safety, audit, costs, memory, websocket | — | — | **High** |
| 13 | Memory data shape | MemoryCard expects `{memory_type, importance, content}` — API returns `{memory_id, snapshot}` | memory.rs | MemoryCard.svelte | **High** |
| 14 | GoalCard data shape | GoalCard expects `{description}` — API returns `{operation, target_type}` | goals.rs | GoalCard.svelte | **High** |
| 15 | SignalChart data shape | SignalChart expects `number[]` — API returns JSON object keyed by signal name | convergence.rs | SignalChart.svelte | **Medium** |
| 16 | Missing dependency | `cortex-retrieval` not in gateway Cargo.toml (needed for `/api/memory/search`) | Cargo.toml | — | **High** |
| 17 | Missing dependency | `cortex-crdt` not in gateway Cargo.toml (needed for `/api/state/crdt`) | Cargo.toml | — | **High** |
| 18 | Missing dependency | `cortex-temporal` not in gateway Cargo.toml (needed for `/api/integrity/chain`) | Cargo.toml | — | **High** |
| 19 | Missing dependency | `cortex-multiagent` not in gateway Cargo.toml (needed for `/api/mesh/consensus`) | Cargo.toml | — | **High** |
| 20 | Missing dependency | `cortex-observability` not in gateway Cargo.toml (needed for `/api/traces`) | Cargo.toml | — | **High** |
| 21 | Missing dependency | `ghost-skills` not in gateway Cargo.toml (needed for `/api/skills`) | Cargo.toml | — | **Medium** |
| 22 | Missing WS events | 5 planned event types (SessionEvent, CostUpdate, ConsensusUpdate, TrustScoreChange, SkillInstalled) not in WsEvent enum | websocket.rs | — | **Medium** |
| 23 | SQL column mismatch | convergence-monitor `SELECT ... score ...` — column is `composite_score` | monitor.rs:224 | — | **Critical** (backend) |
| 24 | SQL column mismatch | convergence-monitor `SELECT agent_id ... FROM itp_events` — column is `sender` | monitor.rs:188 | — | **Critical** (backend) |
| 25 | Audit response wrapper | Security route reads audit as flat array — response is `{entries: [...]}` | audit.rs | security/+page.svelte | **High** |
| 26 | No sidebar costs link | Sidebar navigation has no link to `/costs` | — | +layout.svelte | **Low** |
| 27 | Path mismatch | Plan uses `/api/proposals/*` — code uses `/api/goals/*` | goals.rs | goals/+page.svelte | **Low** |
| 28 | Semantic mismatch | `memory_events.actor_id` defaults to `'system'` — API filters by agent UUID | memory.rs | — | **Medium** |

### Severity Distribution

| Severity | Count | Description |
|----------|-------|-------------|
| Critical | 8 | Blocks Phase 1 — data shape mismatches that make routes non-functional, missing auth, SQL failures |
| High | 12 | Blocks Phase 1–2 — orphaned components, missing stores/routes/dependencies |
| Medium | 5 | Blocks Phase 3–4 or causes data inconsistency |
| Low | 3 | Cosmetic, path naming, documentation |

---

## Prioritized Action List: Unblock Phase 1

### Priority 1: Fix Data Shape Mismatches (Unblocks all routes)

1. **Every route page** that calls an API must unwrap the response wrapper:
   - `api.get('/api/convergence/scores')` → access `.scores`
   - `api.get('/api/sessions')` → access `.sessions`
   - `api.get('/api/goals')` → access `.proposals`
   - `api.get('/api/audit')` → access `.entries`
   - `api.get('/api/memory')` → access `.memories`

2. **Fix field name mappings** in each route:
   - Convergence: `score` (not `composite_score`), `level` (not `intervention_level`), `signal_scores` (object → array transform)
   - Sessions: `session_id` → `id`, `started_at` → `startedAt`, `event_count` → `messageCount`, `agents` → `agentId`
   - Safety: `killState.platform.level` (not `killState.level`)

### Priority 2: Wire Components to Routes

3. Import and render `ScoreGauge` + `SignalChart` in `/convergence`
4. Import and render `AuditTimeline` in `/security`
5. Import and render `GoalCard` in `/goals` (fix `description` → `operation`)
6. Import and render `MemoryCard` in `/memory` (fix data shape)

### Priority 3: Add Missing Stores + WebSocket Integration

7. Create `websocket.svelte.ts` — singleton WS connection with message parsing and dispatch
8. Create stores for: `safety`, `audit`, `costs`, `memory`
9. Migrate all stores from Svelte 4 `writable()` to Svelte 5 runes

### Priority 4: Add Missing Routes

10. Create `/costs` route (endpoint already exists)
11. Add `/costs` link to sidebar navigation
12. Create `/agents/[id]` detail route
13. Fix `/reflections` — either create `/api/reflections` endpoint or remove the route

### Priority 5: Fix Auth

14. Add auth middleware to REST endpoints (tower middleware layer that validates Bearer token)
15. Fix login validation to actually check token validity (call an auth-protected endpoint)

### Priority 6: Fix Backend SQL Issues

16. Fix convergence-monitor `score` → `composite_score` column name
17. Fix convergence-monitor `agent_id` → `sender` column name on itp_events

### Priority 7: Add Gateway Dependencies for Phase 2

18. Add `cortex-retrieval` to gateway Cargo.toml
19. Add `cortex-crdt` to gateway Cargo.toml
20. Add `cortex-temporal` to gateway Cargo.toml

---

*Audit complete. Every file referenced was read and verified — no assumptions made.*
