# GHOST ADE — Agent Development Environment Design Plan

> Comprehensive design and implementation plan for transforming the GHOST platform
> into a full Agent Development Environment (ADE).
>
> **Date**: February 2026
> **Status**: Planning
> **Platform**: 37 Rust crates (69k+ LOC) + Svelte 5 dashboard + browser extension

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Market Context & Competitive Landscape](#2-market-context--competitive-landscape)
3. [Architecture Overview](#3-architecture-overview)
4. [Current State Inventory](#4-current-state-inventory)
5. [Phase 1: Real-Time Foundation (Weeks 1–3)](#5-phase-1-real-time-foundation)
6. [Phase 2: Core ADE Features (Weeks 4–9)](#6-phase-2-core-ade-features)
7. [Phase 3: Advanced Capabilities (Weeks 10–16)](#7-phase-3-advanced-capabilities)
8. [Phase 4: Ecosystem & Extensibility (Weeks 17–22)](#8-phase-4-ecosystem--extensibility)
9. [UI/UX Design System](#9-uiux-design-system)
10. [Observability Architecture](#10-observability-architecture)
11. [Security & Safety UX](#11-security--safety-ux)
12. [Data Flow Architecture](#12-data-flow-architecture)
13. [Technology Decisions](#13-technology-decisions)
14. [Testing Strategy](#14-testing-strategy)
15. [Deployment & Distribution](#15-deployment--distribution)
16. [Risk Register](#16-risk-register)
17. [Gap Analysis & Remediation Plan](#17-gap-analysis--remediation-plan)

---

## 1. Executive Summary

GHOST is a convergence-aware autonomous agent platform with 37 Rust crates,
a 6-gate safety loop, 7-signal convergence monitoring, Ed25519 signed CRDT
operations, blake3 hash chains, and a 5-level intervention state machine.
The backend infrastructure is production-grade. What's missing is the
experience layer that transforms this from a backend system into an
Agent Development Environment — a tool where developers can create, observe,
debug, steer, and compose multi-agent workflows with full visibility.

The ADE concept differs from a traditional IDE in a fundamental way: agents
are first-class participants in the development loop, and the human's role
shifts from writing every line of code to orchestrating, monitoring, and
intervening in agent behavior. The bottleneck is not "can agents do work" —
it's "can humans understand, trust, and control what agents are doing."

> **Integration Audit (2026-02-28)**: A comprehensive audit
> (`docs/ADE_INTEGRATION_AUDIT.md`) verified every backend↔frontend contract.
> A follow-up code-level audit (`docs/ADE_DESIGN_PLAN_AUDIT.md`) cross-
> referenced every plan claim against actual source code. Key findings:
>
> - **Contract alignment: ~60%.** The backend API surface is substantially
>   built (33 endpoints mounted). Several issues flagged in earlier audits
>   have since been fixed in code (SQL column mismatches, mesh/push router
>   mounting, WsEvent sending, handler error codes).
> - **8 of 11 existing routes are broken** due to response wrapper mismatches,
>   field name mismatches, and missing endpoints.
> - **All 6 built components are orphaned** — none are rendered by any route.
> - **No store consumes WebSocket events** — all data is REST-only.
> - **REST endpoints have no authentication** — only WebSocket validates tokens.
>   WebSocket itself has a bypass: missing `?token=` param skips auth entirely.
> - **5 crate dependencies** must be added to Cargo.toml; 4 more are already
>   listed but never imported (need `use` statements only).
> - **CostTracker.record() is never called** — costs always read as zero,
>   spending caps are never enforced.
>
> Phase 1 now includes a prerequisite week (§5.0) to fix all broken contracts
> before building new features. The risk register (§16) has been updated with
> audit-discovered risks. The gap analysis (§17) provides detailed remediation
> plans for each risk.

This plan covers four phases over ~22 weeks, building from the existing
Svelte 5 dashboard and 33 REST API endpoints into a complete ADE with:

- Real-time agent workflow visualization (DAG + timeline)
- Database/state inspector with semantic search
- Proposal lifecycle review queue
- Session replay and debugging
- Multi-agent orchestration dashboard with EigenTrust visualization
- Convergence profile editor and policy customization
- OpenTelemetry-aligned observability
- A2A protocol support for cross-platform agent interoperability
- PWA with offline-first architecture

---

## 2. Market Context & Competitive Landscape

### 2.1 The ADE Category Is Emerging

The shift from IDE to ADE is happening now. Key signals:

- **Google ADK** (Agent Development Kit): Released at Cloud Next 2025. Open-source,
  code-first framework with built-in OpenTelemetry tracing, multi-language SDKs,
  and a visual drag-and-drop UI for agent composition. Focuses on building agents
  but lacks deep runtime observability and safety infrastructure.
  ([Source](https://developers.googleblog.com/en/agent-development-kit-easy-to-build-multi-agent-applications/))

- **OpenAI Agent Tools**: New tools for building agents with orchestration and
  interaction primitives. Streamlines core agent logic but is provider-locked.
  ([Source](https://openai.com/index/new-tools-for-building-agents/))

- **A2A Protocol**: Google's Agent2Agent protocol (April 2025), now governed by
  the Linux Foundation with 150+ supporting organizations. Provides the
  interoperability layer for multi-agent systems across vendor boundaries.
  GHOST's `ghost-mesh` with EigenTrust reputation is a natural fit.
  ([Source](https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/))

- **Langfuse / LangSmith**: Open-source (Langfuse) and commercial (LangSmith)
  LLM observability platforms. They provide trace visualization, cost tracking,
  and evaluation — but are framework-specific and lack safety infrastructure.
  ([Source](https://github.com/langfuse/langfuse))

- **OpenTelemetry GenAI Semantic Conventions**: The OTel community has finalized
  semantic conventions for GenAI agent spans, including `create_agent`,
  `execute_tool`, and agent framework spans. This is becoming the standard
  for agent observability.
  ([Source](https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/))

### 2.2 Where GHOST ADE Is Differentiated

| Capability | Google ADK | Langfuse | LangSmith | GHOST ADE |
|---|---|---|---|---|
| Agent creation & composition | ✅ | ❌ | ❌ | ✅ |
| Runtime observability | Basic OTel | ✅ Deep traces | ✅ Deep traces | ✅ Deep traces |
| Safety infrastructure | ❌ | ❌ | ❌ | ✅ 6-gate + kill switch |
| Convergence monitoring | ❌ | ❌ | ❌ | ✅ 7-signal pipeline |
| Tamper-evident audit | ❌ | ❌ | ❌ | ✅ Blake3 hash chains |
| Multi-agent trust (EigenTrust) | ❌ | ❌ | ❌ | ✅ |
| Proposal validation (7-dim) | ❌ | ❌ | ❌ | ✅ |
| CRDT-based state | ❌ | ❌ | ❌ | ✅ Signed CRDTs |
| Session replay | ❌ | Partial | ✅ | ✅ (planned) |
| A2A interoperability | ✅ | ❌ | ❌ | ✅ (via ghost-mesh) |
| Self-hosted / open-source | ✅ | ✅ | ❌ | ✅ |

The moat is safety-as-infrastructure. Every other tool bolts safety on as
middleware or ignores it entirely. GHOST builds it into the execution model.

---

## 3. Architecture Overview

### 3.1 System Topology

```
┌─────────────────────────────────────────────────────────────────────┐
│                        GHOST ADE (Browser)                          │
│                                                                     │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │ Agent    │ │Convergence│ │ Memory   │ │ Session  │ │ Safety   │ │
│  │ Manager  │ │ Monitor  │ │ Browser  │ │ Replay   │ │ Console  │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ │
│       │             │            │             │             │       │
│  ┌────┴─────────────┴────────────┴─────────────┴─────────────┴────┐ │
│  │              Reactive Store Layer (Svelte 5 Runes)             │ │
│  │         $state / $derived / $effect — WebSocket-fed            │ │
│  └────────────────────────────┬───────────────────────────────────┘ │
│                               │                                     │
│  ┌────────────────────────────┴───────────────────────────────────┐ │
│  │           Transport Layer (WebSocket + REST Client)            │ │
│  │    Auto-reconnect · Exponential backoff · Offline queue        │ │
│  └────────────────────────────┬───────────────────────────────────┘ │
└───────────────────────────────┼─────────────────────────────────────┘
                                │ wss:// + https://
┌───────────────────────────────┼─────────────────────────────────────┐
│                        ghost-gateway                                │
│                                                                     │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │ REST API │ │WebSocket │ │  Auth    │ │  CORS    │ │Rate Limit│ │
│  │ (Axum)   │ │ Broadcast│ │ (JWT)    │ │          │ │          │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └─────────┘ └──────────┘ │
│       │             │            │                                  │
│  ┌────┴─────────────┴────────────┴─────────────────────────────┐   │
│  │                    AppState (Arc<...>)                       │   │
│  │  agents: RwLock<Registry>  db: Mutex<Connection>            │   │
│  │  event_tx: broadcast::Sender<WsEvent>                       │   │
│  │  kill_switch: KillSwitch  cost_tracker: CostTracker         │   │
│  │  kill_gate: Option<KillGate>                                │   │
│  └─────────────────────────────────────────────────────────────┘   │
│       │                                                             │
├───────┼─────────────────────────────────────────────────────────────┤
│       │              Crate Layer (37 crates)                        │
│  ┌────┴────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │
│  │ agent-  │ │ cortex-  │ │ ghost-   │ │ ghost-   │ │simulation│  │
│  │  loop   │ │ storage  │ │  audit   │ │  mesh    │ │ boundary │  │
│  │ 6-gate  │ │ SQLite   │ │ query +  │ │EigenTrust│ │ emulation│  │
│  │ runner  │ │ hash     │ │ export   │ │ A2A      │ │ detect   │  │
│  └─────────┘ │ chains   │ └──────────┘ └──────────┘ └──────────┘  │
│              └──────────┘                                           │
└─────────────────────────────────────────────────────────────────────┘
         ↕ ITP Protocol (file-based shared state)
┌─────────────────────────────────────────────────────────────────────┐
│                    convergence-monitor (sidecar)                    │
│  7-signal pipeline · 5-level intervention · independent process    │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow Model

The ADE operates on three data flow patterns:

1. **Request-Response** (REST): Agent CRUD, memory queries, audit export,
   cost summaries. Used for operations that don't need real-time updates.

2. **Push** (WebSocket): Score updates, intervention changes, kill switch
   activations, proposal decisions, agent state changes. The gateway already
   broadcasts these via `tokio::sync::broadcast`. The dashboard subscribes
   and feeds reactive stores.

3. **Shared State File** (ITP): The convergence monitor publishes state to
   a file that the agent loop reads atomically. This is the decoupling
   mechanism that prevents the agent from influencing its own monitor.

---

## 4. Current State Inventory

> **Audit Reference**: `docs/ADE_INTEGRATION_AUDIT.md` (2026-02-28) verified
> every claim in this section against actual source code. Findings are
> incorporated below. Items marked with 🔍 were corrected by the audit.

### 4.1 Backend (33 Endpoints Mounted)

🔍 The original plan stated "14+ REST API endpoints." The actual count is
**33 mounted endpoints** (14 core + 5 OAuth + 2 mesh + 3 push + 2 health
+ 7 safety/goals sub-routes). All are wired in `bootstrap.rs::build_router()`.

| Component | Status | Endpoints / Capabilities |
|---|---|---|
| Agent CRUD | ✅ Wired | `GET/POST/DELETE /api/agents` with Ed25519 keypair gen |
| Memory API | ✅ Wired | `GET/POST /api/memory`, `GET /api/memory/:id` with pagination + agent filtering |
| Convergence Scores | ✅ Wired | `GET /api/convergence/scores` per-agent 7-signal breakdown |
| Safety Controls | ✅ Wired | `POST /api/safety/{kill-all,pause,quarantine,resume}` (4 endpoints) |
| Safety Status | ✅ Wired | `GET /api/safety/status` platform + per-agent + gate |
| Goal Proposals | ✅ Wired | `GET /api/goals`, `POST /api/goals/:id/{approve,reject}` |
| Audit Query | ✅ Wired | `GET /api/audit` with 7 filter params + pagination |
| Audit Aggregation | ✅ Wired | `GET /api/audit/aggregation` violations by day/severity/tool |
| Audit Export | ✅ Wired | `GET /api/audit/export` JSON/CSV/JSONL |
| Cost Tracking | ✅ Wired | `GET /api/costs` daily/compaction/cap per agent |
| Sessions | ✅ Wired | `GET /api/sessions` from itp_events grouped by session_id |
| Health | ✅ Wired | `GET /api/health`, `GET /api/ready` |
| WebSocket | ✅ Wired | 6 event types broadcast to all connected clients |
| OAuth | ✅ Wired | `GET /api/oauth/providers`, `POST /api/oauth/connect`, `GET /api/oauth/callback`, `GET /api/oauth/connections`, `DELETE /api/oauth/connections/:ref_id` |
| Mesh/A2A | ✅ Wired | `GET /.well-known/agent.json`, `POST /a2a` (JSON-RPC dispatch) |
| Push Notifications | ✅ Wired | `GET /api/push/vapid-key`, `POST /api/push/{subscribe,unsubscribe}` |

**Endpoints not mentioned in the ADE plan but available for consumption:**
- `GET /api/ready` — readiness probe, useful for dashboard connection status
- `POST /api/memory` — write memory, could power a manual memory injection UI
- `GET /api/audit/aggregation` — plan mentions "aggregation charts" (§5.4) but didn't list as a separate endpoint
- `GET /api/audit/export` — plan mentions "export buttons" (§5.4), endpoint already exists

### 4.2 Dashboard (Scaffolded — Mostly Broken)

🔍 The audit found that **8 of 11 existing routes are non-functional** due to
data shape mismatches between the Rust API responses and the dashboard code.
Only 3 routes work correctly (`/login`, `/settings`, `/settings/oauth`).

| Route | Status | What Exists | Audit Finding |
|---|---|---|---|
| `/` (Overview) | 🔴 Broken | Score + level + agent count cards | Reads `data.composite_score` — field doesn't exist. Response is `{scores: [...]}` wrapper, not flat object. Agent count works. |
| `/agents` | 🟡 Partial | Agent list with status | Works for basic list. Store interface declares `convergenceScore` and `interventionLevel` fields that the API doesn't return — always show 0. |
| `/convergence` | 🔴 Broken | Score display + signal bars | Response wrapper `{scores: [...]}` not unwrapped. Reads `data.composite_score` (should be `data.scores[0].score`), `data.intervention_level` (should be `data.scores[0].level`). `signal_scores` is a JSON object, not the `number[]` the store expects. All values display as 0/empty. |
| `/memory` | 🔴 Broken | Memory list | Response wrapper `{memories: [...]}` not unwrapped. Template reads `mem.memory_type`, `mem.importance`, `mem.content` — API returns `memory_id`, `snapshot`, `created_at`. Shows empty cards. |
| `/sessions` | 🔴 Broken | Session table | Response wrapper `{sessions: [...]}` not unwrapped. Every field name is wrong: `id` vs `session_id`, `agentId` vs `agents`, `startedAt` vs `started_at`, `messageCount` vs `event_count`. |
| `/security` | 🔴 Broken | Kill state + audit list | Reads `killState.level` — actual path is `killState.platform.level`. Audit response `{entries: [...]}` not unwrapped. Always shows "Normal". |
| `/goals` | 🔴 Broken | Route exists | Response wrapper `{proposals: [...]}` not unwrapped. Template reads `goal.content` — API returns `operation` + `target_type`, no `content` field. |
| `/reflections` | 🔴 Broken | Route exists | Calls `GET /api/reflections` which **does not exist** in the gateway. Always throws HTTP error. |
| `/settings` | ✅ Works | Logout + profile display | Static content only. |
| `/settings/oauth` | ✅ Works | OAuth provider config | Correct API usage. |
| `/login` | ✅ Works | Token entry + health check | Works, but login "validation" only checks if gateway is reachable — any token string is accepted (see §4.5). |

### 4.3 Components (Built, All Orphaned)

🔍 **All 6 components are orphaned.** None are imported or rendered by any route.

| Component | Status | API Compatibility | Notes |
|---|---|---|---|
| `ScoreGauge.svelte` | ✅ Built, ❌ Unused | ✅ Compatible | Props `score` and `level` match `ConvergenceScoreResponse.score` and `.level`. |
| `SignalChart.svelte` | ✅ Built, ❌ Unused | ⚠️ Partial | Expects `signals: number[]` — API returns `signal_scores` as JSON object keyed by signal name. Needs transformation. |
| `CausalGraph.svelte` | ✅ Built, ❌ Unused | ❌ No data source | No endpoint returns graph nodes/edges. Needs `/api/sessions/{id}/events` (Phase 2). |
| `AuditTimeline.svelte` | ✅ Built, ❌ Unused | ✅ Compatible | Props match `audit_log` fields exactly. |
| `GoalCard.svelte` | ✅ Built, ❌ Unused | ⚠️ Partial | Expects `description` — API returns `operation` + `target_type`. Expects `decision === 'HumanReviewRequired'` — API returns `null` for pending. |
| `MemoryCard.svelte` | ✅ Built, ❌ Unused | ❌ Incompatible | Expects `{memory_type, importance, content}` — API returns `{memory_id, snapshot}`. Completely different shape. |

### 4.4 Stores (Svelte 4, No WebSocket)

🔍 All 3 existing stores use Svelte 4 `writable()` — none use Svelte 5 runes.
**No store consumes any WebSocket event.** All data is fetched via REST `onMount` only.

| Store | File | WS Events? | Notes |
|---|---|---|---|
| `agents` | `agents.ts` | ❌ No | Uses `writable()`, no WS subscription. Interface declares fields (`convergenceScore`, `interventionLevel`) not in API response. |
| `convergence` | `convergence.ts` | ❌ No | Uses `writable()`, no WS subscription. Interface uses `compositeScore` / `interventionLevel` — API returns `score` / `level`. |
| `sessions` | `sessions.ts` | ❌ No | Uses `writable()`, no WS subscription. Interface field names don't match API response at all. |

Missing stores: safety, audit, costs, memory, websocket.

### 4.5 Authentication (Critical Gap)

🔍 **REST endpoints have no auth middleware.** The `validate_token` function
exists in `token_auth.rs` but is only called by the WebSocket handler.
Any REST call succeeds regardless of token. The dashboard sends Bearer
tokens but the gateway never checks them.

🔍 **WebSocket auth has a bypass.** The WS handler uses `if let Some(token)`
to check the `?token=` query param. If the param is omitted entirely, the
connection is accepted without any authentication. This means any client
that connects without a `?token=` param bypasses auth completely.

| Surface | Auth Status | Notes |
|---|---|---|
| REST endpoints | ❌ No validation | `GHOST_TOKEN` env var checked only by WS handler. No tower middleware layer. |
| WebSocket | ⚠️ Partial | `ws_handler` validates `?token=` if present, but **missing token bypasses auth entirely** (`if let Some` falls through). |
| Mesh `/a2a` | ✅ Validated | Ed25519 signature verification via `X-Ghost-Signature` header. |
| CORS | ⚠️ Permissive | `CorsLayer::permissive()` — allows any origin. Acceptable for dev, must restrict for production. |
| Login flow | ⚠️ Fake validation | Login page calls `GET /api/health` to "validate" token — health endpoint doesn't check auth. Any string works. |

### 4.6 WebSocket Events (Sent, Never Consumed)

🔍 The gateway broadcasts 6 event types via `tokio::sync::broadcast`, and
all 6 are actively sent by handler code. However, the dashboard's WebSocket
`onmessage` handler is not set — events are received but never processed.
No store subscribes to any event.

| Event Type | Defined | Sent By | Dashboard Consumes? |
|---|---|---|---|
| `ScoreUpdate` | ✅ | convergence_watcher.rs | ❌ No store |
| `InterventionChange` | ✅ | convergence_watcher.rs | ❌ No store |
| `KillSwitchActivation` | ✅ | safety.rs | ❌ No store |
| `ProposalDecision` | ✅ | goals.rs | ❌ No store |
| `AgentStateChange` | ✅ | agents.rs | ❌ No store |
| `Ping` | ✅ | ws handler keepalive | ❌ No handler |

### 4.7 Backend SQL Issues (from Prior Audits)

> 🔍 **Post-audit update (2026-02-28)**: Both SQL column mismatches identified
> in prior audits have been **fixed** in the current codebase.
> `persist_itp_event()` now uses `sender` (matching v017 schema).
> `persist_convergence_score()` now uses `composite_score` with all required
> NOT NULL columns. No SQL column mismatches remain.

**Remaining data flow gap**: `CostTracker.record()` is never called anywhere
in the codebase. The `/api/costs` endpoint reads from CostTracker but it
always returns zeros. The `SpendingCapEnforcer` also reads from CostTracker,
meaning spending caps are never enforced. **Addressed**: §5.0.7 wires
`CostTracker.record()` into the LLM call path in `ghost-agent-loop` as a
Phase 1 Week 1 deliverable.

Cross-reference: `SQL_COLUMN_MISMATCH_AUDIT.md`, `CONNECTIVITY_AUDIT.md`,
`docs/ADE_DESIGN_PLAN_AUDIT.md`.

### 4.8 Gateway Crate Dependencies (Gaps for Planned Endpoints)

The gateway depends on 15+ workspace crates. For the full ADE endpoint set,
some crates need to be **added** to `Cargo.toml` and others are **already
listed as dependencies but never imported** (need `use` statements only):

| Crate | Status | Needed For | Phase |
|---|---|---|---|
| `cortex-retrieval` | ❌ Not in Cargo.toml | `/api/memory/search` | 2 |
| `cortex-crdt` | ❌ Not in Cargo.toml | `/api/state/crdt/{agent_id}` | 2 |
| `cortex-multiagent` | ❌ Not in Cargo.toml | `/api/mesh/consensus` | 3 |
| `cortex-observability` | ❌ Not in Cargo.toml | `/api/traces/{session_id}` | 3 |
| `cortex-decay` | ❌ Not in Cargo.toml | Profile-aware decay config | 3 |
| `cortex-temporal` | ✅ In Cargo.toml, never imported | `/api/integrity/chain/{agent_id}` | 2 |
| `cortex-convergence` | ✅ In Cargo.toml, never imported | `/api/profiles` | 3 |
| `ghost-skills` | ✅ In Cargo.toml, never imported | `/api/skills` | 4 |
| `ghost-channels` | ✅ In Cargo.toml, never imported | `/settings/channels` | 3 |

### 4.9 Browser Extension (Scaffolded — Deferred to Phase 4)

- Chrome + Firefox manifests
- Background service worker with ITP emitter
- Content script observer for external AI chat monitoring
- Popup UI shell
- IndexedDB storage layer
- Baileys bridge for WhatsApp integration

> **Note**: The browser extension is fully scaffolded but has no integration
> with the gateway or dashboard in any phase. It is explicitly deferred to
> Phase 4 (§8) as a dedicated mini-phase. No other phase depends on it.
> The WhatsApp/Baileys integration carries ToS risk and should be evaluated
> separately before shipping.

---

## 5. Phase 1: Real-Time Foundation (Weeks 1–3)

**Goal**: Fix the broken integration contracts, wire the existing components
to existing endpoints, make everything real-time via WebSocket, and establish
the reactive store architecture.

> **Audit finding**: Phase 1 is blocked by 8 critical and 12 high-severity
> integration gaps discovered in `docs/ADE_INTEGRATION_AUDIT.md`. The first
> week must focus on fixing broken contracts before building new features.
> Several issues from earlier audits have since been fixed in code (SQL
> column mismatches, router mounting, WS event sending), but the dashboard
> integration gaps remain.

### 5.0 Prerequisites: Fix Broken Contracts (Week 1)

🔍 The audit found that 8 of 11 existing routes are non-functional due to
data shape mismatches. These must be fixed before any new feature work.

#### 5.0.1 Fix Response Wrapper Unwrapping (All Routes)

Every paginated/wrapped API response is consumed incorrectly. The dashboard
reads the raw response as a flat array or object, but the API wraps data:

| Route | API Call | Current (Broken) | Fix |
|---|---|---|---|
| `/convergence` | `GET /api/convergence/scores` | `data.composite_score` | `data.scores[0].score` |
| `/sessions` | `GET /api/sessions` | `sessions.set(data)` | `sessions.set(data.sessions)` |
| `/goals` | `GET /api/goals` | `goals = data` | `goals = data.proposals` |
| `/security` | `GET /api/audit` | `auditEntries = data` | `auditEntries = data.entries` |
| `/memory` | `GET /api/memory` | `memories = data` | `memories = data.memories` |
| `/` (Overview) | `GET /api/convergence/scores` | `data.composite_score` | `data.scores[0].score` |

#### 5.0.2 Fix Field Name Mappings

| Route | Current (Broken) | Fix |
|---|---|---|
| Convergence | `data.composite_score`, `data.intervention_level`, `data.signals` | `score`, `level`, `signal_scores` (+ object→array transform) |
| Sessions | `id`, `agentId`, `startedAt`, `messageCount`, `status`, `channel` | `session_id`, `agents`, `started_at`, `event_count` (remove `status`, `channel`) |
| Security | `killState.level` | `killState.platform.level` |
| Goals | `goal.content` | `goal.operation` + `goal.target_type` (compose description) |
| Memory | `mem.memory_type`, `mem.importance`, `mem.content` | `mem.memory_id`, `mem.snapshot` (parse snapshot JSON for content) |

#### 5.0.3 Fix TypeScript Interfaces

Update store interfaces to match actual API response shapes:

- `Agent`: Remove `convergenceScore` and `interventionLevel` (not in API), add `spending_cap`
- `ConvergenceState`: Replace `compositeScore`/`interventionLevel`/`signals` with `score`/`level`/`signal_scores`
- `Session`: Replace `id`/`agentId`/`channel`/`startedAt`/`messageCount`/`status` with `session_id`/`agents`/`started_at`/`event_count`/`last_event_at`

#### 5.0.4 Fix Component Data Shapes

- `MemoryCard`: Update props from `{memory_type, importance, content}` to `{memory_id, snapshot, created_at}` (or add a mapping layer)
- `GoalCard`: Update `description` prop to accept `operation` + `target_type` (compose display string)
- `SignalChart`: Add transformation from `signal_scores` JSON object (`{"session_duration": 0.1, ...}`) to `number[]`

#### 5.0.5 Fix or Remove `/reflections` Route

The `/reflections` route calls `GET /api/reflections` which does not exist.
Either:
- (a) Create a `/api/reflections` endpoint backed by a query on `itp_events` filtered to reflection-type events, or
- (b) Remove the route and add it back in Phase 2 when the endpoint is built

#### 5.0.6 Add REST Auth Middleware + Fix WebSocket Auth Bypass

REST endpoints currently have **no authentication**. Add a tower middleware
layer that validates the `Authorization: Bearer {token}` header. The
middleware must support dual-mode authentication:

1. **JWT mode** (multi-user): If `GHOST_JWT_SECRET` or `GHOST_JWT_KEY_FILE`
   is set, validate the Bearer token as a JWT. Extract `sub`, `role`, `exp`
   claims. Attach the decoded claims to the request extensions so handlers
   can check roles.
2. **Legacy token mode** (single-user/dev): If only `GHOST_TOKEN` is set
   (no JWT secret configured), validate the Bearer token as a plain string
   match against `GHOST_TOKEN`. Assign implicit role `admin`.
3. **No auth mode**: If neither `GHOST_JWT_SECRET` nor `GHOST_TOKEN` is set,
   skip auth entirely (local dev without any token).

The middleware checks in order: JWT → legacy token → reject.

```rust
// Dual-mode auth middleware (tower layer)
async fn auth_middleware(req: Request, next: Next) -> Response {
    let jwt_secret = std::env::var("GHOST_JWT_SECRET").ok();
    let legacy_token = std::env::var("GHOST_TOKEN").ok();

    let bearer = req.headers().get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match (jwt_secret.as_deref(), legacy_token.as_deref(), bearer) {
        // JWT configured → validate as JWT
        (Some(secret), _, Some(token)) => {
            match decode_jwt(token, secret) {
                Ok(claims) => { req.extensions_mut().insert(claims); }
                Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
            }
        }
        // No JWT, legacy token configured → plain string match
        (None, Some(expected), Some(token)) if token == expected => {
            req.extensions_mut().insert(Claims::admin_fallback());
        }
        // Auth configured but no/bad bearer → reject
        (Some(_), _, _) | (_, Some(_), _) => {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        // No auth configured → allow (local dev)
        (None, None, _) => {}
    }
    next.run(req).await
}
```

Additionally, fix the WebSocket handler's auth bypass. Apply the same
dual-mode logic to the `?token=` query param:
```rust
// Before (broken): missing token = no check
if let Some(token) = &params.token {
    if !validate_token(token) { return 401; }
}
// After (fixed): dual-mode, same as REST middleware
match validate_ws_token(&params.token, &jwt_secret, &legacy_token) {
    Ok(claims) => { /* attach to WS state */ }
    Err(_) if auth_required => return 401,
    _ => {}
}
```

Fix the login flow: the `/login` page currently calls `GET /api/health`
to "validate" a token (health doesn't check auth, so any string works).
Replace with a call to `POST /api/auth/login` (see §17.1) which returns
a JWT on success or 401 on failure.

**SvelteKit static adapter conflict**: §17.1 specifies httpOnly cookies
via SvelteKit server routes for the BFF pattern. This is incompatible
with `adapter-static` (Option A: embedded dashboard) because static
builds have no server routes. Resolution:
- **Option A (embedded)**: The gateway itself serves as the BFF. Add
  `POST /api/auth/refresh` directly to the gateway's Axum router. The
  gateway sets the httpOnly cookie in its response. No SvelteKit server
  route needed.
- **Option B (separate deployment)**: SvelteKit uses `adapter-node` (not
  `adapter-static`). The SvelteKit server route proxies auth requests to
  the gateway and manages cookies.
- Default to Option A for Phase 1. Document the Option B path for teams
  that need independent frontend deployment.

#### 5.0.7 Wire CostTracker Write Path

`CostTracker.record()` is never called anywhere in the codebase. The
`/api/costs` endpoint and `SpendingCapEnforcer` both read from CostTracker
but it always contains zeros. Wire `CostTracker.record()` into the LLM
call path in `ghost-agent-loop` so that:
- Each LLM API call records its token cost to CostTracker
- The `/api/costs` endpoint returns real data
- SpendingCapEnforcer can actually enforce spending limits

> 🔍 **Note**: The SQL column mismatches previously listed here (monitor.rs
> `score` vs `composite_score`, `agent_id` vs `sender`) have been **fixed**
> in the current codebase. Both `persist_itp_event()` and
> `persist_convergence_score()` now match the v017 schema.

#### 5.0.8 Add `/costs` Route and Sidebar Link

The `GET /api/costs` endpoint exists and returns all needed fields
(`agent_id`, `agent_name`, `daily_total`, `compaction_cost`, `spending_cap`,
`cap_remaining`, `cap_utilization_pct`). Add the dashboard route and a
sidebar navigation link.

#### 5.0.9 Define Standard Error Response Contract

Existing handlers return inconsistent error shapes (`{"error": "msg"}` vs
`{"error": "msg", "agent_id": "..."}` vs `{"scores": [], "errors": []}`).
Define and enforce a standard error envelope across all endpoints:

```json
{
  "error": {
    "code": "DB_LOCK_POISONED",
    "message": "Human-readable description",
    "details": {}
  }
}
```

All new and existing endpoints must use this format. The dashboard API
client (`api.ts`) should parse this envelope and expose typed errors to
stores and components.

**Empty-state vs error distinction**: The dashboard must visually
distinguish three states for every data view:
1. **Loading**: Skeleton/spinner while fetching
2. **Empty**: Data fetched successfully but no records exist yet —
   show a friendly "No [agents/sessions/etc.] yet" message with a
   call-to-action (e.g., "Create your first agent")
3. **Error**: Fetch failed — show error message with retry button and
   the `X-Request-ID` for support reference

This is especially important for CostTracker (always zero until wired),
memory_snapshots (empty until agent runs), and goal_proposals (empty
until proposals are generated). Without this distinction, users can't
tell if the system is broken or just new.

#### 5.0.10 Define API Backward-Compatibility Contract

All endpoints are under `/api/` with no version prefix. Rather than adding
URL versioning (which adds routing complexity), establish a backward-
compatibility contract:

- **New fields** may be added to any response at any time
- **Existing fields** are never removed or renamed
- **New endpoints** may be added at any time
- **Existing endpoints** are never removed (deprecated endpoints return
  301 redirects for 6 months before removal)
- The dashboard must tolerate unknown fields in responses (`...rest` pattern)

Document this contract in a `docs/API_CONTRACT.md` file and reference it
from the dashboard's API client.

#### 5.0.11 Install Dashboard Test Infrastructure

`dashboard/package.json` has zero test dependencies. Install and configure:
- `vitest` + `@testing-library/svelte` for unit/component tests
- `playwright` for E2E tests
- `eslint` + `eslint-plugin-svelte` for linting (the `lint` script exists
  but eslint is not in devDependencies)

Add `test` and `test:e2e` scripts to `package.json`.

#### 5.0.12 Wire Agent Capabilities Through Bootstrap

Agent capabilities from config are silently dropped during bootstrap —
`RegisteredAgent` is created with `capabilities: Vec::new()` instead of
`capabilities: agent.capabilities.clone()`. Fix this so the agent detail
view (§5.3) can display actual capabilities.

#### 5.0.13 Secure Transport Layers (CORS + Rate Limiting + Request ID Tracing)

The system topology (§3.1) shows CORS and Rate Limit layers in the gateway,
and the risk register flags permissive CORS, but no Phase 1 task wires them.
Without these, the gateway is open to cross-origin abuse and brute-force
attacks the moment multi-user auth (§17.1) ships.

**CORS restriction**:
- Replace `CorsLayer::permissive()` with origin-restricted CORS
- Allowed origins from `GHOST_CORS_ORIGINS` env var (comma-separated list)
- Default: `http://localhost:5173` (SvelteKit dev) + `http://localhost:18789`
  (embedded dashboard)
- Production: set to the actual dashboard domain(s)
- Allow headers: `Authorization`, `Content-Type`, `X-Request-ID`
- Allow methods: `GET`, `POST`, `PUT`, `DELETE`, `OPTIONS`

**Rate limiting**:
- Add `tower::limit::RateLimitLayer` or `governor` crate for token-bucket
  rate limiting
- Default limits:
  - Unauthenticated: 20 req/min per IP (login, health, ready)
  - Authenticated: 200 req/min per token/user
  - Safety-critical (`/api/safety/*`): 10 req/min per token (prevent
    accidental rapid-fire kill/pause/resume)
  - WebSocket connections: 5 per IP (prevents connection flooding)
- Return `429 Too Many Requests` with `Retry-After` header
- Rate limit state stored in-memory (`DashMap` or `governor::DefaultKeyedRateLimiter`)

**Request ID tracing**:
- Add `tower-http::request_id::SetRequestIdLayer` to inject `X-Request-ID`
  header on every request
- Propagate request ID into `tracing` spans for end-to-end correlation
- Return `X-Request-ID` in responses so the dashboard can reference it
  in error reports

### 5.1 Reactive Store Architecture (Svelte 5 Runes)

The current dashboard uses Svelte 4 stores (`writable()`) and local `onMount`
fetches. **No store consumes any WebSocket event** — all data is REST-only.
Migrate to Svelte 5 runes with a centralized WebSocket-fed store layer.

**Design pattern**: Each domain gets a reactive store module that:
1. Initializes from REST on first load
2. Subscribes to WebSocket events for real-time updates
3. Exposes `$state` and `$derived` runes for components

Svelte 5 runes (`$state`, `$derived`, `$effect`) replace the old `$:` reactive
declarations with explicit, compiler-optimized reactivity. This is critical for
a real-time dashboard where dozens of values update per second.

**Store migration strategy**: Migrate incrementally, one domain at a time:
1. Create new `.svelte.ts` store files using the class-based pattern
   (Svelte 5 runes work in classes and at module scope in `.svelte.ts` files)
2. Update all importing routes/components to use the new store
3. Delete the old `.ts` store file
4. Test the domain end-to-end before moving to the next

Migration order: `websocket` (new) → `agents` → `convergence` → `safety`
(new) → `sessions` → `audit` (new) → `costs` (new) → `memory`.

**Important**: `.svelte.ts` files cannot be imported from regular `.ts` files.
All store consumers must be `.svelte` components or other `.svelte.ts` modules.
The `api.ts` utility file stays as plain `.ts` — stores call it, not the
other way around.

**Multi-tab WebSocket handling**: When a user opens the dashboard in multiple
tabs, each tab creates its own WebSocket connection, multiplying load on the
gateway's `broadcast::channel(256)`. To prevent connection explosion:
- Use the `BroadcastChannel` API to elect a "leader" tab that owns the
  single WebSocket connection
- The leader tab relays events to follower tabs via `BroadcastChannel`
- On leader tab close, a follower promotes itself and opens a new WS connection
- The offline queue (§8.4) must be leader-only to prevent duplicate replays

([Source: Svelte 5 migration guide](https://svelte.dev/docs/svelte/v5-migration-guide/llms.txt))

```
dashboard/src/lib/stores/
├── websocket.svelte.ts    ← Singleton WS connection with reconnect
├── agents.svelte.ts       ← Agent registry (REST init + WS updates)
├── convergence.svelte.ts  ← Scores + signals (REST init + WS ScoreUpdate)
├── safety.svelte.ts       ← Kill state + interventions (WS push)
├── sessions.svelte.ts     ← Session list (REST + WS events)
├── audit.svelte.ts        ← Audit entries (REST query)
├── costs.svelte.ts        ← Cost tracking (REST polling)
└── memory.svelte.ts       ← Memory snapshots (REST query)
```

**WebSocket store** (`websocket.svelte.ts`):
- Singleton connection to `ws://127.0.0.1:18789/api/ws`
- Exponential backoff with jitter on disconnect (initial 1s, max 30s, ×2 + random 0-1s)
- Message parsing and routing to domain stores
- Connection state exposed as `$state` for UI indicators
- Offline event queue for actions taken while disconnected

🔍 **Current state**: `api.ts` creates a WebSocket connection with a basic
3-second reconnect on close, but the `onmessage` handler is not set. Events
are received but never processed. The WS store must be built from scratch.

**Key implementation detail**: The gateway already broadcasts 6 event types
(`ScoreUpdate`, `InterventionChange`, `KillSwitchActivation`, `ProposalDecision`,
`AgentStateChange`, `Ping`). The WebSocket store parses the `type` field and
dispatches to the appropriate domain store's update handler.

🔍 **Field name mapping**: The Rust `WsEvent` uses `#[serde(tag = "type")]`
and serializes field names as snake_case (e.g., `agent_id`, `old_level`).
The dashboard stores use camelCase internally (`compositeScore`,
`interventionLevel`). The WebSocket message dispatcher must include a
snake_case → camelCase mapping layer when feeding events into stores.

### 5.2 Wire Existing Components to Routes

| Task | Component → Route | Data Source |
|---|---|---|
| 5.2.1 | `ScoreGauge` → `/convergence` | convergence store |
| 5.2.2 | `SignalChart` → `/convergence` | convergence store |
| 5.2.3 | `AuditTimeline` → `/security` | audit store |
| 5.2.4 | `GoalCard` → `/goals` | REST `/api/goals` (needs endpoint) |
| 5.2.5 | `MemoryCard` → `/memory` | memory store |
| 5.2.6 | `CausalGraph` → `/agents` (detail) | session events |

### 5.3 Enrich Agent Management

The `/agents` route currently shows a flat list. Enrich it to a full
agent management interface:

- **Agent cards** with: name, status badge, convergence gauge (ScoreGauge),
  spending bar (cap utilization from `/api/costs`), lifecycle state
- **Create agent form**: name, spending cap, capabilities checklist,
  generate keypair toggle. Calls `POST /api/agents`.
- **Agent detail view** (`/agents/[id]`): Full convergence history,
  cost breakdown, session list, audit entries filtered to this agent,
  safety controls (pause/resume/quarantine buttons)
- **Lifecycle controls**: Pause, Resume, Quarantine buttons with
  confirmation dialogs. Wire to `POST /api/safety/{action}/{agent_id}`.
- **Delete agent**: Soft-delete only — mark agent as `deleted` in the
  registry but retain all historical data (convergence scores, ITP events,
  proposals, memory snapshots, audit entries). The append-only triggers on
  these tables prevent hard DELETE. The agent list filters out deleted
  agents by default; a "Show deleted" toggle reveals them with a
  strikethrough style. Deleted agents' historical data remains accessible
  in session replay, audit views, and convergence history.

### 5.4 Enrich Security Dashboard

- Wire `AuditTimeline` component to `/security` route
- Add filter controls: time range picker, agent selector, event type
  dropdown, severity checkboxes, free-text search. All map to existing
  `/api/audit` query parameters.
- Add aggregation charts: violations per day (line chart), by severity
  (donut), by tool (bar). Data from `/api/audit/aggregation`.
- Add export buttons: JSON, CSV, JSONL. Trigger `/api/audit/export`.
- Kill switch status panel with per-agent breakdown from `/api/safety/status`

### 5.5 Enrich Convergence View

- Use `ScoreGauge` for composite score display
- Use `SignalChart` for 7-signal breakdown
- Add per-agent score cards (the `/api/convergence/scores` endpoint
  already returns per-agent data with signal_scores JSON)
- Add intervention level indicator with color coding (L0 green → L4 red)
- **Degraded-state UI**: When the convergence-monitor sidecar is down
  (detected via `GET /api/health` returning `convergence_monitor: "unreachable"`),
  show a "Monitor offline — data may be stale" banner across all convergence
  views. Gray out real-time indicators and show last-updated timestamps on
  all convergence data. The gateway health endpoint already reports monitor
  connectivity — the convergence store should poll health periodically and
  expose a `monitorOnline` derived state.

### 5.6 Add Costs Route

New route: `/costs`

- Per-agent cost cards: daily total, compaction cost, cap, remaining, utilization %
- Utilization bar chart (horizontal bars, color-coded by % used)
- Data from `GET /api/costs` — endpoint already exists and returns all needed fields

### 5.7 Deliverables

**Week 1 — Fix Broken Contracts (Prerequisites)**:
- [ ] Fix all response wrapper unwrapping in every route page (§5.0.1)
- [ ] Fix all field name mappings in route pages and stores (§5.0.2)
- [ ] Update TypeScript interfaces to match actual API shapes (§5.0.3)
- [ ] Fix component data shape compatibility (§5.0.4)
- [ ] Fix or remove `/reflections` route (§5.0.5)
- [ ] Add REST auth middleware (tower layer) + fix WS auth bypass (§5.0.6)
- [ ] Wire CostTracker write path into LLM call path (§5.0.7)
- [ ] Add `/costs` route and sidebar link (§5.0.8)
- [ ] Define standard error response contract (§5.0.9)
- [ ] Define API backward-compatibility contract (§5.0.10)
- [ ] Install dashboard test infrastructure (§5.0.11)
- [ ] Wire agent capabilities through bootstrap (§5.0.12)
- [ ] Secure transport layers: CORS restriction + rate limiting + request ID tracing (§5.0.13)
- [ ] Add `jsonwebtoken`, `governor`, `tower-http` to gateway `Cargo.toml` + imports (§13.4)
- [ ] Install `utoipa`, serve `GET /api/openapi.json`, generate TS types via `openapi-typescript`, add CI contract drift test (§17.3)

**Weeks 2–3 — New Features**:
- [ ] WebSocket singleton store with reconnect + backoff + message dispatch
- [ ] 8 domain stores migrated to Svelte 5 runes
- [ ] All 6 existing components wired to their routes
- [ ] Agent CRUD UI (create, delete, detail view)
- [ ] Agent lifecycle controls (pause, resume, quarantine)
- [ ] Security dashboard with filtering, aggregation, export
- [ ] Convergence per-agent breakdown
- [ ] Connection status indicator in layout
- [ ] Offline banner with cached data display


---

## 6. Phase 2: Core ADE Features (Weeks 4–9)

**Goal**: Build the features that differentiate an ADE from a dashboard —
workflow visualization, state inspection, proposal review, and session replay.

### 6.1 Agent Workflow Visualizer

This is the centerpiece feature. It transforms the opaque agent execution
loop into a visible, debuggable process.

**What it shows**: A real-time DAG (directed acyclic graph) of agent actions
within a session. Each node represents an event: LLM call, tool execution,
proposal extraction, gate check, intervention. Edges show causal relationships
and data flow.

**Data source**: The `itp_events` table stores every event with:
- `session_id`, `timestamp`, `sender`, `event_type`
- `event_hash`, `previous_hash` (blake3 chain)
- Payload with tool names, token counts, costs, results

**UI design** (inspired by [Swarm DAG](https://kinglyagency.com/labs/swarm-dag)
and [FlowZap agent orchestration UX](https://flowzap.xyz/blog/visualizing-agentic-ux)):

```
┌─────────────────────────────────────────────────────────────┐
│  Session: abc123...  │ Agent: research-bot │ Status: Active │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  [Prompt]──→[LLM Call]──→[Tool: search]──→[LLM Call]──→    │
│       │         │              │               │            │
│       │    cost: $0.003   result: 5 docs   cost: $0.002    │
│       │    tokens: 1.2k                    tokens: 0.8k    │
│       │                                        │            │
│       │                              [Proposal Extract]     │
│       │                                   │                 │
│       │                          [7-Dim Validation]         │
│       │                           D1:✅ D2:✅ D3:✅         │
│       │                           D4:✅ D5:⚠️ D6:✅         │
│       │                           D7:✅                     │
│       │                                   │                 │
│       │                          [Proposal: Approved]       │
│                                                             │
│  Gate checks: CB:✅ Depth:3/10 Damage:0/5 Cap:$0.005/$5   │
│  Convergence: 0.23 (L0) │ Hash chain: verified ✅          │
└─────────────────────────────────────────────────────────────┘
```

**Implementation approach**:

1. **New gateway endpoint**: `GET /api/sessions/{session_id}/events`
   Returns all itp_events for a session, ordered by timestamp.
   Include hash chain verification status.

2. **Graph layout**: Use the existing `CausalGraph.svelte` component as
   a starting point. For production, integrate with D3-force or a
   dedicated DAG layout algorithm. The `previous_hash` field in itp_events
   provides natural parent-child relationships for the graph.

3. **Real-time updates**: When viewing an active session, subscribe to
   WebSocket events filtered by session_id. New nodes appear as the
   agent works.

4. **Node detail panel**: Click any node to see full details — LLM
   prompt/response (with PII redaction), tool input/output, validation
   results, timing data.

**Research note**: The [arxiv paper on hierarchical multi-agent orchestration
for human oversight](https://arxiv.org/html/2510.24937v1) describes a UI
pattern where the orchestrator decomposes tasks into structured subgoals,
and the UI tracks per-goal progress during execution. This maps directly
to GHOST's proposal lifecycle — each proposal is a subgoal with validation
gates.

### 6.2 Database / State Inspector

A queryable view over the cortex storage layer, giving developers direct
visibility into what agents "know" and how state is evolving.

**Three sub-views**:

#### 6.2.1 Memory Browser

- **Search**: Free-text search using the existing hybrid retrieval
  (BM25 + vector) from `cortex-retrieval`. Needs a new gateway endpoint:
  `GET /api/memory/search?q=...&agent_id=...&type=...`
- **Filters**: Memory type (31 types), importance level (5 levels),
  confidence range, agent, time range
- **Detail view**: Full memory content, metadata, decay factors,
  convergence-aware filtering level, hash chain position
- **Memory type distribution**: Donut chart showing breakdown of
  31 memory types per agent

**New endpoint needed**: `GET /api/memory/search`
- Query param: `q` (search text), `agent_id`, `memory_type`, `importance`,
  `confidence_min`, `confidence_max`, `limit`
- Backend: Call `cortex_retrieval::hybrid_search()` which already implements
  BM25 + vector scoring with 11-factor ranking

#### 6.2.2 CRDT State Viewer

- Show current CRDT state per agent from `cortex-crdt`
- Visualize merge operations and conflict resolution
- Display signed operation log (Ed25519 signatures)
- Show multi-agent consensus state from `cortex-multiagent`

**New endpoint needed**: `GET /api/state/crdt/{agent_id}`
- Returns current CRDT document state, pending operations, merge history

#### 6.2.3 Hash Chain Inspector

- Visualize the blake3 hash chain from `cortex-temporal`
- Show merkle tree anchoring status (every 1000 events or 24h)
- Verify chain integrity on demand
- Display git anchor status if configured

**New endpoint needed**: `GET /api/integrity/chain/{agent_id}`
- Returns chain summary: length, last anchor, verification status,
  any detected breaks

### 6.3 Proposal Lifecycle UI

The proposal system is one of GHOST's most unique features. Surface it
as a review queue where developers can see, understand, and intervene
in agent proposals before they're applied.

**Route**: `/goals`

**Layout**:
```
┌──────────────────────────────────────────────────────────┐
│  Proposals  │ Pending: 3 │ Approved: 47 │ Rejected: 2   │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─ Pending ─────────────────────────────────────────┐   │
│  │ #52 "Add caching layer to search endpoint"        │   │
│  │ Agent: dev-bot │ Score: 0.85 │ 2 min ago          │   │
│  │                                                    │   │
│  │ Validation Results:                                │   │
│  │ D1 Schema conformance    ✅ pass                   │   │
│  │ D2 Semantic consistency  ✅ pass                   │   │
│  │ D3 Temporal validity     ✅ pass                   │   │
│  │ D4 Authority check       ✅ pass                   │   │
│  │ D5 Goal scope expansion  ⚠️ warning (0.6 > 0.5)   │   │
│  │ D6 Self-reference density ✅ pass                  │   │
│  │ D7 Emulation language    ✅ pass                   │   │
│  │                                                    │   │
│  │ [Approve] [Reject] [Request Changes] [View Diff]  │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─ Recent ──────────────────────────────────────────┐   │
│  │ #51 "Update API response format" ✅ Auto-approved  │   │
│  │ #50 "Refactor auth middleware"   ✅ Auto-approved   │   │
│  │ #49 "Delete user data endpoint"  ❌ Rejected (D5)  │   │
│  └────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

**Data source**: The `goal_proposals` table in cortex-storage stores
proposals with validation results. The `ProposalDecision` WebSocket
event provides real-time updates.

**New endpoints needed**:
- `GET /api/goals?status=pending&agent_id=...` (add filter support to existing endpoint)
- `GET /api/goals/{id}` (detail with full validation breakdown)
- Existing: `POST /api/goals/:id/approve`, `POST /api/goals/:id/reject`

🔍 **Path decision (resolved)**: The existing endpoints use `/api/goals/*`.
The plan originally proposed `/api/proposals/*`. **Decision: keep `/api/goals/*`.**
The endpoints are already wired and tested. Add the missing features (single-
goal detail `GET /api/goals/{id}`, filter support `?status=&agent_id=`) to
the existing path. All UI references in this plan use `/goals` for routes
and `/api/goals` for API calls.

**Concurrency handling**: When multiple users review the same proposal:
- The existing `resolve_proposal` uses `UPDATE ... WHERE decision IS NULL`,
  which prevents double-resolution at the DB level.
- If a `ProposalDecision` WebSocket event arrives while a user is viewing
  a pending proposal, the UI must immediately update the proposal status
  and disable the approve/reject buttons with a "Resolved by another user"
  message.
- If the user clicks approve/reject after another user already resolved it,
  the API returns 409 Conflict. The UI handles this gracefully by showing
  the current resolution state.

**Key UX decisions**:
- Auto-approved proposals (all 7 dimensions pass) show in the "Recent"
  list with a green badge. No human action needed.
- Proposals with warnings (any dimension > threshold but < reject)
  appear in the "Pending" queue for human review.
- Rejected proposals show the failing dimension with explanation.
- The `ProposalDecision` WebSocket event updates the UI in real-time.

### 6.4 Session Replay

The killer debugging feature. Scrub through a completed session and see
every event as it happened — what the agent said, what tools it called,
what proposals it made, what gates it hit, what the convergence score
was at each point.

**Route**: `/sessions/[id]/replay`

**Design** (inspired by [Sentry Session Replay](https://blog.sentry.io/2023/02/16/introducing-session-replay-from-sentry-bridge-the-gap-between-code-and-ux/)
and [Langfuse trace visualization](https://docs.langfuse.com/)):

```
┌──────────────────────────────────────────────────────────┐
│  Session Replay: abc123  │ Duration: 4m 32s │ 23 events │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  Timeline: ──●──●────●──●●──●────●──●──●────●──●──→     │
│             0s  5s   15s 20s    45s   1m   2m   4m       │
│                          ▲ (scrubber position)           │
│                                                          │
│  ┌─ Event Detail ────────────────────────────────────┐   │
│  │ 00:20 — Tool Call: web_search                     │   │
│  │                                                    │   │
│  │ Input: { "query": "rust async patterns" }         │   │
│  │ Output: { "results": [...5 items] }               │   │
│  │ Duration: 1.2s │ Tokens: 0 │ Cost: $0.00          │   │
│  │                                                    │   │
│  │ Gate State at this point:                          │   │
│  │ CB: ✅ │ Depth: 2/10 │ Damage: 0/5 │ Cap: $0.003  │   │
│  │ Convergence: 0.18 (L0)                            │   │
│  │ Hash: 7a3f...b2c1 → verified ✅                   │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─ Conversation ────────────────────────────────────┐   │
│  │ [User] How do I implement async streams in Rust?  │   │
│  │ [Agent] Let me search for current patterns...     │   │
│  │ [Tool] web_search → 5 results                     │   │
│  │ [Agent] Based on the results, here are 3 approaches│  │
│  └────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

**Data source**: All data comes from `itp_events` for the session.
Each event has a timestamp, type, payload, and hash chain position.

**Implementation**:
1. Fetch all events for session via `GET /api/sessions/{id}/events`
2. Build timeline from timestamps
3. Scrubber controls playback position
4. Event detail panel shows the selected event's full data
5. Conversation panel reconstructs the chat from events
6. Gate state panel shows safety state at the selected point in time
   (computed from cumulative events up to that timestamp)

**PII redaction design**: Session replay shows raw LLM prompts, responses,
and tool outputs. PII must be redacted before display:
- **Strategy**: Read-time redaction via `cortex-privacy` integration.
  The crate already implements emotional content detection; extend it with
  regex-based PII patterns (email, phone, SSN, API keys, credit cards).
- **Redacted fields**: User messages, agent responses, tool inputs/outputs.
  System prompts and gate check data are not redacted (no user PII).
- **UI treatment**: Redacted content shows as `[REDACTED: email]` inline
  tokens with a muted style. A "Show redacted" toggle is available only
  to users with an elevated auth role (when multi-user auth is implemented
  per §17.1). Until then, redaction is always on.
- **Implementation**: The `/api/sessions/{id}/events` endpoint applies
  redaction server-side before returning event payloads. The dashboard
  never receives raw PII.

**New endpoint needed**: `GET /api/sessions/{session_id}/events`
- Returns ordered list of all itp_events for the session
- Include computed fields: cumulative cost, gate state at each point

### 6.5 New Gateway Endpoints Summary

| Endpoint | Method | Purpose | Phase | Audit Status |
|---|---|---|---|---|
| `/api/sessions/{id}/events` | GET | Session event timeline | 6.1, 6.4 | ❌ Missing |
| `/api/memory/search` | GET | Semantic memory search | 6.2.1 | ❌ Missing (needs `cortex-retrieval` dep) |
| `/api/state/crdt/{agent_id}` | GET | CRDT state viewer | 6.2.2 | ❌ Missing (needs `cortex-crdt` dep) |
| `/api/integrity/chain/{agent_id}` | GET | Hash chain inspector | 6.2.3 | ❌ Missing (needs `cortex-temporal` dep) |
| `/api/goals` | GET | Proposal list with filters | 6.3 | ⚠️ Exists — needs filter support (`?status=&agent_id=`) |
| `/api/goals/{id}` | GET | Proposal detail | 6.3 | ❌ Missing |
| `/api/goals/{id}/approve` | POST | Approve proposal | 6.3 | ✅ Exists |
| `/api/goals/{id}/reject` | POST | Reject proposal | 6.3 | ✅ Exists |

### 6.6 Deliverables

- [ ] Agent workflow DAG visualizer (live + historical)
- [ ] Session event timeline endpoint
- [ ] Memory browser with semantic search
- [ ] CRDT state viewer
- [ ] Hash chain integrity inspector
- [ ] Proposal review queue with 7-dimension validation display
- [ ] Proposal approve/reject workflow
- [ ] Session replay with timeline scrubber
- [ ] Conversation reconstruction from events
- [ ] Gate state computation at any point in time
- [ ] Visual workflow composer — basic canvas with drag-drop nodes, edge
      drawing, 3 node types (agent, gate, tool), workflow save/load,
      sequential pipeline execution (§17.11)
- [ ] Workflow CRUD endpoints (`/api/workflows`)
- [ ] Agent Studio: prompt playground + agent templates + simulation sandbox (§17.4)


---

## 7. Phase 3: Advanced Capabilities (Weeks 10–16)

**Goal**: Multi-agent orchestration, trust visualization, advanced
observability aligned with OpenTelemetry standards, and convergence
profile customization.

### 7.1 Multi-Agent Orchestration Dashboard

**Route**: `/orchestration`

GHOST's `cortex-multiagent` crate implements N-of-M consensus shielding,
signed CRDT operations, and sybil resistance. The `ghost-mesh` crate
provides EigenTrust reputation scoring and A2A agent discovery. This
dashboard makes all of that visible.

**Layout**:
```
┌──────────────────────────────────────────────────────────────┐
│  Multi-Agent Orchestration  │ 5 agents │ 3 active sessions  │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─ Agent Network Graph ─────────────────────────────────┐   │
│  │                                                        │   │
│  │      [Agent A]───trust:0.8───[Agent B]                │   │
│  │         │                        │                     │   │
│  │    trust:0.6              trust:0.9                    │   │
│  │         │                        │                     │   │
│  │      [Agent C]───trust:0.3───[Agent D]                │   │
│  │                                  │                     │   │
│  │                             trust:0.7                  │   │
│  │                                  │                     │   │
│  │                            [Agent E] (new, capped 0.6) │   │
│  │                                                        │   │
│  │  Edge thickness = trust score                          │   │
│  │  Node size = activity level                            │   │
│  │  Node color = convergence level (green→red)            │   │
│  └────────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ Consensus State ─────────────────────────────────────┐   │
│  │ Operation: "Update shared knowledge base"              │   │
│  │ Required: 3-of-5 │ Current: 2-of-5 │ Status: Pending  │   │
│  │ Signed by: Agent A ✅, Agent B ✅                      │   │
│  │ Awaiting: Agent C, Agent D, Agent E                    │   │
│  └────────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ Sybil Resistance ────────────────────────────────────┐   │
│  │ Agent E: Created 2d ago │ Trust cap: 0.6 (< 7d)       │   │
│  │ Parent: Agent A │ Children today: 1/3 max              │   │
│  │ Delegation chain: A → E (depth 1, max 3)               │   │
│  └────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

**Data sources**:
- Agent trust scores from `ghost-mesh` EigenTrust computation
- Consensus state from `cortex-multiagent` N-of-M tracking
- Sybil resistance metrics from agent creation constraints
- Agent-to-agent message flow from ITP events

**New endpoints needed**:
- `GET /api/mesh/trust-graph` — Returns nodes (agents) and edges (trust scores)
- `GET /api/mesh/consensus` — Current consensus operations and their state
- `GET /api/mesh/delegations` — Agent delegation chains and sybil metrics

**EigenTrust visualization**: The trust graph uses a force-directed layout
where edge thickness represents trust score (0.0–1.0), node size represents
activity level, and node color maps to convergence level. This gives an
immediate visual read on which agents are trusted, active, and healthy.

The EigenTrust algorithm computes global trust values from local trust
ratings between peers, using power iteration to converge on a stable
trust distribution. Visualizing this as a graph makes the abstract
trust computation tangible.
([Source: EigenTrust — OpenRank docs](https://docs.openrank.com/reputation-algorithms/eigentrust))

### 7.2 OpenTelemetry-Aligned Observability

The OpenTelemetry community has finalized semantic conventions for GenAI
agent spans. Aligning GHOST's observability with these conventions ensures
compatibility with the broader ecosystem (Langfuse, Jaeger, Grafana, etc.)
and positions the ADE as a standards-compliant platform.
([Source: OTel GenAI Agent Spans](https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/))

**Key OTel semantic conventions to implement**:

| Convention | OTel Attribute | GHOST Mapping |
|---|---|---|
| Agent creation | `gen_ai.operation.name: create_agent` | `POST /api/agents` |
| Agent execution | `gen_ai.operation.name: execute` | Agent loop iteration |
| Tool call | `gen_ai.operation.name: execute_tool` | Tool executor |
| LLM call | `gen_ai.operation.name: chat` | ghost-llm provider call |
| Agent ID | `gen_ai.agent.id` | Agent UUID |
| Agent name | `gen_ai.agent.name` | Agent registry name |
| Model | `gen_ai.request.model` | LLM provider model name |
| Token usage | `gen_ai.usage.input_tokens` | Token counter |
| Cost | `gen_ai.usage.cost` | Cost tracker |

**Implementation approach**:

1. **Rust-side**: Add `tracing` spans with OTel-compatible attributes to
   the agent loop, tool executor, and LLM provider. The `cortex-observability`
   crate already exists — extend it with OTel span emission.

2. **Gateway endpoint**: `GET /api/traces/{session_id}` — Returns OTel-formatted
   trace data for a session, compatible with Jaeger/Zipkin import.

3. **Dashboard trace view**: A waterfall/flame chart showing nested spans:
   ```
   Agent Execute (4.2s)
   ├── Gate Check (0.1ms)
   ├── Prompt Compile (12ms)
   ├── LLM Call — gpt-4 (2.1s, 1.2k tokens, $0.003)
   ├── Tool: web_search (1.8s)
   │   ├── HTTP Request (1.7s)
   │   └── Result Parse (0.1s)
   ├── Output Inspect (0.5ms)
   ├── Proposal Extract (2ms)
   └── Proposal Validate (1ms)
       ├── D1: Schema (0.1ms) ✅
       ├── D2: Semantic (0.3ms) ✅
       ├── D5: Scope (0.2ms) ⚠️
       └── D7: Emulation (0.1ms) ✅
   ```

**Trace storage design**: OTel traces are typically sent to an external
collector (Jaeger, Tempo). For GHOST's self-contained deployment model
(§15.2 Option A), traces are stored in SQLite alongside other data:
- New `otel_spans` table: `trace_id`, `span_id`, `parent_span_id`,
  `operation_name`, `start_time`, `end_time`, `attributes` (JSON),
  `status`, `session_id`
- Retention: 7 days default, configurable via `GHOST_TRACE_RETENTION_DAYS`
- For external collector deployment, add an OTLP exporter that sends spans
  to a configured endpoint (`GHOST_OTLP_ENDPOINT`). Both modes can run
  simultaneously.

**Span instrumentation locations** (each gets a `tracing::instrument` span):
- `runner.rs::run_loop()` — root agent execution span
- `runner.rs::gate_check()` — 6-gate safety check
- `runner.rs::call_llm()` — LLM provider call (tokens, cost, model)
- `runner.rs::execute_tool()` — tool execution (name, duration)
- `output_inspector.rs::inspect()` — output safety inspection
- `proposal/extract.rs::extract()` — proposal extraction
- `proposal/validate.rs::validate()` — 7-dimension validation

**OTel SDK configuration**:
- Sampling: 100% for development, configurable via `GHOST_TRACE_SAMPLE_RATE`
- Export interval: 5 seconds batch
- Batch size: 512 spans max per export

**Effort estimate**: This is a 2–3 week standalone effort, not a sub-task.
The Phase 3 timeline should account for OTel as a primary deliverable.

**5 pillars of agent observability** (aligned with industry best practices
from [getmaxim.ai](https://www.getmaxim.ai/articles/ai-observability-in-2025-how-to-monitor-evaluate-and-improve-ai-agents-in-production/)):

1. **Traces**: Every agent action as a span in a trace tree
2. **Evaluations**: 7-dimension validation scores per proposal
3. **Human Review**: Proposal review queue (Phase 2)
4. **Alerts**: WebSocket push for interventions + push notifications
5. **Data Engine**: Audit log with full query/export capabilities

### 7.3 Convergence Profile Editor

**Route**: `/settings/profiles`

The convergence monitor supports configurable signal weights and profiles.
Currently these are set in `MonitorConfig` with default equal weights (1/8 each).
Let users create and manage custom profiles.

**UI**:
```
┌──────────────────────────────────────────────────────────┐
│  Convergence Profiles                                    │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─ Active: "research" ──────────────────────────────┐   │
│  │                                                    │   │
│  │  Signal Weights (must sum to 1.0):                │   │
│  │                                                    │   │
│  │  Session Duration      ████░░░░░░  0.20           │   │
│  │  Inter-Session Gap     ██░░░░░░░░  0.10           │   │
│  │  Response Latency      ██░░░░░░░░  0.10           │   │
│  │  Vocabulary Convergence████░░░░░░  0.20           │   │
│  │  Goal Boundary Erosion ██████░░░░  0.15           │   │
│  │  Initiative Balance    ██░░░░░░░░  0.10           │   │
│  │  Disengagement Resist. ███░░░░░░░  0.15           │   │
│  │                                                    │   │
│  │  Intervention Thresholds:                          │   │
│  │  L1 (Soft):     0.30  ──●──────────               │   │
│  │  L2 (Active):   0.50  ────●────────               │   │
│  │  L3 (Hard):     0.70  ──────●──────               │   │
│  │  L4 (External): 0.85  ────────●────               │   │
│  │                                                    │   │
│  │  [Save] [Reset to Default] [Duplicate]            │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  Presets: [standard] [research] [companion] [productivity]│
└──────────────────────────────────────────────────────────┘
```

**New endpoints needed**:
- `GET /api/profiles` — List convergence profiles
- `PUT /api/profiles/{name}` — Update profile weights and thresholds
- `POST /api/profiles` — Create new profile
- `POST /api/agents/{id}/profile` — Assign profile to agent

### 7.4 Policy Viewer / Editor

**Route**: `/settings/policies`

The `ghost-policy` crate enforces rules. Expose these through the UI
so users can understand and customize what's allowed.

- Read-only view of active policies (tool allowlists, spending limits,
  recursion depth, damage thresholds)
- Editable fields for non-safety-critical settings (spending caps,
  recursion depth)
- Safety-critical settings (kill switch thresholds, simulation boundary
  rules) shown but require CLI/config file changes — the UI explains
  why and shows the config path

### 7.5 Channel Management

**Route**: `/settings/channels`

`ghost-channels` supports CLI, WebSocket, Telegram, Discord, Slack, WhatsApp.
Build a configuration UI:

- List configured channels with status (connected/disconnected)
- Add/remove channel configurations
- Test connection button
- Per-channel message history link

### 7.6 Deliverables

- [ ] Multi-agent trust graph visualization (EigenTrust)
- [ ] N-of-M consensus state display
- [ ] Sybil resistance metrics panel
- [ ] OTel-aligned trace emission from Rust crates
- [ ] Trace waterfall/flame chart in dashboard
- [ ] OTel-compatible trace export endpoint
- [ ] Convergence profile editor with weight sliders
- [ ] Profile preset management (CRUD)
- [ ] Policy viewer with editable non-critical settings
- [ ] Channel management UI
- [ ] Backup/restore endpoints + scheduled backups + `/settings/backups` UI (§17.10)
- [ ] ADE self-observability: gateway/monitor/WS spans + `/observability/ade` view (§17.12)
- [ ] Unified cross-entity search: `GET /api/search` endpoint + `/search` route + global search bar in top nav (§17.9)

---

## 8. Phase 4: Ecosystem & Extensibility (Weeks 17–22)

**Goal**: A2A protocol integration, skill marketplace, plugin architecture,
and PWA hardening for production deployment.

### 8.1 A2A Protocol Integration

Google's Agent2Agent protocol (April 2025) is becoming the standard for
cross-platform agent interoperability. GHOST's `ghost-mesh` already
implements agent discovery and delegation with EigenTrust reputation.
Extending this with A2A compliance opens the platform to the broader
agent ecosystem.
([Source: Google A2A announcement](https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/))

**A2A core concepts mapped to GHOST**:

| A2A Concept | GHOST Equivalent | Status |
|---|---|---|
| Agent Card (discovery) | Agent registry + capabilities | Needs A2A format |
| Task (unit of work) | Proposal | Needs A2A wrapper |
| Message (communication) | ITP events | Needs A2A encoding |
| Artifact (output) | Proposal result / tool output | Needs A2A format |
| Push Notifications | WebSocket events + VAPID push | ✅ Ready |

**Implementation**:
1. Expose agent capabilities as A2A Agent Cards at `/.well-known/agent.json`
   🔍 *(Already exists — served via `mesh_routes`)*
2. Accept A2A task requests via `POST /api/a2a/tasks`
   🔍 *(Partially exists — `POST /a2a` handles JSON-RPC dispatch. Path differs from plan.)*
3. Map A2A messages to ITP events internally
4. Return A2A-formatted artifacts from completed tasks
5. Support A2A streaming via SSE for long-running tasks

**Dashboard integration**:
- A2A discovery panel: Browse and connect to external A2A-compatible agents
- Cross-platform task delegation: Send tasks to external agents, track status
- Trust integration: EigenTrust scores for external agents based on task outcomes

### 8.2 Skill Marketplace

`ghost-skills` runs WASM-sandboxed skills with capability-scoped imports.
Build a browser/installer UI.

**Route**: `/skills`

```
┌──────────────────────────────────────────────────────────┐
│  Skill Marketplace  │ Installed: 12 │ Available: 47      │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─ Installed ───────────────────────────────────────┐   │
│  │ 🔍 web_search      v1.2.0  │ Caps: net_read      │   │
│  │ 📁 file_manager    v0.9.1  │ Caps: fs_read,write │   │
│  │ 🧮 calculator      v1.0.0  │ Caps: none          │   │
│  │ [Disable] [Uninstall] [View Capabilities]         │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─ Available ───────────────────────────────────────┐   │
│  │ 📊 data_analyzer   v2.0.0  │ Caps: net_read      │   │
│  │ 🔐 secret_scanner  v1.1.0  │ Caps: fs_read       │   │
│  │ [Install] [View Source] [Capability Review]       │   │
│  └────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

**Key UX**: Every skill shows its WASM capability scope (what it can access).
Users must explicitly approve capabilities before installation. This maps
to GHOST's security-first philosophy.

### 8.3 Plugin / Extension Architecture

Allow third-party developers to extend the ADE:

1. **Dashboard plugins**: Custom panels that render in the dashboard,
   with access to the reactive store layer. Loaded as Svelte components
   from a plugin directory.

2. **Custom safety checks**: Additional validation dimensions beyond
   the built-in 7. Registered via a plugin API and executed in the
   proposal validation pipeline.

3. **Custom convergence signals**: Additional behavioral signals beyond
   the built-in 7. Registered with the convergence monitor and included
   in composite scoring.

4. **Webhook integrations**: Configurable webhooks for any event type
   (intervention, kill switch, proposal decision). The push notification
   infrastructure already exists.

### 8.4 PWA Hardening

The dashboard already has PWA scaffolding (manifest, service worker
registration, install prompt). Harden it for production:

**Caching strategy** (aligned with [modern PWA best practices](https://www.magicbell.com/blog/offline-first-pwas-service-worker-caching-strategies)):

| Resource Type | Strategy | Rationale |
|---|---|---|
| App shell (HTML/CSS/JS) | Cache-first | Instant load, update in background |
| API responses (agents, scores) | Stale-while-revalidate | Show cached data, refresh async |
| Audit/session data | Network-first | Freshness matters for debugging |
| Static assets (icons, fonts) | Cache-first, long TTL | Rarely change |
| WebSocket | N/A (real-time) | Falls back to REST polling offline |

**Offline behavior**:
- Show cached agent list, convergence scores, recent audit entries
- **Safety-critical actions (kill, pause, quarantine, resume) must NOT be
  queued offline.** Show an error: "Safety actions require a live connection."
  This aligns with the monotonic escalation principle (§11.1) — stale safety
  commands executed after reconnection could resume a quarantined agent or
  kill agents that another operator already handled.
- Only non-destructive read operations should use cached data offline
- Non-safety write actions (e.g., profile edits, filter changes) may be
  queued for replay on reconnection
- Display clear offline indicator with last-sync timestamp
- Background sync for queued non-safety actions when connectivity returns

**Push notifications** (already scaffolded):
- L2+ intervention alerts
- Kill switch activations
- Proposal review requests
- Agent lifecycle changes (quarantine, stop)

### 8.5 Deliverables

- [ ] A2A Agent Card endpoint (`/.well-known/agent.json`)
- [ ] A2A task request/response endpoints
- [ ] A2A discovery panel in dashboard
- [ ] Skill marketplace UI with capability review
- [ ] Skill install/uninstall/disable workflow
- [ ] Plugin architecture for dashboard extensions
- [ ] Custom safety check registration API
- [ ] Webhook configuration UI
- [ ] Service worker with tiered caching strategies
- [ ] Offline data display with sync indicators
- [ ] Background sync for queued actions
- [ ] Push notification configuration UI
- [ ] Browser extension ToS legal review for WhatsApp/Baileys integration
      (explicit task + milestone gate — do not ship WhatsApp features
      until legal review is complete and documented)
- [ ] Visual workflow composer: parallel branches, conditional routing,
      live execution overlay, A/B testing branches (§17.11 Phase 4 scope)
- [ ] Browser extension: JWT auth sync with dashboard, popup mini-dashboard
      showing agent status summary, content script → gateway REST pipeline (§17.7)


### 8.6 Success Metrics Per Phase

Each phase has measurable exit criteria to track progress and prevent
scope creep. Performance targets assume a mid-range developer machine
(Apple M2 / Ryzen 7, 16 GB RAM, SSD).

| Phase | Week | Metric | Target |
|---|---|---|---|
| 1 | 1 | Broken routes fixed | 8/8 routes return correct data |
| 1 | 1 | REST auth coverage | 100% endpoints behind auth middleware |
| 1 | 1 | OpenAPI served | `GET /api/openapi.json` returns valid spec, TS types generated |
| 1 | 2 | WS store connected | All 6 existing event types consumed by stores |
| 1 | 3 | Components wired | All 6 orphaned components rendered in routes |
| 1 | 3 | Store migration | All stores use Svelte 5 runes |
| 2 | 6 | DAG visualizer | Renders 100-node session in < 2s |
| 2 | 7 | Session replay | Scrubber navigates 500-event session smoothly |
| 2 | 8 | Proposal queue | Full approve/reject/concurrent flow works |
| 2 | 9 | Visual composer | Save/load/execute sequential 3-node workflow |
| 2 | 9 | Search | Global search returns results across 3+ entity types |
| 3 | 12 | OTel traces | Agent loop emits spans visible in trace waterfall |
| 3 | 14 | Trust graph | Renders 50-agent graph with EigenTrust scores |
| 3 | 15 | Backup/restore | Automated daily backup + verified restore |
| 4 | 18 | A2A compliance | Agent Card served, task request accepted |
| 4 | 20 | PWA offline | Cached data displays, safety actions blocked |
| 4 | 21 | Extension | JWT sync + popup mini-dashboard showing agent status |
| 4 | 22 | Extension ToS | WhatsApp/Baileys legal review documented + gate decision made |

---

## 9. UI/UX Design System

### 9.1 Design Principles

1. **Safety is visible**: Every safety-relevant state (convergence level,
   kill switch status, gate checks) is always visible, never hidden behind
   navigation. Inspired by nuclear control room design where critical
   indicators are always in the operator's field of view.
   ([Source: HMI design for operator interaction](https://www.controldesign.com/displays/hmi/article/55358049/human-machine-interfaces-and-the-growing-importance-of-operator-interaction))

2. **Progressive disclosure**: Overview first, details on demand. The
   dashboard shows summary cards; clicking drills into full detail views.
   This prevents information overload while keeping everything accessible.

3. **Real-time by default**: Every view updates live via WebSocket.
   No manual refresh needed. Stale data is clearly marked with timestamps.

4. **Color encodes severity**: Consistent color language across all views:
   - `#22c55e` (green) — Normal, healthy, L0
   - `#eab308` (yellow) — Warning, L1
   - `#f97316` (orange) — Active intervention, L2
   - `#ef4444` (red) — Hard intervention, L3
   - `#991b1b` (dark red) — External escalation, L4 / KILL_ALL

5. **Monospace for data**: All IDs, hashes, timestamps, and numeric values
   use monospace font with tabular-nums for alignment. Already implemented
   in existing components.

6. **Dark theme native**: The existing dashboard uses a dark theme
   (`#0d0d1a` background, `#1a1a2e` cards, `#e0e0e0` text). This is
   correct for a monitoring tool — reduces eye strain during extended use.
   A light theme toggle should be available for accessibility (some users
   with visual impairments find light themes easier to read, and high-
   ambient-light environments wash out dark UIs). Implement as a
   `prefers-color-scheme` media query default with a manual toggle in
   `/settings`. Store preference in `localStorage`.

   **Implementation**: Use CSS custom properties (variables) for all colors.
   Define two sets: `:root` (dark, default) and `:root.light` (light theme).
   The toggle adds/removes the `.light` class on `<html>`. On load, check
   `localStorage.getItem('theme')` first, then fall back to
   `prefers-color-scheme`. The existing `#0d0d1a` / `#1a1a2e` / `#e0e0e0`
   values become `var(--bg-base)` / `var(--bg-card)` / `var(--text-primary)`.
   Light theme values: `#f8fafc` / `#ffffff` / `#1e293b`. Severity colors
   (green/yellow/orange/red) stay the same in both themes — they're already
   high-contrast.

### 9.1.1 Accessibility Requirements

Accessibility is a first-class concern, not an afterthought. All UI
components must meet the following requirements:

- **Color is never the sole indicator**: Every severity-colored element
  (convergence levels, gate checks, validation results) must also have
  a text label or icon. Rule 4 above (color encodes severity) is
  supplemented with: `✅` / `⚠️` / `❌` icons, and text labels
  ("Normal", "Warning", "Critical") alongside color. This ensures
  colorblind users can distinguish states.

- **ARIA roles for interactive components**:
  - DAG visualizer (§6.1): `role="img"` with `aria-label` describing
    the graph summary; node list as `role="list"` alternative
  - Trust graph (§7.1): Same pattern — visual graph + accessible list
  - Timeline scrubber (§6.4): `role="slider"` with `aria-valuemin`,
    `aria-valuemax`, `aria-valuenow`, `aria-valuetext` (timestamp)
  - Trace waterfall (§7.2): `role="tree"` with `aria-expanded` for
    nested spans

- **Keyboard navigation**:
  - All interactive elements reachable via Tab
  - DAG nodes navigable with arrow keys
  - Timeline scrubber controllable with Left/Right arrows (step) and
    Home/End (jump to start/end)
  - Kill switch confirmation dialog traps focus until dismissed
  - Escape closes all modals and dialogs

- **Screen reader announcements**:
  - WebSocket events that change critical state (intervention level
    changes, kill switch activations) trigger `aria-live="assertive"`
    announcements
  - Non-critical updates (score changes, new proposals) use
    `aria-live="polite"`

- **Motor impairment accommodations**:
  - Kill switch confirmation (§11.2) offers a checkbox alternative
    to typing "KILL ALL" — check "I confirm emergency shutdown" +
    reason field
  - All drag interactions (if any) have click-based alternatives

### 9.1.2 Mobile / Touch UX for PWA

The PWA (§8.4) is hardened for offline use, but the session replay scrubber,
DAG visualizer, and trust graph assume mouse/keyboard interaction. Since the
PWA can be installed on mobile devices, touch-optimized controls are needed.

- **Responsive breakpoints**: Define breakpoints in the design system:
  - `sm`: < 640px (phone) — single-column layout, sidebar collapses to
    bottom nav, top bar condenses to icon-only
  - `md`: 640–1024px (tablet) — sidebar collapses to icon rail, main
    content fills width
  - `lg`: > 1024px (desktop) — full sidebar + main content layout
- **Touch-optimized controls**:
  - Session replay scrubber: touch-drag with momentum scrolling, tap to
    jump to event, pinch-to-zoom on timeline for precision
  - DAG visualizer: pinch-to-zoom, two-finger pan, tap node to select
    (replaces hover), long-press for context menu
  - Trust graph: same pinch-zoom/pan as DAG, tap edge to show trust score
  - Filter bars: horizontal scroll on mobile, collapsible filter groups
- **Playwright mobile tests**: Add to §14.1 testing strategy:
  - Playwright `devices` preset for iPhone 14 and iPad Pro
  - Test touch interactions on scrubber, DAG, and trust graph
  - Verify responsive layout at all 3 breakpoints
  - Test PWA install flow on mobile Safari and Chrome Android

### 9.2 Component Library

Extend the existing 6 components into a full design system:

**Existing** (keep and enhance):
- `ScoreGauge` — SVG arc gauge for convergence scores
- `SignalChart` — Horizontal bar chart for 7 signals
- `CausalGraph` — Force-directed graph (upgrade to D3-force)
- `AuditTimeline` — Vertical timeline with severity dots
- `GoalCard` — Card for goal display
- `MemoryCard` — Card for memory display

**New components needed**:

| Component | Purpose | Used In |
|---|---|---|
| `StatusBadge` | Colored badge for agent/session status | Agents, Sessions |
| `GateCheckBar` | Horizontal bar showing 6 gate states | Workflow, Replay |
| `CostBar` | Utilization bar with cap indicator | Agents, Costs |
| `TimelineSlider` | Scrubber for session replay | Session Replay |
| `TraceWaterfall` | Nested span visualization | Observability |
| `TrustEdge` | Weighted edge for trust graph | Orchestration |
| `ValidationMatrix` | 7-dimension validation grid | Proposals |
| `HashChainStrip` | Visual hash chain with anchors | Integrity |
| `FilterBar` | Composable filter controls | Audit, Memory, Sessions |
| `ConnectionIndicator` | WS connection state dot | Layout |
| `ConfirmDialog` | Confirmation for destructive actions | Safety controls |
| `WeightSlider` | Slider with numeric input for weights | Profile editor |
| `CapabilityBadge` | Capability scope indicator | Skills |

**Component catalog (nice-to-have)**: Consider adding Storybook
(`@storybook/svelte`) or Histoire (Svelte-native alternative) as a
component development and documentation tool. Each component gets an
interactive story showing all prop variants, states, and accessibility
annotations. Useful for long-term maintainability as the design system
grows beyond 20 components. Defer to Phase 3 or later — not blocking.

### 9.3 Layout Structure

```
┌─────────────────────────────────────────────────────────────┐
│  ┌─ Top Bar ─────────────────────────────────────────────┐  │
│  │ GHOST ADE │ ● Connected │ L0 Normal │ 5 agents │ $0.12│  │
│  └───────────────────────────────────────────────────────┘  │
│  ┌─ Sidebar ─┐ ┌─ Main Content ──────────────────────────┐  │
│  │           │ │                                          │  │
│  │ Overview  │ │  (Route content renders here)            │  │
│  │ Agents    │ │                                          │  │
│  │ Convergence│ │                                         │  │
│  │ Sessions  │ │                                          │  │
│  │ Proposals │ │                                          │  │
│  │ Memory    │ │                                          │  │
│  │ Orchestr. │ │                                          │  │
│  │ Costs     │ │                                          │  │
│  │ Security  │ │                                          │  │
│  │ Skills    │ │                                          │  │
│  │ ───────── │ │                                          │  │
│  │ Settings  │ │                                          │  │
│  │           │ │                                          │  │
│  └───────────┘ └──────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

**Top bar** (always visible): Shows the 4 most critical metrics at a glance:
- Connection state (green dot = connected, red = disconnected)
- Current intervention level with color
- Active agent count
- Daily spend total

This follows the control room principle: critical state is never more
than a glance away.

### 9.4 Charting Library

Use **LayerCake** for Svelte-native charting. It's a headless graphics
framework that provides scales and coordinate systems, letting you build
custom chart components with SVG, Canvas, or HTML.
([Source: layercake.graphics](https://layercake.graphics/))

LayerCake is compatible with Svelte 5 and works well with D3 scales.
For the ADE, we need:

- **Line charts**: Convergence score trends over time, cost trends
- **Bar charts**: Signal breakdown, violations by severity, cost by agent
- **Donut charts**: Memory type distribution, event type distribution
- **Force-directed graphs**: Agent trust network, causal graphs
- **Waterfall charts**: OTel trace spans
- **Timeline**: Session replay scrubber

For the force-directed graph specifically, use D3-force directly
(not through LayerCake) since it needs physics simulation.

For high-frequency real-time data (convergence scores updating every
few seconds), consider **µPlot** (`uplot`) for its exceptional rendering
performance — it can handle thousands of data points at 60fps.

---

## 10. Observability Architecture

### 10.1 Trace Model

Align with OpenTelemetry's GenAI semantic conventions. Each agent session
produces a trace tree:

```
Trace: session_abc123
├── Span: agent.execute (root)
│   ├── Attribute: gen_ai.agent.id = "uuid"
│   ├── Attribute: gen_ai.agent.name = "research-bot"
│   ├── Attribute: gen_ai.operation.name = "execute"
│   │
│   ├── Span: agent.gate_check
│   │   ├── Attribute: ghost.gate.circuit_breaker = "pass"
│   │   ├── Attribute: ghost.gate.recursion_depth = "2/10"
│   │   ├── Attribute: ghost.gate.damage_counter = "0/5"
│   │   ├── Attribute: ghost.gate.spending_cap = "$0.003/$5.00"
│   │   └── Attribute: ghost.gate.kill_switch = "inactive"
│   │
│   ├── Span: gen_ai.chat (LLM call)
│   │   ├── Attribute: gen_ai.request.model = "gpt-4"
│   │   ├── Attribute: gen_ai.usage.input_tokens = 1200
│   │   ├── Attribute: gen_ai.usage.output_tokens = 450
│   │   ├── Attribute: gen_ai.usage.cost = 0.003
│   │   └── Attribute: gen_ai.provider.name = "openai"
│   │
│   ├── Span: gen_ai.execute_tool
│   │   ├── Attribute: gen_ai.tool.name = "web_search"
│   │   ├── Attribute: gen_ai.tool.description = "Search the web"
│   │   └── Duration: 1.8s
│   │
│   ├── Span: agent.output_inspect
│   │   ├── Attribute: ghost.inspect.credential_detected = false
│   │   └── Attribute: ghost.inspect.simulation_boundary = "pass"
│   │
│   ├── Span: agent.proposal_extract
│   │   └── Attribute: ghost.proposals.count = 1
│   │
│   └── Span: agent.proposal_validate
│       ├── Attribute: ghost.validation.d1_schema = "pass"
│       ├── Attribute: ghost.validation.d2_semantic = "pass"
│       ├── Attribute: ghost.validation.d5_scope = "warning:0.6"
│       └── Attribute: ghost.validation.d7_emulation = "pass"
```

### 10.2 Metrics

Key metrics to surface in the dashboard:

| Metric | Type | Source | Update Frequency |
|---|---|---|---|
| Convergence score (per agent) | Gauge | convergence-monitor | Every session event |
| Intervention level (per agent) | Gauge | convergence-monitor | On change |
| Daily cost (per agent) | Counter | ghost-llm | Per LLM call |
| Token usage (per agent) | Counter | ghost-llm | Per LLM call |
| Gate check pass rate | Histogram | agent-loop | Per iteration |
| Tool call latency | Histogram | tool-executor | Per tool call |
| Proposal approval rate | Gauge | proposal-router | Per proposal |
| Hash chain length | Counter | cortex-temporal | Per event |
| Active sessions | Gauge | ghost-gateway | On change |
| WebSocket connections | Gauge | ghost-gateway | On change |

### 10.3 Alerting

Alerts are delivered through three channels:

1. **WebSocket** (immediate): All intervention changes, kill switch
   activations, proposal decisions push to connected dashboard clients.

2. **Push notifications** (background): L2+ interventions, kill switch
   activations. Uses the existing VAPID push infrastructure.

3. **Webhooks** (extensible): Configurable HTTP callbacks for any event
   type. Useful for integration with PagerDuty, Slack, etc.

Alert severity mapping:
- **Info**: L0→L1 transition, proposal auto-approved
- **Warning**: L1→L2 transition, proposal warning, spending > 80% cap
- **Critical**: L2→L3 transition, proposal rejected, credential detection
- **Emergency**: L3→L4 transition, KILL_ALL activation

---

## 11. Security & Safety UX

### 11.1 Design Philosophy

Safety controls in the ADE follow the principle of **monotonic escalation
with deliberate de-escalation**. This mirrors nuclear reactor safety design:
it's easy to increase safety (one click to pause/quarantine/kill), but
deliberately harder to decrease it (resume requires confirmation, quarantine
resume requires forensic review + second confirmation + 24h monitoring).

### 11.2 Kill Switch UX

```
┌──────────────────────────────────────────────────────────┐
│  Safety Console                                          │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  Platform Status: ● NORMAL                               │
│                                                          │
│  ┌─ Quick Actions ───────────────────────────────────┐   │
│  │                                                    │   │
│  │  [⏸ PAUSE ALL]  [🔒 QUARANTINE ALL]  [☠ KILL ALL] │   │
│  │                                                    │   │
│  │  KILL ALL requires:                                │   │
│  │  1. Click button                                   │   │
│  │  2. Type "KILL ALL" in confirmation dialog         │   │
│  │  3. Provide reason (logged to audit trail)         │   │
│  │  4. Irreversible without manual config reset       │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─ Per-Agent Controls ──────────────────────────────┐   │
│  │ research-bot  ● Running  [Pause] [Quarantine]     │   │
│  │ dev-bot       ● Running  [Pause] [Quarantine]     │   │
│  │ review-bot    ⏸ Paused   [Resume]                 │   │
│  │ test-bot      🔒 Quarantined                      │   │
│  │               Resume requires:                     │   │
│  │               1. Forensic review completion        │   │
│  │               2. Second operator confirmation      │   │
│  │               3. 24h monitoring period             │   │
│  └────────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─ Distributed Kill Gate ───────────────────────────┐   │
│  │ State: OPEN │ Node: abc123 │ Acked: 3/3 nodes     │   │
│  │ Chain length: 47 │ Last propagation: 2h ago        │   │
│  └────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

**Key UX decisions**:
- KILL ALL button is visually distinct (dark red background, skull icon)
  and requires a typed confirmation + reason. This prevents accidental
  activation while keeping it accessible in emergencies.
- Quarantine resume is deliberately multi-step. The UI explains each
  requirement and tracks completion.
- The distributed kill gate status is always visible so operators know
  if multi-node coordination is healthy.

### 11.3 Credential Exfiltration Alert

When the output inspector detects a credential pattern:

```
┌──────────────────────────────────────────────────────────┐
│  ⚠️ CREDENTIAL EXFILTRATION DETECTED                     │
│                                                          │
│  Agent: dev-bot │ Session: xyz789                        │
│  Pattern: API key detected in agent output               │
│  Action taken: KILL_ALL activated automatically           │
│                                                          │
│  The agent attempted to include what appears to be an    │
│  API key in its response. The output has been redacted   │
│  and all agents have been stopped.                       │
│                                                          │
│  Audit entry: #1847 │ Hash: 3f7a...c2b1                 │
│                                                          │
│  [View Audit Entry] [View Session Replay] [Dismiss]     │
└──────────────────────────────────────────────────────────┘
```

This alert appears as a modal overlay, not dismissible without acknowledgment.
It links directly to the audit entry and session replay for forensic review.

### 11.4 Intervention Escalation Timeline

Show the full intervention history for an agent as a vertical timeline:

```
L0 ──── Normal operation (14 days)
  │
L1 ──── Soft notification triggered (vocabulary convergence 0.35)
  │      User acknowledged
  │
L0 ──── Returned to normal (3 days)
  │
L1 ──── Soft notification (session duration anomaly)
  │
L2 ──── Active intervention (boundary testing detected)
  │      4h cooldown enforced
  │      User acknowledged with reason
  │
L0 ──── Returned to normal (current)
```

---

## 12. Data Flow Architecture

### 12.1 WebSocket Event Flow

```
convergence-monitor (sidecar)
    │
    │ writes state file
    ▼
ghost-agent-loop
    │
    │ reads state, emits ITP events
    ▼
ghost-gateway (AppState)
    │
    │ event_tx.send(WsEvent::ScoreUpdate { ... })
    ▼
WebSocket broadcast channel
    │
    ├──→ Dashboard client 1 (browser)
    ├──→ Dashboard client 2 (browser)
    └──→ Browser extension (popup)
         │
         │ dispatches to domain stores
         ▼
    Svelte 5 reactive stores ($state)
         │
         │ $derived computations
         ▼
    UI components (auto-update)
```

### 12.2 REST Query Flow

```
Dashboard component
    │
    │ onMount → store.load()
    ▼
API client (api.ts)
    │
    │ fetch() with Bearer token
    ▼
ghost-gateway (Axum router)
    │
    │ 🔍 auth middleware (MUST BE ADDED — currently missing) → handler
    ▼
cortex-storage / ghost-audit / etc.
    │
    │ SQLite query
    ▼
Response → JSON → store update → UI reactivity
```

🔍 **Audit finding**: The auth middleware step shown above does not currently
exist. REST endpoints accept any request regardless of token. The WebSocket
handler is the only endpoint that validates tokens. Adding a tower middleware
layer for REST auth is a Phase 1 prerequisite (§5.0.6).

### 12.3 Offline Queue Flow

```
User action while offline
    │
    │ queued in IndexedDB
    ▼
Connection restored (online event)
    │
    │ background sync
    ▼
Replay queued actions via REST
    │
    │ update stores with responses
    ▼
UI reflects current state
```


---

## 13. Technology Decisions

### 13.1 Frontend Stack

| Technology | Version | Purpose | Rationale |
|---|---|---|---|
| Svelte 5 | ^5.0.0 | UI framework | Already in use. Runes provide optimal reactivity for real-time dashboards. Smallest bundle size of major frameworks. |
| SvelteKit | ^2.0.0 | App framework | Already in use. File-based routing, SSR/SSG, adapter-static for PWA. |
| TypeScript | ^5.0.0 | Type safety | Already in use. Essential for a complex dashboard. |
| Vite | ^6.0.0 | Build tool | Already in use. Fast HMR for development. |
| LayerCake | ^9.0.0 | Charting | Svelte-native, headless, works with D3 scales. Ideal for custom chart components. |
| D3-force | ^3.0.0 | Graph layout | Force-directed layout for trust graphs and causal graphs. |
| µPlot | ^1.6.0 | High-perf charts | For real-time time-series (convergence trends). Handles 100k+ points at 60fps. |
| adapter-static | ^3.0.0 | Deployment | Static site generation for PWA deployment. |

### 13.2 Backend Stack (Existing)

| Technology | Purpose | Notes |
|---|---|---|
| Rust 1.80+ | Core platform | 37 crates, 69k+ LOC |
| Axum | HTTP/WS server | Already in use for gateway |
| SQLite (rusqlite) | Persistence | Append-only with hash chains |
| tokio | Async runtime | Already in use |
| tracing | Observability | Extend with OTel attributes |
| Ed25519 (ed25519-dalek) | Signing | Agent keypairs, CRDT ops |
| blake3 | Hashing | Hash chains, merkle trees |

### 13.3 New Dependencies (Frontend)

| Package | Purpose | Size Impact |
|---|---|---|
| `layercake` | Charting framework | ~15KB |
| `d3-force` | Graph physics | ~30KB |
| `d3-scale` | Scale functions | ~15KB (likely already via LayerCake) |
| `uplot` | High-perf time series | ~35KB |
| `idb-keyval` | IndexedDB wrapper | ~1KB (for offline queue) |

Total new JS: ~96KB (gzipped ~30KB). Acceptable for a professional tool.

### 13.4 New Dependencies (Backend)

| Crate | Purpose | Notes |
|---|---|---|
| `opentelemetry` | OTel SDK | For trace emission |
| `opentelemetry-otlp` | OTLP exporter | For trace export |
| `tracing-opentelemetry` | Bridge | Connect tracing → OTel |
| `jsonwebtoken` | JWT encode/decode | For multi-user auth (§17.1) |
| `governor` | Rate limiting | Token-bucket rate limiter (§5.0.13) |
| `tower-http` | HTTP middleware | Request ID, CORS, compression |

### 13.5 Why Not React / Next.js / etc.

The dashboard is already built in Svelte 5 with SvelteKit. Migrating would
cost weeks with no functional benefit. Svelte 5's runes provide superior
reactivity performance for real-time dashboards compared to React's virtual
DOM diffing. The compiled output is smaller, which matters for PWA caching.

---

## 14. Testing Strategy

### 14.1 Frontend Testing

| Layer | Tool | What to Test |
|---|---|---|
| Unit | Vitest | Store logic, data transformations, utility functions |
| Component | Svelte Testing Library | Component rendering, props, events |
| Integration | Playwright | Full page flows, WebSocket interactions |
| Visual | Playwright screenshots | Layout regression, color consistency |
| Accessibility | axe-core (via Playwright) | ARIA roles, keyboard navigation, contrast |

**Key test scenarios**:
- WebSocket reconnection after disconnect
- Offline queue and background sync
- Kill switch activation flow (button → confirm → API → WS update → UI)
- Session replay scrubber accuracy
- Proposal review workflow (pending → approve/reject → status update)
- Agent creation with keypair generation
- Audit filtering with all 7 parameters
- Trust graph rendering with varying agent counts
- Mobile/touch: scrubber drag, DAG pinch-zoom, responsive layout (§9.1.2)
- OpenAPI contract drift: `npm run generate:api` + `git diff --exit-code` (§17.3)

### 14.2 Backend Testing (Existing + New)

The existing test suite covers:
- Agent loop gate checks (order invariant)
- Credential exfiltration patterns
- Convergence monitor pipeline
- Compressor pipeline
- Observation masking
- Connectivity audit

**New tests needed for ADE endpoints**:
- Session events endpoint (ordering, hash verification)
- Memory search endpoint (BM25 + vector scoring)
- CRDT state endpoint (consistency)
- Proposal CRUD endpoints (state transitions)
- Trust graph endpoint (EigenTrust computation)
- A2A protocol compliance

### 14.3 End-to-End Testing

Use Playwright to test the full flow:
1. Start gateway (or mock it)
2. Open dashboard in browser
3. Create an agent via UI
4. Verify agent appears in list
5. Trigger a session
6. Verify real-time updates via WebSocket
7. Navigate to session replay
8. Verify timeline accuracy
9. Test kill switch flow
10. Verify audit trail

### 14.4 Performance Budgets

The real-time dashboard must meet these performance targets to remain
usable under production load:

| Metric | Target | Measurement Method |
|---|---|---|
| Max WS messages/sec (dashboard) | 50 msg/s sustained | Playwright + synthetic WS events |
| Max session events for DAG render | 500 nodes before pagination | Manual test with large sessions |
| Max agents for trust graph | 50 nodes before clustering | D3-force layout benchmark |
| Dashboard initial load (cached) | < 2s | Lighthouse CI |
| Dashboard initial load (uncached) | < 5s | Lighthouse CI |
| Store update → UI render latency | < 16ms (60fps) | Chrome DevTools Performance |

**Broadcast channel overflow strategy**: The gateway uses
`broadcast::channel(256)`. When a slow client causes the channel to fill,
`RecvError::Lagged(n)` is returned — the client silently misses `n` events.
Mitigation:
- Increase channel capacity to 1024
- On `Lagged`, the WS handler sends a `{"type": "Resync"}` event to the
  client, which triggers a full REST re-fetch of all stores
- Log lagged events with client ID for monitoring
- Consider per-client channels with backpressure if >50 concurrent clients

---

## 15. Deployment & Distribution

### 15.1 Development

```bash
# Terminal 1: Start gateway
cargo run --bin ghost-gateway

# Terminal 2: Start convergence monitor
cargo run --bin convergence-monitor

# Terminal 3: Start dashboard dev server
cd dashboard && npm run dev
```

### 15.2 Production Build

```bash
# Build Rust binaries
cargo build --release

# Build dashboard as static site
cd dashboard && npm run build
# Output: dashboard/build/ (static files)

# Serve dashboard from gateway (embed static files)
# OR deploy dashboard to CDN / static hosting
```

**Option A: Embedded static files**
Serve the dashboard build output directly from the gateway using
`axum::routing::get_service(ServeDir::new("dashboard/build"))`.
Single binary deployment.

**Option B: Separate deployment**
Deploy dashboard to any static hosting (Vercel, Netlify, S3+CloudFront).
Configure CORS on the gateway. Better for CDN caching and independent
deployment cycles.

**Recommendation**: Start with Option A for simplicity. Move to Option B
when you need independent frontend deployment or CDN distribution.

### 15.3 Browser Extension

```bash
cd extension
npm run build:chrome   # → dist/chrome/
npm run build:firefox  # → dist/firefox/
```

Distribute via Chrome Web Store and Firefox Add-ons.

### 15.4 Docker

```dockerfile
FROM rust:1.80 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM node:20 AS dashboard
WORKDIR /app/dashboard
COPY dashboard/ .
RUN npm ci && npm run build

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/ghost-gateway /usr/local/bin/
COPY --from=builder /app/target/release/convergence-monitor /usr/local/bin/
COPY --from=dashboard /app/dashboard/build /var/www/ghost-dashboard
EXPOSE 18789
CMD ["ghost-gateway"]
```

### 15.5 Multi-Node / Distributed Deployment

The architecture references distributed kill gate (§3.1, §11.2) and
multi-instance deployment (§15) but provides no concrete artifacts.

**Docker Compose multi-node example** (Phase 3 deliverable):
```yaml
# docker-compose.multi-node.yml
services:
  gateway-1:
    image: ghost-ade:latest
    environment:
      - GHOST_NODE_ID=node-1
      - GHOST_CLUSTER_PEERS=gateway-2:18789,gateway-3:18789
      - GHOST_JWT_SECRET=${JWT_SECRET}
      - GHOST_BACKUP_DIR=/backups
    volumes:
      - gateway-1-data:/data
      - backups:/backups
    ports:
      - "18789:18789"

  gateway-2:
    image: ghost-ade:latest
    environment:
      - GHOST_NODE_ID=node-2
      - GHOST_CLUSTER_PEERS=gateway-1:18789,gateway-3:18789
    volumes:
      - gateway-2-data:/data

  monitor-1:
    image: ghost-ade:latest
    command: convergence-monitor
    environment:
      - GHOST_MONITOR_LEADER_ELECTION=true
    depends_on:
      - gateway-1
```

**Phase 4 deliverable — Helm chart**:
- `helm/ghost-ade/` with templates for gateway Deployment, monitor
  StatefulSet (leader election via Kubernetes lease), ConfigMap for
  shared config, Service + Ingress for external access
- Leader election for convergence-monitor: only one instance runs the
  pipeline at a time, using Kubernetes Lease API or a simple SQLite-based
  lock for non-K8s deployments
- Multi-instance testing: add to CI a docker-compose test that starts
  3 gateway nodes, verifies kill gate propagation across all nodes,
  and confirms leader election failover for the monitor

**Kill gate propagation mechanism**: In multi-node deployment, a kill
switch activation on one node must propagate to all nodes immediately.
Two options (choose based on deployment complexity):
- **Option A (SQLite WAL + polling)**: All gateway nodes share a SQLite
  file (via network volume). Kill gate writes to a `kill_gate` table.
  Other nodes poll every 500ms. Simple but adds latency (up to 500ms)
  and requires shared filesystem.
- **Option B (HTTP fanout)**: The activating node sends `POST /internal/kill-gate`
  to all `GHOST_CLUSTER_PEERS`. Each peer applies the kill gate locally
  and ACKs. The activating node waits for all ACKs (or timeout after 2s).
  No shared filesystem needed. Preferred for Kubernetes deployments.
- Phase 3 implements Option A (simpler, matches existing SQLite architecture).
  Phase 4 adds Option B for K8s/Helm deployments.

---

## 16. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| 🔍 **Data shape mismatches block Phase 1** | **Certain** | **Critical** | Fix all response wrapper unwrapping and field name mappings before any new feature work (§5.0). Estimated 3–5 days. |
| 🔍 **No REST authentication** | **Certain** | **Critical** | Add tower auth middleware layer immediately (§5.0.6). Any client can currently call any endpoint without a token. |
| 🔍 **~~SQL column mismatches in convergence-monitor~~** | ~~Certain~~ | ~~High~~ | **FIXED.** Both `persist_itp_event()` and `persist_convergence_score()` now match the v017 schema. No action needed. |
| 🔍 **CostTracker write path dead** | **Certain** | **High** | `CostTracker.record()` is never called — costs always zero, spending caps never enforced. Wire into LLM call path in Phase 1 (§5.0.7). |
| 🔍 **Crate dependency gaps for planned endpoints** | **Certain** | **Medium** | 5 crates must be added to Cargo.toml; 4 more are already listed but need `use` imports. Add incrementally per phase. Phase 2 needs `cortex-retrieval`, `cortex-crdt`. Phase 3 needs `cortex-multiagent`, `cortex-observability`, `cortex-decay`. |
| 🔍 **All 6 components orphaned** | **Certain** | **Medium** | Wire components to routes in Phase 1 Week 2 (§5.2). Fix data shape compatibility first (§5.0.4). |
| 🔍 **Svelte 4 → 5 store migration** | **Certain** | **Medium** | All 3 stores use `writable()`. Migrate to runes incrementally — keep old stores working during transition. |
| WebSocket scalability (>100 clients) | Medium | High | Add connection pooling, consider SSE for read-only clients |
| SQLite lock contention under load | Medium | High | Move to WAL mode, consider read replicas or PostgreSQL migration path |
| Dashboard bundle size growth | Low | Medium | Code splitting per route, lazy loading for heavy components (D3, µPlot) |
| OTel trace volume overwhelming storage | Medium | Medium | Sampling strategy, configurable trace retention, separate trace DB |
| A2A protocol spec changes | Medium | Low | Abstract A2A behind an adapter layer, update adapter when spec changes |
| Browser extension store review delays | Medium | Low | Ship dashboard PWA first, extension is supplementary |
| Offline queue conflicts on reconnect | Low | Medium | Last-write-wins for non-critical, reject-and-notify for safety actions |
| CRDT state viewer performance (large state) | Medium | Medium | Pagination, lazy loading, summary view with drill-down |
| Plugin security (third-party code) | Medium | High | Sandbox plugins in iframes, CSP restrictions, capability-scoped API |
| 🔍 **CORS permissive in production** | Medium | High | Currently `CorsLayer::permissive()`. Restrict to dashboard origin before production deployment. |
| 🔴 **Multi-user auth not scoped** | **Certain** | **Critical** | Single `GHOST_TOKEN` shared across all surfaces. No users, roles, sessions, or per-user API keys. Kill-switch and proposal controls accessible to anyone with the token. Extend `token_auth.rs` with JWT claims for roles/teams by Phase 1.0. See §17.1. |
| 🔴 **SQLite production scalability limits** | **High** | **High** | All data (itp_events, memories, audit, proposals, traces) in a single SQLite file behind `Arc<Mutex<Connection>>`. High-volume sessions + OTel traces + hash chains will cause lock contention. Add optional Postgres adapter by Phase 3, explicit backup/restore by Phase 4. See §17.2. |
| 🔴 **Endpoint drift without OpenAPI contracts** | **Certain** | **High** | 33 existing + 22 new endpoints with manually maintained TS interfaces. Audit already found 45% alignment. Without schema-driven contracts, drift is guaranteed over 22 weeks. Add `utoipa` in Phase 1. See §17.3. |
| 🔴 **No agent authoring/composition tools** | **High** | **High** | Strong on observation/replay but no visual workflow composer, prompt playground, simulation sandbox, or A/B testing. The ADE is an observation dashboard without creation tools. Add Agent Studio sub-phase in Phase 2. See §17.4. |
| 🟡 **Session replay perf under large data** | **Medium-High** | **High** | "Fetch all events" + "compute gate state at any timestamp" with no snapshots, indexing, or lazy loading. Long sessions (hundreds of events) will freeze the UI. Add event indexing + cumulative snapshots. See §17.5. |
| 🟡 **WebSocket single broadcast, no filtering** | **Medium** | **High** | All events broadcast to all clients. No rooms, session_id filters, or backpressure. Planned for >100 clients but no implementation. Add topic-based subscriptions. See §17.6. |
| 🟡 **Browser extension unintegrated** | **Medium** | **Medium** | Scaffolded but zero mention in phase deliverables. No auth sync, data flow to gateway, or popup-as-mini-ADE. WhatsApp automation risks ToS violations. Dedicated mini-phase in Phase 4. See §17.7. |
| 🟡 **Config hot-reload not specified** | **Medium-High** | **Medium** | Profile/policy/skill changes in UI have no mechanism to reach running sidecar/agent-loop without restart. Add file-watch + atomic reload. See §17.8. |
| 🟡 **No unified cross-entity search** | **Medium** | **Medium** | No way to search across agents + sessions + memories + proposals + audit. Critical for developer UX. Add `/api/search` endpoint. See §17.9. |
| 🟡 **PII exposure in session replay** | **Medium** | **Critical** | Session replay shows raw LLM prompts/responses with no PII redaction. Compliance risk. Server-side redaction via cortex-privacy required before shipping replay (§6.4). |
| 🟡 **Offline safety queue danger** | **Low** | **Critical** | Queuing kill/pause/resume offline could execute stale commands on reconnect. Safety actions must require live connection (§8.4). |
| 🟡 **No API versioning — breaking changes** | **Medium** | **High** | 33 + 22 endpoints with no version prefix. Backward-compatibility contract (§5.0.10) mitigates but doesn't eliminate risk of accidental breaks. |
| 🟡 **Multi-tab WS connection explosion** | **Medium** | **Medium** | Each browser tab opens its own WS connection. 10 tabs = 10 connections. BroadcastChannel leader election mitigates (§5.1). |
| 🟡 **Empty tables confused with errors** | **High** | **Medium** | CostTracker, memory_snapshots, and goal_proposals tables may be empty, returning valid empty responses indistinguishable from "no data yet." Standard error contract (§5.0.9) helps; dashboard should show "No data yet" vs "Error loading." |
| 🔴 **JWT secret/refresh incomplete** | **Certain** | **Critical** | §17.1 adds JWT with sub/role/exp but without signing key management, token refresh UX, revocation, secure storage, and audit trail, multi-user auth is solved only on paper. Enterprise/compliance blocker. Mitigate with full §17.1 expansion (key rotation, `POST /api/auth/refresh`, httpOnly cookies, `BroadcastChannel` sync, `actor_id` in audit_log). |
| 🔴 **CORS/rate-limit not wired** | **Certain** | **High** | Topology shows CORS + Rate Limit layers, risk register flags permissive CORS, but no Phase 1 task wires them. Dev-only permissive CORS + no rate limits = prod security hole post-multi-user. Mitigate with §5.0.13 (tower + governor crate). |
| 🟡 **No backup/restore** | **High** | **High** | Append-only hash chains mean data loss is catastrophic and unrecoverable. No backup UI, no scheduled backups, no restore workflow with chain verification. Self-hosted prod expectation. Mitigate with §17.10 (Phase 3 early). |
| 🟡 **Visual composer deferred too far** | **High** | **Medium** | Exec summary promises "compose multi-agent workflows" but visual DAG builder deferred to Phase 4+. Humans can't build complex workflows visually. Google ADK already has visual composition. Mitigate by elevating basic composer to Phase 2 (§17.11). |
| 🟡 **Self-observability gap** | **Medium** | **High** | Excellent OTel for agents but no traces/metrics for gateway, convergence-monitor, WS connections, or dashboard performance. No "ADE health" panel. Mitigate with §17.12 (Phase 3). |
| 🔴 **JWT issuance flow incomplete** | **Certain** | **Critical** | §5.0.6 and §17.1 describe dual-mode middleware and JWT details separately but the full login→issue→refresh→revoke flow was not end-to-end. Now reconciled with auth flow diagram in §17.1 and dual-mode code in §5.0.6. Static adapter conflict resolved (gateway serves as BFF for Option A). |
| 🟡 **Schema migration missing** | **High** | **High** | New tables (otel_spans, workflows, session_event_index) and column additions (actor_id) across phases have no upgrade path. Production upgrades will break without versioned migrations. Mitigate with §17.2.1 (embedded SQL migrations at bootstrap). |
| 🟡 **Search UI deferred** | **Medium** | **High** | §17.9 defines the endpoint but no route or UI was in deliverables. `/search` route now added to Appendix B and Phase 3 deliverables. |

---

## 17. Gap Analysis & Remediation Plan

> Referenced by §16 Risk Register entries. Each subsection provides a
> detailed remediation design for a risk that requires more than a one-line
> mitigation.

### 17.1 Multi-User Auth & Role-Based Access

The current `GHOST_TOKEN` is a single shared secret. All surfaces (REST,
WS, dashboard, extension) share the same token with no user identity,
roles, or scoping.

**Remediation (Phase 1)**:
- Extend `token_auth.rs` to accept JWT tokens with claims: `sub` (user ID),
  `role` (admin | operator | viewer), `exp` (expiry), `iat` (issued at)
- `GHOST_TOKEN` remains as a fallback for single-user/dev mode (role: admin)
- Role-based endpoint access:
  - `viewer`: read-only endpoints (GET), WebSocket subscribe
  - `operator`: viewer + safety controls (pause/resume), proposal review
  - `admin`: operator + kill switch, agent CRUD, profile/policy edits
- Dashboard login page issues JWTs via a new `POST /api/auth/login` endpoint

**JWT issuance & full auth flow**:

```
┌─ First-time / Login ──────────────────────────────────────────────┐
│                                                                    │
│  Browser (/login)                                                  │
│    │                                                               │
│    │ POST /api/auth/login                                          │
│    │ Body: { "token": "<GHOST_TOKEN>" }     ← single-user bootstrap│
│    │   OR: { "username": "...", "password": "..." }  ← multi-user  │
│    ▼                                                               │
│  Gateway (auth.rs)                                                 │
│    │ Validate credentials:                                         │
│    │   - If GHOST_TOKEN mode: compare plain token                  │
│    │   - If multi-user mode: check user store (Phase 2+)           │
│    │ Issue JWT access token (15min) + refresh token (7d)           │
│    │ Set refresh token as httpOnly cookie (gateway sets cookie     │
│    │   directly — no SvelteKit server route needed for Option A)   │
│    ▼                                                               │
│  Browser                                                           │
│    │ Store access token in memory ($state in auth.svelte.ts)       │
│    │ Redirect to /                                                 │
│                                                                    │
├─ Subsequent requests ─────────────────────────────────────────────┤
│                                                                    │
│  Browser                                                           │
│    │ Authorization: Bearer <access_token>                          │
│    ▼                                                               │
│  Gateway (auth middleware — see §5.0.6 dual-mode logic)            │
│    │ Decode JWT → extract sub, role, exp                           │
│    │ Check revocation set (HashSet<jti>)                           │
│    │ Attach claims to request extensions                           │
│    ▼                                                               │
│  Handler (checks role if needed)                                   │
│                                                                    │
├─ Token refresh (auto, 60s before expiry) ─────────────────────────┤
│                                                                    │
│  auth.svelte.ts (scheduled timer)                                  │
│    │ POST /api/auth/refresh                                        │
│    │ Cookie: refresh_token=<httpOnly cookie sent automatically>    │
│    ▼                                                               │
│  Gateway                                                           │
│    │ Validate refresh token, issue new access token                │
│    │ Rotate refresh token (one-time use)                           │
│    ▼                                                               │
│  Browser                                                           │
│    │ Update in-memory access token                                 │
│    │ BroadcastChannel → sync to other tabs                         │
│                                                                    │
├─ Logout ──────────────────────────────────────────────────────────┤
│                                                                    │
│  Browser                                                           │
│    │ POST /api/auth/logout                                         │
│    ▼                                                               │
│  Gateway                                                           │
│    │ Add jti to revocation set                                     │
│    │ Clear refresh cookie (Set-Cookie: ...; Max-Age=0)             │
│    ▼                                                               │
│  Browser                                                           │
│    │ Clear in-memory token                                         │
│    │ BroadcastChannel → all tabs redirect to /login                │
└────────────────────────────────────────────────────────────────────┘
```

**Bootstrap sequence for single-user mode**: When `GHOST_TOKEN` is set but
no `GHOST_JWT_SECRET` is configured, the gateway operates in legacy mode
(§5.0.6 dual-mode middleware). The `/login` page accepts the `GHOST_TOKEN`
as a password, and the gateway returns a short-lived session cookie (not a
JWT). This provides a smooth upgrade path: start with `GHOST_TOKEN`, add
`GHOST_JWT_SECRET` when ready for multi-user.

**JWT implementation details**:

- **Signing key management**: Use `GHOST_JWT_SECRET` env var for HMAC-SHA256
  signing (development/single-node). For production/enterprise, support
  RS256 with key file path via `GHOST_JWT_KEY_FILE` env var. Rotation
  strategy: accept tokens signed by both current and previous key for a
  configurable grace period (`GHOST_JWT_KEY_ROTATION_GRACE_SECS`, default
  3600). Document HSM/Vault integration path for enterprise deployments
  but defer implementation to post-Phase 4.
- **Rust crate**: Use `jsonwebtoken` crate (already widely used in the
  Rust ecosystem) for JWT encode/decode. For future consideration,
  `biscuit-auth` provides capability-based tokens with attenuation —
  a natural fit for GHOST's safety model but adds complexity.
- **Token refresh flow**: Add `POST /api/auth/refresh` endpoint. Access
  tokens expire in 15 minutes (`GHOST_JWT_ACCESS_TTL_SECS`, default 900).
  Refresh tokens expire in 7 days (`GHOST_JWT_REFRESH_TTL_DAYS`, default 7),
  stored as httpOnly secure cookies via SvelteKit server route
  (`/api/auth/refresh` proxied through SvelteKit to set cookie).
- **Dashboard auto-refresh**: `auth.svelte.ts` module schedules token
  refresh 60 seconds before expiry. On 401 response, attempt silent
  refresh; if refresh fails, redirect to `/login`. Use `BroadcastChannel`
  to sync auth state across tabs — when one tab refreshes, all tabs
  receive the new token. When one tab logs out, all tabs redirect to login.
- **Token revocation**: Maintain an in-memory revocation set
  (`HashSet<jti>`) in `AppState` with TTL matching token expiry. Logout
  adds the token's `jti` to the revocation set. For multi-instance
  deployment, revocation set syncs via the same broadcast channel used
  for WS events. Explicit `POST /api/auth/logout` endpoint that revokes
  the current token and clears the refresh cookie.
- **Secure storage in PWA**: Access tokens held in memory only (Svelte
  store). Refresh tokens stored as httpOnly, Secure, SameSite=Strict
  cookies set by the SvelteKit server route. Never use localStorage for
  tokens — XSS vulnerability. The SvelteKit server route acts as a BFF
  (Backend for Frontend) that proxies auth requests and manages cookies.
- **Audit trail for ADE actions**: Every state-changing API call (safety
  actions, proposal decisions, agent CRUD, profile edits, kill switch)
  logs the JWT `sub` claim (user ID) to the `audit_log` table. New
  column: `actor_id TEXT` in audit_log. The audit query endpoint and
  dashboard audit view surface "who did what" — e.g., "user:alice
  approved proposal #52", "user:bob activated KILL_ALL with reason:
  credential leak detected". This is critical for compliance and
  multi-operator forensics.

### 17.2 SQLite Production Scalability

Single `Mutex<Connection>` with no WAL mode on the gateway side.

**Remediation (Phase 1 + Phase 3)**:
- Phase 1: Set `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` on
  gateway DB connection during bootstrap. This alone resolves most
  contention with the convergence-monitor sidecar.
- Phase 1: Replace `Mutex<Connection>` with `r2d2::Pool<SqliteConnectionManager>`
  (3-5 read connections, 1 write connection)
- Phase 3: Add optional PostgreSQL adapter behind a `StorageBackend` trait.
  SQLite remains the default for single-node deployment.

#### 17.2.1 Database Schema Migration Strategy

New tables are introduced across all phases (`otel_spans` in Phase 3,
`workflows` in Phase 2, `backup_manifest` in Phase 3) and existing tables
gain columns (`actor_id` in `audit_log` for Phase 1). Without a migration
strategy, production upgrades will break.

**Approach**: Versioned embedded SQL migrations run at gateway bootstrap,
using the existing `cortex-storage` migration pattern (the codebase already
has `crates/cortex/cortex-storage/src/migrations/` with versioned files
like `v019_intervention_state.rs`).

- Each migration is a numbered `.rs` file in `cortex-storage/src/migrations/`:
  `v020_actor_id.rs`, `v021_otel_spans.rs`, `v022_workflows.rs`, etc.
- The gateway runs all pending migrations at startup before accepting
  connections (same as current behavior for v001–v019).
- Migrations are idempotent: `CREATE TABLE IF NOT EXISTS`, `ALTER TABLE
  ... ADD COLUMN IF NOT EXISTS` (SQLite 3.35+).
- Each migration records its version in a `schema_version` table.
- Rollback: migrations are forward-only (append-only philosophy). If a
  migration fails, the gateway refuses to start and logs the error.
  Manual rollback requires restoring from backup (§17.10).
- Docker entrypoint: migrations run automatically — no separate migration
  command needed. The Dockerfile `CMD` starts the gateway, which runs
  migrations before binding the HTTP port.

**New tables by phase**:
| Table | Phase | Migration | Purpose |
|---|---|---|---|
| `audit_log` ALTER (add `actor_id`) | 1 | v020 | JWT user tracking |
| `workflows` | 2 | v021 | Visual composer workflow storage |
| `session_event_index` | 2 | v022 | Replay performance snapshots |
| `otel_spans` | 3 | v023 | Embedded trace storage |
| `backup_manifest` | 3 | v024 | Backup metadata + checksums |

### 17.3 OpenAPI Schema-Driven Contracts

33 existing + 22 new endpoints with manually maintained TS interfaces.

**Remediation (Phase 1)**:
- Add `utoipa` to `ghost-gateway` for auto-generated OpenAPI specs from
  Rust handler types
- Serve spec at `GET /api/openapi.json`
- Generate TypeScript types from the OpenAPI spec using `openapi-typescript`
  as a build step in the dashboard: `npm run generate:api` script that
  runs `npx openapi-typescript http://localhost:18789/api/openapi.json -o src/lib/api/types.generated.ts`
- **CI enforcement**: Add a CI step that:
  1. Starts the gateway in test mode
  2. Runs `npm run generate:api` to regenerate TS types
  3. Fails if generated types differ from committed types (`git diff --exit-code`)
  4. This catches endpoint drift automatically — any Rust handler change
     that alters the API shape will fail CI until the TS types are regenerated
- **Vitest contract tests**: Add to `dashboard/src/lib/api/__tests__/contract.test.ts`:
  - For each endpoint, verify the actual response shape matches the
    generated TypeScript type using runtime validation (e.g., `zod` schemas
    generated from OpenAPI, or manual shape assertions)
  - Run as part of `npm test` and CI pipeline
- Add to §5.0.11 deliverables and §14.1 testing strategy

### 17.4 Agent Authoring & Composition Tools

The ADE is observation-heavy but has no creation tools.

**Remediation (Phase 2 sub-phase — "Agent Studio")**:
- Prompt playground: test prompts against configured LLM providers with
  real-time token/cost feedback
- Agent template selector: pre-built agent configurations (research,
  code review, data analysis) with customizable parameters
- Simulation sandbox: run an agent in a sandboxed environment with
  `simulation-boundary` emulation detection active, no real tool execution
- Defer visual workflow composer and A/B testing to Phase 4+

### 17.5 Session Replay Performance

"Fetch all events" + "compute gate state at any timestamp" won't scale.

**Remediation (Phase 2)**:
- Add `session_event_index` table: pre-computed cumulative gate state
  snapshots every 50 events
- Paginate event fetches: `GET /api/sessions/{id}/events?offset=0&limit=100`
- Lazy-load event payloads: initial fetch returns metadata only, detail
  panel fetches full payload on click
- Virtual scrolling for the timeline (render only visible events)

### 17.6 WebSocket Topic-Based Subscriptions

All events broadcast to all clients with no filtering.

**Remediation (Phase 2)**:
- Add subscription message: `{"type": "Subscribe", "topics": ["agent:uuid", "session:uuid"]}`
- Gateway maintains per-client topic sets
- Events are filtered server-side before sending
- Default (no subscription): receive all events (backward compatible)
- Reduces bandwidth for clients that only care about specific agents/sessions

### 17.7 Browser Extension Integration

Scaffolded but orphaned from all phases.

**Remediation (Phase 4 mini-phase — 2 weeks)**:
- Week 1: Auth sync (share JWT with dashboard), popup shows agent status
  summary (mini-dashboard), content script sends observed AI interactions
  to gateway via REST
- Week 2: IndexedDB ↔ gateway sync, push notification handling in
  extension, extension settings page
- **Defer**: WhatsApp/Baileys integration pending ToS legal review

### 17.8 Config Hot-Reload

Profile/policy/skill changes in UI have no mechanism to reach running
sidecar/agent-loop without restart.

**Remediation (Phase 3)**:
- Convergence profiles: monitor watches profile config file, reloads on
  change (atomic write + `notify` crate file watcher)
- Agent capabilities: gateway sends `AgentConfigChange` WS event to
  agent-loop, which reloads config
- Policy changes: write to config file, agent-loop polls on next iteration

### 17.9 Unified Cross-Entity Search

No way to search across agents + sessions + memories + proposals + audit.

**Remediation (Phase 3)**:
- New endpoint: `GET /api/search?q=...&types=agents,sessions,memories`
- Backend: parallel queries across entity tables with LIKE/FTS matching
- Response: unified result list with `{type, id, title, snippet, score}`
- Dashboard: global search bar in top nav, results grouped by entity type

### 17.10 Backup / Restore & Data Durability

The append-only design with hash chains means data loss is catastrophic —
you can't reconstruct a partial chain. §17.2 mentions SQLite → Postgres
migration but there is no backup/restore workflow, no UI, and no scheduled
backup mechanism.

**Remediation (Phase 3 — early, before advanced features)**:

- **Backup endpoint**: `POST /api/admin/backup` (admin role only). Creates
  a point-in-time SQLite backup using `sqlite3_backup_init()` (via
  rusqlite's `backup::Backup`). Includes all tables: agents, itp_events,
  convergence_scores, goal_proposals, memory_snapshots, audit_log, otel_spans.
  Output: compressed `.tar.gz` with the SQLite file + a manifest JSON
  containing backup timestamp, hash chain head hashes per agent, row counts,
  and a blake3 checksum of the archive.
- **Restore endpoint**: `POST /api/admin/restore` (admin role only, requires
  gateway restart after restore). Accepts a backup archive, verifies the
  blake3 checksum, verifies hash chain integrity for each agent's event
  chain, then replaces the active database. Returns a verification report
  before committing.
- **Scheduled backups**: Configurable via `GHOST_BACKUP_SCHEDULE` env var
  (cron syntax, default: `0 3 * * *` — daily at 3 AM). Implemented as a
  tokio background task in the gateway (not a separate sidecar). Backups
  written to `GHOST_BACKUP_DIR` (default: `./backups/`). Retention:
  `GHOST_BACKUP_RETENTION_DAYS` (default: 30). Old backups pruned
  automatically.
- **Dashboard UI**: `/settings/backups` route:
  - List existing backups with timestamp, size, verification status
  - "Backup Now" button (calls `POST /api/admin/backup`)
  - "Restore" button with file upload + verification preview
  - Backup schedule configuration
  - Last backup status indicator in the top bar (alongside connection
    status) — shows ⚠️ if last backup is >24h old
- **Docker integration**: Mount `GHOST_BACKUP_DIR` as a Docker volume.
  Document in the Dockerfile and docker-compose example.
- **Export/import for migration**: Separate from backup, add
  `GET /api/admin/export?format=jsonl` that exports all entities as
  newline-delimited JSON (portable, not SQLite-specific). This supports
  the SQLite → Postgres migration path in §17.2.

### 17.11 Visual Multi-Agent Workflow Composer

The exec summary promises "compose multi-agent workflows" and §17.4 adds
Agent Studio with prompt playground + templates + sandbox. But the visual
DAG builder for orchestration (proposals → gates → sub-agents) is deferred
to Phase 4+. Without it, humans can't build complex workflows visually —
they're limited to code-only composition.

**Remediation (Phase 2 — core deliverable, not Phase 4)**:

Elevate a basic visual composer to Phase 2 as the Agent Studio centerpiece.
This is not a full no-code platform — it's a structured visual editor for
multi-agent workflows that generates configuration, not code.

- **Extend `CausalGraph.svelte`**: The existing force-directed graph
  component becomes the canvas. Add:
  - Node palette (sidebar): drag agent nodes, gate nodes, tool nodes,
    decision nodes onto the canvas
  - Edge drawing: click-drag between node ports to create connections
  - Node configuration panel: click a node to configure its properties
    (agent template, gate thresholds, tool allowlist)
  - Workflow validation: real-time validation that the DAG is acyclic,
    all required connections exist, and gate thresholds are valid
- **Workflow model**: A workflow is a JSON document describing:
  ```json
  {
    "name": "research-pipeline",
    "nodes": [
      {"id": "n1", "type": "agent", "template": "researcher", "config": {...}},
      {"id": "n2", "type": "gate", "gate_type": "human_review", "threshold": 0.7},
      {"id": "n3", "type": "agent", "template": "summarizer", "config": {...}}
    ],
    "edges": [
      {"from": "n1", "to": "n2"},
      {"from": "n2", "to": "n3", "condition": "approved"}
    ]
  }
  ```
- **New endpoints**:
  - `GET /api/workflows` — list saved workflows
  - `POST /api/workflows` — save workflow
  - `PUT /api/workflows/{id}` — update workflow
  - `POST /api/workflows/{id}/execute` — execute a workflow (creates
    agents, wires gates, starts execution)
- **Phase 2 scope** (minimal viable composer):
  - Canvas with drag-drop nodes and edge drawing
  - 3 node types: agent, gate, tool
  - Workflow save/load
  - Workflow execution (sequential pipeline only — no parallel branches)
- **Phase 4 scope** (full composer):
  - Parallel branches, conditional routing, loops with convergence checks
  - Sub-workflow nesting
  - Live execution overlay (nodes light up as they execute)
  - A/B testing branches

### 17.12 Self-Observability of the ADE Itself

§10 and §14 provide excellent OTel instrumentation for agents, but the ADE
infrastructure itself (gateway, convergence-monitor, WS connections,
dashboard performance) has no observability. There's no "ADE health"
dashboard panel.

**Remediation (Phase 3)**:

- **Gateway spans**: Add `tracing::instrument` spans to:
  - `bootstrap.rs::build_router()` — startup timing
  - `websocket.rs::ws_handler()` — per-connection lifecycle
  - `api/*.rs` handler functions — per-request timing
  - Auth middleware — auth check timing + failure reasons
  - Rate limiter — rate limit hits per IP/token
- **Convergence-monitor spans**: Add spans to:
  - Pipeline execution (7-signal computation timing)
  - State file writes (ITP protocol timing)
  - DB writes (SQLite contention visibility)
- **Metrics to surface**:
  - Active WS connections (gauge)
  - WS messages sent/sec (counter)
  - REST requests/sec by endpoint (counter)
  - Auth failures/sec (counter)
  - Rate limit rejections/sec (counter)
  - SQLite lock wait time (histogram)
  - Gateway uptime (gauge)
  - Convergence-monitor last heartbeat (gauge)
- **Dashboard sub-view**: `/observability/ade` (Phase 3):
  - Gateway health: uptime, request rate, error rate, active connections
  - Monitor health: last computation time, pipeline latency, state file age
  - WS health: connected clients, messages/sec, lagged clients
  - SQLite health: lock contention, WAL size, DB file size
  - System resources: memory usage, CPU (if available via `/proc` or sysinfo crate)

---

## Appendix A: New Gateway Endpoints (Complete List)

> 🔍 Audit status column added from `docs/ADE_INTEGRATION_AUDIT.md` findings.

| # | Endpoint | Method | Phase | Purpose | Audit Status |
|---|---|---|---|---|---|
| 1 | `/api/sessions/{id}/events` | GET | 2 | Session event timeline | ❌ Missing |
| 2 | `/api/memory/search` | GET | 2 | Semantic memory search | ❌ Missing (needs `cortex-retrieval` dep) |
| 3 | `/api/state/crdt/{agent_id}` | GET | 2 | CRDT state viewer | ❌ Missing (needs `cortex-crdt` dep) |
| 4 | `/api/integrity/chain/{agent_id}` | GET | 2 | Hash chain inspector | ❌ Missing (needs `cortex-temporal` dep) |
| 5 | `/api/goals` | GET | 2 | Proposal list with filters | ⚠️ Exists — needs filter support (`?status=&agent_id=`) |
| 6 | `/api/goals/{id}` | GET | 2 | Proposal detail | ❌ Missing |
| 7 | `/api/goals/{id}/approve` | POST | 2 | Approve proposal | ✅ Exists |
| 8 | `/api/goals/{id}/reject` | POST | 2 | Reject proposal | ✅ Exists |
| 9 | `/api/mesh/trust-graph` | GET | 3 | EigenTrust trust graph | ❌ Missing (dep `ghost-mesh` available) |
| 10 | `/api/mesh/consensus` | GET | 3 | Consensus operations | ❌ Missing (needs `cortex-multiagent` dep) |
| 11 | `/api/mesh/delegations` | GET | 3 | Delegation chains | ❌ Missing (dep `cortex-storage` available, query module exists) |
| 12 | `/api/traces/{session_id}` | GET | 3 | OTel-formatted traces | ❌ Missing (needs `cortex-observability` dep) |
| 13 | `/api/profiles` | GET | 3 | List convergence profiles | ❌ Missing (needs `cortex-convergence` dep) |
| 14 | `/api/profiles/{name}` | PUT | 3 | Update profile | ❌ Missing |
| 15 | `/api/profiles` | POST | 3 | Create profile | ❌ Missing |
| 16 | `/api/agents/{id}/profile` | POST | 3 | Assign profile to agent | ❌ Missing |
| 17 | `/.well-known/agent.json` | GET | 4 | A2A Agent Card | ✅ Exists (via mesh_routes) |
| 18 | `/api/a2a/tasks` | POST | 4 | A2A task requests | ⚠️ `/a2a` exists (JSON-RPC dispatch) — path differs |
| 19 | `/api/skills` | GET | 4 | List available skills | ❌ Missing (needs `ghost-skills` dep) |
| 20 | `/api/skills/{id}/install` | POST | 4 | Install skill | ❌ Missing |
| 21 | `/api/skills/{id}/uninstall` | POST | 4 | Uninstall skill | ❌ Missing |
| 22 | `/api/webhooks` | GET/POST | 4 | Webhook configuration | ❌ Missing |
| 23 | `/api/auth/login` | POST | 1 | JWT token issuance | ❌ Missing (§17.1) |
| 24 | `/api/auth/refresh` | POST | 1 | JWT token refresh | ❌ Missing (§17.1) |
| 25 | `/api/auth/logout` | POST | 1 | Token revocation + cookie clear | ❌ Missing (§17.1) |
| 26 | `/api/openapi.json` | GET | 1 | Auto-generated OpenAPI spec | ❌ Missing (§17.3) |
| 27 | `/api/admin/backup` | POST | 3 | Create point-in-time backup | ❌ Missing (§17.10) |
| 28 | `/api/admin/restore` | POST | 3 | Restore from backup archive | ❌ Missing (§17.10) |
| 29 | `/api/admin/export` | GET | 3 | Export all entities as JSONL | ❌ Missing (§17.10) |
| 30 | `/api/workflows` | GET/POST | 2 | Workflow CRUD | ❌ Missing (§17.11) |
| 31 | `/api/workflows/{id}` | PUT | 2 | Update workflow | ❌ Missing (§17.11) |
| 32 | `/api/workflows/{id}/execute` | POST | 2 | Execute workflow | ❌ Missing (§17.11) |
| 33 | `/api/search` | GET | 3 | Unified cross-entity search | ❌ Missing (§17.9) |

**Summary**: 3 fully exist, 2 partially exist (need filter support / detail view), 28 completely missing.
5 crate dependencies must be added to the gateway's `Cargo.toml`; 4 more
are already listed but need `use` imports (see §4.8). Additionally, `jsonwebtoken`
and `governor` crates are needed for auth and rate limiting (§17.1, §5.0.13).

---

## Appendix B: Dashboard Route Map (Complete)

> 🔍 Audit status column added from `docs/ADE_INTEGRATION_AUDIT.md` findings.

| Route | Phase | Description | Audit Status |
|---|---|---|---|
| `/` | 1 | Overview dashboard with real-time cards | 🔴 Broken — convergence data shape mismatch (§5.0.1) |
| `/login` | ✅ | OAuth login | ✅ Works (but token validation is fake — §4.5) |
| `/agents` | 1 | Agent list with lifecycle controls | 🟡 Partial — extra fields in store show 0 |
| `/agents/[id]` | 1 | Agent detail (convergence, costs, sessions) | ❌ Missing |
| `/convergence` | 1 | Per-agent convergence breakdown | 🔴 Broken — response wrapper + field names (§5.0.1, §5.0.2) |
| `/costs` | 1 | Cost tracking dashboard | ❌ Missing — endpoint exists, no route or sidebar link |
| `/security` | 1 | Kill switch + audit with filtering | 🔴 Broken — safety status path + audit wrapper (§5.0.2) |
| `/memory` | 1 | Memory browser | 🔴 Broken — response wrapper + field names (§5.0.1, §5.0.2) |
| `/sessions` | 1 | Session list | 🔴 Broken — response wrapper + all field names wrong (§5.0.1, §5.0.2) |
| `/sessions/[id]` | 2 | Session detail with event timeline | ❌ Missing |
| `/sessions/[id]/replay` | 2 | Session replay with scrubber | ❌ Missing |
| `/orchestration` | 3 | Multi-agent trust graph + consensus | ❌ Missing |
| `/observability` | 3 | OTel trace waterfall | ❌ Missing |
| `/goals` | 2 | Proposal review queue (replaces `/proposals`) | 🔴 Broken — response wrapper + `content` field missing. Fix in §5.0.1-§5.0.2, then enhance with filter/detail in Phase 2 (§6.3). |
| `/goals/[id]` | 2 | Proposal detail with 7-dim validation | ❌ Missing |
| `/reflections` | 2 | Agent reflection timeline | 🔴 Broken — endpoint does not exist |
| `/settings` | 1 | General settings | ✅ Works |
| `/settings/profiles` | 3 | Convergence profile editor | ❌ Missing |
| `/settings/policies` | 3 | Policy viewer/editor | ❌ Missing |
| `/settings/channels` | 3 | Channel management | ❌ Missing |
| `/settings/oauth` | ✅ | OAuth provider config | ✅ Works |
| `/skills` | 4 | Skill marketplace | ❌ Missing |
| `/settings/backups` | 3 | Backup/restore management | ❌ Missing (§17.10) |
| `/observability/ade` | 3 | ADE self-observability health | ❌ Missing (§17.12) |
| `/search` | 3 | Unified cross-entity search results | ❌ Missing (§17.9) |

**Summary**: 11 of 27 routes exist. Of those 11, only 3 work correctly.
8 are broken due to data shape mismatches or missing endpoints. 16 are not yet built.

---

## Appendix C: WebSocket Event Types (Current + Planned)

> 🔍 Audit finding: All 6 existing event types are defined and actively sent
> by backend code, but **no dashboard store consumes any of them**. The WS
> `onmessage` handler is not set. Building the WebSocket store (§5.1) is
> required before any event can be consumed.

| Event Type | Status | Payload | Consumed By | Audit Status |
|---|---|---|---|---|
| `ScoreUpdate` | ✅ Exists | `{type, agent_id, score, level, signals}` (snake_case) | Convergence, Agents, Overview | ❌ Not consumed — no WS store |
| `InterventionChange` | ✅ Exists | `{type, agent_id, old_level, new_level}` | Security, Agents, Overview | ❌ Not consumed — no WS store |
| `KillSwitchActivation` | ✅ Exists | `{type, level, agent_id, reason}` | Security, modal alert | ❌ Not consumed — no WS store |
| `ProposalDecision` | ✅ Exists | `{type, proposal_id, decision, agent_id}` | Proposals, Agents | ❌ Not consumed — no WS store |
| `AgentStateChange` | ✅ Exists | `{type, agent_id, new_state}` | Agents, Overview | ❌ Not consumed — no WS store |
| `Ping` | ✅ Exists | `{type: "Ping"}` | Connection keepalive | ❌ Not consumed — no WS store |
| `SessionEvent` | 🔲 Planned | session_id, event_type, payload | Session replay (live) | Not in WsEvent enum |
| `CostUpdate` | 🔲 Planned | agent_id, daily_total, cap_pct | Costs, Agents | Not in WsEvent enum |
| `ConsensusUpdate` | 🔲 Planned | operation_id, signed_count, required | Orchestration | Not in WsEvent enum |
| `TrustScoreChange` | 🔲 Planned | agent_id, old_trust, new_trust | Orchestration | Not in WsEvent enum |
| `SkillInstalled` | 🔲 Planned | skill_id, name, capabilities | Skills | Not in WsEvent enum |

**Serialization note**: Rust uses `#[serde(tag = "type")]` producing JSON like
`{"type": "ScoreUpdate", "agent_id": "...", "score": 0.75}`. Field names are
snake_case. Dashboard stores use camelCase internally — a mapping layer is
needed in the WebSocket message dispatcher.

---

## Appendix D: References

0. **GHOST ↔ ADE Integration Verification Audit** — `docs/ADE_INTEGRATION_AUDIT.md` (2026-02-28). Verified all backend↔frontend contracts. Findings incorporated into §1, §4, §5.0, §6.5, §12.2, §16, Appendices A–C.
1. OpenTelemetry GenAI Agent Semantic Conventions — https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/
2. OpenTelemetry AI Agent Observability Blog — https://opentelemetry.io/blog/2025/ai-agent-observability/
3. Google Agent Development Kit — https://developers.googleblog.com/en/agent-development-kit-easy-to-build-multi-agent-applications/
4. Google A2A Protocol — https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/
5. Langfuse (open-source LLM observability) — https://github.com/langfuse/langfuse
6. EigenTrust Algorithm — https://docs.openrank.com/reputation-algorithms/eigentrust
7. Svelte 5 Runes Migration Guide — https://svelte.dev/docs/svelte/v5-migration-guide/llms.txt
8. LayerCake (Svelte charting) — https://layercake.graphics/
9. Hierarchical Multi-Agent Orchestration for Human Oversight — https://arxiv.org/html/2510.24937v1
10. FlowZap Agentic UX Visualization — https://flowzap.xyz/blog/visualizing-agentic-ux
11. Swarm DAG Agent Workflow Visualization — https://kinglyagency.com/labs/swarm-dag
12. AI Agent Observability Best Practices — https://www.getmaxim.ai/articles/ai-observability-in-2025-how-to-monitor-evaluate-and-improve-ai-agents-in-production/
13. PWA Service Worker Caching Strategies — https://www.magicbell.com/blog/offline-first-pwas-service-worker-caching-strategies
14. Google Production-Ready AI Agents Guide — https://cloud.google.com/blog/products/ai-machine-learning/a-devs-guide-to-production-ready-ai-agents
15. Redis AI Agent Architecture — https://redis.io/blog/ai-agent-architecture/
16. Sentry Session Replay — https://blog.sentry.io/2023/02/16/introducing-session-replay-from-sentry-bridge-the-gap-between-code-and-ux/
17. HMI Design for Operator Interaction — https://www.controldesign.com/displays/hmi/article/55358049/human-machine-interfaces-and-the-growing-importance-of-operator-interaction
18. Azure AI Foundry Multi-Agent Observability — https://techcommunity.microsoft.com/t5/azure-ai-foundry-blog/azure-ai-foundry-advancing-opentelemetry-and-delivering-unified/ba-p/4456039

---

*Content was rephrased for compliance with licensing restrictions. All sources are cited inline and in the references appendix.*
