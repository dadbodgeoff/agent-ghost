# Deep Audit Prompts — GHOST Platform

Run each prompt in a **fresh conversation** for maximum context window.
Each targets a specific failure class that has already bitten us.

---

## PROMPT 1: Dead Write Paths (The "Empty Table" Audit)

```
Ultra think. For every SQL table created in cortex-storage migrations (v001 through v018),
do the following:

1. List every table name and its columns
2. For each table, search the ENTIRE workspace for INSERT statements that target it
3. For each table, search for any gateway API endpoint that SELECTs from it
4. Flag any table where: reads exist but no writes exist (silent empty results),
   OR writes exist but no reads exist (dead data), OR neither exists (dead schema)

Check raw SQL strings, query helper functions in cortex-storage/src/queries/,
and any ORM-like abstractions. Check ghost-audit's ensure_table() separately
since it creates tables outside migrations.

Output a table: [Table Name | Has INSERT | Has SELECT | Has API Endpoint | Verdict].
Verdict is one of: LIVE, WRITE-ONLY, READ-ONLY-EMPTY, DEAD-SCHEMA.

For every READ-ONLY-EMPTY table, identify what SHOULD be writing to it and where
that write call is missing in the codebase.
```

---

## PROMPT 2: SQL Column Mismatch Audit

```
Ultra think. This workspace has had bugs where handlers query columns that don't exist
or use wrong table names. Do a full SQL correctness audit:

1. Find every raw SQL string in crates/ghost-gateway/src/ (SELECT, INSERT, UPDATE, DELETE)
2. Find every raw SQL string in crates/ghost-audit/src/
3. Find every raw SQL string in crates/cortex/cortex-storage/src/queries/
4. For each SQL statement, verify:
   a. The table name matches exactly what's in the CREATE TABLE migration
   b. Every column referenced exists in that table's schema
   c. Column types match what the Rust code expects (e.g., TEXT vs INTEGER,
      row.get::<_, i64> vs row.get::<_, String>)
   d. Parameter binding indices (?1, ?2...) match the params array length
   e. No SQL injection via string formatting (format! with user input)

Also check for:
- Tables referenced that don't exist in any migration (like the old "proposals" bug)
- PRAGMA statements that could conflict
- Missing indexes on columns used in WHERE clauses with large expected row counts

Output: [File:Line | SQL Fragment | Issue | Severity]
```

---

## PROMPT 3: AppState Field Lifecycle Audit

```
Ultra think. Audit every field on AppState (crates/ghost-gateway/src/state.rs):

For each field:
1. WHERE IS IT CONSTRUCTED? Trace the exact line in bootstrap.rs where it's built.
   Is it built with real data or a default/empty value?
2. WHO WRITES TO IT AT RUNTIME? Find every place that mutates it after bootstrap
   (lock + write, atomic store, etc.). If nothing mutates it, it's frozen at bootstrap value.
3. WHO READS IT? Find every handler that accesses it. For each reader, does it handle
   the "empty/default" case gracefully or does it silently return empty data?
4. CAN IT DEADLOCK? If it's behind a Mutex or RwLock, find every place that holds
   the lock and check if any of those code paths try to acquire another lock on AppState
   (potential deadlock). Check if any lock is held across an .await point (Mutex + async = bad).
5. IS THE ARC NECESSARY? If a field is Arc<RwLock<T>> but only ever read, the RwLock is waste.

Also check: Is there state that SHOULD be in AppState but isn't? (e.g., CostTracker
was missing before we added it — what else is floating as a local variable in some
function that should be shared?)

Output: [Field | Constructed With | Runtime Writers | Runtime Readers | Issues Found]
```

---

## PROMPT 4: Error Swallowing Audit

```
Ultra think. This codebase has a pattern of silently swallowing errors and returning
empty success responses. Find every instance of:

1. `let _ = ` on a Result (ignoring errors)
2. `.unwrap_or_default()` or `.unwrap_or(0)` that hides a real failure
3. `if let Ok(...)` that silently drops the Err case without logging
4. `.ok()` or `.ok()?` that converts errors to None silently
5. Handlers that return 200 OK with empty data when the real issue is a missing
   table, failed query, or uninitialized state
6. `match` arms that catch errors but return success status codes
7. `.flatten()` on iterators of Results (silently drops all Err items)
8. Any place where a database query failure results in an empty Vec being
   returned to the API caller with no error indication

For each finding, classify as:
- CRITICAL: Safety-related error swallowed (kill switch, quarantine, audit)
- HIGH: Data endpoint returns empty instead of error
- MEDIUM: Non-critical error dropped but should be logged
- LOW: Intentional fallback (document why it's intentional)

Output: [File:Line | Pattern | What's Swallowed | Classification | Fix]
```

---

## PROMPT 5: Handler-to-Backing-Store Type Contract Audit

```
Ultra think. Audit every API handler in crates/ghost-gateway/src/api/ for type
contract violations between the handler and its backing store:

1. For handlers that call cortex-storage query functions:
   - Does the handler pass the right types? (String vs &str vs Uuid)
   - Does the handler correctly destructure the return type?
   - If the query returns CortexResult<T>, does the handler map the error properly?

2. For handlers that call ghost-audit functions:
   - Does AuditFilter field mapping match AuditQueryParams exactly?
   - Are there AuditQueryParams fields that get silently ignored?

3. For handlers that use raw SQL:
   - Does rusqlite::params![] match the query's ?N placeholders?
   - Are row.get::<_, T>(N) column indices correct for the SELECT column order?
   - What happens if a column is NULL but the handler expects non-null?

4. For handlers that serialize to JSON:
   - Are there struct fields that are never populated (always default)?
   - Are there serde rename attributes that could cause field name mismatches?
   - Does the JSON response match what the frontend/client expects?

5. Cross-crate version skew:
   - If cortex-storage adds a column in a migration but the query function
     doesn't select it, the handler can't access it
   - If a query function returns a struct with pub fields, but the handler
     only uses some of them, are the unused ones still correct?

Output: [Handler | Backing Call | Contract Issue | Impact]
```

---

## PROMPT 6: Safety System Integrity Audit

```
Ultra think. The safety system (kill switch, quarantine, kill gates) is the most
critical subsystem. Audit it for completeness:

1. KILL SWITCH ENFORCEMENT:
   - Find every place in the codebase that should check KillSwitch::check() before
     proceeding. Is the agent loop's GATE 3 the only check point, or should API
     handlers also refuse requests for killed agents?
   - Can a killed agent still have its goals approved? Its sessions queried?
   - What happens if kill_state.json is corrupted or contains invalid JSON on startup?

2. QUARANTINE COMPLETENESS:
   - When an agent is quarantined, what state is preserved? Is the forensic snapshot
     actually taken, or just logged?
   - Can a quarantined agent's data still be modified through other endpoints?
   - Is the 24h heightened monitoring after quarantine resume actually implemented,
     or just a JSON field in the response?

3. KILL GATE CONSISTENCY:
   - If the distributed gate is closed but the local KillSwitch is Normal, what happens?
   - If the local KillSwitch is KillAll but the gate is Normal, what happens?
   - Is there a reconciliation path for split-brain scenarios?

4. AUDIT TRAIL COMPLETENESS:
   - Every safety action (pause, quarantine, kill, resume) should write to the audit log.
     Does it? Check both the in-memory KillSwitch audit_log AND the SQLite audit_log table.
   - Can the audit trail be tampered with? (The append-only triggers protect SQLite,
     but what about the in-memory Vec<AuditEntry>?)

5. RACE CONDITIONS:
   - Two simultaneous kill-all requests
   - Pause + quarantine on same agent simultaneously
   - Resume while a new quarantine is being applied
   - Kill gate close arriving while local resume is in progress

Output: [Component | Scenario | Expected Behavior | Actual Behavior | Gap]
```

---

## PROMPT 7: Bootstrap Sequence Correctness Audit

```
Ultra think. The bootstrap sequence is the foundation — if it's wrong, everything
downstream is broken. Audit GatewayBootstrap::run() step by step:

1. ORDERING DEPENDENCIES:
   - Does step N depend on step N-1 completing? Map the actual dependency graph.
   - Can any steps run in parallel? Are any unnecessarily sequential?
   - If step 3 (monitor check) is slow (15s timeout × 3 retries = 45s), does it
     block step 4 and 5 unnecessarily?

2. FAILURE MODES:
   - For each step, what happens on failure? Is the error propagated correctly?
   - If step 2 (migrations) partially succeeds, is the DB left in a consistent state?
   - If step 4 (agent init) fails for one agent, do the other agents still register?
   - What if the DB file is locked by another process?

3. RESOURCE LEAKS:
   - step2_run_migrations opens a Connection, runs migrations, then drops it.
     Then run() opens ANOTHER Connection for AppState. Is the first one properly closed?
   - If bootstrap fails after opening the DB connection but before creating AppState,
     is the connection leaked?

4. CONFIG VALIDATION:
   - What happens with: empty agents list? Duplicate agent names? Zero spending cap?
     Missing db_path? Invalid bind address? Port already in use?
   - Are there config fields that are loaded but never validated?

5. IDEMPOTENCY:
   - If the gateway crashes and restarts, does bootstrap handle existing state correctly?
   - kill_state.json recovery — what if the file exists but KillSwitch was already Normal?
   - Do migrations handle being run twice on the same DB? (CREATE IF NOT EXISTS?)

Output: [Step | Scenario | Expected | Actual | Risk Level]
```

---

## PROMPT 8: Inter-Crate API Surface Audit

```
Ultra think. This is a 37-crate workspace. Audit the API boundaries between crates
that ghost-gateway depends on:

1. For each crate in ghost-gateway's Cargo.toml [dependencies]:
   a. What public functions/types does ghost-gateway actually import and use?
   b. Are there public APIs in the crate that ghost-gateway SHOULD be using but isn't?
   c. Are there any version/feature flag mismatches?

2. Specifically check these high-risk boundaries:
   - ghost-gateway → cortex-storage: Are all query modules used? Any queries that
     exist but aren't called from any handler?
   - ghost-gateway → ghost-audit: Is the full AuditQueryEngine API used, or just
     parts of it?
   - ghost-gateway → ghost-kill-gates: Is the relay/propagation actually functional,
     or just constructed and never used?
   - ghost-gateway → ghost-identity: Is keypair rotation wired, or just generation?
   - ghost-gateway → ghost-channels: Are channel adapters beyond CLI actually functional?
   - ghost-gateway → cortex-convergence: Is the signal computer called anywhere, or
     do convergence scores stay at 0.0 forever?
   - ghost-gateway → ghost-heartbeat: Is the heartbeat engine running, or just importable?

3. For each unused API, determine:
   - Is it genuinely not needed yet (future phase)?
   - Or is it a gap where the gateway should be calling it but isn't?

Output: [Crate | API | Used By Gateway? | Should Be Used? | Gap Description]
```

---

## Running Order

Recommended execution order (each builds on findings from the previous):

1. **Prompt 1** (Dead Write Paths) — finds tables nobody writes to
2. **Prompt 2** (SQL Correctness) — finds wrong table/column references
3. **Prompt 4** (Error Swallowing) — finds places that hide the above bugs
4. **Prompt 5** (Type Contracts) — finds mismatches between layers
5. **Prompt 3** (AppState Lifecycle) — finds state management issues
6. **Prompt 6** (Safety Integrity) — audits the most critical subsystem
7. **Prompt 7** (Bootstrap) — audits the foundation
8. **Prompt 8** (Inter-Crate) — finds unused capabilities across the workspace

After running all 8, consolidate findings into a single remediation plan
sorted by severity, then by dependency order.
