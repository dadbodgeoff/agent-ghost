# GHOST ADE — Implementation Tasks

> Derived from `docs/ADE_DESIGN_PLAN.md` and audit findings.
> Each task is atomic, ordered by dependency, and tagged with its phase/week.
>
> **Legend**: ⬜ Not started · 🟡 In progress · ✅ Done · 🚫 Blocked
>
> **Cross-references**: `§5.0.1` = ADE_DESIGN_PLAN.md section 5.0.1

---

## Phase 1: Real-Time Foundation (Weeks 1–3)

### Week 1 — Fix Broken Contracts (Prerequisites)

> Nothing new gets built until the existing integration is solid.
> 8 of 11 dashboard routes are broken. REST has zero auth. CostTracker is dead.

#### 1.1 Backend: Auth & Security

- [ ] **T-1.1.1** Add dual-mode REST auth middleware (tower layer) `§5.0.6`
  - Implement `auth_middleware` in `ghost-gateway/src/api/`
  - JWT mode: decode `Authorization: Bearer <jwt>` via `jsonwebtoken` crate, extract `sub`/`role`/`exp`
  - Legacy token mode: match `Bearer <token>` against `GHOST_TOKEN` env var, assign implicit `admin` role
  - No-auth mode: if neither `GHOST_JWT_SECRET` nor `GHOST_TOKEN` set, skip auth (local dev)
  - Apply as tower layer to all `/api/*` routes in `build_router()`
  - Files: `ghost-gateway/src/api/auth.rs` (new), `ghost-gateway/src/bootstrap.rs`

- [ ] **T-1.1.2** Fix WebSocket auth bypass `§5.0.6`
  - Change `if let Some(token)` to require token when `GHOST_TOKEN` or `GHOST_JWT_SECRET` is set
  - Apply same dual-mode logic as REST middleware
  - File: `ghost-gateway/src/api/websocket.rs`

- [ ] **T-1.1.3** Add JWT auth endpoints `§17.1`
  - `POST /api/auth/login` — validate credentials, issue JWT access token (15min) + refresh token (7d httpOnly cookie)
  - `POST /api/auth/refresh` — validate refresh cookie, rotate tokens
  - `POST /api/auth/logout` — add `jti` to in-memory revocation set, clear refresh cookie
  - Add `jsonwebtoken` crate to `ghost-gateway/Cargo.toml`
  - Files: `ghost-gateway/src/api/auth.rs`, `ghost-gateway/Cargo.toml`

- [ ] **T-1.1.4** Restrict CORS `§5.0.13`
  - Replace `CorsLayer::permissive()` with origin-restricted CORS
  - Read allowed origins from `GHOST_CORS_ORIGINS` env var (comma-separated)
  - Default: `http://localhost:5173,http://localhost:18789`
  - File: `ghost-gateway/src/bootstrap.rs`

- [ ] **T-1.1.5** Add rate limiting `§5.0.13`
  - Add `governor` crate to `ghost-gateway/Cargo.toml`
  - Unauthenticated: 20 req/min per IP
  - Authenticated: 200 req/min per token
  - Safety-critical (`/api/safety/*`): 10 req/min per token
  - WebSocket connections: 5 per IP
  - Return `429 Too Many Requests` with `Retry-After` header
  - Files: `ghost-gateway/src/api/rate_limit.rs` (new), `ghost-gateway/src/bootstrap.rs`

- [ ] **T-1.1.6** Add request ID tracing `§5.0.13`
  - Add `tower-http` `SetRequestIdLayer` to inject `X-Request-ID` on every request
  - Propagate into `tracing` spans
  - Return `X-Request-ID` in responses
  - File: `ghost-gateway/src/bootstrap.rs`

- [ ] **T-1.1.7** Add `actor_id` column to `audit_log` table `§17.1, §17.2.1`
  - New migration `v020_actor_id.rs`: `ALTER TABLE audit_log ADD COLUMN actor_id TEXT`
  - Log JWT `sub` claim on every state-changing API call
  - Files: `cortex-storage/src/migrations/v020_actor_id.rs` (new), handler files

#### 1.2 Backend: Dead Write Paths & Dependencies

- [ ] **T-1.2.1** Wire `CostTracker.record()` into LLM call path `§5.0.7`
  - Each LLM API call in `ghost-agent-loop` records token cost to CostTracker
  - `/api/costs` returns real data; `SpendingCapEnforcer` enforces limits
  - Files: `ghost-agent-loop/src/runner.rs`, relevant LLM call sites

- [ ] **T-1.2.2** Wire agent capabilities through bootstrap `§5.0.12`
  - Fix `RegisteredAgent` creation: use `capabilities: agent.capabilities.clone()` instead of `Vec::new()`
  - File: `ghost-gateway/src/bootstrap.rs`

- [ ] **T-1.2.3** Set SQLite WAL mode + busy timeout `§17.2`
  - Add `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` at gateway DB init
  - Replace `Mutex<Connection>` with connection pool (r2d2, 3-5 read + 1 write)
  - File: `ghost-gateway/src/bootstrap.rs`

- [ ] **T-1.2.4** Add backend crate dependencies for Phase 1 `§13.4`
  - Add to `ghost-gateway/Cargo.toml`: `jsonwebtoken`, `governor`, `tower-http`
  - Add `utoipa` for OpenAPI generation
  - File: `ghost-gateway/Cargo.toml`

#### 1.3 Backend: OpenAPI & Contracts

- [ ] **T-1.3.1** Install `utoipa` and serve OpenAPI spec `§17.3`
  - Annotate existing handler types with `utoipa` macros
  - Serve `GET /api/openapi.json`
  - File: `ghost-gateway/src/api/openapi.rs` (new), handler files

- [ ] **T-1.3.2** Define standard error response contract `§5.0.9`
  - Define `ErrorResponse { code, message, details }` envelope
  - Update all handlers to use consistent error shape
  - Files: `ghost-gateway/src/api/error.rs` (new), all handler files

- [ ] **T-1.3.3** Define API backward-compatibility contract `§5.0.10`
  - Create `docs/API_CONTRACT.md`
  - Rules: new fields may be added, existing fields never removed/renamed, new endpoints may be added, deprecated endpoints return 301 for 6 months

#### 1.4 Dashboard: Fix Response Wrappers & Field Names

- [ ] **T-1.4.1** Fix response wrapper unwrapping on all routes `§5.0.1`
  - `/convergence`: `data.scores[0].score` not `data.composite_score`
  - `/sessions`: `data.sessions` not `data`
  - `/goals`: `data.proposals` not `data`
  - `/security`: `data.entries` not `data` (audit), `data.platform.level` not `data.level` (safety)
  - `/memory`: `data.memories` not `data`
  - `/` (Overview): `data.scores[0].score` not `data.composite_score`
  - Files: all route `+page.svelte` files in `dashboard/src/routes/`

- [ ] **T-1.4.2** Fix field name mappings in routes and stores `§5.0.2`
  - Convergence: `score`, `level`, `signal_scores` (+ object→array transform)
  - Sessions: `session_id`, `agents`, `started_at`, `event_count`
  - Security: `killState.platform.level`
  - Goals: `goal.operation` + `goal.target_type` (compose description)
  - Memory: `mem.memory_id`, `mem.snapshot` (parse snapshot JSON)
  - Files: route pages + store files in `dashboard/src/`

- [ ] **T-1.4.3** Update TypeScript interfaces to match API shapes `§5.0.3`
  - `Agent`: remove `convergenceScore`/`interventionLevel`, add `spending_cap`
  - `ConvergenceState`: replace `compositeScore`/`interventionLevel`/`signals` with `score`/`level`/`signal_scores`
  - `Session`: replace `id`/`agentId`/`channel`/`startedAt`/`messageCount`/`status` with `session_id`/`agents`/`started_at`/`event_count`/`last_event_at`
  - Files: store `.ts` files in `dashboard/src/lib/stores/`

- [ ] **T-1.4.4** Fix component data shapes `§5.0.4`
  - `MemoryCard`: update props from `{memory_type, importance, content}` to `{memory_id, snapshot, created_at}`
  - `GoalCard`: update `description` prop to accept `operation` + `target_type`
  - `SignalChart`: add transform from `signal_scores` JSON object to `number[]`
  - Files: component `.svelte` files in `dashboard/src/lib/components/`

- [ ] **T-1.4.5** Fix or remove `/reflections` route `§5.0.5`
  - Option A: create `GET /api/reflections` backed by `itp_events` filtered to reflection-type
  - Option B: remove route, re-add in Phase 2
  - Files: `dashboard/src/routes/reflections/`, optionally `ghost-gateway/src/api/`

#### 1.5 Dashboard: Fix Login Flow

- [ ] **T-1.5.1** Replace fake login validation with real auth `§5.0.6`
  - Replace `GET /api/health` "validation" with `POST /api/auth/login`
  - Handle 401 on bad credentials
  - Store access token in memory (`$state` in `auth.svelte.ts`)
  - Set up auto-refresh timer (60s before expiry)
  - Files: `dashboard/src/routes/login/+page.svelte`, `dashboard/src/lib/stores/auth.svelte.ts` (new)

#### 1.6 Dashboard: Test Infrastructure

- [ ] **T-1.6.1** Install dashboard test dependencies `§5.0.11`
  - `vitest` + `@testing-library/svelte` for unit/component tests
  - `playwright` for E2E tests
  - `eslint` + `eslint-plugin-svelte` for linting
  - Add `test`, `test:e2e`, `lint` scripts to `package.json`
  - File: `dashboard/package.json`

- [ ] **T-1.6.2** Generate TS types from OpenAPI spec `§17.3`
  - Add `npm run generate:api` script using `openapi-typescript`
  - Output: `src/lib/api/types.generated.ts`
  - Add CI step: regenerate + `git diff --exit-code` to catch drift
  - File: `dashboard/package.json`, `dashboard/src/lib/api/types.generated.ts`


### Weeks 2–3 — Real-Time Stores, Component Wiring, New Routes

#### 1.7 Dashboard: WebSocket Store (Foundation)

- [ ] **T-1.7.1** Build WebSocket singleton store `§5.1`
  - Create `dashboard/src/lib/stores/websocket.svelte.ts`
  - Singleton connection to `ws://127.0.0.1:18789/api/ws`
  - Exponential backoff with jitter (initial 1s, max 30s, ×2 + random 0-1s)
  - Parse incoming messages, route by `type` field to domain stores
  - snake_case → camelCase mapping layer
  - Expose connection state as `$state` for UI indicators
  - Handle `Ping` events for keepalive
  - Multi-tab leader election via `BroadcastChannel` API `§5.1`

- [ ] **T-1.7.2** Build offline event queue `§5.1, §8.4`
  - Queue non-safety write actions in IndexedDB while disconnected
  - Replay on reconnection
  - Safety actions (kill, pause, quarantine, resume) must NOT be queued — show error
  - Dependency: `idb-keyval` package

#### 1.8 Dashboard: Migrate Stores to Svelte 5 Runes

> Migration order: websocket → agents → convergence → safety → sessions → audit → costs → memory

- [ ] **T-1.8.1** Migrate `agents` store to Svelte 5 runes `§5.1`
  - Create `dashboard/src/lib/stores/agents.svelte.ts` (class-based, `$state`/`$derived`)
  - REST init from `GET /api/agents`
  - Subscribe to `AgentStateChange` WS events
  - Delete old `agents.ts`

- [ ] **T-1.8.2** Migrate `convergence` store to Svelte 5 runes `§5.1`
  - Create `dashboard/src/lib/stores/convergence.svelte.ts`
  - REST init from `GET /api/convergence/scores`
  - Subscribe to `ScoreUpdate` + `InterventionChange` WS events
  - Expose `monitorOnline` derived state (poll `GET /api/health` for monitor connectivity)
  - Delete old `convergence.ts`

- [ ] **T-1.8.3** Create `safety` store (new) `§5.1`
  - Create `dashboard/src/lib/stores/safety.svelte.ts`
  - REST init from `GET /api/safety/status`
  - Subscribe to `KillSwitchActivation` + `InterventionChange` WS events

- [ ] **T-1.8.4** Migrate `sessions` store to Svelte 5 runes `§5.1`
  - Create `dashboard/src/lib/stores/sessions.svelte.ts`
  - REST init from `GET /api/sessions`
  - Delete old `sessions.ts`

- [ ] **T-1.8.5** Create `audit` store (new) `§5.1`
  - Create `dashboard/src/lib/stores/audit.svelte.ts`
  - REST query from `GET /api/audit` with filter params

- [ ] **T-1.8.6** Create `costs` store (new) `§5.1`
  - Create `dashboard/src/lib/stores/costs.svelte.ts`
  - REST query from `GET /api/costs`

- [ ] **T-1.8.7** Migrate `memory` store to Svelte 5 runes `§5.1`
  - Create `dashboard/src/lib/stores/memory.svelte.ts`
  - REST query from `GET /api/memory`

#### 1.9 Dashboard: Wire Existing Components to Routes

- [ ] **T-1.9.1** Wire `ScoreGauge` → `/convergence` route `§5.2.1`
- [ ] **T-1.9.2** Wire `SignalChart` → `/convergence` route `§5.2.2`
  - Add signal_scores object→array transformation
- [ ] **T-1.9.3** Wire `AuditTimeline` → `/security` route `§5.2.3`
- [ ] **T-1.9.4** Wire `GoalCard` → `/goals` route `§5.2.4`
- [ ] **T-1.9.5** Wire `MemoryCard` → `/memory` route `§5.2.5`
- [ ] **T-1.9.6** Wire `CausalGraph` → `/agents/[id]` detail view `§5.2.6`

#### 1.10 Dashboard: Enrich Agent Management

- [ ] **T-1.10.1** Build agent cards with status, convergence gauge, spending bar `§5.3`
  - Use `ScoreGauge` for convergence, `CostBar` (new) for spending
- [ ] **T-1.10.2** Build create agent form `§5.3`
  - Fields: name, spending cap, capabilities checklist, generate keypair toggle
  - Calls `POST /api/agents`
- [ ] **T-1.10.3** Build agent detail view `/agents/[id]` `§5.3`
  - Convergence history, cost breakdown, session list, audit entries filtered to agent
  - Safety controls: pause/resume/quarantine buttons with confirmation dialogs
  - Wire to `POST /api/safety/{action}/{agent_id}`
- [ ] **T-1.10.4** Implement soft-delete for agents `§5.3`
  - Mark as `deleted` in registry, retain all historical data
  - "Show deleted" toggle with strikethrough style

#### 1.11 Dashboard: Enrich Security Dashboard

- [ ] **T-1.11.1** Wire `AuditTimeline` + add filter controls `§5.4`
  - Time range picker, agent selector, event type dropdown, severity checkboxes, free-text search
  - Map to existing `/api/audit` query parameters
- [ ] **T-1.11.2** Add aggregation charts `§5.4`
  - Violations per day (line), by severity (donut), by tool (bar)
  - Data from `GET /api/audit/aggregation`
- [ ] **T-1.11.3** Add export buttons (JSON, CSV, JSONL) `§5.4`
  - Trigger `GET /api/audit/export`
- [ ] **T-1.11.4** Kill switch status panel with per-agent breakdown `§5.4`
  - Data from `GET /api/safety/status`

#### 1.12 Dashboard: Enrich Convergence View

- [ ] **T-1.12.1** Per-agent score cards with signal breakdown `§5.5`
- [ ] **T-1.12.2** Intervention level indicator (L0 green → L4 red) `§5.5`
- [ ] **T-1.12.3** Degraded-state UI when monitor is offline `§5.5`
  - "Monitor offline — data may be stale" banner
  - Gray out real-time indicators, show last-updated timestamps

#### 1.13 Dashboard: New Costs Route

- [ ] **T-1.13.1** Create `/costs` route `§5.0.8`
  - Per-agent cost cards: daily total, compaction cost, cap, remaining, utilization %
  - Utilization bar chart (horizontal bars, color-coded)
  - Data from `GET /api/costs`
  - Add sidebar navigation link

#### 1.14 Dashboard: Layout & UX

- [ ] **T-1.14.1** Add `ConnectionIndicator` component to layout `§9.3`
  - Green dot = connected, red = disconnected
  - Show in top bar alongside intervention level, agent count, daily spend
- [ ] **T-1.14.2** Implement empty-state vs error distinction `§5.0.9`
  - Loading: skeleton/spinner
  - Empty: "No [agents/sessions/etc.] yet" with call-to-action
  - Error: error message + retry button + `X-Request-ID`
- [ ] **T-1.14.3** Add CSS custom properties for dark/light theme `§9.1`
  - `:root` (dark default) and `:root.light` (light theme)
  - Toggle in `/settings`, persist in `localStorage`
  - Respect `prefers-color-scheme` as default

### Phase 1 Exit Criteria

| Metric | Target |
|---|---|
| Broken routes fixed | 8/8 routes return correct data |
| REST auth coverage | 100% endpoints behind auth middleware |
| OpenAPI served | `GET /api/openapi.json` returns valid spec |
| WS store connected | All 6 event types consumed by stores |
| Components wired | All 6 orphaned components rendered in routes |
| Store migration | All stores use Svelte 5 runes |

---

## Phase 2: Core ADE Features (Weeks 4–9)

> The features that differentiate an ADE from a dashboard — workflow visualization,
> state inspection, proposal review, session replay, and agent authoring.

### 2.1 Backend: New Endpoints for Phase 2

- [ ] **T-2.1.1** Implement `GET /api/sessions/{id}/events` `§6.1, §6.4`
  - Return all `itp_events` for a session, ordered by timestamp
  - Include hash chain verification status
  - Support pagination: `?offset=0&limit=100`
  - Include computed fields: cumulative cost, gate state at each point
  - Apply PII redaction server-side via `cortex-privacy` `§6.4`
  - File: `ghost-gateway/src/api/sessions.rs`

- [ ] **T-2.1.2** Implement `GET /api/memory/search` `§6.2.1`
  - Add `cortex-retrieval` to `ghost-gateway/Cargo.toml`
  - Query params: `q`, `agent_id`, `memory_type`, `importance`, `confidence_min`, `confidence_max`, `limit`
  - Backend: call `cortex_retrieval::hybrid_search()` (BM25 + vector, 11-factor ranking)
  - File: `ghost-gateway/src/api/memory.rs`

- [ ] **T-2.1.3** Implement `GET /api/state/crdt/{agent_id}` `§6.2.2`
  - Add `cortex-crdt` to `ghost-gateway/Cargo.toml`
  - Return current CRDT document state, pending operations, merge history
  - File: `ghost-gateway/src/api/state.rs` (new)

- [ ] **T-2.1.4** Implement `GET /api/integrity/chain/{agent_id}` `§6.2.3`
  - Add `cortex-temporal` `use` import (already in Cargo.toml)
  - Return chain summary: length, last anchor, verification status, detected breaks
  - File: `ghost-gateway/src/api/integrity.rs` (new)

- [ ] **T-2.1.5** Add filter support to `GET /api/goals` `§6.3`
  - Add query params: `?status=pending&agent_id=...`
  - File: `ghost-gateway/src/api/goals.rs`

- [ ] **T-2.1.6** Implement `GET /api/goals/{id}` (detail) `§6.3`
  - Return full proposal with 7-dimension validation breakdown
  - File: `ghost-gateway/src/api/goals.rs`

- [ ] **T-2.1.7** Add `session_event_index` table `§17.5, §17.2.1`
  - Migration `v022_session_event_index.rs`: pre-computed cumulative gate state snapshots every 50 events
  - File: `cortex-storage/src/migrations/v022_session_event_index.rs` (new)

- [ ] **T-2.1.8** Implement WebSocket topic-based subscriptions `§17.6`
  - Accept `{"type": "Subscribe", "topics": ["agent:uuid", "session:uuid"]}`
  - Maintain per-client topic sets, filter events server-side
  - Default (no subscription): receive all events (backward compatible)
  - File: `ghost-gateway/src/api/websocket.rs`

- [ ] **T-2.1.9** Implement workflow CRUD endpoints `§17.11`
  - `GET /api/workflows` — list saved workflows
  - `POST /api/workflows` — save workflow
  - `PUT /api/workflows/{id}` — update workflow
  - `POST /api/workflows/{id}/execute` — execute workflow (sequential pipeline)
  - Migration `v021_workflows.rs`: `workflows` table
  - Files: `ghost-gateway/src/api/workflows.rs` (new), `cortex-storage/src/migrations/v021_workflows.rs` (new)

### 2.2 Dashboard: Agent Workflow Visualizer

- [ ] **T-2.2.1** Build DAG visualizer component `§6.1`
  - Real-time DAG of agent actions within a session
  - Nodes: LLM call, tool execution, proposal extraction, gate check, intervention
  - Edges: causal relationships from `previous_hash` field
  - Use D3-force for layout (upgrade existing `CausalGraph.svelte`)
  - Add `d3-force`, `d3-scale` to `dashboard/package.json`

- [ ] **T-2.2.2** Build node detail panel `§6.1`
  - Click any node → full details: LLM prompt/response (PII-redacted), tool I/O, validation results, timing
  - Gate state bar at bottom: CB, Depth, Damage, Cap, Convergence, Hash chain

- [ ] **T-2.2.3** Wire live updates for active sessions `§6.1`
  - Subscribe to session-specific WS events
  - New nodes appear as agent works

### 2.3 Dashboard: Database / State Inspector

- [ ] **T-2.3.1** Build Memory Browser view `§6.2.1`
  - Route: enhance `/memory` with search
  - Free-text search via `GET /api/memory/search`
  - Filters: memory type (31 types), importance (5 levels), confidence range, agent, time range
  - Detail view: full content, metadata, decay factors, hash chain position
  - Memory type distribution donut chart

- [ ] **T-2.3.2** Build CRDT State Viewer `§6.2.2`
  - New route or sub-view under `/agents/[id]`
  - Show current CRDT state, merge operations, conflict resolution
  - Display signed operation log (Ed25519 signatures)

- [ ] **T-2.3.3** Build Hash Chain Inspector `§6.2.3`
  - New route or sub-view under `/agents/[id]`
  - Visualize blake3 hash chain
  - Show merkle tree anchoring status
  - Verify chain integrity on demand
  - New component: `HashChainStrip`

### 2.4 Dashboard: Proposal Lifecycle UI

- [ ] **T-2.4.1** Build proposal review queue `§6.3`
  - Route: `/goals`
  - Pending proposals with 7-dimension validation display (`ValidationMatrix` component)
  - Recent proposals list (auto-approved, rejected)
  - Filter by status, agent

- [ ] **T-2.4.2** Build approve/reject workflow `§6.3`
  - Approve, Reject, Request Changes buttons
  - Handle concurrent resolution: if `ProposalDecision` WS event arrives, disable buttons + show "Resolved by another user"
  - Handle 409 Conflict from API gracefully

- [ ] **T-2.4.3** Build proposal detail view `/goals/[id]` `§6.3`
  - Full 7-dimension validation breakdown
  - Diff view for proposed changes

### 2.5 Dashboard: Session Replay

- [ ] **T-2.5.1** Build session detail view `/sessions/[id]` `§6.4`
  - Event timeline from `GET /api/sessions/{id}/events`
  - Event list with metadata

- [ ] **T-2.5.2** Build session replay `/sessions/[id]/replay` `§6.4`
  - Timeline scrubber (`TimelineSlider` component) with `role="slider"` ARIA
  - Event detail panel: full data for selected event
  - Conversation panel: reconstruct chat from events
  - Gate state panel: computed from cumulative events up to selected timestamp

- [ ] **T-2.5.3** Implement replay performance optimizations `§17.5`
  - Use `session_event_index` snapshots (every 50 events)
  - Paginate event fetches
  - Lazy-load event payloads (metadata first, full payload on click)
  - Virtual scrolling for timeline

### 2.6 Dashboard: Visual Workflow Composer (Basic) `§17.11`

- [ ] **T-2.6.1** Build workflow canvas
  - Extend `CausalGraph.svelte` as the canvas base
  - Node palette sidebar: drag agent, gate, tool nodes onto canvas
  - Edge drawing: click-drag between node ports

- [ ] **T-2.6.2** Build node configuration panel
  - Click node → configure properties (agent template, gate thresholds, tool allowlist)
  - Workflow validation: DAG is acyclic, required connections exist, thresholds valid

- [ ] **T-2.6.3** Implement workflow save/load/execute
  - Save/load via `GET/POST /api/workflows`
  - Execute sequential pipeline via `POST /api/workflows/{id}/execute`

### 2.7 Dashboard: Agent Studio `§17.4`

- [ ] **T-2.7.1** Build prompt playground
  - Test prompts against configured LLM providers
  - Real-time token count + cost feedback

- [ ] **T-2.7.2** Build agent template selector
  - Pre-built configs: research, code review, data analysis
  - Customizable parameters

- [ ] **T-2.7.3** Build simulation sandbox
  - Run agent in sandboxed environment with `simulation-boundary` emulation detection
  - No real tool execution

### Phase 2 Exit Criteria

| Metric | Target |
|---|---|
| DAG visualizer | Renders 100-node session in < 2s |
| Session replay | Scrubber navigates 500-event session smoothly |
| Proposal queue | Full approve/reject/concurrent flow works |
| Visual composer | Save/load/execute sequential 3-node workflow |
| Search | Global search returns results across 3+ entity types |

---

## Phase 3: Advanced Capabilities (Weeks 10–16)

> Multi-agent orchestration, trust visualization, OTel observability,
> convergence profiles, backup/restore, self-observability, unified search.

### 3.1 Backend: OTel Instrumentation

- [ ] **T-3.1.1** Add OTel crate dependencies `§13.4`
  - Add `opentelemetry`, `opentelemetry-otlp`, `tracing-opentelemetry` to relevant `Cargo.toml` files

- [ ] **T-3.1.2** Instrument agent loop with OTel spans `§7.2`
  - `runner.rs::run_loop()` — root agent execution span
  - `runner.rs::gate_check()` — 6-gate safety check
  - `runner.rs::call_llm()` — LLM call (tokens, cost, model)
  - `runner.rs::execute_tool()` — tool execution (name, duration)
  - `output_inspector.rs::inspect()` — output safety inspection
  - `proposal/extract.rs::extract()` — proposal extraction
  - `proposal/validate.rs::validate()` — 7-dimension validation
  - Use OTel GenAI semantic conventions: `gen_ai.operation.name`, `gen_ai.agent.id`, etc.

- [ ] **T-3.1.3** Add `otel_spans` table `§7.2, §17.2.1`
  - Migration `v023_otel_spans.rs`: `trace_id`, `span_id`, `parent_span_id`, `operation_name`, `start_time`, `end_time`, `attributes` (JSON), `status`, `session_id`
  - Retention: 7 days default, configurable via `GHOST_TRACE_RETENTION_DAYS`

- [ ] **T-3.1.4** Implement `GET /api/traces/{session_id}` `§7.2`
  - Add `cortex-observability` to `ghost-gateway/Cargo.toml`
  - Return OTel-formatted trace data, compatible with Jaeger/Zipkin import

- [ ] **T-3.1.5** Add optional OTLP exporter `§7.2`
  - Send spans to configured `GHOST_OTLP_ENDPOINT`
  - Configurable sampling via `GHOST_TRACE_SAMPLE_RATE`
  - Batch export: 5s interval, 512 spans max

### 3.2 Backend: Multi-Agent & Trust Endpoints

- [ ] **T-3.2.1** Implement `GET /api/mesh/trust-graph` `§7.1`
  - Return EigenTrust trust scores between agents
  - Node: agent ID, name, activity level, convergence level
  - Edge: trust score (0.0–1.0)

- [ ] **T-3.2.2** Implement `GET /api/mesh/consensus` `§7.1`
  - Add `cortex-multiagent` to `ghost-gateway/Cargo.toml`
  - Return N-of-M consensus state, signed operations

- [ ] **T-3.2.3** Implement `GET /api/mesh/delegations` `§7.1`
  - Return delegation chains and sybil resistance metrics

### 3.3 Backend: Convergence Profiles

- [ ] **T-3.3.1** Implement profile CRUD endpoints `§7.3`
  - `GET /api/profiles` — list profiles
  - `POST /api/profiles` — create profile
  - `PUT /api/profiles/{name}` — update weights and thresholds
  - `POST /api/agents/{id}/profile` — assign profile to agent
  - Add `cortex-convergence` `use` import (already in Cargo.toml)

### 3.4 Backend: Backup / Restore `§17.10`

- [ ] **T-3.4.1** Implement `POST /api/admin/backup`
  - Point-in-time SQLite backup via `sqlite3_backup_init()`
  - Output: compressed `.tar.gz` + manifest JSON (timestamp, hash chain heads, row counts, blake3 checksum)
  - Admin role only

- [ ] **T-3.4.2** Implement `POST /api/admin/restore`
  - Accept backup archive, verify blake3 checksum + hash chain integrity
  - Return verification report before committing
  - Requires gateway restart after restore
  - Admin role only

- [ ] **T-3.4.3** Implement scheduled backups
  - `GHOST_BACKUP_SCHEDULE` env var (cron syntax, default: `0 3 * * *`)
  - Tokio background task in gateway
  - Write to `GHOST_BACKUP_DIR` (default: `./backups/`)
  - Retention: `GHOST_BACKUP_RETENTION_DAYS` (default: 30), auto-prune

- [ ] **T-3.4.4** Implement `GET /api/admin/export?format=jsonl` `§17.10`
  - Export all entities as newline-delimited JSON (portable, for SQLite→Postgres migration)

- [ ] **T-3.4.5** Add `backup_manifest` table `§17.2.1`
  - Migration `v024_backup_manifest.rs`: backup metadata + checksums

### 3.5 Backend: Unified Search `§17.9`

- [ ] **T-3.5.1** Implement `GET /api/search`
  - Query params: `q`, `types=agents,sessions,memories,proposals,audit`
  - Parallel queries across entity tables with LIKE/FTS matching
  - Response: `{type, id, title, snippet, score}` unified result list

### 3.6 Backend: Config Hot-Reload `§17.8`

- [ ] **T-3.6.1** Implement profile config file watching
  - Monitor watches profile config file, reloads on change
  - Atomic write + `notify` crate file watcher

- [ ] **T-3.6.2** Implement agent config change propagation
  - Gateway sends `AgentConfigChange` WS event to agent-loop on capability/config changes

### 3.7 Backend: Self-Observability Spans `§17.12`

- [ ] **T-3.7.1** Add gateway instrumentation spans
  - `bootstrap.rs::build_router()` — startup timing
  - `websocket.rs::ws_handler()` — per-connection lifecycle
  - `api/*.rs` handlers — per-request timing
  - Auth middleware — timing + failure reasons
  - Rate limiter — hits per IP/token

- [ ] **T-3.7.2** Add convergence-monitor instrumentation spans
  - Pipeline execution (7-signal computation timing)
  - State file writes (ITP protocol timing)
  - DB writes (SQLite contention visibility)

### 3.8 Dashboard: Trace Waterfall

- [ ] **T-3.8.1** Build `TraceWaterfall` component `§7.2`
  - Nested span visualization (waterfall/flame chart)
  - `role="tree"` with `aria-expanded` for nested spans
  - Show timing, tokens, cost per span

- [ ] **T-3.8.2** Build `/observability` route `§7.2`
  - Trace waterfall for selected session
  - Data from `GET /api/traces/{session_id}`

### 3.9 Dashboard: Multi-Agent Orchestration

- [ ] **T-3.9.1** Build trust graph visualization `§7.1`
  - Route: `/orchestration`
  - Force-directed layout (D3-force)
  - Edge thickness = trust score, node size = activity, node color = convergence level
  - Tap/click edge to show trust score detail

- [ ] **T-3.9.2** Build N-of-M consensus state display `§7.1`
  - Show signed operation count vs required threshold

- [ ] **T-3.9.3** Build sybil resistance metrics panel `§7.1`
  - Display delegation chains and sybil detection metrics

### 3.10 Dashboard: Convergence Profile Editor

- [ ] **T-3.10.1** Build `/settings/profiles` route `§7.3`
  - Weight sliders for 7 signals (must sum to 1.0) — `WeightSlider` component
  - Intervention threshold sliders (L1–L4)
  - Save, Reset to Default, Duplicate actions
  - Preset management: standard, research, companion, productivity

### 3.11 Dashboard: Policy & Channel Management

- [ ] **T-3.11.1** Build `/settings/policies` route `§7.4`
  - Read-only view of active policies
  - Editable fields for non-safety-critical settings (spending caps, recursion depth)
  - Safety-critical settings shown but require CLI/config changes

- [ ] **T-3.11.2** Build `/settings/channels` route `§7.5`
  - List configured channels with status
  - Add/remove channel configurations
  - Test connection button

### 3.12 Dashboard: Backup / Restore UI `§17.10`

- [ ] **T-3.12.1** Build `/settings/backups` route
  - List existing backups (timestamp, size, verification status)
  - "Backup Now" button → `POST /api/admin/backup`
  - "Restore" button with file upload + verification preview
  - Backup schedule configuration
  - Last backup status indicator in top bar (⚠️ if >24h old)

### 3.13 Dashboard: Unified Search UI `§17.9`

- [ ] **T-3.13.1** Build `/search` route
  - Results grouped by entity type
  - Data from `GET /api/search`

- [ ] **T-3.13.2** Add global search bar to top nav
  - Keyboard shortcut (Cmd+K / Ctrl+K)
  - Inline results dropdown, full results page on Enter

### 3.14 Dashboard: ADE Self-Observability `§17.12`

- [ ] **T-3.14.1** Build `/observability/ade` route
  - Gateway health: uptime, request rate, error rate, active connections
  - Monitor health: last computation time, pipeline latency, state file age
  - WS health: connected clients, messages/sec, lagged clients
  - SQLite health: lock contention, WAL size, DB file size

### Phase 3 Exit Criteria

| Metric | Target |
|---|---|
| OTel traces | Agent loop emits spans visible in trace waterfall |
| Trust graph | Renders 50-agent graph with EigenTrust scores |
| Backup/restore | Automated daily backup + verified restore |

---

## Phase 4: Ecosystem & Extensibility (Weeks 17–22)

> A2A protocol, skill marketplace, plugin architecture, PWA hardening,
> browser extension integration, advanced workflow composer.

### 4.1 Backend: A2A Protocol

- [ ] **T-4.1.1** Enhance A2A Agent Card at `/.well-known/agent.json` `§8.1`
  - Ensure full A2A compliance (capabilities, supported task types)
  - Already exists via `mesh_routes` — verify format

- [ ] **T-4.1.2** Implement A2A task request/response `§8.1`
  - `POST /api/a2a/tasks` — accept A2A task requests
  - Map A2A messages to ITP events internally
  - Return A2A-formatted artifacts from completed tasks
  - Support A2A streaming via SSE for long-running tasks

### 4.2 Backend: Skills

- [ ] **T-4.2.1** Implement skill endpoints `§8.2`
  - Add `ghost-skills` `use` import (already in Cargo.toml)
  - `GET /api/skills` — list available skills with capability scopes
  - `POST /api/skills/{id}/install` — install skill (requires capability approval)
  - `POST /api/skills/{id}/uninstall` — uninstall skill

### 4.3 Backend: Webhooks & Plugins

- [ ] **T-4.3.1** Implement webhook configuration endpoints `§8.3`
  - `GET /api/webhooks` — list configured webhooks
  - `POST /api/webhooks` — create webhook (event type, URL, secret)
  - Fire webhooks on intervention, kill switch, proposal decision events

- [ ] **T-4.3.2** Implement custom safety check registration API `§8.3`
  - Allow registering additional validation dimensions beyond built-in 7
  - Execute in proposal validation pipeline

### 4.4 Dashboard: A2A Discovery Panel

- [ ] **T-4.4.1** Build A2A discovery panel in `/orchestration` `§8.1`
  - Browse and connect to external A2A-compatible agents
  - Cross-platform task delegation: send tasks, track status
  - Trust integration: EigenTrust scores for external agents

### 4.5 Dashboard: Skill Marketplace

- [ ] **T-4.5.1** Build `/skills` route `§8.2`
  - Installed skills list with capability badges (`CapabilityBadge` component)
  - Available skills browser
  - Install/uninstall/disable workflow
  - Capability review before installation (explicit approval)

### 4.6 Dashboard: Webhook Configuration

- [ ] **T-4.6.1** Build webhook configuration UI `§8.3`
  - Add to `/settings` or new sub-route
  - List, create, edit, delete webhooks
  - Test webhook button

### 4.7 Dashboard: PWA Hardening `§8.4`

- [ ] **T-4.7.1** Implement tiered service worker caching
  - App shell: cache-first
  - API responses: stale-while-revalidate
  - Audit/session data: network-first
  - Static assets: cache-first, long TTL

- [ ] **T-4.7.2** Implement offline data display
  - Show cached agent list, convergence scores, recent audit entries
  - Safety actions blocked offline with error message
  - Clear offline indicator with last-sync timestamp

- [ ] **T-4.7.3** Implement background sync for queued actions
  - Non-safety write actions replayed on reconnection
  - Safety actions never queued

- [ ] **T-4.7.4** Implement push notification configuration UI `§8.4`
  - L2+ intervention alerts, kill switch activations
  - Proposal review requests, agent lifecycle changes

### 4.8 Dashboard: Advanced Workflow Composer `§17.11`

- [ ] **T-4.8.1** Add parallel branches and conditional routing
- [ ] **T-4.8.2** Add sub-workflow nesting
- [ ] **T-4.8.3** Add live execution overlay (nodes light up during execution)
- [ ] **T-4.8.4** Add A/B testing branches

### 4.9 Browser Extension Integration `§17.7`

- [ ] **T-4.9.1** Implement JWT auth sync with dashboard
  - Share JWT token between dashboard and extension
- [ ] **T-4.9.2** Build popup mini-dashboard
  - Agent status summary, connection indicator
- [ ] **T-4.9.3** Wire content script → gateway REST pipeline
  - Content script sends observed AI interactions to gateway
- [ ] **T-4.9.4** Implement IndexedDB ↔ gateway sync
- [ ] **T-4.9.5** WhatsApp/Baileys ToS legal review `§8.5`
  - Explicit task: do NOT ship WhatsApp features until legal review is complete and documented
  - Milestone gate: legal sign-off required before any WhatsApp code ships

### 4.10 Dashboard: Mobile / Touch UX `§9.1.2`

- [ ] **T-4.10.1** Implement responsive breakpoints
  - `sm` (<640px): single-column, sidebar → bottom nav
  - `md` (640–1024px): icon rail sidebar
  - `lg` (>1024px): full sidebar
- [ ] **T-4.10.2** Add touch-optimized controls
  - Session replay scrubber: touch-drag, tap-to-jump, pinch-to-zoom
  - DAG visualizer: pinch-to-zoom, two-finger pan, tap to select, long-press context menu
  - Trust graph: same pinch-zoom/pan, tap edge for detail
- [ ] **T-4.10.3** Add Playwright mobile tests
  - iPhone 14 + iPad Pro device presets
  - Touch interactions, responsive layout at all 3 breakpoints
  - PWA install flow on mobile Safari + Chrome Android

### Phase 4 Exit Criteria

| Metric | Target |
|---|---|
| A2A compliance | Agent Card served, task request accepted |
| PWA offline | Cached data displays, safety actions blocked |
| Extension | JWT sync + popup mini-dashboard showing agent status |
| Extension ToS | WhatsApp/Baileys legal review documented + gate decision made |

---

## Cross-Cutting Concerns (All Phases)

### Accessibility `§9.1.1`

- [ ] **T-X.1** Color is never the sole indicator — all severity-colored elements have text labels + icons
- [ ] **T-X.2** ARIA roles: DAG (`role="img"`), trust graph (`role="img"`), scrubber (`role="slider"`), waterfall (`role="tree"`)
- [ ] **T-X.3** Keyboard navigation: Tab for all interactive elements, arrow keys for DAG nodes, Left/Right for scrubber, Escape closes modals
- [ ] **T-X.4** Screen reader: `aria-live="assertive"` for critical state changes (intervention, kill switch), `aria-live="polite"` for non-critical
- [ ] **T-X.5** Motor impairment: checkbox alternative for kill switch confirmation, click alternatives for all drag interactions

### Component Library `§9.2`

> Build as needed per phase. New components listed here for tracking.

- [ ] **T-X.6** `StatusBadge` — agent/session status (Phase 1)
- [ ] **T-X.7** `GateCheckBar` — 6 gate states (Phase 2)
- [ ] **T-X.8** `CostBar` — utilization bar with cap (Phase 1)
- [ ] **T-X.9** `TimelineSlider` — session replay scrubber (Phase 2)
- [ ] **T-X.10** `TraceWaterfall` — nested span visualization (Phase 3)
- [ ] **T-X.11** `TrustEdge` — weighted edge for trust graph (Phase 3)
- [ ] **T-X.12** `ValidationMatrix` — 7-dimension validation grid (Phase 2)
- [ ] **T-X.13** `HashChainStrip` — visual hash chain (Phase 2)
- [ ] **T-X.14** `FilterBar` — composable filter controls (Phase 1)
- [ ] **T-X.15** `ConnectionIndicator` — WS connection state (Phase 1)
- [ ] **T-X.16** `ConfirmDialog` — destructive action confirmation (Phase 1)
- [ ] **T-X.17** `WeightSlider` — slider with numeric input (Phase 3)
- [ ] **T-X.18** `CapabilityBadge` — capability scope indicator (Phase 4)

### Charting `§9.4`

- [ ] **T-X.19** Install LayerCake + D3 scales for Svelte-native charting (Phase 1)
- [ ] **T-X.20** Install µPlot for high-frequency real-time time-series (Phase 2)
- [ ] **T-X.21** Install D3-force for trust graph + causal graph physics (Phase 2)

### Deployment `§15`

- [ ] **T-X.22** Docker multi-stage build (Rust builder + Node dashboard + slim runtime) `§15.4`
- [ ] **T-X.23** Docker Compose multi-node example `§15.5` (Phase 3)
- [ ] **T-X.24** Kill gate propagation: Option A (SQLite WAL + polling) `§15.5` (Phase 3)
- [ ] **T-X.25** Kill gate propagation: Option B (HTTP fanout) `§15.5` (Phase 4)
- [ ] **T-X.26** Helm chart for Kubernetes deployment `§15.5` (Phase 4)

### Database Migrations `§17.2.1`

> Tracked inline with their respective phase tasks. Summary for reference:

| Migration | Phase | Task |
|---|---|---|
| `v020_actor_id.rs` | 1 | T-1.1.7 |
| `v021_workflows.rs` | 2 | T-2.1.9 |
| `v022_session_event_index.rs` | 2 | T-2.1.7 |
| `v023_otel_spans.rs` | 3 | T-3.1.3 |
| `v024_backup_manifest.rs` | 3 | T-3.4.5 |

### Performance Budgets `§14.4`

| Metric | Target |
|---|---|
| Max WS messages/sec (dashboard) | 50 msg/s sustained |
| Max session events for DAG render | 500 nodes before pagination |
| Max agents for trust graph | 50 nodes before clustering |
| Dashboard initial load (cached) | < 2s |
| Dashboard initial load (uncached) | < 5s |
| Store update → UI render latency | < 16ms (60fps) |

- [ ] **T-X.27** Increase broadcast channel capacity from 256 to 1024 `§14.4`
- [ ] **T-X.28** Implement `Resync` event on `Lagged` — trigger full REST re-fetch `§14.4`

---

## Audit Notes (2026-03-01)

> Every task cross-referenced against actual source code.

### Stale Findings (Already Fixed)

| Finding | Evidence |
|---|---|
| RegisteredAgent uses Vec::new() for capabilities | FIXED: bootstrap.rs line 336 uses agent.capabilities.clone() |
| SQL column mismatches in convergence-monitor | FIXED per design plan 4.7 |
| Mesh router never merged | FIXED: bootstrap.rs line 419 |
| Push routes never mounted | FIXED: bootstrap.rs line 425 |
| WsEvent types never sent | FIXED: convergence_watcher.rs and safety.rs send events |
| Handlers return 200 on DB errors | FIXED: all return 500 |

Impact: T-1.2.2 (capabilities) removed as no-op.

### Verified Response Shape Mismatches

| Endpoint | Actual Shape | Dashboard Bug |
|---|---|---|
| GET /api/agents | Flat array [{id, name, status, spending_cap}] | Reads convergenceScore/interventionLevel (don't exist) |
| GET /api/convergence/scores | {scores: [{score, level, signal_scores}]} | Reads data.composite_score (broken) |
| GET /api/sessions | {sessions: [...]} | sessions.set(data) missing .sessions |
| GET /api/goals | {proposals: [{operation, target_type, ...}]} | goals = data missing .proposals, reads goal.content (doesn't exist) |
| GET /api/safety/status | {platform_level, per_agent, ...} | Reads killState?.level should be platform_level |
| GET /api/audit | {entries: [...], page, total} | auditEntries = data missing .entries |
| GET /api/memory | {memories: [...]} | memories = data missing .memories |
| GET /api/costs | Flat array [{agent_id, daily_total, ...}] | No route exists yet |

### Verified Dead Paths

- CostTracker: runner.rs cost_recorder defaults to None (line 125), never set by gateway
- WS Auth: websocket.rs line 65 if-let-Some bypasses auth when token param missing
- api.ts onmessage: WS connection created but onmessage never set (events lost)

### Dashboard File Structure (Verified)

- Components at dashboard/src/components/ (NOT dashboard/src/lib/components/)
- 6 components: ScoreGauge, SignalChart, CausalGraph, AuditTimeline, GoalCard, MemoryCard
- Stores at dashboard/src/lib/stores/ (3 files: agents.ts, convergence.ts, sessions.ts)
- Auth at dashboard/src/lib/auth.ts (sessionStorage, no JWT)
- Layout sidebar: 9 links, missing Costs, has broken Reflections link
- eslint in lint script but NOT in devDependencies

### Gaps Found (New Tasks Added)

| Gap | Severity | Task |
|---|---|---|
| api.ts reads token from sessionStorage | HIGH | T-1.5.2 |
| layout.svelte reads token from sessionStorage | HIGH | T-1.5.3 |
| api.ts connectWebSocket() conflicts with new WS store | MEDIUM | T-1.7.1 |
| Auth endpoints must be excluded from auth middleware | HIGH | T-1.1.1 |
| CORS must allow credentials for refresh cookies | HIGH | T-1.1.4 |
| AppState needs revocation set field | MEDIUM | T-1.1.1 |
| write_audit_entry() must accept actor_id | MEDIUM | T-1.1.7 |
| Connection pool refactor touches 10+ handlers | HIGH | T-1.2.3 |
| .svelte.ts cannot be imported from .ts files | HIGH | T-1.7.1 |

### Missing Cargo.toml Dependencies

- jsonwebtoken (JWT auth, Phase 1)
- governor (rate limiting, Phase 1)
- utoipa (OpenAPI, Phase 1)
- cortex-retrieval (memory search, Phase 2)
- cortex-crdt (CRDT viewer, Phase 2)
- cortex-multiagent (consensus, Phase 3)
- cortex-observability (traces, Phase 3)
- cortex-decay (profiles, Phase 3)
- tower-http already present (no action)

### Missing package.json Dependencies

- eslint (lint script exists but package missing)
- vitest, @testing-library/svelte, jsdom (testing)
- playwright (E2E)
- layercake (charting, Phase 1-2)
- d3-force, d3-scale (graphs, Phase 2)
- uplot (real-time charts, Phase 2)
- idb-keyval (offline queue, Phase 1)
- openapi-typescript (type generation, Phase 1)
