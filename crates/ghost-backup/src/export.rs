//! Backup export — collect, encrypt, archive (Req 30 AC3).
//!
//! This archive is intended for verified operator export/import flows. It is
//! distinct from the DB-adjacent migration rollback backup.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use age::secrecy::SecretString as AgeSecretString;
use age::Encryptor;
use rusqlite::{Connection, DatabaseName, OpenFlags};
use tempfile::NamedTempFile;

use crate::{
    BackupError, BackupManifest, BackupResult, ManifestEntry, ARCHIVE_MAGIC,
    CURRENT_BACKUP_FORMAT_VERSION, MANAGED_ROOTS,
};

/// Collects platform state and writes a `.ghost-backup` archive.
pub struct BackupExporter {
    ghost_dir: PathBuf,
}

impl BackupExporter {
    pub fn new(ghost_dir: impl Into<PathBuf>) -> Self {
        Self {
            ghost_dir: ghost_dir.into(),
        }
    }

    /// Export platform state to a backup archive at `output_path`.
    ///
    /// Collects the managed platform roots and snapshots `data/ghost.db`
    /// through SQLite's backup API so committed WAL state is preserved.
    pub fn export(&self, output_path: &Path, passphrase: &str) -> BackupResult<BackupManifest> {
        let passphrase = require_passphrase(passphrase)?;
        let mut archive_data: BTreeMap<String, Vec<u8>> = BTreeMap::new();

        for root in MANAGED_ROOTS {
            let dir = self.ghost_dir.join(root);
            if dir.exists() {
                self.collect_dir(&dir, root, &mut archive_data)?;
            }
        }

        let entries = archive_data
            .iter()
            .map(|(path, data)| ManifestEntry {
                path: path.clone(),
                size: data.len() as u64,
                blake3_hash: blake3::hash(data).to_hex().to_string(),
            })
            .collect();

        let manifest = BackupManifest {
            version: CURRENT_BACKUP_FORMAT_VERSION.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            platform_version: env!("CARGO_PKG_VERSION").to_string(),
            entries,
        };

        let plaintext = encode_archive(&manifest, &archive_data)?;
        let encrypted = encrypt_archive(&plaintext, passphrase)?;

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(output_path)?;
        file.write_all(&encrypted)?;
        file.sync_all()?;

        tracing::info!(
            path = %output_path.display(),
            entries = manifest.entries.len(),
            "Backup exported"
        );

        Ok(manifest)
    }

    fn collect_dir(
        &self,
        dir: &Path,
        prefix: &str,
        archive: &mut BTreeMap<String, Vec<u8>>,
    ) -> BackupResult<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|a| a.file_name());

        for entry in entries {
            let path = entry.path();
            let rel = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
            if path.is_file() {
                archive.insert(rel.clone(), self.read_file_for_backup(&path, &rel)?);
            } else if path.is_dir() {
                self.collect_dir(&path, &rel, archive)?;
            }
        }

        Ok(())
    }

    fn read_file_for_backup(&self, path: &Path, rel: &str) -> BackupResult<Vec<u8>> {
        if rel == "data/ghost.db" {
            snapshot_sqlite_db(path)
        } else {
            fs::read(path).map_err(BackupError::from)
        }
    }
}

pub(crate) fn encode_archive(
    manifest: &BackupManifest,
    archive_data: &BTreeMap<String, Vec<u8>>,
) -> BackupResult<Vec<u8>> {
    let manifest_json = serde_json::to_vec(manifest)
        .map_err(|error| BackupError::SerializationError(error.to_string()))?;

    let mut raw = Vec::new();
    raw.extend_from_slice(ARCHIVE_MAGIC);
    raw.extend_from_slice(&(manifest_json.len() as u64).to_le_bytes());
    raw.extend_from_slice(&manifest_json);

    for entry in &manifest.entries {
        let data = archive_data.get(&entry.path).ok_or_else(|| {
            BackupError::IntegrityError(format!(
                "manifest entry missing archived data: {}",
                entry.path
            ))
        })?;
        raw.extend_from_slice(&(data.len() as u64).to_le_bytes());
        raw.extend_from_slice(data);
    }

    Ok(raw)
}

pub(crate) fn encrypt_archive(data: &[u8], passphrase: &str) -> BackupResult<Vec<u8>> {
    let encryptor = Encryptor::with_user_passphrase(AgeSecretString::from(passphrase.to_string()));
    let mut encrypted = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|error| BackupError::EncryptionError(error.to_string()))?;
    writer
        .write_all(data)
        .map_err(|error| BackupError::EncryptionError(error.to_string()))?;
    writer
        .finish()
        .map_err(|error| BackupError::EncryptionError(error.to_string()))?;
    Ok(encrypted)
}

pub(crate) fn require_passphrase(passphrase: &str) -> BackupResult<&str> {
    let trimmed = passphrase.trim();
    if trimmed.is_empty() {
        return Err(BackupError::EncryptionError(
            "backup passphrase must be non-empty".to_string(),
        ));
    }
    Ok(trimmed)
}

fn snapshot_sqlite_db(db_path: &Path) -> BackupResult<Vec<u8>> {
    let source = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        BackupError::IntegrityError(format!("open SQLite source {}: {error}", db_path.display()))
    })?;
    source
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| {
            BackupError::IntegrityError(format!(
                "set SQLite busy timeout {}: {error}",
                db_path.display()
            ))
        })?;

    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let snapshot = NamedTempFile::new_in(parent).map_err(BackupError::Io)?;
    source
        .backup(DatabaseName::Main, snapshot.path(), None)
        .map_err(|error| {
            BackupError::IntegrityError(format!(
                "backup SQLite database {}: {error}",
                db_path.display()
            ))
        })?;

    verify_sqlite_snapshot(snapshot.path(), db_path)?;

    fs::read(snapshot.path()).map_err(BackupError::from)
}

pub(crate) fn verify_sqlite_snapshot(
    snapshot_path: &Path,
    context_path: &Path,
) -> BackupResult<()> {
    let snapshot = Connection::open(snapshot_path).map_err(|error| {
        BackupError::IntegrityError(format!(
            "open SQLite snapshot {}: {error}",
            snapshot_path.display()
        ))
    })?;
    let integrity: String = snapshot
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| {
            BackupError::IntegrityError(format!(
                "verify SQLite snapshot {}: {error}",
                context_path.display()
            ))
        })?;
    if integrity != "ok" {
        return Err(BackupError::IntegrityError(format!(
            "SQLite snapshot integrity check failed for {}: {integrity}",
            context_path.display()
        )));
    }

    let violations = snapshot
        .prepare("PRAGMA foreign_key_check")
        .map_err(|error| {
            BackupError::IntegrityError(format!(
                "prepare foreign_key_check {}: {error}",
                context_path.display()
            ))
        })?
        .query_map([], |_row| Ok(()))
        .map_err(|error| {
            BackupError::IntegrityError(format!(
                "run foreign_key_check {}: {error}",
                context_path.display()
            ))
        })?
        .count();
    if violations != 0 {
        return Err(BackupError::IntegrityError(format!(
            "SQLite snapshot foreign key check failed for {}: {violations} violation(s)",
            context_path.display()
        )));
    }

    Ok(())
}
