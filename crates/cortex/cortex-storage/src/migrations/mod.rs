//! Forward-only migrations. No down/rollback.

pub mod v016_convergence_safety;
pub mod v017_convergence_tables;
pub mod v018_delegation_state;
pub mod v019_intervention_state;
pub mod v020_actor_id;
pub mod v021_workflows;
pub mod v022_session_event_index;
pub mod v023_otel_spans;
pub mod v024_backup_manifest;
pub mod v025_convergence_profiles;
pub mod v026_webhooks;
pub mod v027_installed_skills;
pub mod v028_a2a_tasks;
pub mod v029_archival;
pub mod v030_memory_compaction;

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub const LATEST_VERSION: u32 = 30;

type MigrationFn = fn(&Connection) -> CortexResult<()>;

const MIGRATIONS: [(u32, &str, MigrationFn); 15] = [
    (16, "convergence_safety", v016_convergence_safety::migrate),
    (17, "convergence_tables", v017_convergence_tables::migrate),
    (18, "delegation_state", v018_delegation_state::migrate),
    (19, "intervention_state", v019_intervention_state::migrate),
    (20, "actor_id", v020_actor_id::migrate),
    (21, "workflows", v021_workflows::migrate),
    (22, "session_event_index", v022_session_event_index::migrate),
    (23, "otel_spans", v023_otel_spans::migrate),
    (24, "backup_manifest", v024_backup_manifest::migrate),
    (25, "convergence_profiles", v025_convergence_profiles::migrate),
    (26, "webhooks", v026_webhooks::migrate),
    (27, "installed_skills", v027_installed_skills::migrate),
    (28, "a2a_tasks", v028_a2a_tasks::migrate),
    (29, "archival", v029_archival::migrate),
    (30, "memory_compaction", v030_memory_compaction::migrate),
];

/// Query the current schema version from the database.
/// Returns 0 if no migrations have been applied yet.
pub fn current_version(conn: &Connection) -> CortexResult<u32> {
    // Ensure the table exists before querying.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );"
    ).map_err(|e| to_storage_err(e.to_string()))?;

    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
    .map_err(|e| to_storage_err(e.to_string()))
}

/// Ensure the schema_version table exists and run pending migrations.
pub fn run_migrations(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );"
    ).map_err(|e| to_storage_err(e.to_string()))?;

    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    for &(version, name, migrate_fn) in &MIGRATIONS {
        if version > current {
            tracing::info!(version, name, "running migration");
            // Wrap migration + version record in a transaction for atomicity.
            // If the migration succeeds but the version INSERT fails (e.g., disk full),
            // the entire migration is rolled back, preventing partial application
            // that would cause ALTER TABLE failures on re-run.
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| to_storage_err(format!("begin transaction for v{version}: {e}")))?;
            match migrate_fn(conn) {
                Ok(()) => {
                    conn.execute(
                        "INSERT INTO schema_version (version, name) VALUES (?1, ?2)",
                        rusqlite::params![version, name],
                    )
                    .map_err(|e| {
                        // Rollback on version INSERT failure.
                        let _ = conn.execute_batch("ROLLBACK");
                        to_storage_err(format!("record migration v{version}: {e}"))
                    })?;
                    conn.execute_batch("COMMIT")
                        .map_err(|e| to_storage_err(format!("commit migration v{version}: {e}")))?;
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}
