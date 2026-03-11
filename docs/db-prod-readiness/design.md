# Database Production Safety Design

## Decision

This database setup is not production-ready until schema ownership, migration execution, and SQLite operational policy are made explicit and fail-closed.

The required end state is simple:

- one schema authority
- no runtime DDL for persistent objects
- `migration complete` means `schema ready`
- startup refuses incomplete, older, or newer schema
- backups are SQLite-consistent
- writers run with durable settings
- partial deploys and version skew fail closed

## Hard Rules

### 1. One authority

All persistent tables, indexes, and triggers are owned by canonical migrations in:

- `crates/cortex/cortex-storage/src/migrations`

Runtime code may verify schema. Runtime code may not create schema.

### 2. Schema ready is a contract

`schema_version == LATEST_VERSION` is not enough.

A database is ready only if all required:

- tables
- columns
- indexes
- triggers

exist and integrity checks pass.

### 3. Production startup is verify-only

Production startup must:

- open DB
- read schema version
- reject newer schema than the binary understands
- reject older schema until explicit migration runs
- run schema verification
- exit on any failure

Production startup must not run schema DDL.

### 4. Migrations are maintenance operations

Schema-changing releases are not normal startup behavior.

They require:

- writers stopped
- maintenance lock acquired
- SQLite-consistent pre-migration backup
- forward-only migration run
- post-migration verification
- receipt written

### 5. Rollback means restore

For schema-changing releases, rollback is:

- stop writers
- restore pre-migration backup
- start older binaries

Do not pretend application rollback alone is safe.

### 6. Durability wins

All write-capable connections must use:

- `journal_mode=WAL`
- `busy_timeout=5000`
- `foreign_keys=ON`
- `synchronous=FULL`

This system stores append-only audit trails, hash chains, and intervention state. Acknowledged-write loss is not acceptable.

## Proven Defects To Remove

- `audit_log` is split between migrations and runtime `ensure_table()`
- `monitor_threshold_config` and `monitor_threshold_history` are runtime-owned
- `channels` is created both in migrations and lazily in request handling
- migration backup currently relies on file-copy semantics instead of SQLite backup semantics
- `ghost db status` can say "up to date" while required schema is missing
- monitor config can drift from gateway config
- monitor health can stay green after DB persistence failure
- runtime code mutates append-only `convergence_scores`

## Required Architecture

### Canonical migration adoption

Do not rewrite historical migrations. Add new forward-only migrations to:

- adopt monitor threshold tables
- canonicalize `audit_log`
- canonicalize `channels`
- introduce a proper mutable table for profile assignment

If an on-disk legacy shape is unsupported, fail loudly. Do not silently guess.

### Shared schema verifier

Add a verifier in `cortex-storage` that is the production definition of `schema ready`.

It must be used by:

- gateway bootstrap
- monitor startup
- `ghost db status`
- a strict `ghost db verify` command

### Shared connection policy

Centralize writer connection setup so gateway, monitor, CLI, and any remaining legacy path all apply the same pragmas and timeouts.

### Maintenance lock

Add a DB-adjacent maintenance lock so migrations and restarts cannot race.

### Migration receipts

Every migration writes a JSON receipt next to the database:

- directory: `{db_path}.migration-receipts/`
- filename: `{target_version}_{timestamp}.json`

## Exact Code Areas

- `crates/cortex/cortex-storage/src/migrations/mod.rs`
- `crates/cortex/cortex-storage/src/migrations/v047_*`
- `crates/cortex/cortex-storage/src/migrations/v048_*`
- `crates/cortex/cortex-storage/src/migrations/v049_*`
- `crates/cortex/cortex-storage/src/migrations/v050_*`
- `crates/cortex/cortex-storage/src/schema_contract.rs` (new)
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/cli/db.rs`
- `crates/ghost-gateway/src/db_pool.rs`
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

## Ship Gate

Safe to ship only when all of the following are true:

- no production code path creates persistent schema at runtime
- schema verification is strict and startup-blocking
- migration backup is SQLite-consistent
- all writers use `synchronous=FULL`
- monitor and gateway share the same DB authority
- append-only tables remain append-only
- restore from backup has been exercised, not assumed
