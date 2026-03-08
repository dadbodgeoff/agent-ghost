//! ghost-backup — encrypted state backup and restore (Req 30 AC3–AC5).
//!
//! Exports platform state to `.ghost-backup` archives (zstd + encryption),
//! imports with integrity verification, and supports scheduled automatic backups.

pub mod export;
pub mod import;
pub mod scheduler;

pub use export::BackupExporter;
pub use import::BackupImporter;
pub use scheduler::BackupScheduler;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) const ARCHIVE_MAGIC: &[u8] = b"GHOST-BACKUP-V2\0";
pub(crate) const MANAGED_ROOTS: &[&str] = &["data", "config", "agents"];

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("integrity check failed: {0}")]
    IntegrityError(String),
    #[error("encryption error: {0}")]
    EncryptionError(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
    #[error("version mismatch: archive={archive}, current={current}")]
    VersionMismatch { archive: String, current: String },
    #[error("unsupported archive format: {0}")]
    UnsupportedArchive(String),
    #[error("invalid restore target: {0}")]
    InvalidRestoreTarget(String),
}

pub type BackupResult<T> = Result<T, BackupError>;
pub const CURRENT_BACKUP_FORMAT_VERSION: &str = "2";

/// Manifest embedded in every backup archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: String,
    pub created_at: String,
    pub platform_version: String,
    pub entries: Vec<ManifestEntry>,
}

/// A single file entry in the backup manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub size: u64,
    pub blake3_hash: String,
}
