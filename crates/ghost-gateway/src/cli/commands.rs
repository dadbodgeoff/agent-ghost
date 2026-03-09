//! Backup, Export, Migrate CLI commands (Task 6.6).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::bootstrap::{ghost_home, shellexpand_tilde};

use super::error::CliError;

fn resolve_active_ghost_dir() -> PathBuf {
    std::env::var("GHOST_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(ghost_home)
}

fn resolve_backup_passphrase() -> Result<String, CliError> {
    std::env::var("GHOST_BACKUP_PASSPHRASE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("GHOST_BACKUP_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            let key_path = ghost_home().join("backup.key");
            std::fs::read_to_string(key_path).ok().and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
        })
        .ok_or_else(|| {
            CliError::Config(
                "set GHOST_BACKUP_PASSPHRASE (or legacy GHOST_BACKUP_KEY), or create ~/.ghost/backup.key before running backup or restore"
                    .into(),
            )
        })
}

fn map_backup_error(action: &str, error: ghost_backup::BackupError) -> CliError {
    match error {
        ghost_backup::BackupError::Io(io) if io.kind() == ErrorKind::NotFound => {
            CliError::NotFound(format!("{action} input not found: {io}"))
        }
        ghost_backup::BackupError::InvalidRestoreTarget(message) => CliError::Conflict(message),
        ghost_backup::BackupError::VersionMismatch { archive, current } => CliError::Conflict(
            format!("archive version {archive} does not match current version {current}"),
        ),
        ghost_backup::BackupError::IntegrityError(message)
        | ghost_backup::BackupError::EncryptionError(message)
        | ghost_backup::BackupError::SerializationError(message)
        | ghost_backup::BackupError::UnsupportedArchive(message) => {
            CliError::Usage(format!("{action} failed: {message}"))
        }
        ghost_backup::BackupError::Io(io) => CliError::Internal(format!("{action} failed: {io}")),
    }
}

fn create_backup_archive(
    ghost_dir: &Path,
    output_path: &Path,
    passphrase: &str,
) -> Result<ghost_backup::BackupManifest, CliError> {
    let exporter = ghost_backup::export::BackupExporter::new(ghost_dir);
    exporter
        .export(output_path, passphrase)
        .map_err(|error| map_backup_error("backup", error))
}

fn import_backup_archive(
    archive_path: &Path,
    restore_target: &Path,
    passphrase: &str,
) -> Result<ghost_backup::BackupManifest, CliError> {
    let importer = ghost_backup::import::BackupImporter::new(restore_target);
    importer
        .import(archive_path, passphrase)
        .map_err(|error| map_backup_error("restore", error))
}

fn default_restore_target(active_ghost_dir: &Path) -> PathBuf {
    let parent = active_ghost_dir.parent().unwrap_or_else(|| Path::new("."));
    let leaf = active_ghost_dir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("ghost");
    parent.join(format!("{leaf}-restored-{}", uuid::Uuid::now_v7()))
}

/// Run a backup operation.
pub fn run_backup(output: Option<&str>) -> Result<(), CliError> {
    let ghost_dir = resolve_active_ghost_dir();
    let output_path = output.map(PathBuf::from).unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        ghost_dir.join(format!("backups/ghost_{}.ghost-backup", ts))
    });

    if let Some(parent) = output_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(error = %e, path = %parent.display(), "failed to create backup directory");
        }
    }

    let passphrase = resolve_backup_passphrase()?;
    match create_backup_archive(&ghost_dir, &output_path, &passphrase) {
        Ok(manifest) => {
            println!(
                "Backup created: {} ({} entries)",
                output_path.display(),
                manifest.entries.len()
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Run a restore into a fresh target directory.
pub fn run_restore(input: &str, target: Option<&str>) -> Result<(), CliError> {
    let archive_path = PathBuf::from(shellexpand_tilde(input));
    let active_ghost_dir = resolve_active_ghost_dir();
    let restore_target = target
        .map(shellexpand_tilde)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_restore_target(&active_ghost_dir));
    let passphrase = resolve_backup_passphrase()?;

    let manifest = import_backup_archive(&archive_path, &restore_target, &passphrase)?;
    println!(
        "Backup restored into fresh target: {} ({} entries, format v{})",
        restore_target.display(),
        manifest.entries.len(),
        manifest.version
    );
    println!(
        "Active Ghost directory remains unchanged: {}",
        active_ghost_dir.display()
    );
    Ok(())
}

/// Run an export analysis.
pub fn run_export(path: &str) -> Result<(), CliError> {
    let analyzer = ghost_export::analyzer::ExportAnalyzer::new();
    match analyzer.analyze(Path::new(path)) {
        Ok(result) => {
            println!("Export Analysis Results");
            println!("──────────────────────");
            println!("Format:     {}", result.source_format);
            println!("Messages:   {}", result.total_messages);
            println!("Sessions:   {}", result.total_sessions);
            println!("Rec. Level: {}", result.recommended_level);
            if !result.flagged_sessions.is_empty() {
                println!("Flagged:    {} sessions", result.flagged_sessions.len());
            }
            Ok(())
        }
        Err(e) => Err(CliError::Internal(format!("export analysis failed: {e}"))),
    }
}

/// Run an OpenClaw migration.
pub fn run_migrate(source: &str) -> Result<(), CliError> {
    let source_path = PathBuf::from(shellexpand_tilde(source));
    let target_path = resolve_active_ghost_dir();

    if !ghost_migrate::migrator::OpenClawMigrator::detect(&source_path) {
        return Err(CliError::NotFound(format!(
            "no OpenClaw installation found at: {}",
            source_path.display()
        )));
    }

    let migrator = ghost_migrate::migrator::OpenClawMigrator::new(&source_path, &target_path);
    match migrator.migrate() {
        Ok(result) => {
            println!("Migration Complete");
            println!("─────────────────");
            println!("Imported:  {} items", result.imported.len());
            println!("Skipped:   {} items", result.skipped.len());
            println!("Warnings:  {}", result.warnings.len());
            println!("Review:    {} items", result.review_items.len());
            for warning in &result.warnings {
                println!("  ⚠ {}", warning);
            }
            for review in &result.review_items {
                println!("  → {}", review);
            }
            Ok(())
        }
        Err(e) => Err(CliError::Internal(format!("migration failed: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_archive_into_explicit_target() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(source_dir.join("data")).unwrap();
        std::fs::write(source_dir.join("data/session.txt"), "hello").unwrap();

        let archive_path = temp_dir.path().join("backup.ghost-backup");
        create_backup_archive(&source_dir, &archive_path, "test-passphrase").unwrap();

        let restore_target = temp_dir.path().join("restored");
        let manifest =
            import_backup_archive(&archive_path, &restore_target, "test-passphrase").unwrap();

        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(
            std::fs::read_to_string(restore_target.join("data/session.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn restore_archive_rejects_existing_target() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(source_dir.join("data")).unwrap();
        std::fs::write(source_dir.join("data/session.txt"), "hello").unwrap();

        let archive_path = temp_dir.path().join("backup.ghost-backup");
        create_backup_archive(&source_dir, &archive_path, "test-passphrase").unwrap();

        let restore_target = temp_dir.path().join("restored");
        std::fs::create_dir_all(&restore_target).unwrap();

        let error =
            import_backup_archive(&archive_path, &restore_target, "test-passphrase").unwrap_err();
        assert!(matches!(error, CliError::Conflict(_)));
    }
}
