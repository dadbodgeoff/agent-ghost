# cortex-storage

> SQLite persistence with append-only triggers, hash chain columns, and forward-only migrations — the tamper-evident data layer for the entire GHOST platform.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 1 (Cortex Foundation) |
| Type | Library |
| Location | `crates/cortex/cortex-storage/` |
| Workspace deps | `cortex-core` |
| External deps | `rusqlite` 0.32 (bundled SQLite), `blake3`, `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, `tracing` |
| Modules | `migrations` (v016, v017, v018), `queries` (6 query modules) |
| Tables | 10 total: `schema_version`, `memory_events`, `memory_audit_log`, `memory_snapshots`, `itp_events`, `convergence_scores`, `intervention_history`, `goal_proposals`, `reflection_entries`, `boundary_violations`, `delegation_state` |
| Public API | `open_in_memory()`, `run_all_migrations()`, `to_storage_err()`, 6 query modules |
| Downstream consumers | `ghost-audit`, `convergence-monitor`, `ghost-gateway`, `ghost-integration-tests` |

---

## Why This Crate Exists

Every piece of state in the GHOST platform — ITP events, convergence scores, intervention history, proposals, reflections, boundary violations — needs to be persisted with three guarantees:

1. **Append-only.** Once written, records cannot be modified or deleted. This is enforced at the SQLite trigger level — `UPDATE` and `DELETE` statements are rejected with `RAISE(ABORT)`.

2. **Hash-chained.** Every record contains `event_hash` and `previous_hash` columns. Each record's hash includes the previous record's hash, forming a tamper-evident chain. If any record is modified outside the trigger system (e.g., direct SQLite file manipulation), the chain breaks and the tampering is detectable.

3. **Forward-only migrations.** There are no rollback migrations. Schema changes are additive only. This prevents accidental data loss and ensures the database schema is always monotonically advancing.

---

## Migration System

### Architecture

```rust
pub const LATEST_VERSION: u32 = 18;

const MIGRATIONS: [(u32, &str, MigrationFn); 3] = [
    (16, "convergence_safety", v016_convergence_safety::migrate),
    (17, "convergence_tables", v017_convergence_tables::migrate),
    (18, "delegation_state", v018_delegation_state::migrate),
];
```

Migrations are numbered, named, and stored in a `schema_version` table. On startup, `run_migrations()` checks the current version and runs any pending migrations in order.

**Why start at v016?** The first 15 migrations are from the pre-convergence era (the original Cortex memory system). The convergence safety foundation starts at v016. The numbering is continuous with the legacy system.

**No rollback.** The `MIGRATIONS` array contains only forward functions. There is no `down()` or `rollback()`. If a migration needs to be undone, you restore from backup (`ghost-backup`). This is a deliberate design choice — rollback migrations are a common source of data corruption bugs.

### v016 — Convergence Safety Foundation

Creates the base infrastructure:

1. **Base tables** (`memory_events`, `memory_audit_log`, `memory_snapshots`) — Created if they don't exist (idempotent for fresh databases).

2. **Hash chain columns** — Adds `event_hash BLOB NOT NULL` and `previous_hash BLOB NOT NULL` to `memory_events`.

3. **Append-only triggers** — 6 triggers total:
   - `prevent_memory_events_update` / `prevent_memory_events_delete`
   - `prevent_audit_log_update` / `prevent_audit_log_delete`
   - `prevent_snapshots_update` / `prevent_snapshots_delete`

4. **Genesis block** — Inserts a `__GENESIS__` record into `memory_audit_log` marking the start of the hash chain era. Events before this point are pre-chain and not hash-verified.

### v017 — Convergence Core Tables

Creates 6 new tables, all with hash chain columns and append-only triggers:

| Table | Purpose | Special Rules |
|-------|---------|---------------|
| `itp_events` | Interaction Telemetry Protocol events | Fully append-only |
| `convergence_scores` | Composite convergence scores per agent | Fully append-only |
| `intervention_history` | Intervention level transitions | Fully append-only |
| `goal_proposals` | Agent state change proposals | **UPDATE exception for unresolved proposals (AC10)** |
| `reflection_entries` | Agent reflection chains | Fully append-only |
| `boundary_violations` | Simulation boundary violations | Fully append-only |

**The goal_proposals UPDATE Exception (AC10):**

This is the most nuanced design decision in the storage layer. `goal_proposals` allows `UPDATE` on unresolved proposals (where `resolved_at IS NULL`) but rejects updates on resolved proposals. The trigger:

```sql
CREATE TRIGGER goal_proposals_append_guard
BEFORE UPDATE ON goal_proposals
BEGIN
    SELECT CASE WHEN OLD.resolved_at IS NOT NULL
        THEN RAISE(ABORT, 'SAFETY: resolved proposals are immutable.')
    END;
END;
```

Why? Because proposals go through a lifecycle:
1. Created with `decision = 'HumanReviewRequired'` and `resolved_at = NULL`
2. A human or the system resolves it by setting `decision`, `resolver`, and `resolved_at`
3. Once resolved, the proposal is immutable forever

Without this exception, resolving a proposal would require inserting a new "resolution" record and joining them — adding complexity without security benefit. The trigger ensures that once resolved, the record is as immutable as any other append-only table.

### v018 — Delegation State

Adds the `delegation_state` table for inter-agent task delegation (A2A protocol support). Same append-only pattern with a state machine guard:

```sql
CREATE TRIGGER delegation_state_append_guard
BEFORE UPDATE ON delegation_state
BEGIN
    SELECT CASE
        WHEN OLD.state IN ('Completed', 'Disputed', 'Rejected')
        THEN RAISE(ABORT, 'SAFETY: resolved delegations are immutable.')
    END;
END;
```

Valid state transitions:
- `Offered` → `Accepted` or `Rejected`
- `Accepted` → `Completed` or `Disputed`
- `Completed`, `Disputed`, `Rejected` → immutable (no further transitions)

---

## Table Schema Details

### Hash Chain Columns

Every convergence table has two BLOB columns:

| Column | Purpose |
|--------|---------|
| `event_hash` | blake3 hash of the current record's content |
| `previous_hash` | blake3 hash of the previous record in the chain |

The hash chain is computed by the caller (typically `cortex-temporal`), not by the storage layer. `cortex-storage` stores the hashes but doesn't compute or verify them — that's a separation of concerns. The storage layer guarantees immutability; the temporal layer guarantees chain integrity.

### Index Strategy

Each table has targeted indexes for the most common query patterns:

| Table | Indexes | Query Pattern |
|-------|---------|---------------|
| `itp_events` | `(session_id, sequence_number)`, `(timestamp)` | Session replay, time-range queries |
| `convergence_scores` | `(agent_id, computed_at)` | Latest score per agent |
| `intervention_history` | `(agent_id, created_at)`, `(intervention_level, created_at)` | Agent history, level-based queries |
| `goal_proposals` | `(agent_id, created_at)`, `(decision) WHERE decision = 'HumanReviewRequired'` | Agent proposals, pending review queue |
| `reflection_entries` | `(session_id, created_at)`, `(chain_id, depth)` | Session reflections, chain traversal |
| `boundary_violations` | `(session_id, created_at)`, `(violation_type, severity)` | Session violations, severity ranking |
| `delegation_state` | `(delegation_id)`, `(sender_id, created_at)`, `(recipient_id, created_at)`, `(state) WHERE state IN ('Offered', 'Accepted')` | Delegation lookup, pending delegations |

The `goal_proposals` and `delegation_state` tables use partial indexes (`WHERE` clause) to efficiently query only pending/active records without scanning resolved ones.

---

## Query Modules

Six typed query modules, one per convergence table:

### `itp_event_queries`
- `insert_itp_event()` — Insert with all fields including hash chain
- `query_by_session()` — All events for a session, ordered by sequence number
- `query_by_time_range()` — Events within a time window

### `convergence_score_queries`
- `insert_score()` — Insert a new convergence score
- `query_by_agent()` — All scores for an agent, newest first
- `latest_by_agent()` — Most recent score for an agent

### `intervention_history_queries`
- `insert_intervention()` — Record an intervention level transition
- `query_by_agent()` — Intervention history for an agent
- `query_by_level()` — All interventions at a specific level

### `goal_proposal_queries`
- `insert_proposal()` — Create a new proposal
- `resolve_proposal()` — Resolve an unresolved proposal (AC10 safe)
- `query_pending()` — All unresolved proposals
- `query_by_agent()` — All proposals for an agent

### `reflection_queries`
- `insert_reflection()` — Record a reflection entry
- `query_by_session()` — All reflections in a session
- `count_per_session()` — Count reflections (for limit enforcement)

### `boundary_violation_queries`
- `insert_violation()` — Record a boundary violation
- `query_by_agent_session()` — Violations in a session
- `query_by_type()` — Violations by type, sorted by severity

All query functions return typed `Row` structs (e.g., `ITPEventRow`, `ScoreRow`, `ProposalRow`). These are plain data structs with no behavior — they're the storage layer's output format.

---

## Security Properties

### Append-Only Enforcement

SQLite triggers reject `UPDATE` and `DELETE` with `RAISE(ABORT)`. This is enforced at the database engine level — even raw SQL executed against the connection is blocked. The only way to bypass triggers is to:
1. Open the SQLite file directly and modify bytes (detectable via hash chain)
2. Drop and recreate the table (detectable via schema_version mismatch)
3. Disable triggers (requires `PRAGMA writable_schema = ON`, which is not exposed)

### Hash Chain Integrity

Every record links to the previous via `previous_hash`. If a record is modified outside the trigger system, the hash chain breaks at that point. Verification is done by `cortex-temporal`, not by `cortex-storage`.

### NOT NULL Constraints

`event_hash` and `previous_hash` are `NOT NULL`. You cannot insert a record without providing hash chain data. The test suite verifies this with an adversarial test that attempts to insert `NULL` for `event_hash`.

### Genesis Block

The `__GENESIS__` record in `memory_audit_log` marks the boundary between pre-chain and post-chain data. This prevents an attacker from claiming that missing hash chain data is "from before the chain was implemented."

---

## Test Strategy

### Migration Tests (`tests/migration_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `v016_triggers_exist` | All 4 base append-only triggers created |
| `v017_all_six_tables_created` | All 6 convergence tables exist |
| `insert_itp_event_succeeds` | Happy path insert |
| `update_itp_events_rejected_by_trigger` | UPDATE blocked with "append-only" message |
| `delete_itp_events_rejected_by_trigger` | DELETE blocked |
| `update_convergence_scores_rejected` | Scores are immutable |
| `update_unresolved_goal_proposal_succeeds` | AC10: unresolved proposals can be updated |
| `update_resolved_goal_proposal_rejected` | AC10: resolved proposals are immutable |
| `delete_goal_proposals_rejected` | No deletes ever |
| `delete_on_all_convergence_tables_rejected` | DELETE blocked on reflections and violations |
| `query_itp_events_by_session` | Session query returns ordered results |
| `query_convergence_scores_by_agent` | Agent score query |
| `latest_score_by_agent` | Latest score selection |
| `query_pending_proposals` | Pending proposal filter |
| `adversarial_insert_with_null_event_hash_rejected` | NOT NULL constraint enforcement |
| `v018_delegation_state_table_created` | Table exists |
| `v018_insert_delegation_succeeds` | Happy path |
| `v018_update_offered_delegation_succeeds` | Active delegations updatable |
| `v018_update_resolved_delegation_rejected` | Completed delegations immutable |
| `v018_delete_delegation_rejected` | No deletes |
| `v018_rejected/disputed_delegation_immutable` | Terminal states are final |
| `v018_schema_version_updated` | Version = 18 |

---

## File Map

```
crates/cortex/cortex-storage/
├── Cargo.toml
├── src/
│   ├── lib.rs                                      # open_in_memory, run_all_migrations, to_storage_err
│   ├── migrations/
│   │   ├── mod.rs                                  # Migration runner, LATEST_VERSION = 18
│   │   ├── v016_convergence_safety.rs              # Base tables, hash chains, append-only triggers, genesis
│   │   ├── v017_convergence_tables.rs              # 6 convergence tables + AC10 exception
│   │   └── v018_delegation_state.rs                # Delegation state machine table
│   └── queries/
│       ├── mod.rs                                  # Module declarations
│       ├── itp_event_queries.rs                    # ITP event CRUD
│       ├── convergence_score_queries.rs            # Score CRUD + latest_by_agent
│       ├── intervention_history_queries.rs         # Intervention CRUD + level queries
│       ├── goal_proposal_queries.rs                # Proposal CRUD + resolve + pending
│       ├── reflection_queries.rs                   # Reflection CRUD + count_per_session
│       └── boundary_violation_queries.rs           # Violation CRUD + type queries
└── tests/
    └── migration_tests.rs                          # 22 tests covering all tables and triggers
```
