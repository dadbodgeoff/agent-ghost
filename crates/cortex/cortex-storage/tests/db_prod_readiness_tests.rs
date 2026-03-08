use std::path::{Path, PathBuf};

use cortex_storage::migrations::{
    self, v016_convergence_safety, v017_convergence_tables, v018_delegation_state,
    v019_intervention_state, v020_actor_id, v021_workflows, v022_session_event_index,
    v023_otel_spans, v024_backup_manifest, v025_convergence_profiles, v026_webhooks,
    v027_installed_skills, v028_a2a_tasks, v029_archival, v030_memory_compaction, v031_fts5_search,
    v032_embeddings, v033_bundled_skill_tables, v034_pc_control_actions, v035_convergence_links,
    v036_citation_count, v037_studio_chat_tables, v038_marketplace, v039_stream_event_log,
    v040_phase3_tables, v041_revoked_tokens, v042_cost_snapshots, v043_session_lifecycle,
    v044_studio_session_agent_id, v045_operation_journal, v046_goal_proposal_v2,
};
use cortex_storage::schema_contract::{require_schema_ready, SchemaContractError};
use cortex_storage::sqlite::apply_writer_pragmas;
use rusqlite::{Connection, DatabaseName};

type LegacyMigration = fn(&Connection) -> cortex_core::models::error::CortexResult<()>;

fn open_db(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    apply_writer_pragmas(&conn).unwrap();
    conn
}

fn setup_db_to_v46(path: &Path) -> Connection {
    let conn = open_db(path);
    conn.execute_batch(
        "CREATE TABLE schema_version (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();

    let legacy: &[(u32, &str, LegacyMigration)] = &[
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

    for (version, name, migrate_fn) in legacy {
        migrate_fn(&conn).unwrap();
        conn.execute(
            "INSERT INTO schema_version (version, name) VALUES (?1, ?2)",
            rusqlite::params![version, name],
        )
        .unwrap();
    }

    conn
}

fn receipt_dir(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.migration-receipts", path.display()))
}

#[test]
fn fresh_db_from_canonical_migrations_is_schema_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("fresh.db");
    let conn = open_db(&db_path);
    cortex_storage::run_all_migrations(&conn).unwrap();

    let report = require_schema_ready(&conn).unwrap();
    assert_eq!(report.current_version, migrations::LATEST_VERSION);
}

#[test]
fn latest_version_db_missing_audit_log_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch("DROP TABLE audit_log;").unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(err.to_string().contains("missing table audit_log"), "{err}");
}

#[test]
fn latest_version_db_missing_latest_migration_table_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch("DROP TABLE rate_limit_buckets;")
        .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string().contains("missing table rate_limit_buckets"),
        "{err}"
    );
}

#[test]
fn latest_version_db_missing_channels_indexes_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch("DROP INDEX idx_channels_agent;")
        .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string().contains("missing index idx_channels_agent"),
        "{err}"
    );
}

#[test]
fn latest_version_db_missing_workflow_execution_state_version_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch(
        "DROP TABLE workflow_executions;
         CREATE TABLE workflow_executions (
            id TEXT PRIMARY KEY,
            state TEXT NOT NULL DEFAULT '{}',
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )
    .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string()
            .contains("table workflow_executions missing column state_version"),
        "{err}"
    );
}

#[test]
fn latest_version_db_missing_live_execution_state_version_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch(
        "DROP TABLE live_execution_records;
         CREATE TABLE live_execution_records (
            id TEXT PRIMARY KEY,
            journal_id TEXT NOT NULL UNIQUE,
            operation_id TEXT NOT NULL UNIQUE,
            route_kind TEXT NOT NULL,
            actor_key TEXT NOT NULL,
            status TEXT NOT NULL,
            state_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )
    .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string()
            .contains("table live_execution_records missing column state_version"),
        "{err}"
    );
}

#[test]
fn latest_version_db_with_non_unique_workflow_execution_journal_index_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch(
        "DROP INDEX idx_workflow_executions_journal_id;
         CREATE INDEX idx_workflow_executions_journal_id
            ON workflow_executions(journal_id);",
    )
    .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string().contains(
            "index idx_workflow_executions_journal_id missing contract workflow execution journal uniqueness"
        ),
        "{err}"
    );
}

#[test]
fn latest_version_db_missing_operation_journal_owner_contract_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS prevent_operation_journal_delete;
         DROP TRIGGER IF EXISTS operation_journal_commit_requires_current_request;
         DROP TABLE operation_journal;
         CREATE TABLE operation_journal (
            id TEXT PRIMARY KEY,
            actor_key TEXT NOT NULL,
            method TEXT NOT NULL,
            route_template TEXT NOT NULL,
            operation_id TEXT NOT NULL,
            request_id TEXT,
            idempotency_key TEXT NOT NULL,
            request_fingerprint TEXT NOT NULL,
            request_body TEXT NOT NULL DEFAULT 'null',
            status TEXT NOT NULL CHECK(status IN ('in_progress', 'committed')),
            response_status_code INTEGER,
            response_body TEXT,
            response_content_type TEXT,
            created_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            committed_at TEXT,
            lease_expires_at TEXT
         );
         CREATE UNIQUE INDEX idx_operation_journal_actor_key_idempotency
            ON operation_journal(actor_key, idempotency_key);
         CREATE UNIQUE INDEX idx_operation_journal_operation_id
            ON operation_journal(operation_id);
         CREATE INDEX idx_operation_journal_status_lease
            ON operation_journal(status, lease_expires_at);
         CREATE INDEX idx_operation_journal_fingerprint
            ON operation_journal(request_fingerprint);",
    )
    .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string()
            .contains("table operation_journal missing column owner_token"),
        "{err}"
    );
}

#[test]
fn latest_version_db_missing_operation_journal_delete_trigger_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch("DROP TRIGGER prevent_operation_journal_delete;")
        .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string()
            .contains("missing trigger prevent_operation_journal_delete"),
        "{err}"
    );
}

#[test]
fn latest_version_db_with_wrong_column_type_fails_verification() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute_batch(
        "DROP TABLE monitor_threshold_config;
         CREATE TABLE monitor_threshold_config (
             config_key TEXT PRIMARY KEY,
             critical_override_threshold TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             updated_by TEXT NOT NULL,
             confirmed_by TEXT
         );",
    )
    .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(
        err.to_string().contains(
            "table monitor_threshold_config column critical_override_threshold has type TEXT, expected REAL"
        ),
        "{err}"
    );
}

#[test]
fn runtime_created_audit_log_shape_migrates_to_canonical_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("runtime-audit.db");
    let conn = setup_db_to_v46(&db_path);

    conn.execute_batch(
        "DROP TRIGGER IF EXISTS prevent_audit_log_row_update;
         DROP TRIGGER IF EXISTS prevent_audit_log_row_delete;
         DROP TABLE audit_log;
         CREATE TABLE audit_log (
             id TEXT PRIMARY KEY,
             timestamp TEXT NOT NULL,
             agent_id TEXT NOT NULL,
             event_type TEXT NOT NULL,
             severity TEXT NOT NULL DEFAULT 'info',
             tool_name TEXT,
             details TEXT NOT NULL DEFAULT '',
             session_id TEXT,
             operation_id TEXT,
             request_id TEXT,
             idempotency_key TEXT,
             idempotency_status TEXT
         );
         CREATE INDEX idx_audit_timestamp ON audit_log(timestamp);
         CREATE INDEX idx_audit_agent ON audit_log(agent_id);
         CREATE INDEX idx_audit_event_type ON audit_log(event_type);
         CREATE INDEX idx_audit_severity ON audit_log(severity);
         CREATE INDEX idx_audit_operation_id ON audit_log(operation_id);
         CREATE INDEX idx_audit_idempotency_key ON audit_log(idempotency_key);",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO audit_log (id, timestamp, agent_id, event_type, details)
         VALUES ('audit-1', '2026-03-01T00:00:00Z', 'agent-1', 'boot', '{\"actor\":\"operator-1\"}')",
        [],
    )
    .unwrap();

    migrations::run_migrations_with_backup(&conn, Some(&db_path)).unwrap();

    require_schema_ready(&conn).unwrap();
    let actor: Option<String> = conn
        .query_row(
            "SELECT actor_id FROM audit_log WHERE id = 'audit-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(actor.as_deref(), Some("operator-1"));
}

#[test]
fn migration_era_audit_log_missing_later_columns_migrates_to_canonical_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("migration-audit.db");
    let conn = setup_db_to_v46(&db_path);

    conn.execute_batch(
        "DROP TRIGGER IF EXISTS prevent_audit_log_row_update;
         DROP TRIGGER IF EXISTS prevent_audit_log_row_delete;
         DROP TABLE audit_log;
         CREATE TABLE audit_log (
             id TEXT PRIMARY KEY,
             timestamp TEXT NOT NULL,
             agent_id TEXT NOT NULL,
             event_type TEXT NOT NULL,
             severity TEXT NOT NULL DEFAULT 'info',
             tool_name TEXT,
             details TEXT NOT NULL DEFAULT '',
             session_id TEXT,
             actor_id TEXT
         );
         CREATE INDEX idx_audit_timestamp ON audit_log(timestamp);
         CREATE INDEX idx_audit_agent ON audit_log(agent_id);
         CREATE INDEX idx_audit_event_type ON audit_log(event_type);
         CREATE INDEX idx_audit_severity ON audit_log(severity);
         CREATE INDEX idx_audit_log_actor_id ON audit_log(actor_id);",
    )
    .unwrap();

    migrations::run_migrations_with_backup(&conn, Some(&db_path)).unwrap();
    require_schema_ready(&conn).unwrap();

    let columns = conn
        .prepare("SELECT name FROM pragma_table_info('audit_log')")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for column in [
        "operation_id",
        "request_id",
        "idempotency_key",
        "idempotency_status",
    ] {
        assert!(columns.contains(&column.to_string()), "missing {column}");
    }
}

#[test]
fn runtime_created_monitor_threshold_tables_migrate_to_canonical_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("monitor-thresholds.db");
    let conn = setup_db_to_v46(&db_path);

    conn.execute_batch(
        "CREATE TABLE monitor_threshold_config (
            config_key TEXT PRIMARY KEY,
            critical_override_threshold REAL NOT NULL,
            updated_at TEXT NOT NULL,
            updated_by TEXT NOT NULL,
            confirmed_by TEXT
        );
        CREATE TABLE monitor_threshold_history (
            id TEXT PRIMARY KEY,
            config_key TEXT NOT NULL,
            previous_value REAL NOT NULL,
            new_value REAL NOT NULL,
            initiated_by TEXT NOT NULL,
            confirmed_by TEXT,
            change_mode TEXT NOT NULL,
            changed_at TEXT NOT NULL
        );",
    )
    .unwrap();

    migrations::run_migrations_with_backup(&conn, Some(&db_path)).unwrap();
    require_schema_ready(&conn).unwrap();
}

#[test]
fn runtime_created_channels_without_indexes_migrate_to_canonical_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("channels.db");
    let conn = setup_db_to_v46(&db_path);

    conn.execute_batch("DROP INDEX idx_channels_agent; DROP INDEX idx_channels_type;")
        .unwrap();

    migrations::run_migrations_with_backup(&conn, Some(&db_path)).unwrap();
    require_schema_ready(&conn).unwrap();
}

#[test]
fn unsupported_legacy_shape_fails_loudly_and_precisely() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("bad-audit.db");
    let conn = setup_db_to_v46(&db_path);

    conn.execute_batch(
        "DROP TRIGGER IF EXISTS prevent_audit_log_row_update;
         DROP TRIGGER IF EXISTS prevent_audit_log_row_delete;
         DROP TABLE audit_log;
         CREATE TABLE audit_log (
             id TEXT PRIMARY KEY,
             timestamp TEXT NOT NULL,
             agent_id TEXT NOT NULL,
             event_type TEXT NOT NULL,
             severity TEXT NOT NULL DEFAULT 'info',
             tool_name TEXT,
             session_id TEXT
         );",
    )
    .unwrap();

    let err = migrations::run_migrations_with_backup(&conn, Some(&db_path)).unwrap_err();
    assert!(
        err.to_string()
            .contains("unsupported legacy audit_log shape: missing required columns [details]"),
        "{err}"
    );
}

#[test]
fn migration_backup_is_restorable_under_wal_mode_and_receipt_is_written() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("backup.db");
    let conn = setup_db_to_v46(&db_path);

    conn.execute(
        "INSERT INTO audit_log (id, timestamp, agent_id, event_type, severity, details, actor_id)
         VALUES ('audit-backup', '2026-03-01T00:00:00Z', 'agent-1', 'boot', 'info', 'before migration', 'operator-1')",
        [],
    )
    .unwrap();

    migrations::run_migrations_with_backup(&conn, Some(&db_path)).unwrap();

    let backup_path = PathBuf::from(format!("{}.pre-migration-v47.bak", db_path.display()));
    assert!(
        backup_path.exists(),
        "missing backup at {}",
        backup_path.display()
    );

    let restored_path = tmp.path().join("restored.db");
    let mut restored = open_db(&restored_path);
    restored
        .restore(
            DatabaseName::Main,
            &backup_path,
            None::<fn(rusqlite::backup::Progress)>,
        )
        .unwrap();

    let version: u32 = restored
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, 46);
    let restored_details: String = restored
        .query_row(
            "SELECT details FROM audit_log WHERE id = 'audit-backup'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored_details, "before migration");

    let receipts = std::fs::read_dir(receipt_dir(&db_path))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let expected_prefix = format!("{}_", migrations::LATEST_VERSION);
    assert!(
        receipts
            .iter()
            .any(|name| name.starts_with(&expected_prefix) && name.ends_with(".json")),
        "expected receipt in {:?}, got {:?}",
        receipt_dir(&db_path),
        receipts
    );
}

#[test]
fn newer_schema_is_rejected() {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO schema_version (version, name) VALUES (?1, ?2)",
        rusqlite::params![999u32, "future"],
    )
    .unwrap();

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(matches!(
        err,
        SchemaContractError::UnsupportedNewerSchema {
            current_version: 999,
            ..
        }
    ));
}

#[test]
fn older_schema_requires_migration() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("old.db");
    let conn = setup_db_to_v46(&db_path);

    let err = require_schema_ready(&conn).unwrap_err();
    assert!(matches!(
        err,
        SchemaContractError::MigrationRequired {
            current_version: 46,
            expected_version: migrations::LATEST_VERSION
        }
    ));
}
