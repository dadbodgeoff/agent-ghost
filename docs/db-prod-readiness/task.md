# Task: Make Database Migrations and SQLite Operations Production-Safe

## Mission

Implement the database hardening required to make this repo safe for production under partial deploys, restarts, rollback pressure, and version skew.

Do not do a general cleanup pass. Do not widen scope. Close the production DB risks only.

Primary design reference:

- `/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/db-prod-readiness/design.md`

## Operating Standard

Work like the tests are trying to trick you.

- do not trust `schema_version` alone
- do not trust passing tests that bypass canonical migrations
- do not preserve convenience runtime DDL for persistent tables
- do not silently accept drifted legacy shapes you cannot prove safe
- fail closed instead of guessing

The correct bar is deterministic, auditable, forward-only, and durable.

## Non-Negotiables

- do not renumber or rewrite historical migrations
- do not add best-effort startup repair for production schema
- do not leave `synchronous=NORMAL` on any write-capable connection
- do not ship if `migration complete` can still mean `schema incomplete`
- do not keep lazy runtime creation for `audit_log`, `channels`, or monitor threshold tables
- do not mutate append-only `convergence_scores`

## Scope

In scope:

- schema ownership and migration authority
- adoption of runtime-created persistent tables into canonical migrations
- bootstrap-time mutation vs migration-time mutation
- SQLite backup, restore, checkpoint, and writer settings
- startup verification and version-skew failure behavior
- drift between runtime code, tests, and migration history

Out of scope:

- changing away from SQLite
- unrelated API, frontend, or product work
- broad refactors not needed for DB safety

## Required Deliverables

### D1. Canonical schema verifier

Add a shared verifier in `cortex-storage` that checks required:

- tables
- columns
- indexes
- triggers

and exposes a strict readiness result.

Use it from:

- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/cli/db.rs`
- `crates/convergence-monitor/src/monitor.rs`

Expected behavior:

- older schema than binary: fail startup, migration required
- newer schema than binary: fail startup
- latest version but missing required objects: fail startup and fail CLI verify/status

### D2. Forward-only adoption migrations

Add new migrations after current latest (`v046` at time of writing; if that changes, use the next free versions).

Required migration intents:

- adopt `monitor_threshold_config`
- adopt `monitor_threshold_history`
- canonicalize `audit_log`
- canonicalize `channels`
- add a mutable profile assignment table

Rules:

- preserve forward-only history
- support proven legacy shapes only
- fail with explicit error on unsupported shapes
- create all missing indexes required by the canonical contract

### D3. Remove runtime DDL for persistent objects

Delete production runtime schema creation from:

- `crates/ghost-audit/src/query_engine.rs`
- `crates/ghost-gateway/src/api/channels.rs`
- `crates/convergence-monitor/src/monitor.rs`

Replacement behavior:

- verify schema or fail
- never `CREATE TABLE IF NOT EXISTS` for long-lived production tables during normal runtime

### D4. Make migration and backup semantics real

Replace rollback-grade backup paths with SQLite-consistent backup behavior.

At minimum:

- migration backup must use SQLite backup API or SQLite-driven snapshot semantics
- CLI migration must use the same safety path
- admin/scheduled backup paths must not imply restore-grade DB safety if they are just file packaging

Add migration receipts at:

- `{db_path}.migration-receipts/{target_version}_{timestamp}.json`

### D5. Centralize SQLite writer policy

All write-capable connections must apply:

- `journal_mode=WAL`
- `busy_timeout=5000`
- `foreign_keys=ON`
- `synchronous=FULL`

This includes:

- gateway writer
- monitor writer
- migration CLI
- any remaining legacy write path

### D6. Repair runtime/schema contract drift

Required fixes:

- stop updating `convergence_scores` in `crates/ghost-gateway/src/api/profiles.rs`
- write profile assignment to the new mutable table instead
- make audit writers populate `actor_id` as a real column in `crates/ghost-gateway/src/api/mutation.rs`

### D7. Monitor correctness

The monitor must:

- load the same config source as the gateway
- resolve the same DB path as the gateway
- fail closed on schema readiness failure
- report unhealthy when DB persistence is failing

## Exact Files To Inspect and Change

- `crates/cortex/cortex-storage/src/migrations/mod.rs`
- `crates/cortex/cortex-storage/src/migrations`
- `crates/cortex/cortex-storage/tests/migration_tests.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/db_pool.rs`
- `crates/ghost-gateway/src/cli/db.rs`
- `crates/ghost-gateway/src/api/channels.rs`
- `crates/ghost-gateway/src/api/profiles.rs`
- `crates/ghost-gateway/src/api/mutation.rs`
- `crates/ghost-audit/src/query_engine.rs`
- `crates/convergence-monitor/src/config.rs`
- `crates/convergence-monitor/src/monitor.rs`
- `crates/convergence-monitor/src/transport/http_api.rs`
- `crates/ghost-backup/src/export.rs`
- `crates/ghost-gateway/src/api/admin.rs`
- `crates/ghost-gateway/src/backup_scheduler.rs`

## Adversarial Acceptance Tests

These are not optional. If one fails, the work is not done.

### Schema authority

- fresh DB created only through canonical migrations contains all required production objects
- latest-version DB missing `audit_log` fails verification
- latest-version DB missing canonical `channels` indexes fails verification
- production bootstrap against a ready DB performs no schema DDL

### Legacy adoption

- DB with runtime-created `audit_log` shape migrates to canonical shape
- DB with migration-era `audit_log` plus missing later columns migrates to canonical shape
- DB with runtime-created monitor threshold tables migrates to canonical shape
- DB with runtime-created `channels` but missing indexes migrates to canonical shape
- unsupported legacy shapes fail loudly and precisely

### SQLite durability and recovery

- migration backup produces a restorable DB under WAL mode
- restore test proves data correctness after restore, not just successful open
- migration receipt is written to the DB-adjacent receipt directory
- all write-capable connections run with `synchronous=FULL`

### Version skew and startup

- old binary against newer schema fails closed
- new binary against older schema fails with migration-required path
- monitor on non-default config uses the same DB path as gateway
- startup fails if maintenance lock is held

### Data contracts

- profile assignment updates the new mutable table and does not mutate `convergence_scores`
- authenticated audit writes populate `actor_id`
- monitor health degrades on DB write failure
- `ghost db status` and `ghost db verify` fail on missing required objects instead of printing fake zero counts

## Execution Order

Do this in order.

1. Add schema verifier and readiness contract.
2. Add forward-only adoption migrations.
3. Remove runtime DDL and switch callers to verification.
4. Harden migration backup, maintenance lock, and receipts.
5. Centralize writer pragmas and enforce `synchronous=FULL`.
6. Fix profile assignment and audit writer contracts.
7. Fix monitor config and health behavior.
8. Add and run adversarial tests.

Do not start with test cleanup. Lock the production contract first, then make tests prove it.

## Definition of Done

Done means all of the following are true:

- one canonical migration authority owns all long-lived persisted schema
- production startup is verify-only and fail-closed
- migration backup is SQLite-consistent
- every writer uses `synchronous=FULL`
- runtime code no longer creates persistent schema
- append-only tables remain append-only
- monitor and gateway cannot silently drift to different DBs
- adversarial tests cover legacy shapes, missing objects, version skew, backup restore, and health degradation

## Final Instruction

Prefer explicit failure over compatibility theater.

If you hit an existing on-disk shape you cannot prove safe, stop and fail with a precise error. That is the correct production behavior.
