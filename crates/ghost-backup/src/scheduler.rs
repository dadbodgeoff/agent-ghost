//! Backup scheduler — configurable automatic backups (Req 30 AC5).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::export::BackupExporter;
use crate::BackupResult;

/// Backup schedule interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupInterval {
    Daily,
    Weekly,
}

impl BackupInterval {
    pub fn as_duration(&self) -> Duration {
        match self {
            Self::Daily => Duration::from_secs(86_400),
            Self::Weekly => Duration::from_secs(604_800),
        }
    }
}

/// Backup scheduler configuration.
#[derive(Debug, Clone)]
pub struct BackupSchedulerConfig {
    pub interval: BackupInterval,
    pub retention_count: usize,
    pub backup_dir: PathBuf,
    pub ghost_dir: PathBuf,
    pub passphrase_env: String,
}

impl Default for BackupSchedulerConfig {
    fn default() -> Self {
        Self {
            interval: BackupInterval::Daily,
            retention_count: 7,
            backup_dir: PathBuf::from("~/.ghost/backups"),
            ghost_dir: PathBuf::from("~/.ghost"),
            passphrase_env: "GHOST_BACKUP_KEY".to_string(),
        }
    }
}

/// Manages scheduled backups with retention policy.
pub struct BackupScheduler {
    config: BackupSchedulerConfig,
}

impl BackupScheduler {
    pub fn new(config: BackupSchedulerConfig) -> Self {
        Self { config }
    }

    /// Run a single backup cycle: create backup, enforce retention.
    pub fn run_once(&self) -> BackupResult<PathBuf> {
        let passphrase = std::env::var(&self.config.passphrase_env).unwrap_or_else(|_| {
            tracing::warn!(
                env_var = %self.config.passphrase_env,
                "backup passphrase env var not set — using empty passphrase (backup will NOT be encrypted)"
            );
            String::new()
        });
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("ghost_{}.ghost-backup", timestamp);
        let output_path = self.config.backup_dir.join(&filename);

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let exporter = BackupExporter::new(&self.config.ghost_dir);
        exporter.export(&output_path, &passphrase)?;

        self.enforce_retention()?;

        Ok(output_path)
    }

    /// Delete old backups beyond retention count.
    fn enforce_retention(&self) -> BackupResult<()> {
        let dir = &self.config.backup_dir;
        if !dir.exists() {
            return Ok(());
        }

        let mut backups: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|ext| ext == "ghost-backup")
                    .unwrap_or(false)
            })
            .collect();

        // Sort by name (timestamp-based, so lexicographic = chronological)
        backups.sort();

        // Remove oldest beyond retention
        while backups.len() > self.config.retention_count {
            if let Some(oldest) = backups.first() {
                if let Err(e) = std::fs::remove_file(oldest) {
                    tracing::warn!(path = %oldest.display(), error = %e, "failed to remove old backup during retention enforcement");
                }
                backups.remove(0);
            }
        }

        Ok(())
    }
}
