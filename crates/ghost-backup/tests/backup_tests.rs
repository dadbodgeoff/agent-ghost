//! Adversarial tests for ghost-backup.

use std::fs;

use ghost_backup::export::BackupExporter;
use ghost_backup::import::BackupImporter;
use ghost_backup::scheduler::{BackupInterval, BackupScheduler, BackupSchedulerConfig};
use rusqlite::Connection;
use tempfile::TempDir;

fn setup_ghost_dir(tmp: &TempDir) -> std::path::PathBuf {
    let ghost = tmp.path().join("ghost");
    let data = ghost.join("data");
    let config = ghost.join("config");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&config).unwrap();

    let db_path = data.join("ghost.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    conn.execute(
        "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO notes (body) VALUES ('committed wal row')", [])
        .unwrap();

    let wal_path = data.join("ghost.db-wal");
    assert!(wal_path.exists(), "test requires WAL-backed sqlite state");

    fs::write(config.join("ghost.yml"), b"gateway:\n  port: 18789").unwrap();
    ghost
}

fn unique_env_var(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

#[test]
fn export_creates_valid_archive() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    let exporter = BackupExporter::new(&ghost_dir);
    let manifest = exporter.export(&archive, "test-pass").unwrap();

    assert!(archive.exists());
    assert!(!manifest.entries.is_empty());
    assert_eq!(manifest.version, "2");
}

#[test]
fn verify_archive_rejects_wrong_passphrase() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    BackupExporter::new(&ghost_dir)
        .export(&archive, "correct-pass")
        .unwrap();

    let result = BackupImporter::verify_archive(&archive, "wrong-pass");
    assert!(result.is_err());
}

#[test]
fn import_restores_all_data_into_fresh_target() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    BackupExporter::new(&ghost_dir)
        .export(&archive, "test-pass")
        .unwrap();

    let restore_dir = tmp.path().join("restored");
    let importer = BackupImporter::new(&restore_dir);
    let manifest = importer.import(&archive, "test-pass").unwrap();

    assert!(!manifest.entries.is_empty());
    for entry in &manifest.entries {
        assert!(restore_dir.join(&entry.path).exists());
    }
}

#[test]
fn import_rejects_existing_restore_target() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    BackupExporter::new(&ghost_dir)
        .export(&archive, "test-pass")
        .unwrap();

    let restore_dir = tmp.path().join("restored");
    fs::create_dir_all(&restore_dir).unwrap();

    let importer = BackupImporter::new(&restore_dir);
    let result = importer.import(&archive, "test-pass");
    assert!(result.is_err());
    assert!(restore_dir.exists());
    assert!(fs::read_dir(&restore_dir).unwrap().next().is_none());
}

#[test]
fn export_import_roundtrip_preserves_wal_backed_sqlite_state() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    BackupExporter::new(&ghost_dir)
        .export(&archive, "roundtrip")
        .unwrap();

    let restore_dir = tmp.path().join("restored");
    BackupImporter::new(&restore_dir)
        .import(&archive, "roundtrip")
        .unwrap();

    let restored_db = Connection::open(restore_dir.join("data/ghost.db")).unwrap();
    let body: String = restored_db
        .query_row("SELECT body FROM notes ORDER BY id LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(body, "committed wal row");
}

#[test]
fn import_corrupted_archive_fails() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("corrupted.ghost-backup");

    BackupExporter::new(&ghost_dir)
        .export(&archive, "test-pass")
        .unwrap();

    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x5a;
    fs::write(&archive, bytes).unwrap();

    let result = BackupImporter::verify_archive(&archive, "test-pass");
    assert!(result.is_err());
}

#[test]
fn scheduler_requires_non_empty_passphrase() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let backup_dir = tmp.path().join("backups");
    let env_var = unique_env_var("GHOST_BACKUP_TEST_EMPTY");

    std::env::remove_var(&env_var);
    let config = BackupSchedulerConfig {
        interval: BackupInterval::Daily,
        retention_count: 3,
        backup_dir,
        ghost_dir,
        passphrase_env: env_var.clone(),
    };

    let scheduler = BackupScheduler::new(config);
    let result = scheduler.run_once();
    assert!(result.is_err());
}

#[test]
fn scheduler_retention_policy() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let backup_dir = tmp.path().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    for i in 0..5 {
        fs::write(
            backup_dir.join(format!("ghost_2026010{}_120000.ghost-backup", i)),
            b"fake",
        )
        .unwrap();
    }

    let env_var = unique_env_var("GHOST_BACKUP_TEST_PASS");
    std::env::set_var(&env_var, "scheduler-passphrase");

    let config = BackupSchedulerConfig {
        interval: BackupInterval::Daily,
        retention_count: 3,
        backup_dir: backup_dir.clone(),
        ghost_dir,
        passphrase_env: env_var.clone(),
    };

    let scheduler = BackupScheduler::new(config);
    scheduler.run_once().unwrap();

    let count = fs::read_dir(&backup_dir)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .ok()
                .and_then(|entry| entry.path().extension().map(|ext| ext == "ghost-backup"))
                .unwrap_or(false)
        })
        .count();
    assert!(count <= 3);

    std::env::remove_var(&env_var);
}
