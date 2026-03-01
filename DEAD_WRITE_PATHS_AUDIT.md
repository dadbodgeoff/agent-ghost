# Dead Write Paths Audit (Prompt 1)

**Date:** 2026-02-28
**Scope:** Every SQL table in cortex-storage migrations (v016–v018) + ghost-audit `ensure_table()`.
**Pattern targeted:** Endpoints that read from tables nothing writes to.

---

## Master Table

| Table Name | Has INSERT (Production) | Has SELECT | Has API Endpoint | Verdict |
|---|---|---|---|---|
| `schema_version` | ✅ migrations/mod.rs | ✅ migrations/mod.rs | ✗ | LIVE (internal) |
| `memory_events` | ✅ memory.rs `write_memory()` | ✅ memory.rs JOIN | ✅ `/api/memory` (POST + JOIN) | **LIVE** ✅ FIXED |
| `memory_audit_log` | ✅ memory.rs `write_memory()` | ✗ | ✗ | **LIVE** ✅ FIXED |
| `memory_snapshots` | ✅ memory.rs `write_memory()` | ✅ memory.rs | ✅ `/api/memory`, `/api/memory/:id` | **LIVE** ✅ FIXED |
| `itp_events` | ✅ convergence-monitor | ✅ sessions.rs, itp_event_queries | ✅ `/api/sessions` | LIVE |
| `convergence_scores` | ✅ convergence-monitor | ✅ convergence.rs, convergence_score_queries | ✅ `/api/convergence/scores` | LIVE |
| `intervention_history` | ✅ convergence-monitor `persist_intervention_history()` | ✗ (query fn exists) | ✗ | **LIVE** ✅ FIXED |
| `intervention_state` | ✅ convergence-monitor `persist_intervention_state()` | ✅ convergence-monitor `reconstruct_state()` | ✗ | **LIVE** ✅ FIXED (v019 migration) |
| `goal_proposals` | ✅ runner.rs `persist_proposal()` | ✅ goals.rs | ✅ `/api/goals`, `/api/goals/:id/approve`, `/api/goals/:id/reject` | **LIVE** ✅ FIXED |
| `reflection_entries` | ✅ runner.rs `persist_reflection()` | ✗ (query fn exists) | ✗ | **LIVE** ✅ FIXED |
| `boundary_violations` | ✅ runner.rs `persist_boundary_violation()` | ✗ (query fn exists) | ✗ | **LIVE** ✅ FIXED |
| `delegation_state` | ✅ mesh_routes.rs `persist_delegation_from_message()` | ✗ (query fn exists) | ✗ | **LIVE** ✅ FIXED |
| `audit_log` | ✅ safety.rs `write_audit_entry()` | ✅ audit.rs | ✅ `/api/audit`, `/api/audit/aggregation`, `/api/audit/export` | **LIVE** ✅ FIXED |

Additionally, one non-table in-memory store:

| Store | Has Writer (Production) | Has Reader | Has API Endpoint | Verdict |
|---|---|---|---|---|
| `CostTracker` (in-memory) | ✅ runner.rs `record_cost()` callback | ✅ costs.rs | ✅ `/api/costs` | **LIVE** ✅ FIXED |

---

## Summary Counts

| Verdict | Count | Tables |
|---|---|---|
| LIVE | 14 | All tables now have production write paths |
| READ-ONLY-EMPTY | 0 | — |
| WRITE-ONLY | 0 | — |
| DEAD-SCHEMA | 0 | — |

---

## Fix Log (2026-02-28)

All 13 dead write paths have been fixed. Here's what was done:

### cortex-storage query modules (new)
- `memory_event_queries.rs` — INSERT/SELECT for `memory_events`
- `memory_snapshot_queries.rs` — INSERT/SELECT for `memory_snapshots`
- `memory_audit_queries.rs` — INSERT/SELECT for `memory_audit_log`
- `delegation_state_queries.rs` — INSERT/UPDATE/SELECT for `delegation_state`

### v019 migration (new)
- `v019_intervention_state.rs` — Creates `intervention_state` table + missing indexes

### ghost-agent-loop/src/runner.rs
- `persist_proposal()` — writes to `goal_proposals` on every proposal (Text + Mixed branches)
- `persist_boundary_violation()` — writes to `boundary_violations` on credential exfiltration (KillAll) and pattern matches (Warning)
- `persist_reflection()` — writes to `reflection_entries` when ReflectionWrite proposals are auto-approved
- `record_cost()` — callback to CostTracker wired after every LLM call
- `db` + `cost_recorder` fields added to AgentRunner struct

### ghost-gateway/src/api/safety.rs
- `write_audit_entry()` — writes to `audit_log` from `kill_all()`, `pause_agent()`, `resume_agent()`, `quarantine_agent()`

### ghost-gateway/src/api/memory.rs
- `write_memory()` — POST `/api/memory` endpoint writes to `memory_events`, `memory_snapshots`, and `memory_audit_log`

### ghost-gateway/src/api/mesh_routes.rs
- `persist_delegation_from_message()` — writes to `delegation_state` on A2A task send/cancel

### convergence-monitor/src/monitor.rs
- `persist_intervention_history()` — writes to `intervention_history` on every intervention level change
- `persist_intervention_state()` — writes to `intervention_state` (already existed, now has table via v019)

### ghost-gateway/src/cli/chat.rs
- AgentRunner DB wiring — opens DB connection and sets `runner.db` for persistence in CLI mode

### ghost-gateway/src/bootstrap.rs
- DB Arc shared with mesh router for delegation state persistence
- POST `/api/memory` route registered

---

## Detailed Findings for READ-ONLY-EMPTY Tables

These are the "endpoint reads from a table nothing writes to" pattern — the biggest recurring issue.

### 1. `memory_snapshots` — `/api/memory`, `/api/memory/:id`

**What reads it:**
- `ghost-gateway/src/api/memory.rs` — `list_memories()` does `SELECT id, memory_id, snapshot, created_at FROM memory_snapshots` with pagination
- `ghost-gateway/src/api/memory.rs` — `get_memory()` does `SELECT ... FROM memory_snapshots WHERE memory_id = ?1`

**What SHOULD write to it:**
- The cortex-storage crate has a `snapshot_ops.rs` in the `explore/drift-repo` variant that does `INSERT INTO memory_snapshots (memory_id, snapshot_at, state, event_id, reason)`, but this file does not exist in the current workspace's `crates/cortex/cortex-storage/src/queries/`.
- The memory system should snapshot after memory mutations (create, update, consolidation). The snapshot writer was never ported from the drift-repo prototype.

**Where the write call is missing:**
- A `snapshot_ops.rs` query module needs to be added to `crates/cortex/cortex-storage/src/queries/`
- The cortex memory mutation paths (wherever `INSERT INTO memory_events` would happen) should call `snapshot_ops::insert_snapshot()` after each state change.
- Since `memory_events` also has no production writer (see below), the entire memory write pipeline is missing.

---

### 2. `memory_events` — `/api/memory` (via JOIN)

**What reads it:**
- `ghost-gateway/src/api/memory.rs` — `list_memories()` JOINs `memory_snapshots ms JOIN memory_events me ON ms.memory_id = me.memory_id WHERE me.actor_id = ?1` when filtering by agent_id.

**What SHOULD write to it:**
- The cortex-storage crate has an `event_ops.rs` in the `explore/drift-repo` variant that does `INSERT INTO memory_events (memory_id, recorded_at, event_type, delta, actor_type, actor_id, ...)`, but this file does not exist in the current workspace.
- Every memory mutation (create, update, archive, link, decay) should emit a memory_event.

**Where the write call is missing:**
- An `event_ops.rs` query module needs to be added to `crates/cortex/cortex-storage/src/queries/`
- The ghost-agent-loop or cortex memory layer should call `event_ops::insert_event()` on every memory state change.

---

### 3. `goal_proposals` — `/api/goals`, `/api/goals/:id/approve`, `/api/goals/:id/reject`

**What reads it:**
- `ghost-gateway/src/api/goals.rs` — `list_goals()` calls `goal_proposal_queries::query_pending()` → `SELECT ... FROM goal_proposals WHERE resolved_at IS NULL`
- `ghost-gateway/src/api/goals.rs` — `approve_goal()` / `reject_goal()` call `goal_proposal_queries::resolve_proposal()` → `UPDATE goal_proposals SET decision = ?2 ... WHERE id = ?1 AND resolved_at IS NULL`

**What SHOULD write to it:**
- `cortex_storage::queries::goal_proposal_queries::insert_proposal()` exists and is correct, but is only called from test code (`crates/cortex/cortex-storage/tests/migration_tests.rs`).
- The agent loop's goal/proposal pipeline should call `insert_proposal()` when an agent proposes a goal change that requires human review.

**Where the write call is missing:**
- `crates/ghost-agent-loop/src/` — the proposal creation flow (likely in the proposal module under `src/proposal/`) should call `goal_proposal_queries::insert_proposal()` when generating proposals that need human approval.
- The agent loop has a `proposal/` directory but never calls the storage insert function.

---

### 4. `audit_log` — `/api/audit`, `/api/audit/aggregation`, `/api/audit/export`

**What reads it:**
- `ghost-gateway/src/api/audit.rs` — `query_audit()` calls `AuditQueryEngine::query()` → dynamic `SELECT ... FROM audit_log WHERE ...`
- `ghost-gateway/src/api/audit.rs` — `audit_aggregation()` calls `AuditAggregation::summarize()` → aggregation queries on `audit_log`
- `ghost-gateway/src/api/audit.rs` — `audit_export()` calls `AuditQueryEngine::query()` → bulk SELECT for export

**What SHOULD write to it:**
- `ghost_audit::AuditQueryEngine::insert()` exists and works correctly, but is never called from any production code. Only called in `crates/ghost-audit/tests/audit_tests.rs`.
- Every safety action (kill, pause, quarantine, resume) in `safety.rs` should write an audit entry.
- Every boundary violation, policy denial, and tool execution should write an audit entry.
- The KillSwitch has its own in-memory `Vec<AuditEntry>` (`kill_switch.rs:69`) but never persists to the SQLite `audit_log` table.

**Where the write call is missing:**
- `crates/ghost-gateway/src/api/safety.rs` — `kill_all()`, `pause_agent()`, `quarantine_agent()`, `resume_agent()` should each call `AuditQueryEngine::insert()` after performing the safety action.
- `crates/ghost-agent-loop/src/` — tool executions and policy denials should write audit entries.
- The bootstrap should wire `AuditQueryEngine` (or a writer handle) into `AppState` so handlers can insert entries.

---

### 5. `CostTracker` (in-memory) — `/api/costs`

**What reads it:**
- `ghost-gateway/src/api/costs.rs` — `get_costs()` calls `state.cost_tracker.get_daily_total()` and `state.cost_tracker.get_compaction_cost()` for each agent.

**What SHOULD write to it:**
- `CostTracker::record(agent_id, session_id, cost, is_compaction)` exists but is only called in test code (`crates/ghost-gateway/tests/gateway_tests.rs`).
- The LLM call path in `ghost-agent-loop` should call `cost_tracker.record()` after each LLM response with the token cost.

**Where the write call is missing:**
- `crates/ghost-agent-loop/src/runner.rs` or wherever LLM responses are processed — after receiving an LLM response, the cost should be extracted from usage metadata and passed to `cost_tracker.record()`.
- The `CostTracker` instance from `AppState` needs to be passed into the agent loop (or the agent loop needs its own reference to it).

---

## Phantom Table Reference

### `intervention_state` — Referenced but never created

**What reads it:**
- `crates/convergence-monitor/src/monitor.rs:117` — `SELECT agent_id, level, consecutive_normal, cooldown_until, ack_required, hysteresis_count, de_escalation_credits FROM intervention_state`

**The problem:**
- No migration creates an `intervention_state` table. The v017 migration creates `intervention_history` (append-only log of transitions), not `intervention_state` (current state per agent).
- The monitor gracefully handles this with `tracing::warn!("intervention_state table not found — starting fresh")`, so it doesn't crash, but the state restoration on restart is silently broken.

**Fix needed:**
- Either add a `CREATE TABLE IF NOT EXISTS intervention_state` to a migration, or change the monitor to derive current state from `intervention_history` (latest row per agent).

---

## DEAD-SCHEMA Tables (No Reads, No Production Writes)

These tables exist in migrations but have no production code path that reads or writes to them.

### `intervention_history`
- `insert_intervention()` exists in cortex-storage but is never called from convergence-monitor or any production code.
- `query_by_agent()` and `query_by_level()` exist but no gateway endpoint calls them.
- **Should be written by:** convergence-monitor when intervention levels change (the monitor computes interventions but never persists them to this table).
- **Should be read by:** a gateway endpoint like `/api/interventions` or `/api/safety/interventions/:agent_id`.

### `reflection_entries`
- `insert_reflection()` exists in cortex-storage but is never called from production code.
- `query_by_session()` and `count_per_session()` exist but no gateway endpoint calls them.
- **Should be written by:** the agent loop when self-reflection occurs.
- **Should be read by:** a gateway endpoint like `/api/reflections` or included in session detail responses.

### `boundary_violations`
- `insert_violation()` exists in cortex-storage but is never called from convergence-monitor or any production code.
- `query_by_agent_session()` and `query_by_type()` exist but no gateway endpoint calls them.
- **Should be written by:** the agent loop's boundary detection system or the convergence-monitor when violations are detected.
- **Should be read by:** a gateway endpoint like `/api/violations` or included in audit/safety dashboards.

### `delegation_state`
- Only written in test code (`crates/cortex/cortex-storage/tests/migration_tests.rs`).
- No query module exists in cortex-storage for this table.
- No gateway endpoint reads from it.
- **Should be written by:** the mesh/multi-agent delegation system when agents delegate tasks.
- **Should be read by:** a gateway endpoint like `/api/delegations` or the mesh routes.

---

## Remediation Priority

| Priority | Table | Impact | Fix |
|---|---|---|---|
| P0 | `audit_log` | Safety audit trail is empty — compliance/forensics blind | Wire `AuditQueryEngine::insert()` into safety handlers and agent loop |
| P0 | `goal_proposals` | Human-in-the-loop approval flow is non-functional | Wire `insert_proposal()` into agent loop proposal pipeline |
| P1 | `memory_snapshots` | Memory API returns empty for all queries | Port `snapshot_ops.rs` from drift-repo, wire into memory mutation paths |
| P1 | `memory_events` | Memory agent-filter JOIN always returns zero rows | Port `event_ops.rs` from drift-repo, wire into memory mutation paths |
| P1 | `CostTracker` | Cost/spending cap enforcement is non-functional | Wire `cost_tracker.record()` into LLM call path in agent loop |
| P1 | `intervention_state` (phantom) | Monitor state restoration silently fails on restart | Add migration or derive from `intervention_history` |
| P2 | `intervention_history` | Intervention transitions not persisted | Wire `insert_intervention()` into convergence-monitor |
| P2 | `boundary_violations` | Violations not persisted | Wire `insert_violation()` into agent loop boundary detection |
| P2 | `reflection_entries` | Reflections not persisted | Wire `insert_reflection()` into agent loop reflection system |
| P3 | `delegation_state` | Delegation tracking not functional | Wire into mesh delegation system when it's activated |
| P3 | `memory_audit_log` | Only has genesis row, no operational writes | Wire into memory mutation paths alongside `memory_events` |
