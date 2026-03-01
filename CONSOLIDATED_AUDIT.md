# Consolidated Audit — Prompts 1-6, 8

Covers all audit prompts except Prompt 7 (Bootstrap), which was completed previously.

---

## PROMPT 1: Dead Write Paths

| Table | Has INSERT | Has SELECT | Has API Endpoint | Verdict |
|-------|-----------|-----------|-----------------|---------|
| `memory_events` | ✅ memory.rs `write_memory()`, agent-loop runner | ✅ memory_event_queries | ✅ `/api/memory` (JOIN) | LIVE |
| `memory_audit_log` | ✅ memory.rs `write_memory()`, v016 genesis | ✅ memory_audit_queries | ✗ (no dedicated endpoint) | LIVE |
| `memory_snapshots` | ✅ memory.rs `write_memory()`, runner `persist_memory_snapshot()` | ✅ memory_snapshot_queries | ✅ `/api/memory`, `/api/memory/:id` | LIVE |
| `itp_events` | ✅ convergence-monitor `persist_itp_event()` | ✅ itp_event_queries, sessions.rs | ✅ `/api/sessions` | LIVE |
| `convergence_scores` | ✅ convergence-monitor `persist_convergence_score()` | ✅ convergence_score_queries, convergence_watcher | ✅ `/api/convergence/scores` | LIVE |
| `intervention_history` | ✅ convergence-monitor `persist_intervention_history()` | ✅ intervention_history_queries | ✗ (no dedicated endpoint) | LIVE (query fns exist, no API) |
| `goal_proposals` | ✅ agent-loop runner `persist_goal_proposal()` | ✅ goal_proposal_queries | ✅ `/api/goals`, `/api/goals/:id/approve`, `/api/goals/:id/reject` | LIVE |
| `reflection_entries` | ✅ agent-loop runner `persist_reflection()` | ✅ reflection_queries | ✗ (no dedicated endpoint) | LIVE (query fns exist, no API) |
| `boundary_violations` | ✅ agent-loop runner `persist_boundary_violation()` | ✅ boundary_violation_queries | ✗ (no dedicated endpoint) | LIVE (query fns exist, no API) |
| `delegation_state` | ✅ mesh_routes.rs `persist_delegation_from_message()` | ✅ delegation_state_queries | ✗ (no dedicated endpoint) | LIVE (query fns exist, no API) |
| `intervention_state` | ✅ convergence-monitor `persist_intervention_state()` | ✅ monitor `reconstruct_state()` | ✗ (internal to monitor) | LIVE |
| `audit_log` | ✅ ghost-audit `insert()`, safety.rs `write_audit_entry()` | ✅ ghost-audit `query()` | ✅ `/api/audit`, `/api/audit/aggregation`, `/api/audit/export` | LIVE |
| `schema_version` | ✅ migrations mod.rs | ✅ migrations mod.rs (MAX check) | ✗ (internal) | LIVE |

**Verdict: No dead tables.** All 13 tables have both write and read paths. 4 tables (intervention_history, reflection_entries, boundary_violations, delegation_state) have query functions but no dedicated API endpoints — these are available for future dashboard features.

---

## PROMPT 2: SQL Column Mismatch

All SQL statements verified correct. One semantic issue found and **FIXED**:

| # | Location | Issue | Severity | Status |
|---|----------|-------|----------|--------|
| 1 | `convergence-monitor/src/monitor.rs:783` | ITP INSERT omits `sequence_number` — all monitor events get DEFAULT 0, breaking `ORDER BY sequence_number ASC` in session queries | HIGH | **FIXED** — Added auto-incrementing subquery `(SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM itp_events WHERE session_id = ?2)` |

All other SQL statements (40+ checked) have correct table names, column names, types, and parameter counts.

---

## PROMPT 3: AppState Field Lifecycle

| Field | Constructed With | Runtime Writers | Runtime Readers | Issues |
|-------|-----------------|----------------|----------------|--------|
| `gateway` | `GatewaySharedState::new()` (Initializing) | `transition_to()` in bootstrap | health.rs, ready.rs | None |
| `agents` | `AgentRegistry` from config | create_agent, delete_agent | list_agents, costs, convergence, safety, convergence_watcher | None |
| `kill_switch` | `KillSwitch::new()` (Normal) | safety handlers, auto_triggers | safety_status, check(), resume | None |
| `quarantine` | `QuarantineManager::new()` | quarantine_agent | safety handlers | None — but forensic snapshot is logged, not actually taken |
| `db` | Single `Connection::open()` | All write handlers | All read handlers | Mutex held across sync code only — no async deadlock risk |
| `event_tx` | `broadcast::channel(256)` | All state-changing handlers | websocket.rs, convergence_watcher | None |
| `cost_tracker` | `CostTracker::new()` (empty) | ghost-llm after LLM calls | costs.rs | None |
| `kill_gate` | `Some(bridge)` if mesh enabled, else `None` | kill_all propagation | safety_status, health | None |
| `secret_provider` | `build_secret_provider()` | Never (read-only) | OAuth, future credential lookups | Arc<dyn> is correct for read-only |
| `oauth_broker` | `OAuthBroker::new()` with empty providers | Runtime via oauth_routes | oauth_routes | None |
| `soul_drift_threshold` | From config (default 0.15) | Never (frozen at bootstrap) | Not yet consumed at runtime | LOW — config value loaded but not checked against actual drift scores |
| `convergence_profile` | From config (default "standard") | Never (frozen at bootstrap) | Not yet consumed at runtime | LOW — same as above |
| `model_providers` | From config (default empty) | Never (frozen at bootstrap) | Not yet consumed at runtime | LOW — loaded but not used to configure LLM clients |

**No deadlock risks found.** The `db` Mutex is never held across `.await` points. The `agents` RwLock and `kill_switch` RwLock are never held simultaneously in the same code path.

---

## PROMPT 4: Error Swallowing

| # | Location | Pattern | What's Swallowed | Classification | Status |
|---|----------|---------|-----------------|----------------|--------|
| 1 | `convergence_watcher.rs:73,82` | `let _ = state.event_tx.send(...)` | ScoreUpdate/InterventionChange broadcast failures | HIGH (safety-related) | **FIXED** — Now logs warning |
| 2 | `convergence_watcher.rs:56` | `.unwrap_or_default()` on signal_scores JSON | Malformed convergence signal data | MEDIUM | **FIXED** — Now logs warning with raw data |
| 3 | `cli/chat.rs:124` | `let _ = conn.execute_batch(PRAGMA...)` | PRAGMA failures on agent loop DB | MEDIUM | **FIXED** — Now logs warning |
| 4 | `cli/commands.rs:21` | `.unwrap_or_default()` on GHOST_BACKUP_KEY | Empty passphrase for backup | LOW — Intentional fallback, already logged on line 23 | No fix needed |
| 5 | `session/manager.rs:88` | `.unwrap_or_default()` | Session list returns empty on lock failure | MEDIUM — but lock poisoning is already catastrophic | No fix needed |
| 6 | `periodic.rs:239,249` | `let _ = handle.await` | Test-only code (tokio join handle) | LOW — test code | No fix needed |
| 7 | `mesh_routes.rs:94` | `.and_then(\|v\| v.to_str().ok())` | Non-UTF8 signature header | LOW — Intentional, handled by None match arm below | No fix needed |

---

## PROMPT 5: Handler-to-Backing-Store Type Contracts

All handler-to-store type contracts verified correct:
- `goals.rs` → `goal_proposal_queries`: Types match (String for TEXT, correct column indices)
- `memory.rs` → `memory_*_queries`: Types match (i64 for INTEGER PK, String for TEXT)
- `convergence.rs` → `convergence_score_queries`: Types match (f64 for REAL, i32 for INTEGER)
- `audit.rs` → `ghost_audit::AuditQueryEngine`: All AuditFilter fields mapped correctly
- `sessions.rs` → raw SQL: Column indices match SELECT order, NULL handling via COALESCE

No type contract violations found.

---

## PROMPT 6: Safety System Integrity

| Component | Scenario | Expected | Actual | Status |
|-----------|----------|----------|--------|--------|
| KillSwitch | resume_agent audit trail | Both SQLite and in-memory audit log updated | SQLite ✅, in-memory ❌ (was missing) | **FIXED** — `resume_agent()` now calls `log_audit()` |
| KillSwitch | Poisoned RwLock | Treat as PlatformKilled | ✅ Correct — stores true to PLATFORM_KILLED | OK |
| KillSwitch | Monotonicity | Level never decreases without resume | ✅ Correct — checked in `activate_agent()` | OK |
| KillSwitch | Duplicate kill_all | Idempotent | ✅ Correct — early return if already KillAll | OK |
| Safety API | All actions write audit | Every pause/quarantine/kill/resume writes to audit_log | ✅ All 4 handlers call `write_audit_entry()` | OK |
| Safety API | Quarantine resume requires forensic review | 400 if not reviewed | ✅ Correct — checks `forensic_reviewed` and `second_confirmation` | OK |
| Safety API | Cannot resume from KillAll via agent resume | 409 Conflict | ✅ Correct | OK |
| Kill Gate | Local KillAll + gate Normal | Should propagate | ✅ `kill_all` handler calls `bridge.close_and_propagate()` | OK |
| Kill Gate | Gate closed + local Normal | Should reconcile | ⚠️ No reconciliation path — gate state is checked in health but not enforced | LOW (future work) |
| Quarantine | Forensic snapshot | Should preserve agent state | ⚠️ Logged but not actually taken — `QuarantineManager` is a placeholder | LOW (future work) |
| Append-only | Audit trail tamper resistance | SQLite protected by triggers, in-memory Vec is append-only | ✅ SQLite triggers prevent UPDATE/DELETE. In-memory Vec only has `push()` | OK |

---

## PROMPT 8: Inter-Crate API Surface

| Crate | Used By Gateway? | Should Be Used? | Gap |
|-------|-----------------|----------------|-----|
| `cortex-storage` | ✅ All query modules used | ✅ | None |
| `ghost-audit` | ✅ Full API (query, aggregation, export) | ✅ | None |
| `ghost-kill-gates` | ✅ Constructed and propagated in kill_all | ✅ | None |
| `ghost-identity` | ✅ Keypair generation in create_agent | ⚠️ Rotation not wired | LOW — generation works, rotation is future |
| `ghost-signing` | ✅ Signature verification in mesh_routes | ✅ | None |
| `ghost-mesh` | ✅ A2A dispatch, agent card | ✅ | None |
| `ghost-oauth` | ✅ Full broker lifecycle | ✅ | None |
| `ghost-secrets` | ✅ SecretProvider construction | ✅ | None |
| `ghost-egress` | ✅ Policy application in bootstrap | ✅ | None |
| `ghost-llm` | ✅ Proxy registry for egress | ⚠️ No actual LLM calls from gateway | LOW — gateway is orchestrator, agent-loop makes LLM calls |
| `ghost-agent-loop` | ✅ AgentRunner in CLI chat | ✅ | None |
| `ghost-backup` | ✅ CLI backup command | ✅ | None |
| `ghost-export` | ✅ CLI export command | ✅ | None |
| `ghost-migrate` | ✅ CLI migrate command | ✅ | None |
| `cortex-core` | ✅ Models, error types, TriggerEvent | ✅ | None |

No unused crate dependencies. All 15 workspace crate dependencies are actively used.

---

## Summary of Fixes Applied

1. **convergence-monitor ITP sequence_number** — Added auto-incrementing subquery so events are properly ordered per session
2. **convergence_watcher broadcast error logging** — `let _ =` replaced with `if let Err` + tracing::warn for safety-critical events
3. **convergence_watcher signal_scores parsing** — `unwrap_or_default()` replaced with explicit error logging
4. **CLI chat PRAGMA error logging** — `let _ =` replaced with `if let Err` + tracing::warn
5. **KillSwitch resume audit trail** — `resume_agent()` now logs to in-memory audit trail on both pause and quarantine resume
