# Speculative Context Schema Draft

Status: March 10, 2026

Purpose: define the migration-grade schema draft for phase 1 of the Speculative Context Layer in the current GHOST SQLite/Cortex storage model.

This document is a planning artifact. It is not a committed migration implementation.

Primary dependencies:

- `SPECULATIVE_CONTEXT_LAYER_DESIGN.md`
- `SPECULATIVE_CONTEXT_EXECUTION_PLAN.md`
- `SPECULATIVE_CONTEXT_HOT_PATH_CONTRACT.md`
- `SPECULATIVE_CONTEXT_HYDRATOR_CONTRACT.md`

## Scope

This draft covers phase 1 storage only:

- speculative attempt storage
- fast-gate decision storage
- async job storage
- retrieval indexes
- TTL and cleanup behavior

This draft does not yet cover:

- durable promotion tables beyond a placeholder note
- cross-session speculative retrieval
- speculative graph or citation-link tables

## Storage Decision

Phase 1 should use the existing SQLite authority with new dedicated tables in `cortex-storage`.

Reason:

- simplest migration path
- simplest auditability story
- no new operational store
- easiest future transactional promotion path

## Proposed Migration Slot

Current latest schema version in the repo is `v059`.

Recommended planning slot:

- `v060_speculative_context_phase1`

This version should be reserved for:

- `context_attempts`
- `context_attempt_validation`
- `context_attempt_jobs`
- retrieval and cleanup indexes

## Table 1: `context_attempts`

This is the primary authority table for speculative context candidates.

### Purpose

- hold per-turn speculative summary attempts
- store request-safe retrieval metadata
- provide the query source for phase 1 same-session speculative hydration

### Proposed columns

- `id TEXT PRIMARY KEY`
- `agent_id TEXT NOT NULL`
- `session_id TEXT NOT NULL`
- `turn_id TEXT NOT NULL`
- `attempt_kind TEXT NOT NULL`
- `content TEXT NOT NULL`
- `redacted_content TEXT`
- `status TEXT NOT NULL`
- `severity REAL NOT NULL DEFAULT 0.0`
- `confidence REAL NOT NULL DEFAULT 0.0`
- `retrieval_weight REAL NOT NULL DEFAULT 0.0`
- `source_refs TEXT NOT NULL`
- `source_hash BLOB`
- `fast_gate_version INTEGER NOT NULL DEFAULT 1`
- `contradicted_by_memory_id TEXT`
- `promotion_candidate INTEGER NOT NULL DEFAULT 0`
- `expires_at TEXT NOT NULL`
- `created_at TEXT NOT NULL DEFAULT (datetime('now'))`
- `updated_at TEXT NOT NULL DEFAULT (datetime('now'))`

### Constraints

`attempt_kind` should be constrained in phase 1 to:

- `summary`

Later versions may expand to:

- `fact_candidate`
- `goal_candidate`
- `reflection_candidate`
- `tool_observation`

`status` should be constrained to:

- `pending`
- `retrievable`
- `flagged`
- `blocked`
- `promoted`
- `expired`

### Phase 1 notes

- `source_refs` should be JSON text, not a separate join table in phase 1
- `source_hash` is optional in phase 1 but worth keeping in the schema to preserve provenance hardening later
- `contradicted_by_memory_id` is nullable and likely unused in phase 1, but it avoids a later table rewrite

## Table 2: `context_attempt_validation`

Append-only gate record table.

### Purpose

- preserve fast-gate outcomes
- support future deep validation records without schema redesign
- make blocked/flagged reasons auditable

### Proposed columns

- `id TEXT PRIMARY KEY`
- `attempt_id TEXT NOT NULL REFERENCES context_attempts(id) ON DELETE CASCADE`
- `gate_name TEXT NOT NULL`
- `decision TEXT NOT NULL`
- `reason TEXT`
- `score REAL`
- `details_json TEXT`
- `created_at TEXT NOT NULL DEFAULT (datetime('now'))`

### Constraints

`decision` should be constrained to:

- `passed`
- `flagged`
- `blocked`
- `deferred`

Phase 1 `gate_name` values likely include:

- `fast_gate`
- `malformed_check`
- `severity_check`
- `secret_leak_check`
- `emulation_check`

## Table 3: `context_attempt_jobs`

Async work queue authority for speculative-context follow-up work.

### Purpose

- queue deep validation
- queue expiration work
- provide retry-safe job state

### Proposed columns

- `id TEXT PRIMARY KEY`
- `attempt_id TEXT NOT NULL REFERENCES context_attempts(id) ON DELETE CASCADE`
- `job_type TEXT NOT NULL`
- `status TEXT NOT NULL`
- `retry_count INTEGER NOT NULL DEFAULT 0`
- `last_error TEXT`
- `run_after TEXT NOT NULL`
- `created_at TEXT NOT NULL DEFAULT (datetime('now'))`
- `updated_at TEXT NOT NULL DEFAULT (datetime('now'))`

### Constraints

`job_type` phase 1:

- `deep_validate`
- `expire`

Reserved for later:

- `embed`
- `promote`

`status`:

- `pending`
- `running`
- `succeeded`
- `failed`
- `dead_letter`

## Deferred Table: `context_attempt_promotion`

Not required for phase 1.

Reserve for future phase:

- `id TEXT PRIMARY KEY`
- `attempt_id TEXT NOT NULL`
- `promoted_memory_id TEXT NOT NULL`
- `promotion_type TEXT NOT NULL`
- `created_at TEXT NOT NULL`

This should land only when promotion is in scope.

## Index Plan

Indexes matter more than the base tables here because the hot path depends on bounded reads.

### `context_attempts`

Required indexes:

1. same-session retrieval

- `(session_id, status, expires_at, created_at DESC)`

Purpose:

- fetch retrievable, non-expired same-session entries quickly

2. agent/session cleanup

- `(agent_id, session_id, expires_at)`

Purpose:

- TTL cleanup and operational inspection

3. promotion candidates

- `(promotion_candidate, status, created_at)`

Purpose:

- later promotion worker lookup

4. turn uniqueness support

- `(session_id, turn_id, attempt_kind, created_at DESC)`

Purpose:

- duplicate suppression and per-turn inspection

### `context_attempt_validation`

Required indexes:

1. per-attempt audit trail

- `(attempt_id, created_at)`

2. gate lookup

- `(gate_name, decision, created_at)`

### `context_attempt_jobs`

Required indexes:

1. worker dequeue

- `(job_type, status, run_after, created_at)`

2. attempt-job lookup

- `(attempt_id, job_type, created_at)`

## Query Shapes

The schema is only valid if it supports the actual request and worker queries cheaply.

### Q1. Hot-path speculative retrieval

Inputs:

- `session_id`
- `now`
- bounded limit

Predicate:

- same session
- `status = retrievable`
- `expires_at > now`
- ordered by recency

Expected plan:

- satisfy from same-session retrieval index

### Q2. Expiration sweep

Inputs:

- `now`
- bounded batch size

Predicate:

- `status IN ('pending', 'retrievable', 'flagged')`
- `expires_at <= now`

Expected plan:

- partial scan aided by TTL-oriented index

### Q3. Worker dequeue

Inputs:

- `job_type`
- `now`

Predicate:

- `status = pending`
- `run_after <= now`

Expected plan:

- satisfy from worker dequeue index

### Q4. Duplicate suppression

Inputs:

- `session_id`
- `turn_id`
- `attempt_kind`

Predicate:

- recent same-turn attempts

Expected plan:

- satisfy from turn uniqueness support index

## TTL Strategy

TTL enforcement must exist in two layers:

1. request-time filtering
2. background cleanup

Background cleanup alone is not sufficient.

### Phase 1 TTL policy

For `summary` attempts:

- short TTL

This should be measured in minutes or hours, not days.

The exact value is a product/runtime policy decision, but the schema must support:

- request-time exclusion using `expires_at`
- later hard expiration via cleanup worker

### TTL rule

Expired entries:

- must not be retrievable even if cleanup has not run
- may remain in the table temporarily for cleanup batching

## Status Transition Storage Rules

The storage layer must preserve these invariants:

- blocked entries stay non-retrievable
- expired entries stay non-retrievable
- updates to `status` must also update `updated_at`
- every material status decision should also create a validation row

## Suggested SQL Shape

Illustrative only:

```sql
CREATE TABLE context_attempts (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    attempt_kind TEXT NOT NULL CHECK (
        attempt_kind IN ('summary')
    ),
    content TEXT NOT NULL,
    redacted_content TEXT,
    status TEXT NOT NULL CHECK (
        status IN ('pending', 'retrievable', 'flagged', 'blocked', 'promoted', 'expired')
    ),
    severity REAL NOT NULL DEFAULT 0.0,
    confidence REAL NOT NULL DEFAULT 0.0,
    retrieval_weight REAL NOT NULL DEFAULT 0.0,
    source_refs TEXT NOT NULL,
    source_hash BLOB,
    fast_gate_version INTEGER NOT NULL DEFAULT 1,
    contradicted_by_memory_id TEXT,
    promotion_candidate INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_context_attempts_session_status_expiry
    ON context_attempts(session_id, status, expires_at, created_at DESC);

CREATE INDEX idx_context_attempts_agent_session_expiry
    ON context_attempts(agent_id, session_id, expires_at);

CREATE INDEX idx_context_attempts_promotion
    ON context_attempts(promotion_candidate, status, created_at);

CREATE INDEX idx_context_attempts_turn_kind
    ON context_attempts(session_id, turn_id, attempt_kind, created_at DESC);
```

## Migration Notes

### Recommended migration implementation file

- `crates/cortex/cortex-storage/src/migrations/v060_speculative_context_phase1.rs`

### Required `mod.rs` changes

- add the new migration module
- bump `LATEST_VERSION`
- append the migration tuple

### Schema-contract follow-up

Once the migration is implemented, the schema contract should be updated so speculative-context tables are part of the verified schema authority rather than an undocumented extension.

## Capacity Planning Note

Phase 1 intentionally limits write amplification by allowing only one bounded summary attempt per successful turn.

### Expected write fanout per turn

Phase 1 rough upper bound:

- 1 row in `context_attempts`
- 1 to N rows in `context_attempt_validation`
- 1 to 2 rows in `context_attempt_jobs`

This is small enough to stay in the current SQLite authority if batching and indexes remain disciplined.

### What would make this design unsafe

- multiple speculative attempts per tool call
- unbounded per-turn validation row fanout
- cross-session speculative scans on the request path
- promotion tables added before phase 1 retrieval is proven

## Open Questions

- Should `source_refs` remain JSON text in phase 1 or be normalized immediately?
- Should phase 1 use hard delete or soft-expire plus later delete?
- Is a partial index on `status = 'retrievable'` worth it for SQLite performance here?
- Should blocked entries retain raw `content`, or should phase 1 immediately redact and retain only `redacted_content` plus audit detail?

## Final Position

This schema draft is intentionally conservative.

It favors:

- simple bounded reads
- strong request-time filtering
- low write fanout
- future promotion compatibility

That is the right trade for phase 1. The job now is not to model every future speculative-memory behavior. The job is to create the smallest storage shape that can prove:

- the hot path stays fast
- blocked content stays out
- same-session continuity improves
