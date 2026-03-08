//! Forward-only migrations with SQLite-consistent backup, maintenance lock,
//! post-migration schema verification, and DB-adjacent receipts.

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
pub mod v047_monitor_threshold_tables;
pub mod v048_audit_log_canonicalization;
pub mod v049_channels_canonicalization;
pub mod v050_profile_assignments;
pub mod v051_live_execution_records;
pub mod v052_skill_install_state;
pub mod v053_stream_event_log_unscoped;
pub mod v054_operation_journal_ownership;
pub mod v055_external_skill_pipeline;
pub mod v056_workflow_execution_contract;
pub mod v057_live_execution_contract;
pub mod v058_rate_limit_buckets;

use std::path::{Path, PathBuf};

use crate::schema_contract::require_schema_ready;
use crate::sqlite::{acquire_maintenance_lock, apply_writer_pragmas};
use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{Connection, DatabaseName};

pub const LATEST_VERSION: u32 = 58;

/// Maximum number of migration backup files to retain.
const MAX_MIGRATION_BACKUPS: usize = 3;

type MigrationFn = fn(&Connection) -> CortexResult<()>;

const MIGRATIONS: [(u32, &str, MigrationFn); 43] = [
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
    (
        47,
        "monitor_threshold_tables",
        v047_monitor_threshold_tables::migrate,
    ),
    (
        48,
        "audit_log_canonicalization",
        v048_audit_log_canonicalization::migrate,
    ),
    (
        49,
        "channels_canonicalization",
        v049_channels_canonicalization::migrate,
    ),
    (50, "profile_assignments", v050_profile_assignments::migrate),
    (
        51,
        "live_execution_records",
        v051_live_execution_records::migrate,
    ),
    (52, "skill_install_state", v052_skill_install_state::migrate),
    (
        53,
        "stream_event_log_unscoped",
        v053_stream_event_log_unscoped::migrate,
    ),
    (
        54,
        "operation_journal_ownership",
        v054_operation_journal_ownership::migrate,
    ),
    (
        55,
        "external_skill_pipeline",
        v055_external_skill_pipeline::migrate,
    ),
    (
        56,
        "workflow_execution_contract",
        v056_workflow_execution_contract::migrate,
    ),
    (
        57,
        "live_execution_contract",
        v057_live_execution_contract::migrate,
    ),
    (58, "rate_limit_buckets", v058_rate_limit_buckets::migrate),
];

/// Query the current schema version from the database.
/// Returns 0 if no migrations have been applied yet.
pub fn current_version(conn: &Connection) -> CortexResult<u32> {
    if !has_schema_version_table(conn)? {
        return Ok(0);
    }

    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
    .map_err(|error| to_storage_err(error.to_string()))
}

/// Run pending migrations without pre-migration backup or maintenance lock.
/// Intended for tests and in-memory databases.
pub fn run_migrations(conn: &Connection) -> CortexResult<()> {
    run_migrations_with_backup(conn, None)
}

/// Run migrations with optional SQLite-consistent pre-migration backup.
///
/// When `db_path` is `Some`, the migration run is treated as a maintenance
/// operation: a DB-adjacent maintenance lock is acquired, a SQLite backup
/// snapshot is created before any schema change, and a JSON receipt is written
/// after post-migration schema verification succeeds.
pub fn run_migrations_with_backup(conn: &Connection, db_path: Option<&Path>) -> CortexResult<()> {
    apply_writer_pragmas(conn).map_err(|error| to_storage_err(error.to_string()))?;

    let current = current_version(conn)?;
    if current > LATEST_VERSION {
        return Err(to_storage_err(format!(
            "unsupported newer schema: database is v{current}, binary supports up to v{LATEST_VERSION}"
        )));
    }

    let pending: Vec<(u32, &str, MigrationFn)> = MIGRATIONS
        .iter()
        .filter(|(version, _, _)| *version > current)
        .map(|(version, name, migrate_fn)| (*version, *name, *migrate_fn))
        .collect();

    if pending.is_empty() {
        require_schema_ready(conn).map_err(|error| to_storage_err(error.to_string()))?;
        return Ok(());
    }

    let _maintenance_lock = if let Some(path) = db_path {
        Some(acquire_maintenance_lock(path).map_err(|error| to_storage_err(error.to_string()))?)
    } else {
        None
    };

    let first_pending_version = pending[0].0;
    let last_pending_version = pending.last().expect("pending not empty").0;
    let backup_path = if let Some(path) = db_path {
        Some(create_pre_migration_backup(
            conn,
            path,
            first_pending_version,
            current,
            last_pending_version,
            pending.len(),
        )?)
    } else {
        None
    };

    ensure_schema_version_table(conn)?;

    let mut applied = Vec::new();
    for (version, name, migrate_fn) in &pending {
        tracing::info!(version, name, "running migration");
        conn.execute_batch("BEGIN IMMEDIATE").map_err(|error| {
            to_storage_err(format!("begin transaction for v{version}: {error}"))
        })?;

        match migrate_fn(conn) {
            Ok(()) => {
                conn.execute(
                    "INSERT INTO schema_version (version, name) VALUES (?1, ?2)",
                    rusqlite::params![version, name],
                )
                .map_err(|error| {
                    let _ = conn.execute_batch("ROLLBACK");
                    migration_failure(
                        *version,
                        name,
                        &backup_path,
                        format!("record migration v{version}: {error}"),
                    )
                })?;
                conn.execute_batch("COMMIT").map_err(|error| {
                    to_storage_err(format!("commit migration v{version}: {error}"))
                })?;
                applied.push((*version, (*name).to_string()));
            }
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(migration_failure(
                    *version,
                    name,
                    &backup_path,
                    format!("migration v{version} ({name}) failed: {error}"),
                ));
            }
        }
    }

    let final_version = current_version(conn)?;
    if final_version != last_pending_version {
        return Err(to_storage_err(format!(
            "post-migration version mismatch: expected v{last_pending_version}, got v{final_version}"
        )));
    }

    require_schema_ready(conn).map_err(|error| {
        to_storage_err(format!(
            "post-migration schema verification failed at v{final_version}: {error}"
        ))
    })?;

    if let Some(path) = db_path {
        write_migration_receipt(
            path,
            current,
            final_version,
            &applied,
            backup_path.as_deref(),
        )?;
        cleanup_old_backups(path);
    }

    tracing::info!(
        from_version = current,
        to_version = final_version,
        applied = applied.len(),
        "migrations completed and schema verified",
    );

    Ok(())
}

fn ensure_schema_version_table(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|error| to_storage_err(error.to_string()))
}

pub(crate) fn materialize_latest_schema_reference(conn: &Connection) -> CortexResult<()> {
    ensure_schema_version_table(conn)?;
    for (version, name, migrate_fn) in &MIGRATIONS {
        migrate_fn(conn)?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version, name) VALUES (?1, ?2)",
            rusqlite::params![version, name],
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    }
    Ok(())
}

fn has_schema_version_table(conn: &Connection) -> CortexResult<bool> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version' LIMIT 1",
        [],
        |_row| Ok(()),
    )
    .map(|_| true)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(false),
        other => Err(other),
    })
    .map_err(|error| to_storage_err(error.to_string()))
}

fn create_pre_migration_backup(
    conn: &Connection,
    db_path: &Path,
    first_pending_version: u32,
    current: u32,
    target: u32,
    pending_count: usize,
) -> CortexResult<PathBuf> {
    let backup = backup_path(db_path, first_pending_version);
    let _ = std::fs::remove_file(&backup);

    tracing::info!(
        from_version = current,
        to_version = target,
        backup = %backup.display(),
        "creating SQLite-consistent pre-migration backup before running {pending_count} pending migration(s)",
    );

    conn.backup(DatabaseName::Main, &backup, None)
        .map_err(|error| {
            to_storage_err(format!(
                "failed to create SQLite backup at {}: {error}",
                backup.display()
            ))
        })?;

    let backup_conn = Connection::open(&backup).map_err(|error| {
        to_storage_err(format!(
            "failed to open SQLite backup {}: {error}",
            backup.display()
        ))
    })?;
    let integrity: String = backup_conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| {
            to_storage_err(format!(
                "failed to verify SQLite backup {}: {error}",
                backup.display()
            ))
        })?;
    if integrity != "ok" {
        return Err(to_storage_err(format!(
            "SQLite backup integrity check failed for {}: {integrity}",
            backup.display()
        )));
    }

    Ok(backup)
}

fn backup_path(db_path: &Path, first_pending_version: u32) -> PathBuf {
    PathBuf::from(format!(
        "{}.pre-migration-v{first_pending_version}.bak",
        db_path.display()
    ))
}

fn write_migration_receipt(
    db_path: &Path,
    from_version: u32,
    to_version: u32,
    applied: &[(u32, String)],
    backup_path: Option<&Path>,
) -> CortexResult<()> {
    let receipt_dir = receipt_dir(db_path);
    std::fs::create_dir_all(&receipt_dir).map_err(|error| {
        to_storage_err(format!(
            "create migration receipt dir {}: {error}",
            receipt_dir.display()
        ))
    })?;

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let receipt_path = receipt_dir.join(format!("{to_version}_{timestamp}.json"));
    let payload = serde_json::json!({
        "db_path": db_path.display().to_string(),
        "from_version": from_version,
        "to_version": to_version,
        "applied_migrations": applied
            .iter()
            .map(|(version, name)| serde_json::json!({ "version": version, "name": name }))
            .collect::<Vec<_>>(),
        "backup_path": backup_path.map(|path| path.display().to_string()),
        "completed_at": chrono::Utc::now().to_rfc3339(),
    });

    let serialized = serde_json::to_vec_pretty(&payload)
        .map_err(|error| to_storage_err(format!("serialize migration receipt: {error}")))?;
    std::fs::write(&receipt_path, serialized).map_err(|error| {
        to_storage_err(format!(
            "write migration receipt {}: {error}",
            receipt_path.display()
        ))
    })?;
    tracing::info!(path = %receipt_path.display(), "wrote migration receipt");
    Ok(())
}

fn receipt_dir(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.migration-receipts", db_path.display()))
}

fn migration_failure(
    version: u32,
    name: &str,
    backup_path: &Option<PathBuf>,
    message: String,
) -> cortex_core::models::error::CortexError {
    if let Some(backup) = backup_path {
        tracing::error!(
            version,
            name,
            backup = %backup.display(),
            "migration failed; restore from SQLite backup {}",
            backup.display(),
        );
        return to_storage_err(format!(
            "{message}. Pre-migration SQLite backup available at: {}",
            backup.display()
        ));
    }

    to_storage_err(message)
}

/// Remove old `.pre-migration-v*.bak` files, keeping only the most recent ones.
fn cleanup_old_backups(db_path: &Path) {
    let parent = match db_path.parent() {
        Some(parent) => parent,
        None => return,
    };
    let stem = match db_path.file_name().and_then(|file| file.to_str()) {
        Some(stem) => stem,
        None => return,
    };

    let prefix = format!("{stem}.pre-migration-v");
    let mut backups: Vec<PathBuf> = match std::fs::read_dir(parent) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|file| file.to_str())
                    .map(|file| file.starts_with(&prefix) && file.ends_with(".bak"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => return,
    };

    if backups.len() <= MAX_MIGRATION_BACKUPS {
        return;
    }

    backups.sort_by(|left, right| {
        let left_time = left.metadata().and_then(|meta| meta.modified()).ok();
        let right_time = right.metadata().and_then(|meta| meta.modified()).ok();
        left_time.cmp(&right_time)
    });

    let to_remove = backups.len() - MAX_MIGRATION_BACKUPS;
    for path in backups.into_iter().take(to_remove) {
        tracing::debug!(path = %path.display(), "removing old migration backup");
        let _ = std::fs::remove_file(&path);
    }
}
