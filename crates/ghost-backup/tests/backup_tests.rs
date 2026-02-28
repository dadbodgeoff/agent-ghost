//! Tests for ghost-backup (Task 6.2).

use ghost_backup::export::BackupExporter;
use ghost_backup::import::BackupImporter;
use ghost_backup::scheduler::{BackupInterval, BackupScheduler, BackupSchedulerConfig};
use std::fs;
use tempfile::TempDir;

fn setup_ghost_dir(tmp: &TempDir) -> std::path::PathBuf {
    let ghost = tmp.path().join("ghost");
    let data = ghost.join("data");
    let config = ghost.join("config");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::write(data.join("ghost.db"), b"fake-sqlite-data").unwrap();
    fs::write(config.join("ghost.yml"), b"gateway:\n  port: 18789").unwrap();
    ghost
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
    assert_eq!(manifest.version, "1");
}

#[test]
fn import_restores_all_data() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    let exporter = BackupExporter::new(&ghost_dir);
    exporter.export(&archive, "test-pass").unwrap();

    // Restore to a different directory
    let restore_dir = tmp.path().join("restored");
    fs::create_dir_all(&restore_dir).unwrap();

    let importer = BackupImporter::new(&restore_dir);
    let manifest = importer.import(&archive, "test-pass").unwrap();

    assert!(!manifest.entries.is_empty());
    // Verify restored files exist
    for entry in &manifest.entries {
        assert!(restore_dir.join(&entry.path).exists());
    }
}

#[test]
fn export_import_roundtrip_identical() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    let exporter = BackupExporter::new(&ghost_dir);
    exporter.export(&archive, "roundtrip").unwrap();

    let restore_dir = tmp.path().join("restored");
    fs::create_dir_all(&restore_dir).unwrap();

    let importer = BackupImporter::new(&restore_dir);
    importer.import(&archive, "roundtrip").unwrap();

    // Verify content matches
    let orig = fs::read(ghost_dir.join("data/ghost.db")).unwrap();
    let restored = fs::read(restore_dir.join("data/ghost.db")).unwrap();
    assert_eq!(orig, restored);
}

#[test]
fn import_wrong_passphrase_fails() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let archive = tmp.path().join("test.ghost-backup");

    let exporter = BackupExporter::new(&ghost_dir);
    exporter.export(&archive, "correct-pass").unwrap();

    let restore_dir = tmp.path().join("restored");
    fs::create_dir_all(&restore_dir).unwrap();

    let importer = BackupImporter::new(&restore_dir);
    let result = importer.import(&archive, "wrong-pass");
    assert!(result.is_err());
}

#[test]
fn import_corrupted_archive_fails() {
    let tmp = TempDir::new().unwrap();
    let archive = tmp.path().join("corrupted.ghost-backup");
    fs::write(&archive, b"not-a-valid-archive").unwrap();

    let restore_dir = tmp.path().join("restored");
    fs::create_dir_all(&restore_dir).unwrap();

    let importer = BackupImporter::new(&restore_dir);
    let result = importer.import(&archive, "any-pass");
    assert!(result.is_err());
}

#[test]
fn scheduler_retention_policy() {
    let tmp = TempDir::new().unwrap();
    let ghost_dir = setup_ghost_dir(&tmp);
    let backup_dir = tmp.path().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Create some fake old backups
    for i in 0..5 {
        fs::write(
            backup_dir.join(format!("ghost_2026010{}_120000.ghost-backup", i)),
            b"fake",
        )
        .unwrap();
    }

    let config = BackupSchedulerConfig {
        interval: BackupInterval::Daily,
        retention_count: 3,
        backup_dir: backup_dir.clone(),
        ghost_dir,
        passphrase_env: "GHOST_BACKUP_KEY".to_string(),
    };

    let scheduler = BackupScheduler::new(config);
    scheduler.run_once().unwrap();

    let count = fs::read_dir(&backup_dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .ok()
                .and_then(|e| e.path().extension().map(|ext| ext == "ghost-backup"))
                .unwrap_or(false)
        })
        .count();
    assert!(count <= 3);
}
