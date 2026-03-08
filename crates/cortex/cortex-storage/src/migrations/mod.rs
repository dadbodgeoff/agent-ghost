//! Forward-only migrations with pre-migration backup safety net.
//!
//! Before running pending migrations, the DB file is copied to a backup.
//! On failure, the backup path is logged so the operator can restore.
//! Last 3 migration backups are retained; older ones are cleaned up.

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
pub mod v031_fts5_search;
pub mod v032_embeddings;
pub mod v033_bundled_skill_tables;
pub mod v034_pc_control_actions;
pub mod v035_convergence_links;
pub mod v036_citation_count;
pub mod v037_studio_chat_tables;
pub mod v038_marketplace;
pub mod v039_stream_event_log;
pub mod v040_phase3_tables;
pub mod v041_revoked_tokens;
pub mod v042_cost_snapshots;
pub mod v043_session_lifecycle;
pub mod v044_studio_session_agent_id;
pub mod v045_operation_journal;
pub mod v046_goal_proposal_v2;

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub const LATEST_VERSION: u32 = 46;

/// Maximum number of migration backup files to retain.
const MAX_MIGRATION_BACKUPS: usize = 3;

type MigrationFn = fn(&Connection) -> CortexResult<()>;

const MIGRATIONS: [(u32, &str, MigrationFn); 31] = [
    (16, "convergence_safety", v016_convergence_safety::migrate),
    (17, "convergence_tables", v017_convergence_tables::migrate),
    (18, "delegation_state", v018_delegation_state::migrate),
    (19, "intervention_state", v019_intervention_state::migrate),
    (20, "actor_id", v020_actor_id::migrate),
    (21, "workflows", v021_workflows::migrate),
    (22, "session_event_index", v022_session_event_index::migrate),
    (23, "otel_spans", v023_otel_spans::migrate),
    (24, "backup_manifest", v024_backup_manifest::migrate),
    (
        25,
        "convergence_profiles",
        v025_convergence_profiles::migrate,
    ),
    (26, "webhooks", v026_webhooks::migrate),
    (27, "installed_skills", v027_installed_skills::migrate),
    (28, "a2a_tasks", v028_a2a_tasks::migrate),
    (29, "archival", v029_archival::migrate),
    (30, "memory_compaction", v030_memory_compaction::migrate),
    (31, "fts5_search", v031_fts5_search::migrate),
    (32, "embeddings", v032_embeddings::migrate),
    (
        33,
        "bundled_skill_tables",
        v033_bundled_skill_tables::migrate,
    ),
    (34, "pc_control_actions", v034_pc_control_actions::migrate),
    (35, "convergence_links", v035_convergence_links::migrate),
    (36, "citation_count", v036_citation_count::migrate),
    (37, "studio_chat_tables", v037_studio_chat_tables::migrate),
    (38, "marketplace", v038_marketplace::migrate),
    (39, "stream_event_log", v039_stream_event_log::migrate),
    (40, "phase3_tables", v040_phase3_tables::migrate),
    (41, "revoked_tokens", v041_revoked_tokens::migrate),
    (42, "cost_snapshots", v042_cost_snapshots::migrate),
    (43, "session_lifecycle", v043_session_lifecycle::migrate),
    (
        44,
        "studio_session_agent_id",
        v044_studio_session_agent_id::migrate,
    ),
    (45, "operation_journal", v045_operation_journal::migrate),
    (46, "goal_proposal_v2", v046_goal_proposal_v2::migrate),
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
        );",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
    .map_err(|e| to_storage_err(e.to_string()))
}

/// Ensure the schema_version table exists and run pending migrations.
///
/// If `db_path` is provided and there are pending migrations, a backup of the
/// DB file is created before any migration runs. On failure, the error message
/// includes the backup path for manual restoration.
pub fn run_migrations(conn: &Connection) -> CortexResult<()> {
    run_migrations_with_backup(conn, None)
}

/// Run migrations with optional pre-migration backup.
///
/// When `db_path` is `Some`, the DB file is copied to
/// `{db_path}.pre-migration-v{first_pending}.bak` before running any
/// pending migration. After all migrations succeed, `user_version` is
/// verified. Old backups beyond the retention limit are cleaned up.
pub fn run_migrations_with_backup(
    conn: &Connection,
    db_path: Option<&std::path::Path>,
) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    // Collect pending migrations.
    let pending: Vec<_> = MIGRATIONS
        .iter()
        .filter(|(version, _, _)| *version > current)
        .collect();

    if pending.is_empty() {
        return Ok(());
    }

    let first_pending_version = pending[0].0;
    let last_pending_version = pending.last().unwrap().0;

    // Create pre-migration backup if db_path is provided.
    let backup_path = if let Some(path) = db_path {
        let backup = path.with_extension(format!("pre-migration-v{first_pending_version}.bak"));
        tracing::info!(
            from_version = current,
            to_version = last_pending_version,
            backup = %backup.display(),
            "Creating pre-migration backup before running {} pending migration(s)",
            pending.len(),
        );
        if let Err(e) = std::fs::copy(path, &backup) {
            return Err(to_storage_err(format!(
                "failed to create pre-migration backup at {}: {e}",
                backup.display()
            )));
        }
        // Also copy WAL and SHM files if they exist (ensures backup is consistent).
        let wal = path.with_extension("db-wal");
        if wal.exists() {
            let _ = std::fs::copy(&wal, backup.with_extension("bak-wal"));
        }
        let shm = path.with_extension("db-shm");
        if shm.exists() {
            let _ = std::fs::copy(&shm, backup.with_extension("bak-shm"));
        }
        Some(backup)
    } else {
        None
    };

    // Run each pending migration.
    for &(version, name, migrate_fn) in &pending {
        tracing::info!(version, name, "running migration");
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| to_storage_err(format!("begin transaction for v{version}: {e}")))?;
        match migrate_fn(conn) {
            Ok(()) => {
                conn.execute(
                    "INSERT INTO schema_version (version, name) VALUES (?1, ?2)",
                    rusqlite::params![version, name],
                )
                .map_err(|e| {
                    let _ = conn.execute_batch("ROLLBACK");
                    let msg = format!("record migration v{version}: {e}");
                    if let Some(ref bp) = backup_path {
                        to_storage_err(format!(
                            "{msg}. Pre-migration backup available at: {}",
                            bp.display()
                        ))
                    } else {
                        to_storage_err(msg)
                    }
                })?;
                conn.execute_batch("COMMIT")
                    .map_err(|e| to_storage_err(format!("commit migration v{version}: {e}")))?;
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                let msg = format!("migration v{version} ({name}) failed: {e}");
                if let Some(ref bp) = backup_path {
                    tracing::error!(
                        version,
                        name,
                        backup = %bp.display(),
                        "Migration failed. Restore from backup: {}",
                        bp.display(),
                    );
                    return Err(to_storage_err(format!(
                        "{msg}. Pre-migration backup available at: {}",
                        bp.display()
                    )));
                }
                return Err(to_storage_err(msg));
            }
        }
    }

    // Post-migration verification: ensure schema_version matches expected.
    let final_version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    if final_version != last_pending_version {
        let msg = format!(
            "post-migration version mismatch: expected v{last_pending_version}, got v{final_version}"
        );
        if let Some(ref bp) = backup_path {
            tracing::error!(
                expected = last_pending_version,
                actual = final_version,
                backup = %bp.display(),
                "Post-migration verification failed. Restore from backup: {}",
                bp.display(),
            );
        }
        return Err(to_storage_err(msg));
    }

    tracing::info!(
        from_version = current,
        to_version = final_version,
        "All {} migration(s) completed successfully",
        pending.len(),
    );

    // Cleanup old migration backups — retain only the last MAX_MIGRATION_BACKUPS.
    if let Some(path) = db_path {
        cleanup_old_backups(path);
    }

    Ok(())
}

/// Remove old `.pre-migration-v*.bak` files, keeping only the most recent ones.
fn cleanup_old_backups(db_path: &std::path::Path) {
    let parent = match db_path.parent() {
        Some(p) => p,
        None => return,
    };
    let stem = match db_path.file_name().and_then(|f| f.to_str()) {
        Some(s) => s,
        None => return,
    };

    let prefix = format!("{stem}.pre-migration-v");
    let mut backups: Vec<std::path::PathBuf> = match std::fs::read_dir(parent) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|f| f.to_str())
                    .map(|f| f.starts_with(&prefix) && f.ends_with(".bak"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => return,
    };

    if backups.len() <= MAX_MIGRATION_BACKUPS {
        return;
    }

    // Sort by modification time (oldest first).
    backups.sort_by(|a, b| {
        let ta = a.metadata().and_then(|m| m.modified()).ok();
        let tb = b.metadata().and_then(|m| m.modified()).ok();
        ta.cmp(&tb)
    });

    let to_remove = backups.len() - MAX_MIGRATION_BACKUPS;
    for path in backups.into_iter().take(to_remove) {
        tracing::debug!(path = %path.display(), "removing old migration backup");
        let _ = std::fs::remove_file(&path);
        // Also remove companion WAL/SHM backup files.
        let _ = std::fs::remove_file(path.with_extension("bak-wal"));
        let _ = std::fs::remove_file(path.with_extension("bak-shm"));
    }
}
