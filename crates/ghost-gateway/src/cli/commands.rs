//! Backup, Export, Migrate CLI commands (Task 6.6).

use std::path::{Path, PathBuf};

/// Run a backup operation.
pub fn run_backup(output: Option<&str>) {
    let ghost_dir = expand_tilde("~/.ghost");
    let output_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            ghost_dir.join(format!("backups/ghost_{}.ghost-backup", ts))
        });

    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let passphrase = std::env::var("GHOST_BACKUP_KEY").unwrap_or_default();
    if passphrase.is_empty() {
        tracing::warn!("GHOST_BACKUP_KEY not set — backup will use empty passphrase");
    }

    let exporter = ghost_backup::export::BackupExporter::new(&ghost_dir);
    match exporter.export(&output_path, &passphrase) {
        Ok(manifest) => {
            println!(
                "Backup created: {} ({} entries)",
                output_path.display(),
                manifest.entries.len()
            );
        }
        Err(e) => {
            eprintln!("Backup failed: {}", e);
            std::process::exit(1);
        }
    }
}

/// Run an export analysis.
pub fn run_export(path: &str) {
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
        }
        Err(e) => {
            eprintln!("Export analysis failed: {}", e);
            std::process::exit(1);
        }
    }
}

/// Run an OpenClaw migration.
pub fn run_migrate(source: &str) {
    let source_path = expand_tilde(source);
    let target_path = expand_tilde("~/.ghost");

    if !ghost_migrate::migrator::OpenClawMigrator::detect(&source_path) {
        eprintln!("No OpenClaw installation found at: {}", source_path.display());
        std::process::exit(1);
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
        }
        Err(e) => {
            eprintln!("Migration failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs_home() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
