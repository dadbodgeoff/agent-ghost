# SQL Column Mismatch & Type Contract Audit

**Audit Prompts:** #2 (SQL Correctness) + #5 (Cross-Crate Type Contracts)
**Date:** 2026-02-28
**Scope:** Every SQL statement in gateway handlers + ghost-audit + cortex-storage queries, cross-referenced against migration DDL ground truth.

---

## Schema Ground Truth (from migrations)

| Table | Source | PK Type | Notable Columns |
|---|---|---|---|
| `memory_events` | v016 | `event_id INTEGER PK AUTO` | `memory_id TEXT`, `actor_id TEXT`, `recorded_at TEXT` |
| `memory_audit_log` | v016 | `id INTEGER PK AUTO` | `memory_id TEXT`, `operation TEXT`, `timestamp TEXT` |
| `memory_snapshots` | v016 | `id INTEGER PK AUTO` | `memory_id TEXT`, `snapshot TEXT`, `created_at TEXT` |
| `itp_events` | v017 | `id TEXT PK` | `session_id TEXT`, `sender TEXT` (nullable), `timestamp TEXT`, `sequence_number INTEGER` |
| `convergence_scores` | v017 | `id TEXT PK` | `agent_id TEXT`, `composite_score REAL`, `level INTEGER`, `signal_scores TEXT` |
| `goal_proposals` | v017 | `id TEXT PK` | `agent_id TEXT NOT NULL`, `session_id TEXT NOT NULL`, `decision TEXT` (nullable), `resolved_at TEXT` (nullable) |
| `audit_log` | ensure_table() | `id TEXT PK` | `agent_id TEXT NOT NULL`, `severity TEXT NOT NULL DEFAULT 'info'`, `tool_name TEXT` (nullable) |

---

## Findings Table

| # | Sev | Category | File | Line(s) | Finding | Impact |
|---|-----|----------|------|---------|---------|--------|
| 1 | `convergence-monitor/src/monitor.rs:224` | `SELECT agent_id, score, level FROM convergence_scores WHERE rowid IN (SELECT MAX(rowid) ...)` | Column `score` does not exist. Schema defines `composite_score`. This SELECT will fail at runtime with "no such column: score". The old "proposals" class of bug — code uses a shorthand name that doesn't match the actual column. | CRITICAL |
| 2 | `convergence-monitor/src/monitor.rs:117` | `SELECT agent_id, level, consecutive_normal, cooldown_until, ack_required, hysteresis_count, de_escalation_credits FROM intervention_state` | Table `intervention_state` does not exist in any migration (v016, v017, v018) or in `ensure_table()`. No CREATE TABLE for it anywhere in the codebase. The monitor gracefully degrades (logs warning, starts fresh), but this is a phantom table reference — the state reconstruction silently skips, meaning intervention levels are always lost on restart. | HIGH |
| 3 | `convergence-monitor/src/monitor.rs:188` | `SELECT agent_id, COUNT(*) FROM itp_events WHERE event_type = 'SessionStart' GROUP BY agent_id` | Column `agent_id` does not exist on `itp_events`. Schema defines `sender` (TEXT). The monitor writes `sender` correctly in its INSERT (finding #5 below), but reads `agent_id` here. This SELECT will fail at runtime. Calibration counts are never restored from DB. | CRITICAL |
| 4 | `convergence-monitor/src/monitor.rs:728` | `INSERT INTO itp_events (id, session_id, event_type, sender, timestamp, content_hash, content_length, event_hash, previous_hash) VALUES (?1..?9)` | Missing NOT NULL column `privacy_level` (default 'standard'). INSERT will succeed because the column has a DEFAULT, but `sequence_number` (INTEGER NOT NULL DEFAULT 0) is also omitted — also has a DEFAULT so it works. Columns `latency_ms`, `token_count`, `attributes` are nullable/defaulted. This INSERT is technically valid but omits `privacy_level` explicitly — relies on DEFAULT. Not a failure, but the monitor never sets privacy_level, so all events are 'standard'. | LOW |
| 5 | `convergence-monitor/src/monitor.rs:762` | `INSERT INTO convergence_scores (id, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at, event_hash, previous_hash) VALUES (?1..?10)` | All 10 columns match the v017 schema exactly. Column names correct, types correct, all NOT NULL columns provided. `created_at` has DEFAULT. **VALID.** Previous connectivity audit (Finding #1) was wrong — it described a different INSERT that no longer exists. The monitor's INSERT was fixed. | OK |
| 6 | `ghost-gateway/src/api/memory.rs:47` | `SELECT COUNT(*) FROM memory_snapshots ms JOIN memory_events me ON ms.memory_id = me.memory_id WHERE me.actor_id = ?1` | JOIN condition `ms.memory_id = me.memory_id` is valid (both columns exist). Filter `me.actor_id = ?1` is valid (column exists). However, `actor_id` defaults to `'system'` and the API passes a UUID agent_id string. If the writer uses 'system' as actor_id, the JOIN returns zero rows for any agent_id filter. Semantic mismatch, not a schema error. | MEDIUM |
| 7 | `ghost-gateway/src/api/memory.rs:79` | `SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at FROM memory_snapshots ms JOIN memory_events me ON ms.memory_id = me.memory_id WHERE me.actor_id = ?1 GROUP BY ms.id ORDER BY ms.created_at DESC LIMIT ?2 OFFSET ?3` | `ms.id` is `INTEGER PRIMARY KEY AUTOINCREMENT`, read as `row.get::<_, i64>(0)` — correct. `ms.memory_id` is `TEXT`, read as `String` — correct. `ms.snapshot` is `TEXT`, read as `String` — correct. `ms.created_at` is `TEXT`, read as `String` — correct. Params: 3 params for ?1, ?2, ?3 — correct. **VALID.** | OK |
| 8 | `ghost-gateway/src/api/memory.rs:120` | `SELECT id, memory_id, snapshot, created_at FROM memory_snapshots ORDER BY created_at DESC LIMIT ?1 OFFSET ?2` | All columns exist. `id` read as `i64` matches `INTEGER PRIMARY KEY`. 2 params for ?1, ?2 — correct. **VALID.** | OK |
| 9 | `ghost-gateway/src/api/memory.rs:187` | `SELECT id, memory_id, snapshot, created_at FROM memory_snapshots WHERE memory_id = ?1` | All columns exist. 1 param for ?1 — correct. **VALID.** | OK |
| 10 | `ghost-gateway/src/api/memory.rs:200` | `SELECT id, memory_id, snapshot, created_at FROM memory_snapshots WHERE id = ?1` | `id` is `INTEGER PRIMARY KEY` but `?1` is bound to a `String` (from URL path). SQLite will attempt type coercion — if the string is numeric it works, if not it returns no rows. Not a crash, but the fallback from memory_id lookup means this path only fires when the first query fails. | LOW |
| 11 | `ghost-gateway/src/api/sessions.rs:35` | `SELECT session_id, MIN(timestamp) as started_at, MAX(timestamp) as last_event_at, COUNT(*) as event_count, GROUP_CONCAT(DISTINCT sender) as agents FROM itp_events GROUP BY session_id ORDER BY started_at DESC LIMIT 100` | All columns exist in v017 schema: `session_id` ✓, `timestamp` ✓, `sender` ✓. `event_count` read as `i64` — `COUNT(*)` returns INTEGER, correct. `agents` read as `String` — `GROUP_CONCAT` returns TEXT, correct. No params — correct. **VALID.** | OK |
| 12 | `ghost-gateway/src/api/goals.rs:33` | Calls `cortex_storage::queries::goal_proposal_queries::query_pending()` | Delegates to cortex-storage. The query uses `goal_proposals` (correct table name, not the old `proposals` bug). All columns in the SELECT match v017 schema. **VALID.** | OK |
| 13 | `ghost-gateway/src/api/goals.rs:85` | `SELECT COALESCE(agent_id, '') FROM goal_proposals WHERE id = ?1` | `agent_id` is `TEXT NOT NULL` in v017 schema. `COALESCE` with NOT NULL column is redundant but harmless. 1 param for ?1 — correct. **VALID.** | OK |
| 14 | `ghost-gateway/src/api/goals.rs:107` | `SELECT COUNT(*) FROM goal_proposals WHERE id = ?1` | Column `id` exists (TEXT PRIMARY KEY). 1 param — correct. **VALID.** | OK |
| 15 | `ghost-gateway/src/api/convergence.rs:55` | Calls `cortex_storage::queries::convergence_score_queries::latest_by_agent()` | Delegates to cortex-storage. The underlying query SELECTs `id, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at` — all exist in v017 schema. **VALID.** | OK |
| 16 | `cortex-storage/queries/itp_event_queries.rs:22` | `INSERT INTO itp_events (id, session_id, event_type, sender, timestamp, sequence_number, content_hash, content_length, privacy_level, event_hash, previous_hash) VALUES (?1..?11)` | All 11 columns exist in v017 schema. Types: `id` TEXT ✓, `session_id` TEXT ✓, `event_type` TEXT ✓, `sender` Option<&str> for nullable TEXT ✓, `timestamp` TEXT ✓, `sequence_number` i64 for INTEGER ✓, `content_hash` Option<&str> for nullable TEXT ✓, `content_length` Option<i64> for nullable INTEGER ✓, `privacy_level` TEXT ✓, `event_hash` &[u8] for BLOB ✓, `previous_hash` &[u8] for BLOB ✓. 11 params for ?1..?11 — correct. **VALID.** | OK |
| 17 | `cortex-storage/queries/itp_event_queries.rs:39` | `SELECT id, session_id, event_type, sender, timestamp, sequence_number, content_hash, event_hash, previous_hash FROM itp_events WHERE session_id = ?1 ORDER BY sequence_number ASC` | All 9 columns exist. `event_hash` read as `Vec<u8>` for BLOB ✓, `previous_hash` read as `Vec<u8>` for BLOB ✓. 1 param — correct. **VALID.** | OK |
| 18 | `cortex-storage/queries/convergence_score_queries.rs:21` | `INSERT INTO convergence_scores (id, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at, event_hash, previous_hash) VALUES (?1..?10)` | All 10 columns exist. Types: `composite_score` f64 for REAL ✓, `signal_scores` &str for TEXT ✓, `level` i32 for INTEGER ✓, `event_hash`/`previous_hash` &[u8] for BLOB ✓. 10 params — correct. **VALID.** | OK |
| 19 | `cortex-storage/queries/convergence_score_queries.rs:36` | `SELECT id, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at FROM convergence_scores WHERE agent_id = ?1 ORDER BY computed_at DESC` | All 8 columns exist. `composite_score` read as `f64` ✓, `session_id` read as `Option<String>` for nullable TEXT ✓, `level` read as `i32` for INTEGER ✓. 1 param — correct. **VALID.** | OK |
| 20 | `cortex-storage/queries/goal_proposal_queries.rs:22` | `INSERT INTO goal_proposals (id, agent_id, session_id, proposer_type, operation, target_type, content, cited_memory_ids, decision, event_hash, previous_hash) VALUES (?1..?11)` | All 11 columns exist in v017 schema. 11 params — correct. **VALID.** | OK |
| 21 | `cortex-storage/queries/goal_proposal_queries.rs:45` | `UPDATE goal_proposals SET decision = ?2, resolver = ?3, resolved_at = ?4 WHERE id = ?1 AND resolved_at IS NULL` | All columns exist: `decision` TEXT ✓, `resolver` TEXT ✓, `resolved_at` TEXT ✓, `id` TEXT PK ✓. 4 params — correct. Protected by AC10 append-only trigger (UPDATE only allowed when `resolved_at IS NULL`). **VALID.** | OK |
| 22 | `cortex-storage/queries/goal_proposal_queries.rs:56` | `SELECT id, agent_id, session_id, proposer_type, operation, target_type, decision, resolved_at, created_at FROM goal_proposals WHERE resolved_at IS NULL ORDER BY created_at ASC` | All 9 columns exist. No params — correct. **VALID.** | OK |
| 23 | `cortex-storage/queries/intervention_history_queries.rs:21` | `INSERT INTO intervention_history (id, agent_id, session_id, intervention_level, previous_level, trigger_score, trigger_signals, action_type, event_hash, previous_hash) VALUES (?1..?10)` | All 10 columns exist. `trigger_score` f64 for REAL ✓, `trigger_signals` &str for TEXT ✓, `intervention_level`/`previous_level` i32 for INTEGER ✓. 10 params — correct. **VALID.** | OK |
| 24 | `cortex-storage/queries/intervention_history_queries.rs:37` | `SELECT id, agent_id, session_id, intervention_level, previous_level, trigger_score, action_type, created_at FROM intervention_history WHERE agent_id = ?1 ORDER BY created_at DESC` | All 8 columns exist. `trigger_score` read as `f64` ✓. 1 param — correct. **VALID.** | OK |
| 25 | `cortex-storage/queries/reflection_queries.rs:20` | `INSERT INTO reflection_entries (id, session_id, chain_id, depth, trigger_type, reflection_text, self_reference_ratio, event_hash, previous_hash) VALUES (?1..?9)` | All 9 columns exist. `depth` i32 for INTEGER ✓, `self_reference_ratio` f64 for REAL ✓. 9 params — correct. **VALID.** | OK |
| 26 | `cortex-storage/queries/reflection_queries.rs:36` | `SELECT id, session_id, chain_id, depth, trigger_type, reflection_text, self_reference_ratio, created_at FROM reflection_entries WHERE session_id = ?1 ORDER BY created_at ASC` | All 8 columns exist. 1 param — correct. **VALID.** | OK |
| 27 | `cortex-storage/queries/boundary_violation_queries.rs:22` | `INSERT INTO boundary_violations (id, session_id, violation_type, severity, trigger_text_hash, matched_patterns, action_taken, convergence_score, intervention_level, event_hash, previous_hash) VALUES (?1..?11)` | All 11 columns exist. `severity` f64 for REAL ✓, `convergence_score` Option<f64> for nullable REAL ✓, `intervention_level` Option<i32> for nullable INTEGER ✓. 11 params — correct. **VALID.** | OK |
| 28 | `cortex-storage/queries/boundary_violation_queries.rs:42` | `SELECT id, session_id, violation_type, severity, action_taken, convergence_score, intervention_level, created_at FROM boundary_violations WHERE session_id = ?1 ORDER BY created_at DESC` | All 8 columns exist. 1 param — correct. **VALID.** | OK |
| 29 | `ghost-audit/src/query_engine.rs:84` | `CREATE TABLE IF NOT EXISTS audit_log (id TEXT PRIMARY KEY, timestamp TEXT NOT NULL, agent_id TEXT NOT NULL, event_type TEXT NOT NULL, severity TEXT NOT NULL DEFAULT 'info', tool_name TEXT, details TEXT NOT NULL DEFAULT '', session_id TEXT)` | Self-consistent schema created by `ensure_table()`. All subsequent queries in this file reference these exact columns. **VALID.** | OK |
| 30 | `ghost-audit/src/query_engine.rs:100` | `INSERT INTO audit_log (id, timestamp, agent_id, event_type, severity, tool_name, details, session_id) VALUES (?1..?8)` | All 8 columns match `ensure_table()` schema. 8 params — correct. **VALID.** | OK |
| 31 | `ghost-audit/src/query_engine.rs:177` | `SELECT id, timestamp, agent_id, event_type, severity, tool_name, details, session_id FROM audit_log {where} ORDER BY timestamp DESC LIMIT ?N OFFSET ?N+1` | All 8 columns match. Dynamic param indices computed correctly (param_values.len() + 1/+2). **VALID.** | OK |
| 32 | `ghost-audit/src/aggregation.rs:53` | `SELECT DATE(timestamp) as day, COUNT(*) as cnt FROM audit_log WHERE event_type = 'violation' {AND agent_id = ?1}` | `timestamp` and `event_type` exist in audit_log. `agent_id` exists. `DATE()` is a valid SQLite function. **VALID.** | OK |
| 33 | `ghost-audit/src/aggregation.rs:65` | `SELECT severity, COUNT(*) as cnt FROM audit_log WHERE event_type = 'violation' {AND agent_id = ?1}` | `severity` exists. **VALID.** | OK |
| 34 | `ghost-audit/src/aggregation.rs:77` | `SELECT COALESCE(tool_name, 'unknown') as tool, COUNT(*) as cnt FROM audit_log WHERE event_type = 'policy_denial' {AND agent_id = ?1}` | `tool_name` exists (nullable TEXT). **VALID.** | OK |
| 35 | `ghost-audit/src/aggregation.rs:92` | `SELECT details, COUNT(*) as cnt FROM audit_log WHERE event_type = 'boundary_violation' {AND agent_id = ?1}` | `details` exists. **VALID.** | OK |
| 36 | `ghost-audit/src/query_engine.rs:152` | `format!("%{}%", search)` in LIKE clause | The `search` value is user-provided (from `AuditFilter.search`). While the value is bound via `?N` (not string-interpolated into SQL), the `%` wrapping means SQLite LIKE wildcards in the search string (`%`, `_`) are not escaped. A search for `%` matches everything. Not SQL injection (parameterized), but a LIKE wildcard injection. | LOW |
| 37 | `ghost-audit/src/aggregation.rs:150` | `agent_filter()` always returns `"AND agent_id = ?1"` | When `agent_id` is Some, the WHERE clause becomes e.g. `WHERE event_type = 'violation' AND agent_id = ?1`. The `?1` is correct because `params![aid]` passes it as the first (and only) positional parameter. **VALID.** | OK |
| 38 | `ghost-gateway/src/bootstrap.rs:87` | `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` | Same PRAGMAs used by convergence-monitor (line 709). No conflict — both set WAL mode and 5s busy timeout. If both processes open the same DB file, WAL mode supports concurrent readers. **VALID.** | OK |

---

## Summary of Actual Issues

### CRITICAL (will fail at runtime)

| # | Location | Bug |
|---|----------|-----|
| 1 | `convergence-monitor/src/monitor.rs:224` | `SELECT ... score ... FROM convergence_scores` — column is `composite_score`, not `score`. State reconstruction query fails, score cache is never restored from DB on restart. |
| 3 | `convergence-monitor/src/monitor.rs:188` | `SELECT agent_id ... FROM itp_events` — column is `sender`, not `agent_id`. Calibration count restoration fails on restart. |

### HIGH (phantom table)

| # | Location | Bug |
|---|----------|-----|
| 2 | `convergence-monitor/src/monitor.rs:117` | `SELECT ... FROM intervention_state` — table does not exist in any migration. No CREATE TABLE anywhere. Intervention state is silently lost on every restart. The monitor always starts at L0 for all agents regardless of pre-crash state. |

### MEDIUM (semantic mismatch)

| # | Location | Bug |
|---|----------|-----|
| 6 | `ghost-gateway/src/api/memory.rs:47` | JOIN filter `me.actor_id = ?1` compares against agent UUID, but `actor_id` defaults to `'system'`. Unless the writer explicitly sets `actor_id` to the agent's UUID, the filtered query always returns zero rows. |

### LOW (non-breaking quirks)

| # | Location | Bug |
|---|----------|-----|
| 4 | `convergence-monitor/src/monitor.rs:728` | ITP event INSERT omits `privacy_level` — relies on DEFAULT 'standard'. Works but means the monitor can never set a non-standard privacy level. |
| 10 | `ghost-gateway/src/api/memory.rs:200` | `WHERE id = ?1` binds a String to an INTEGER PK. SQLite coerces, but non-numeric strings silently return no rows (caught by the 404 fallback). |
| 36 | `ghost-audit/src/query_engine.rs:152` | LIKE wildcard characters in search input are not escaped. `%` or `_` in search string match more than intended. |

---

## Tables Referenced That Don't Exist in Any Migration

| Table Name | Referenced In | Status |
|------------|--------------|--------|
| `intervention_state` | `convergence-monitor/src/monitor.rs:117` | **NO CREATE TABLE anywhere.** The monitor's `reconstruct_state()` queries 7 columns from this table. It gracefully handles the missing table (logs warning, starts fresh), but this means intervention state persistence is completely non-functional. |

---

## Missing Indexes on High-Cardinality WHERE Clauses

| Table | Column(s) in WHERE | Has Index? | Concern |
|-------|-------------------|------------|---------|
| `itp_events` | `event_type` (used in calibration count query) | No dedicated index | `event_type = 'SessionStart'` scans all events. With 10K events/sec target, this table grows fast. The existing indexes are on `(session_id, sequence_number)` and `(timestamp)`. |
| `convergence_scores` | `rowid` subquery `MAX(rowid) GROUP BY agent_id` | Index on `(agent_id, computed_at)` | The `rowid` subquery bypasses the agent_id index. Could use `ORDER BY computed_at DESC LIMIT 1` per agent instead. |
| `audit_log` | `event_type` (used in 4 aggregation queries) | Yes (`idx_audit_event_type`) | Covered. |
| `memory_events` | `memory_id` (used in JOIN) | No index | JOIN `ON ms.memory_id = me.memory_id` has no index on `memory_events.memory_id`. |

---

## SQL Injection Check

No SQL injection vulnerabilities found. All user-provided values are bound via `?N` parameterized queries. The `format!` usage in `ghost-audit/src/query_engine.rs` and `aggregation.rs` only interpolates internally-constructed WHERE clauses and parameter indices — never user input directly into SQL strings.

---

## PRAGMA Conflict Check

All three DB consumers use identical PRAGMAs:
- `ghost-gateway/bootstrap.rs`: `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;`
- `convergence-monitor/monitor.rs`: `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;`
- Test harness: `PRAGMA journal_mode=WAL;`

No conflicts. WAL mode supports the multi-process access pattern (gateway reads, monitor writes).

---

## Corrections to Previous Connectivity Audit

The Connectivity Audit (Findings #1 and #2) described schema mismatches in the monitor's INSERT statements. After reading the actual current code:

- **Finding #1 (convergence_scores INSERT):** The connectivity audit described an INSERT using `(agent_id, score, level, recorded_at)`. The actual code at line 762 uses the correct columns: `(id, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at, event_hash, previous_hash)`. This INSERT is **VALID**. The connectivity audit finding is stale/incorrect for the INSERT path.

- **Finding #2 (itp_events INSERT):** The connectivity audit described an INSERT using `(session_id, agent_id, event_type, payload, timestamp, event_hash, previous_hash)`. The actual code at line 728 uses `(id, session_id, event_type, sender, timestamp, content_hash, content_length, event_hash, previous_hash)`. This INSERT is **VALID** (uses `sender` correctly, not `agent_id`). The connectivity audit finding is stale/incorrect for the INSERT path.

However, the **SELECT** paths in `reconstruct_state()` have the column mismatch bugs (Findings #1 and #3 in this audit). The bug class is the same — wrong column names — but it's in the read path, not the write path.

---

## Recommended Fixes (Priority Order)

1. **Fix `score` → `composite_score`** in monitor.rs:224. One-word change.
2. **Fix `agent_id` → `sender`** in monitor.rs:188 calibration count query.
3. **Create `intervention_state` table** — either add a v019 migration or add it to the monitor's startup code via `CREATE TABLE IF NOT EXISTS`. The table needs columns: `agent_id TEXT PK, level INTEGER, consecutive_normal INTEGER, cooldown_until TEXT, ack_required INTEGER, hysteresis_count INTEGER, de_escalation_credits INTEGER`. The monitor also needs a write path to persist intervention state changes to this table.
4. **Add index on `memory_events(memory_id)`** for the JOIN in memory.rs.
5. **Add index on `itp_events(event_type)`** if calibration count queries run at startup with large tables.
| F1 | S1 | SQL/Type | `memory.rs` | ~165-175 | `get_memory()` fallback query: `WHERE id = ?1` passes `&id` (a `String`) to match against `memory_snapshots.id` which is `INTEGER PRIMARY KEY AUTOINCREMENT`. SQLite will coerce, but a non-numeric string like `"abc"` silently returns 0 rows → 404 instead of 400. The first query (by `memory_id TEXT`) failing is expected, but the fallback silently swallows the type mismatch. | User gets misleading 404 for malformed numeric IDs. No crash, but wrong HTTP semantics. |
| F2 | S2 | SQL/Join | `memory.rs` | ~50-55, ~82-92 | Agent-filtered memory queries JOIN `memory_snapshots` to `memory_events` on `ms.memory_id = me.memory_id`. `memory_events` can have MULTIPLE rows per `memory_id` (it's an event log). The COUNT query has no GROUP BY, so `COUNT(*)` returns the total number of joined rows (inflated). The SELECT query has `GROUP BY ms.id` which deduplicates snapshots, but the COUNT and SELECT disagree — `total` may be larger than the actual number of returned pages. | Pagination metadata (`total`) is inflated when multiple events exist per memory. API consumers see `total: 50` but only 10 unique snapshots exist. |
| F3 | S2 | SQL/Null | `sessions.rs` | ~65 | `GROUP_CONCAT(DISTINCT sender)` on `itp_events.sender` which is `TEXT` (nullable per v017 DDL). SQLite's `GROUP_CONCAT` skips NULL values, but if ALL senders are NULL for a session, `row.get::<_, String>(4)` will fail with `InvalidColumnType` because the result is SQL NULL, not empty string. `unwrap_or_default()` catches this via the `unwrap_or_default` on the inner get, but the outer `row.get` returns `Err` which is caught by `Ok(serde_json::json!({...}))` — actually, the `unwrap_or_default()` is on the inner expression inside the json macro, so it IS safe. However, sessions with only NULL senders will show `agents: ""` which is semantically misleading. | Minor: empty string instead of null/empty-array for agent-less sessions. No crash. |
| F4 | S1 | SQL/Param | `aggregation.rs` | ~120 | `agent_filter()` always returns `"AND agent_id = ?1"` regardless of how many other parameters exist in the query. This works ONLY because all aggregation queries use hardcoded string literals for their WHERE conditions (e.g., `event_type = 'violation'`), so `?1` is always the first and only placeholder. If any future query adds a parameterized condition before the agent filter, `?1` will bind to the wrong value. | Currently correct but extremely fragile. Any refactor that adds parameterized conditions will silently bind wrong values — a latent injection-class bug. |
| F5 | S2 | SQL/Semantic | `aggregation.rs` | ~62-70 | `violations_per_day()` queries `audit_log WHERE event_type = 'violation'` and groups by `DATE(timestamp)`. But `audit_log.timestamp` is stored as `TEXT NOT NULL` with no format constraint. If timestamps are stored in non-ISO formats (e.g., epoch seconds), `DATE()` returns NULL and all violations collapse into a single NULL-keyed bucket. | Aggregation silently produces wrong results if timestamp format varies. |
| F6 | S0 | Type/Contract | `convergence.rs` | ~55-60 | `convergence.rs` calls `cortex_storage::queries::convergence_score_queries::latest_by_agent()` which returns `CortexResult<Option<ScoreRow>>`. The handler accesses `row.composite_score` (f64), `row.level` (i32), `row.profile` (String), `row.signal_scores` (String), `row.computed_at` (String). All match the `ScoreRow` struct fields. `signal_scores` is parsed via `serde_json::from_str` with fallback to empty object. **CORRECT — no mismatch.** | None — this is clean. |
| F7 | S0 | Type/Contract | `goals.rs` | ~35-50 | `goals.rs` calls `goal_proposal_queries::query_pending()` which returns `Vec<ProposalRow>`. Handler accesses `r.id`, `r.agent_id`, `r.session_id`, `r.proposer_type`, `r.operation`, `r.target_type`, `r.decision`, `r.created_at`. All are fields on `ProposalRow`. `decision` is `Option<String>` — serialized as JSON null when None. **CORRECT.** | None. |
| F8 | S0 | Type/Contract | `goals.rs` | ~80-95 | `resolve_proposal()` returns `CortexResult<bool>`. Handler checks `Ok(true)` (updated), `Ok(false)` (not found or already resolved), `Err`. Then does a follow-up `SELECT COUNT(*)` to distinguish not-found vs already-resolved. The follow-up query uses `row.get::<_, u32>(0)` for `COUNT(*)` — SQLite returns INTEGER for COUNT, and `u32` is valid. **CORRECT.** | None. |
| F9 | S1 | Type/Contract | `goals.rs` | ~90 | After `resolve_proposal` succeeds, handler queries `SELECT COALESCE(agent_id, '') FROM goal_proposals WHERE id = ?1`. But per v017 DDL, `agent_id` is `TEXT NOT NULL` — it can never be NULL. The `COALESCE` is unnecessary but harmless. However, the real issue: if the proposal was just resolved by another concurrent request between the `resolve_proposal` call and this SELECT, the SELECT still succeeds (the row exists, just resolved). This is fine. **Low severity — unnecessary COALESCE on NOT NULL column.** | No functional impact. Code clarity issue only. |
| F10 | S1 | SQL/Schema | `audit.rs` → `query_engine.rs` | ~105-140 | `AuditQueryEngine::query()` builds dynamic WHERE with `?N` placeholders using `param_values.len() + 1` for indexing. This is correct — each new condition gets the next sequential placeholder. LIMIT and OFFSET are appended as `?N+1` and `?N+2`. **CORRECT — parameter indexing is sound.** But: the `search` filter uses `LIKE ?N` with `format!("%{}%", search)` — this does NOT escape SQL LIKE wildcards (`%`, `_`) in user input. A search for literal `%` or `_` will match unintended rows. | Search filter doesn't escape LIKE metacharacters. User searching for literal `_` gets wildcard matches. |
| F11 | S2 | Type/Contract | `audit.rs` | ~130-145 | `audit_export()` constructs `AuditFilter` with `..Default::default()` for fields not explicitly set. `AuditFilter` derives `Default` — `page` defaults to 0, `page_size` defaults to 0. But the export handler sets `page: 1, page_size: 10_000` explicitly. The `..Default::default()` fills `event_type`, `severity`, `tool_name`, `search` as `None`. **CORRECT — no issue.** | None. |
| F12 | S2 | SQL/Semantic | `aggregation.rs` | ~75-85 | `top_violation_types()` groups by `severity` but the function name says "violation types". The `severity` column contains values like `"info"`, `"warning"`, `"critical"` — these are severity levels, not violation types. The `event_type` column would be more appropriate for "types". This is a semantic mismatch between the function name and what it actually computes. | API returns severity distribution labeled as "top_violation_types" — misleading to consumers. |
| F13 | S1 | Type/Contract | `memory.rs` | ~95-100 | `list_memories()` reads `row.get::<_, i64>(0)` for `memory_snapshots.id` which is `INTEGER PRIMARY KEY AUTOINCREMENT`. This is correct — SQLite AUTOINCREMENT PKs are i64. But `get_memory()` at line ~155 also reads `row.get::<_, i64>(0)` for the same column. **CORRECT.** | None. |
| F14 | S2 | Type/Contract | `sessions.rs` | ~60 | `event_count` is read as `row.get::<_, i64>(3)` for `COUNT(*)`. SQLite `COUNT(*)` returns INTEGER which maps to i64. **CORRECT.** | None. |
| F15 | S1 | SQL/Semantic | `sessions.rs` | ~45-55 | Sessions query uses `ORDER BY started_at DESC` where `started_at` is an alias for `MIN(timestamp)`. SQLite allows ordering by column aliases in the outer query. **CORRECT.** But: `LIMIT 100` is hardcoded with no pagination support. Large deployments with >100 sessions will silently truncate. | Sessions endpoint silently drops sessions beyond 100. No pagination params accepted. |
| F16 | S0 | Type/Contract | `costs.rs` | all | `get_costs()` reads from in-memory `CostTracker` and `AgentRegistry` — no SQL involved. `a.spending_cap` is f64, `daily`/`compaction` are f64 from CostTracker. Division by zero is guarded (`if a.spending_cap > 0.0`). **CORRECT.** | None. |
| F17 | S0 | Type/Contract | `agents.rs` | all | `list_agents()` reads from `AgentRegistry` — no SQL. `create_agent()` generates UUID, writes to registry. `delete_agent()` reads from registry + kill_switch. No SQL queries. **CORRECT.** | None. |
| F18 | S0 | Type/Contract | `safety.rs` | all | All safety endpoints operate on in-memory `KillSwitch` and `KillGateBridge`. No SQL queries. `kill_state.json` is written as a file, not to DB. **CORRECT.** | None. |
| F19 | S0 | Type/Contract | `health.rs` | all | Health reads from `GatewaySharedState` (in-memory), filesystem (`convergence_state/*.json`), and `KillGateBridge`. No SQL. **CORRECT.** | None. |
| F20 | S2 | SQL/Schema | `query_engine.rs` | ~80-95 | `ensure_table()` creates `audit_log` with `severity TEXT NOT NULL DEFAULT 'info'` and `details TEXT NOT NULL DEFAULT ''`. But the `insert()` method binds `entry.severity` and `entry.details` directly — if an `AuditEntry` is constructed with empty strings, the DEFAULT is bypassed (empty string is not NULL, so NOT NULL is satisfied). This is fine. However, `tool_name` and `session_id` are nullable in both the schema and the `AuditEntry` struct (`Option<String>`). **CORRECT.** | None. |
| F21 | S1 | Type/Contract | `convergence_score_queries.rs` | ~35-55 | `query_by_agent()` reads `session_id` as column index 2 via `row.get(2)?`. The `ScoreRow.session_id` is `Option<String>`. The DDL says `session_id TEXT` (nullable). rusqlite's `row.get::<_, Option<String>>()` handles NULL correctly. **CORRECT.** | None. |
| F22 | S1 | Type/Contract | `goal_proposal_queries.rs` | ~55-75 | `query_pending()` reads `decision` (index 6) and `resolved_at` (index 7) as `Option<String>`. DDL has both as nullable TEXT. For pending proposals, `resolved_at IS NULL` is the filter, so `resolved_at` will always be None. `decision` can be NULL or a string like `"HumanReviewRequired"`. **CORRECT.** | None. |
| F23 | S2 | Cross-Crate | `convergence.rs` → `cortex_storage` | ~55 | `latest_by_agent()` returns `CortexResult<Option<ScoreRow>>`. `CortexResult` is `Result<T, CortexError>`. The gateway handler maps `Err(e)` to a tracing error + sets `had_db_error = true`, but continues processing other agents. If ALL agents fail, returns 500. If some succeed, returns partial results with 200. **This is a design choice, not a bug, but the API consumer has no way to know which agents had errors.** | Partial failure is silent — 200 response may be missing agents due to DB errors with no indication. |
| F24 | S1 | Cross-Crate | `goals.rs` → `cortex_storage` | ~35 | `query_pending()` returns `CortexResult<Vec<ProposalRow>>`. On error, gateway returns 500 with the error message. The error type is `CortexError` which has a `Display` impl. The `format!("query failed: {e}")` exposes the internal error message to the API consumer. | Internal DB error details leaked to API consumers. Should be logged, not returned. |
| F25 | S2 | Cross-Crate | `audit.rs` → `ghost_audit` | ~55-60 | `AuditFilter` is constructed in `audit.rs` by copying fields from `AuditQueryParams`. Both structs have identical field names and types. If `ghost_audit` adds a new filter field to `AuditFilter`, the gateway handler won't pass it through — but `..Default::default()` in the export handler would mask this. The main `query_audit` handler constructs `AuditFilter` field-by-field, so a new field would cause a compile error. **This is actually good — the field-by-field construction acts as a compile-time contract check.** | None for query_audit. Export handler's `..Default::default()` could silently ignore new fields. |

---

## Summary by Severity

| Severity | Count | Description |
|----------|-------|-------------|
| S0 (Clean) | 8 | No issues found — type contracts verified correct |
| S1 (Low) | 7 | Fragile patterns, unnecessary code, minor semantic issues |
| S2 (Medium) | 6 | Inflated counts, misleading labels, silent truncation, partial failure opacity |
| S3 (Critical) | 0 | No critical SQL/type mismatches found |

---

## Actionable Findings (Grouped by Fix Phase)

### Phase A — Quick Fixes (no schema changes)

**F1: memory.rs id type coercion**
- Add explicit numeric parse check before fallback query
- Return 400 if `id` is not a valid `memory_id` format AND not a valid integer
```rust
// After first query fails:
if id.parse::<i64>().is_err() {
    return (StatusCode::NOT_FOUND, Json(json!({"error": "memory not found", "id": id})));
}
// Then do fallback query
```

**F4: aggregation.rs fragile ?1 indexing**
- Refactor `agent_filter()` to accept a `param_index: usize` parameter
- Return the placeholder with the correct index: `format!("AND agent_id = ?{}", param_index)`
```rust
fn agent_filter(agent_id: Option<&str>, next_idx: usize) -> (String, Option<String>) {
    match agent_id {
        Some(id) => (format!("AND agent_id = ?{next_idx}"), Some(id.to_string())),
        None => (String::new(), None),
    }
}
```

**F10: query_engine.rs LIKE wildcard escape**
- Escape `%` and `_` in search input before wrapping with wildcards
```rust
let escaped = search.replace('%', "\\%").replace('_', "\\_");
param_values.push(Box::new(format!("%{escaped}%")));
// Add ESCAPE clause: "details LIKE ?N ESCAPE '\\'"
```

**F12: aggregation.rs misleading function name**
- Rename `top_violation_types` → `violations_by_severity` (or change the GROUP BY to `event_type`)

**F15: sessions.rs hardcoded LIMIT**
- Accept pagination params like other endpoints
- Add `page` and `page_size` query params to `list_sessions`

**F24: goals.rs error message leakage**
- Log the full error, return generic message to client
```rust
Err(e) => {
    tracing::error!(error = %e, "goal query failed");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})))
}
```

### Phase B — Logic Fixes (behavioral changes)

**F2: memory.rs inflated COUNT with JOIN**
- Use a subquery or DISTINCT for the count query:
```sql
SELECT COUNT(DISTINCT ms.id) FROM memory_snapshots ms
JOIN memory_events me ON ms.memory_id = me.memory_id
WHERE me.actor_id = ?1
```

**F3: sessions.rs NULL sender handling**
- Use `COALESCE` or filter NULLs:
```sql
GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown'))
```
- Or return as JSON array instead of comma-separated string

**F23: convergence.rs partial failure opacity**
- Add an `errors` field to the response when some agents fail:
```json
{"scores": [...], "errors": [{"agent_id": "x", "error": "db timeout"}]}
```

### Phase C — Structural Improvements (optional)

**F5: aggregation.rs timestamp format assumption**
- Add a comment documenting the expected ISO-8601 format
- Consider adding a CHECK constraint or validation on insert

**F9: goals.rs unnecessary COALESCE**
- Remove `COALESCE(agent_id, '')` since `agent_id` is `NOT NULL`

**F25: audit export Default masking**
- Construct export filter field-by-field like `query_audit` does, to get compile-time contract checking

---

## Cross-Crate Type Contract Summary

| Gateway Handler | Backing Store | Return Type | Contract Status |
|----------------|---------------|-------------|-----------------|
| `convergence::get_scores` | `convergence_score_queries::latest_by_agent` | `CortexResult<Option<ScoreRow>>` | ✅ Clean |
| `goals::list_goals` | `goal_proposal_queries::query_pending` | `CortexResult<Vec<ProposalRow>>` | ✅ Clean |
| `goals::approve_goal` | `goal_proposal_queries::resolve_proposal` | `CortexResult<bool>` | ✅ Clean |
| `goals::reject_goal` | `goal_proposal_queries::resolve_proposal` | `CortexResult<bool>` | ✅ Clean |
| `audit::query_audit` | `AuditQueryEngine::query` | `AuditResult<PagedResult<AuditEntry>>` | ✅ Clean |
| `audit::audit_aggregation` | `AuditAggregation::summarize` | `AuditResult<AggregationResult>` | ✅ Clean |
| `audit::audit_export` | `AuditQueryEngine::query` + `AuditExporter::export` | `AuditResult<PagedResult>` → `io::Result` | ✅ Clean (⚠️ Default masking) |
| `memory::list_memories` | Direct SQL on `memory_snapshots` | `rusqlite::Result` | ⚠️ F2: JOIN inflation |
| `memory::get_memory` | Direct SQL on `memory_snapshots` | `rusqlite::Result` | ⚠️ F1: type coercion |
| `sessions::list_sessions` | Direct SQL on `itp_events` | `rusqlite::Result` | ⚠️ F3: NULL sender |
| `costs::get_costs` | In-memory `CostTracker` | No SQL | ✅ Clean |
| `agents::*` | In-memory `AgentRegistry` | No SQL | ✅ Clean |
| `safety::*` | In-memory `KillSwitch` | No SQL | ✅ Clean |
| `health::*` | Filesystem + in-memory | No SQL | ✅ Clean |

---

## Verdict

No S3 (critical) SQL column mismatches found. The original `proposals` → `goal_proposals` class of bug has been fixed. The remaining issues are:
- 2 handlers with direct SQL that bypass cortex-storage's typed query layer (`memory.rs`, `sessions.rs`) — these are where the bugs cluster
- 1 fragile parameter indexing pattern in `aggregation.rs` that will break on the next refactor
- Handlers that delegate to cortex-storage typed queries (`convergence.rs`, `goals.rs`) are clean

The pattern is clear: **handlers using typed query modules are safe; handlers with inline SQL are where bugs live.** The fix strategy should prioritize moving `memory.rs` and `sessions.rs` to typed query modules in cortex-storage.
