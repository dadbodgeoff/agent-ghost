//! Backup, Export, Migrate CLI commands (Task 6.6).

use std::path::{Path, PathBuf};

use crate::bootstrap::shellexpand_tilde;

use super::error::CliError;

/// Run a backup operation.
pub fn run_backup(output: Option<&str>) -> Result<(), CliError> {
    let ghost_dir = shellexpand_tilde("~/.ghost");
    let ghost_dir = PathBuf::from(&ghost_dir);
    let output_path = output.map(PathBuf::from).unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        ghost_dir.join(format!("backups/ghost_{}.ghost-backup", ts))
    });

    if let Some(parent) = output_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(error = %e, path = %parent.display(), "failed to create backup directory");
        }
    }

    let passphrase = std::env::var("GHOST_BACKUP_PASSPHRASE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("GHOST_BACKUP_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| {
            CliError::Config(
                "set GHOST_BACKUP_PASSPHRASE (or legacy GHOST_BACKUP_KEY) before creating backups"
                    .into(),
            )
        })?;

    let exporter = ghost_backup::export::BackupExporter::new(&ghost_dir);
    match exporter.export(&output_path, &passphrase) {
        Ok(manifest) => {
            println!(
                "Backup created: {} ({} entries)",
                output_path.display(),
                manifest.entries.len()
            );
            Ok(())
        }
        Err(e) => Err(CliError::Internal(format!("backup failed: {e}"))),
    }
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
    let target_path = PathBuf::from(shellexpand_tilde("~/.ghost"));

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
