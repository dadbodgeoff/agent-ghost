# ADE Design Plan — Gap & Flaw Audit

**Date:** 2026-02-28
**Scope:** `docs/ADE_DESIGN_PLAN.md` (1899 lines, 16 sections, 4 phases)
**Method:** Cross-referenced every claim against actual source code, SQL schemas,
API handlers, dashboard routes, crate structures, and prior audit findings.
Web-verified technology claims (OTel GenAI conventions, A2A protocol, Svelte 5 runes).

---

## Severity Legend

- **BLOCKER** — Will prevent a phase from starting or completing. Must fix before task.md.
- **CRITICAL** — Significant gap that will cause rework if not addressed in planning.
- **HIGH** — Missing detail that will stall implementation without a design decision.
- **MEDIUM** — Oversight that should be addressed but won't block progress.
- **LOW** — Minor inconsistency or nice-to-have improvement.

---

## 1. BLOCKER: Stale Audit Findings Baked Into the Plan

The plan incorporates findings from `CONNECTIVITY_AUDIT.md` and
`docs/ADE_INTEGRATION_AUDIT.md`, but several of those findings are now
**stale** — the code has been fixed since the audit was written.

| Plan Section | Claim | Actual Code State |
|---|---|---|
| §4.7 "monitor.rs:224 SELECT ... score" | SQL column mismatch | **FIXED.** `persist_convergence_score()` now uses `composite_score` correctly (monitor.rs:822). |
| §4.7 "monitor.rs:188 SELECT agent_id FROM itp_events" | Column is `sender` | **FIXED.** `persist_itp_event()` now uses `sender` correctly (monitor.rs:800). |
| §5.0.7 "Fix Backend SQL Column Mismatches" | Two critical SQL bugs | **Already fixed.** This task is a no-op. |
| CONNECTIVITY_AUDIT Finding #21 "Mesh router never merged" | Unreachable endpoints | **FIXED.** `build_router()` merges mesh_router at line 419: `app = app.merge(mesh)`. |
| CONNECTIVITY_AUDIT Finding #22 "Push routes never mounted" | Unreachable endpoints | **FIXED.** `build_router()` merges push_router at line 425. |
| CONNECTIVITY_AUDIT Finding #13 "WsEvent::ScoreUpdate never sent" | Dead variant | **FIXED.** `convergence_watcher.rs:83` sends ScoreUpdate. |
| CONNECTIVITY_AUDIT Finding #14 "WsEvent::InterventionChange never sent" | Dead variant | **FIXED.** `convergence_watcher.rs:94` sends InterventionChange. |

**Impact:** Phase 1 Week 1 (§5.0) includes tasks that are already done. The
task.md will waste effort on completed work unless the plan is updated.
Additionally, the "33 endpoints" count in §4.1 is correct (mesh + push ARE
mounted), contradicting the CONNECTIVITY_AUDIT's claim that they're unreachable.

**Action needed:** Reconcile the plan with current code state. Remove completed
tasks from §5.0. Update §4.7 to reflect fixes. Re-verify all audit findings
before generating task.md.

---

## 2. BLOCKER: Single SQLite Connection Under Concurrent Load

The plan's architecture (§3.1) shows the gateway serving multiple WebSocket
clients and REST requests simultaneously, but `AppState.db` is a single
`Mutex<rusqlite::Connection>`. This is a serialization bottleneck.

**What the plan says:** §16 Risk Register mentions "SQLite lock contention
under load" with mitigation "Move to WAL mode, consider read replicas."

**What's actually happening:**
- The gateway uses a single `Mutex<Connection>` (state.rs:38)
- Every REST handler that touches the DB acquires this mutex
- The convergence-monitor has its OWN separate connection (monitor.rs:770)
- Both processes write to the same SQLite file
- WAL mode is set by the monitor (`PRAGMA journal_mode=WAL`) but NOT by the gateway

**Gaps not addressed:**
1. The gateway never sets `PRAGMA journal_mode=WAL` — it uses the default
   rollback journal, which means the monitor's WAL writes and the gateway's
   reads can conflict.
2. No `PRAGMA busy_timeout` is set on the gateway connection — any lock
   contention returns SQLITE_BUSY immediately instead of retrying.
3. The plan mentions "read replicas" as mitigation but provides no design
   for how to implement this with SQLite (it's not trivial — you'd need
   Litestream or a custom replication layer).
4. The plan doesn't address the fundamental issue: the gateway and
   convergence-monitor are separate processes sharing a SQLite file.
   Under load, this WILL cause `SQLITE_BUSY` errors that the gateway
   silently swallows as empty results (CONNECTIVITY_AUDIT Findings #36-39).

**Action needed:** Add a Phase 1 task to:
- Set `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` on the gateway's
  DB connection during bootstrap
- Consider using a connection pool (r2d2-sqlite or deadpool-sqlite) instead
  of a single Mutex<Connection>
- Design the error handling strategy for SQLITE_BUSY (retry vs 503)

---

## 3. BLOCKER: No WebSocket Authentication for Token-less Connections

The plan (§5.0.6) correctly identifies that REST endpoints lack auth. But
the WebSocket auth has its own critical gap:

```rust
// websocket.rs:68-73
if let Some(token) = &params.token {
    if !crate::auth::token_auth::validate_token(token) {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
}
```

If `token` is `None` (no `?token=` query param), the connection is accepted
WITHOUT any authentication. The `if let Some` means missing tokens are
silently allowed. The plan doesn't mention this — it says "WebSocket: ✅
Validated" in §4.5.

**Action needed:** Fix the WS handler to reject connections with no token
(unless GHOST_TOKEN is unset, matching the existing "no token configured =
no auth" behavior in `validate_token`).

---

## 4. CRITICAL: No API Versioning Strategy

The plan adds 22 new endpoints across 4 phases but never discusses API
versioning. All endpoints are under `/api/` with no version prefix.

**Why this matters:**
- The dashboard and browser extension are deployed independently from the
  gateway (§15.2 Option B)
- Phase 2-4 endpoints will change response shapes as features evolve
- The existing `/api/goals` vs planned `/api/proposals` path conflict
  (§6.3 audit note) is a symptom of this — there's no strategy for
  deprecating old paths

**Action needed:** Decide on a versioning strategy before Phase 2:
- Option A: URL prefix (`/api/v1/`, `/api/v2/`)
- Option B: Header-based (`Accept: application/vnd.ghost.v1+json`)
- Option C: No versioning, backward-compatible changes only (document the contract)

---

## 5. CRITICAL: No Error Response Contract

The plan specifies request/response shapes for new endpoints but never
defines a standard error response format. The existing handlers are
inconsistent:

| Handler | Error Format |
|---|---|
| `agents.rs` | `{"error": "message"}` with appropriate status codes |
| `safety.rs` | `{"error": "message"}` with status codes |
| `sessions.rs` | Returns `200 OK` with empty `[]` on DB errors |
| `convergence.rs` | Returns `200 OK` with default score on DB errors |
| `memory.rs` | Returns `200 OK` with empty results on DB errors |

**Why this matters:** The dashboard needs to distinguish "no data yet" from
"server error" to show appropriate UI states. The plan's UI designs (§6.1,
§6.4) show error states but don't specify how the frontend detects them.

**Action needed:** Define a standard error envelope:
```json
{"error": {"code": "DB_ERROR", "message": "...", "details": {...}}}
```
Add this as a Phase 1 prerequisite alongside §5.0.6 (auth middleware).

---

## 6. CRITICAL: Convergence-Monitor Write Path Still Partially Broken

While the SQL column names are fixed (see §1 above), the CONNECTIVITY_AUDIT
Finding #1 identified a deeper issue: the convergence_scores INSERT was
missing required NOT NULL columns. Let me verify the current state:

The current `persist_convergence_score()` (monitor.rs:822) inserts:
`(id, agent_id, session_id, composite_score, signal_scores, level, profile,
computed_at, event_hash, previous_hash)` — this matches the v017 schema.
**This is now correct.**

However, the CONNECTIVITY_AUDIT Finding #8 is still valid: `memory_snapshots`
has NO production write path. The plan's Memory Browser (§6.2.1) depends on
this table having data. The plan doesn't address how memory snapshots get
written — it only plans the read UI.

Similarly, Finding #9: `CostTracker.record()` is never called. The plan's
Costs route (§5.6) will show all zeros.

**Action needed:** Add Phase 1 tasks to:
- Wire `CostTracker.record()` into the LLM call path in ghost-agent-loop
- Wire memory snapshot persistence into the agent loop's memory pipeline
- Wire `insert_proposal()` into the proposal creation flow

---

## 7. CRITICAL: `/api/goals` vs `/api/proposals` Path Decision Not Made

The plan (§6.3) introduces `/api/proposals/*` endpoints but acknowledges
that `/api/goals/*` already exists with the same functionality. The audit
note says "Decision needed: either rename the existing endpoints to
`/api/proposals/*` or update the plan to use `/api/goals/*`."

**This decision is not made.** The plan proceeds to use `/api/proposals/*`
in all UI designs and route maps (Appendix B) while the existing code uses
`/api/goals/*`. This will cause confusion during implementation.

**Action needed:** Make the decision and update the plan. Recommendation:
keep `/api/goals/*` (it's already wired and tested) and add the missing
features (single-proposal detail, filter support) to the existing path.

---

## 8. CRITICAL: No Data Migration Strategy for Dashboard Store Rewrite

Phase 1 (§5.1) plans to rewrite all stores from Svelte 4 `writable()` to
Svelte 5 runes (`.svelte.ts` files). This is a breaking change to every
route that imports a store.

**What's missing:**
- No migration path — do you rewrite all stores at once or incrementally?
- Svelte 5 runes in `.svelte.ts` files can't be imported from regular `.ts`
  files. The plan's store file naming (`websocket.svelte.ts`) is correct,
  but the existing route pages that import stores will need to be updated.
- The plan doesn't mention that `$state` in module-level `.svelte.ts` files
  has specific rules — it works in classes and at module scope, but the
  pattern differs from `writable()`. ([Source](https://mainmatter.com/blog/2025/03/11/global-state-in-svelte-5/))

**Action needed:** Add implementation notes to §5.1:
- Stores should be migrated incrementally (one domain at a time)
- Each store migration includes updating all importing routes
- Use the class-based pattern for stores (Svelte 5 runes work in classes)
- Test each store migration independently before moving to the next

---

## 9. CRITICAL: Session Replay (§6.4) Has No PII Redaction Design

The Session Replay feature shows "LLM prompt/response" in the node detail
panel (§6.1) and "Conversation reconstruction from events" (§6.6). The plan
mentions "(with PII redaction)" parenthetically but provides no design for:

- How PII is detected in stored ITP events
- Whether redaction happens at write time (in the monitor) or read time
  (in the API endpoint)
- What the redaction UI looks like (masked text? placeholder tokens?)
- How `cortex-privacy` (which exists and does emotional content detection)
  integrates with the replay view

**Why this matters:** The plan positions safety as the core differentiator.
Showing raw LLM conversations in a browser without PII redaction contradicts
this positioning and creates compliance risk.

**Action needed:** Add a PII redaction design to §6.4:
- Define which fields are redacted (user messages, tool outputs, agent responses)
- Specify redaction strategy (regex patterns, NER, or cortex-privacy integration)
- Design the UI for redacted content (expandable with auth, or permanently masked)

---

## 10. CRITICAL: No Accessibility Plan

The plan's UI/UX Design System (§9) covers color, typography, layout, and
charting but never mentions accessibility. For a professional tool:

- Color-only severity encoding (§9.1 rule 4) fails WCAG — colorblind users
  can't distinguish green/red/orange. Need icons or text labels alongside color.
- The DAG visualizer (§6.1), trust graph (§7.1), and trace waterfall (§7.2)
  are purely visual — no screen reader alternative is designed.
- Keyboard navigation for the session replay scrubber (§6.4) is not specified.
- The kill switch confirmation dialog (§11.2) requires typing "KILL ALL" —
  no alternative for users with motor impairments.

**Action needed:** Add an accessibility section to §9 covering:
- ARIA roles for all interactive components
- Keyboard navigation patterns for complex widgets (DAG, timeline, graph)
- Text/icon alternatives for color-coded severity
- Screen reader announcements for real-time WebSocket updates

---

## 11. HIGH: Browser Extension Integration Not Planned

§4.9 lists the browser extension as "Scaffolded" with 6 components, but
the plan never mentions it again in any phase. There's no:

- Phase assignment for when the extension gets wired to the gateway
- Design for how the extension popup communicates with the dashboard
- Plan for how the content script observer feeds data to the ADE
- Integration between the extension's IndexedDB and the dashboard's stores

The extension is a significant piece of scaffolded code that's completely
orphaned from the 22-week plan.

**Action needed:** Either:
- Add extension integration tasks to Phase 3 or 4
- Explicitly defer it with a note explaining why

---

## 12. HIGH: No Concurrency Design for Proposal Review Queue

The Proposal Lifecycle UI (§6.3) shows approve/reject buttons but doesn't
address concurrent review:

- What happens if two users approve/reject the same proposal simultaneously?
- The existing `resolve_proposal` in cortex-storage uses UPDATE with a
  WHERE clause that checks `decision IS NULL` (AC10), but the plan doesn't
  mention this safeguard in the UI design.
- No optimistic locking or version field is shown in the proposal detail view.
- The `ProposalDecision` WebSocket event should update the UI to remove
  the proposal from the pending queue, but what if the user already clicked
  "Approve" and the WS event arrives showing someone else rejected it?

**Action needed:** Add concurrency handling to §6.3:
- Show "already resolved" state if WS event arrives before user action
- Disable approve/reject buttons when proposal is no longer pending
- Handle 409 Conflict response from the API gracefully

---

## 13. HIGH: OTel Integration (§7.2) Underestimates Complexity

The plan says "Add `tracing` spans with OTel-compatible attributes to the
agent loop, tool executor, and LLM provider." This is a multi-week effort
that's allocated as part of a 7-week phase alongside 4 other major features.

**What's actually needed:**
1. Add `opentelemetry`, `opentelemetry-otlp`, and `tracing-opentelemetry`
   to the agent-loop crate (not just the gateway)
2. Configure the OTel SDK with a trace exporter (OTLP, Jaeger, or in-process)
3. Add span instrumentation to 6+ code paths in the agent loop
4. Add custom attributes matching the GenAI semantic conventions
5. Build the `/api/traces/{session_id}` endpoint that reads OTel spans
6. Build the trace waterfall UI component
7. Handle trace storage (separate from SQLite? In-memory? OTLP collector?)

The plan doesn't address trace storage at all. OTel traces are typically
sent to an external collector (Jaeger, Tempo). If GHOST wants self-contained
deployment (§15.2 Option A), it needs an embedded trace store.

**Action needed:** Expand §7.2 with:
- Trace storage design (embedded vs external collector)
- Span instrumentation locations (list every function that gets a span)
- OTel SDK configuration (sampling rate, export interval, batch size)
- Estimated effort (this is 2-3 weeks alone, not a sub-task of Phase 3)

---

## 14. HIGH: Offline Queue (§8.4) Safety Implications Not Addressed

The plan says "Queue safety actions (pause, resume) for execution when
reconnected." This is dangerous:

- If a user clicks "KILL ALL" while offline, the action is queued. By the
  time connectivity returns, the situation may have changed — agents may
  have already been stopped by another operator, or the threat may have
  passed.
- Queuing a "resume" action while offline could resume an agent that was
  quarantined by another operator during the offline period.
- The plan's own safety philosophy (§11.1) says "monotonic escalation with
  deliberate de-escalation" — offline queuing of de-escalation actions
  violates this principle.

**Action needed:** Revise §8.4:
- Safety-critical actions (kill, pause, quarantine, resume) should NOT be
  queued offline. Show an error: "Safety actions require a live connection."
- Only non-destructive read operations should use cached data offline.
- The offline queue should be limited to non-safety actions (e.g., audit
  filter changes, profile edits).

---

## 15. HIGH: No Load Testing or Performance Budget

The plan specifies "dozens of values update per second" (§5.1) for the
real-time dashboard but provides no performance budget:

- How many WebSocket messages per second can the dashboard handle before
  the UI becomes unresponsive?
- What's the maximum number of events in a session before the DAG
  visualizer (§6.1) becomes unusable?
- What's the maximum number of agents before the trust graph (§7.1)
  becomes unreadable?
- The `broadcast::channel(256)` capacity — what happens when the channel
  is full? (Answer: `RecvError::Lagged` — the WS handler logs a warning
  but the client misses events silently.)

**Action needed:** Add performance budgets to §14 (Testing Strategy):
- Max WS messages/second the dashboard must handle: target N
- Max session events for DAG rendering: target N
- Max agents for trust graph: target N
- Broadcast channel overflow strategy (increase capacity? backpressure?)

---

## 16. HIGH: Config Fields Loaded But Never Consumed

The plan doesn't address 5 config fields that are loaded into AppState
but never read at runtime (CONSOLIDATED_AUDIT §Prompt 3):

| Field | AppState Location | Consumed? |
|---|---|---|
| `soul_drift_threshold` | `state.soul_drift_threshold` | ❌ Never read |
| `convergence_profile` | `state.convergence_profile` | ❌ Never read |
| `model_providers` | `state.model_providers` | ❌ Never read |
| `agent.capabilities` | Dropped during bootstrap (Vec::new()) | ❌ Never stored |
| `agent.template` | Loaded but never read | ❌ Never read |

The Convergence Profile Editor (§7.3) plans to use profiles, but the
existing `convergence_profile` config field isn't wired to anything.
The plan doesn't mention connecting these existing config values to
the new features.

**Action needed:** Add Phase 1 tasks to either:
- Wire these config fields to their intended consumers
- Remove them from AppState if they're not needed yet
- Document which phase will consume each field

---

## 17. MEDIUM: Dashboard Has No Testing Infrastructure

The plan's Testing Strategy (§14.1) lists Vitest, Svelte Testing Library,
Playwright, and axe-core. But `dashboard/package.json` has zero test
dependencies:

```json
{
  "devDependencies": {
    "@sveltejs/adapter-static": "^3.0.0",
    "@sveltejs/kit": "^2.0.0",
    "@sveltejs/vite-plugin-svelte": "^4.0.0",
    "svelte": "^5.0.0",
    "typescript": "^5.0.0",
    "vite": "^6.0.0"
  }
}
```

No Vitest, no testing library, no Playwright, no ESLint config (the `lint`
script exists but eslint isn't in devDependencies). The plan doesn't include
a task to set up the test infrastructure.

**Action needed:** Add a Phase 1 task to install and configure:
- `vitest` + `@testing-library/svelte` for unit/component tests
- `playwright` for E2E tests
- `eslint` + `eslint-plugin-svelte` for linting
- `axe-core` for accessibility testing

---

## 18. MEDIUM: No Graceful Degradation When Monitor Is Down

The architecture (§3.1) shows the convergence-monitor as an independent
sidecar. The gateway's health endpoint checks monitor connectivity. But
the plan doesn't design what the dashboard shows when the monitor is down:

- Convergence scores will be stale (last computed values in SQLite)
- Intervention levels won't update
- The convergence view (§5.5) will show frozen data with no indication

The gateway health endpoint returns `convergence_monitor: "unreachable"`
but the plan's dashboard designs don't show a degraded state indicator
for the convergence panel.

**Action needed:** Add degraded-state UI designs to §5.5 and §9.3:
- Show "Monitor offline — data may be stale" banner on convergence views
- Gray out real-time indicators when monitor is unreachable
- Show last-updated timestamp on all convergence data

---

## 19. MEDIUM: LayerCake Version Claim May Be Wrong

§13.1 specifies `LayerCake ^9.0.0`. The latest LayerCake version as of
early 2026 is likely in the 8.x range. The plan should verify the actual
available version before specifying it.

Also, the plan recommends µPlot for "high-frequency real-time data" but
doesn't address that µPlot has a very different API from LayerCake — using
both means two charting paradigms in the same dashboard. This increases
maintenance burden.

**Action needed:** Verify LayerCake version. Consider whether µPlot is
worth the complexity or if LayerCake + Canvas rendering is sufficient.

---

## 20. MEDIUM: No Design for Multi-Tab/Multi-Window Behavior

The plan's WebSocket store (§5.1) is a "singleton connection." But what
happens when a user opens the dashboard in multiple tabs?

- Each tab creates its own WebSocket connection
- The `broadcast::channel(256)` sends events to ALL connections
- If 10 tabs are open, the gateway maintains 10 WS connections
- The offline queue (§8.4) could replay actions from multiple tabs

**Action needed:** Add multi-tab handling to §5.1:
- Use `BroadcastChannel` API or `SharedWorker` for cross-tab coordination
- Elect a "leader" tab for the WS connection, share events via BroadcastChannel
- Prevent duplicate offline queue replays across tabs

---

## 21. MEDIUM: OAuth Routes Are Stubs (Not Mentioned in Plan)

The plan lists OAuth as "✅ Wired" in §4.1 and "✅ Works" for
`/settings/oauth` in §4.2. But CONNECTIVITY_AUDIT Findings #3-7 show
ALL 5 OAuth endpoints are stubs returning hardcoded data. The `ghost-oauth`
crate is never imported.

The plan doesn't include any task to wire OAuth routes to the actual
`ghost-oauth::OAuthBroker`. This means the "working" OAuth settings page
is actually showing fake data.

**Action needed:** Add a Phase 1 task to wire OAuth routes to the
`ghost-oauth` crate, or mark OAuth as "Scaffolded, not functional" in §4.1.

---

## 22. MEDIUM: No Design for Agent Deletion Cascade

§5.3 adds agent lifecycle controls including delete. But the plan doesn't
address what happens to an agent's data when deleted:

- Convergence scores in `convergence_scores` table
- ITP events in `itp_events` table
- Goal proposals in `goal_proposals` table
- Memory snapshots in `memory_snapshots` table
- Audit log entries in `audit_log` table

The append-only triggers prevent DELETE on these tables. So deleted agents
leave orphaned data. The plan should specify whether:
- Agent deletion is soft (mark as deleted, keep data)
- Agent deletion is hard (requires special migration)
- Data is retained for audit purposes (most likely, given safety philosophy)

**Action needed:** Add a note to §5.3 specifying soft-delete behavior
and how the UI handles deleted agents' historical data.

---

## 23. MEDIUM: Risk Register (§16) Missing Key Risks

The risk register has 10 entries but misses several risks discovered
during this audit:

| Missing Risk | Likelihood | Impact |
|---|---|---|
| Stale audit findings causing wasted work | High | Medium |
| SQLite BUSY errors from dual-process writes | High | High |
| PII exposure in session replay | Medium | Critical |
| Offline safety action queue causing stale commands | Low | Critical |
| OAuth stubs mistaken for working features | High | Medium |
| No API versioning causing breaking changes | Medium | High |
| Multi-tab WebSocket connection explosion | Medium | Medium |
| Empty tables causing "no data" confusion vs actual errors | High | Medium |

---

## 24. LOW: Inconsistent Crate Dependency Claims

§4.8 says "9 additional crate dependencies are needed" and lists them.
But the CONNECTIVITY_AUDIT shows that several of these crates are ALREADY
in `ghost-gateway/Cargo.toml` as unused dependencies:

- `cortex-temporal` — Finding #23: already a dep, never imported
- `cortex-convergence` — Finding #24: already a dep, never imported
- `ghost-skills` — Finding #29: already a dep, never imported
- `ghost-channels` — Finding #28: already a dep, never imported

So the plan says these need to be "added" but they're already there —
they just need to be imported and used. The actual missing deps are fewer
than claimed.

**Action needed:** Update §4.8 to distinguish "needs to be added to
Cargo.toml" from "already a dep, needs to be imported and used."

---

## 25. LOW: Timeline Estimates Are Aggressive

The plan allocates:
- 3 weeks for Phase 1 (fix 8 broken routes + build 8 stores + wire 6
  components + build agent CRUD UI + security dashboard + convergence
  view + costs route + offline support)
- 6 weeks for Phase 2 (DAG visualizer + state inspector + proposal UI +
  session replay + 8 new endpoints)

Phase 1 alone has ~30 distinct deliverables across frontend and backend.
At 1 developer, that's roughly 1 deliverable per half-day, with no buffer
for debugging, testing, or the inevitable "this doesn't work like the plan
said" discoveries.

**Action needed:** Either:
- Scope Phase 1 down to just the broken contract fixes + WebSocket store
  (2 weeks), then a separate phase for new UI features
- Add explicit developer count assumptions to the timeline
- Add buffer weeks between phases

---

## Summary

| Severity | Count | Key Themes |
|---|---|---|
| BLOCKER | 3 | Stale audit findings, SQLite concurrency, WS auth gap |
| CRITICAL | 7 | No API versioning, no error contract, empty write paths, path conflict, store migration, PII redaction, accessibility |
| HIGH | 6 | Browser extension orphaned, proposal concurrency, OTel complexity, offline safety, no perf budget, unused config |
| MEDIUM | 7 | No test infra, monitor degradation, charting versions, multi-tab, OAuth stubs, delete cascade, risk register gaps |
| LOW | 2 | Dep claims, timeline estimates |

**Total: 25 findings.**

The plan is thorough in its vision and architecture but has significant
gaps in operational details — error handling, concurrency, security edge
cases, data lifecycle, and realistic scoping. The biggest risk is that
Phase 1 tasks reference already-fixed bugs, which will cause confusion
when generating task.md.
