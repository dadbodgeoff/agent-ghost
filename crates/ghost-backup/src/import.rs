//! Backup import — decrypt, verify, staged restore (Req 30 AC4).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use age::secrecy::SecretString as AgeSecretString;
use age::Decryptor;
use uuid::Uuid;

use crate::export::require_passphrase;
use crate::{
    BackupError, BackupManifest, BackupResult, ARCHIVE_MAGIC, CURRENT_BACKUP_FORMAT_VERSION,
};

/// Imports and restores from a `.ghost-backup` archive.
pub struct BackupImporter {
    ghost_dir: PathBuf,
}

impl BackupImporter {
    pub fn new(ghost_dir: impl Into<PathBuf>) -> Self {
        Self {
            ghost_dir: ghost_dir.into(),
        }
    }

    /// Verify an archive without restoring it.
    pub fn verify_archive(
        archive_path: &std::path::Path,
        passphrase: &str,
    ) -> BackupResult<BackupManifest> {
        let (manifest, _) = read_archive(archive_path, passphrase)?;
        Ok(manifest)
    }

    /// Import from a backup archive into a fresh restore target.
    ///
    /// In-place overwrite is disabled. Callers must restore into a target path
    /// that does not already exist, then perform any operator-controlled swap
    /// outside this library.
    pub fn import(
        &self,
        archive_path: &std::path::Path,
        passphrase: &str,
    ) -> BackupResult<BackupManifest> {
        let (manifest, data) = read_archive(archive_path, passphrase)?;
        self.restore_verified_archive(&data)?;

        tracing::info!(
            entries = manifest.entries.len(),
            target = %self.ghost_dir.display(),
            "Backup imported and restored into fresh target"
        );

        Ok(manifest)
    }

    fn restore_verified_archive(&self, data: &BTreeMap<String, Vec<u8>>) -> BackupResult<()> {
        if self.ghost_dir.exists() {
            return Err(BackupError::InvalidRestoreTarget(format!(
                "restore target {} already exists; in-place restore is disabled",
                self.ghost_dir.display()
            )));
        }

        let parent = self.ghost_dir.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let stage_root = parent.join(format!(".ghost-restore-{}", Uuid::now_v7()));
        if stage_root.exists() {
            return Err(BackupError::InvalidRestoreTarget(format!(
                "staging path {} already exists",
                stage_root.display()
            )));
        }
        fs::create_dir_all(&stage_root)?;

        let restore_result = (|| {
            for (rel_path, file_data) in data {
                let sanitized = sanitize_entry_path(rel_path)?;
                let target = stage_root.join(&sanitized);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&target, file_data)?;
            }
            fs::rename(&stage_root, &self.ghost_dir)?;
            Ok(())
        })();

        if restore_result.is_err() {
            let _ = fs::remove_dir_all(&stage_root);
        }

        restore_result
    }
}

pub(crate) fn read_archive(
    archive_path: &Path,
    passphrase: &str,
) -> BackupResult<(BackupManifest, BTreeMap<String, Vec<u8>>)> {
    let passphrase = require_passphrase(passphrase)?;
    let encrypted = fs::read(archive_path)?;
    let raw = decrypt_archive(&encrypted, passphrase)?;
    parse_archive_bytes(&raw)
}

pub(crate) fn decrypt_archive(data: &[u8], passphrase: &str) -> BackupResult<Vec<u8>> {
    let decryptor = Decryptor::new(data).map_err(|error| {
        BackupError::UnsupportedArchive(format!("invalid age envelope: {error}"))
    })?;
    if !decryptor.is_scrypt() {
        return Err(BackupError::UnsupportedArchive(
            "only passphrase-encrypted backup archives are supported".to_string(),
        ));
    }

    let passphrase = AgeSecretString::from(passphrase.to_string());
    let identity = age::scrypt::Identity::new(passphrase);
    let identities = [&identity as &dyn age::Identity];
    let mut reader = decryptor
        .decrypt(identities.into_iter())
        .map_err(|error| BackupError::EncryptionError(error.to_string()))?;

    let mut plaintext = Vec::new();
    reader
        .read_to_end(&mut plaintext)
        .map_err(|error| BackupError::EncryptionError(error.to_string()))?;
    Ok(plaintext)
}

pub(crate) fn parse_archive_bytes(
    raw: &[u8],
) -> BackupResult<(BackupManifest, BTreeMap<String, Vec<u8>>)> {
    if raw.len() < ARCHIVE_MAGIC.len() + 8 {
        return Err(BackupError::IntegrityError("archive too small".to_string()));
    }
    if &raw[..ARCHIVE_MAGIC.len()] != ARCHIVE_MAGIC {
        return Err(BackupError::UnsupportedArchive(
            "legacy or unknown backup archive format".to_string(),
        ));
    }

    let mut cursor = ARCHIVE_MAGIC.len();
    let manifest_len = read_u64(raw, &mut cursor)? as usize;
    if raw.len() < cursor + manifest_len {
        return Err(BackupError::IntegrityError(
            "manifest truncated".to_string(),
        ));
    }

    let manifest: BackupManifest = serde_json::from_slice(&raw[cursor..cursor + manifest_len])
        .map_err(|error| BackupError::IntegrityError(format!("manifest parse: {error}")))?;
    cursor += manifest_len;

    if manifest.version != CURRENT_BACKUP_FORMAT_VERSION {
        return Err(BackupError::VersionMismatch {
            archive: manifest.version.clone(),
            current: CURRENT_BACKUP_FORMAT_VERSION.to_string(),
        });
    }

    let mut seen_paths = BTreeSet::new();
    let mut data = BTreeMap::new();
    for entry in &manifest.entries {
        if !seen_paths.insert(entry.path.clone()) {
            return Err(BackupError::IntegrityError(format!(
                "duplicate manifest entry: {}",
                entry.path
            )));
        }

        let file_len = read_u64(raw, &mut cursor)? as usize;
        if raw.len() < cursor + file_len {
            return Err(BackupError::IntegrityError(format!(
                "file data truncated for {}",
                entry.path
            )));
        }

        let file_data = raw[cursor..cursor + file_len].to_vec();
        cursor += file_len;

        if entry.size != file_data.len() as u64 {
            return Err(BackupError::IntegrityError(format!(
                "size mismatch for {}: manifest={}, actual={}",
                entry.path,
                entry.size,
                file_data.len()
            )));
        }
        let hash = blake3::hash(&file_data).to_hex().to_string();
        if hash != entry.blake3_hash {
            return Err(BackupError::IntegrityError(format!(
                "hash mismatch for {}",
                entry.path
            )));
        }

        data.insert(entry.path.clone(), file_data);
    }

    if cursor != raw.len() {
        return Err(BackupError::IntegrityError(
            "archive contains unexpected trailing data".to_string(),
        ));
    }

    Ok((manifest, data))
}

fn read_u64(raw: &[u8], cursor: &mut usize) -> BackupResult<u64> {
    if raw.len() < *cursor + 8 {
        return Err(BackupError::IntegrityError(
            "archive truncated while reading length".to_string(),
        ));
    }
    let mut len_bytes = [0u8; 8];
    len_bytes.copy_from_slice(&raw[*cursor..*cursor + 8]);
    *cursor += 8;
    Ok(u64::from_le_bytes(len_bytes))
}

fn sanitize_entry_path(path: &str) -> BackupResult<PathBuf> {
    if path.is_empty() {
        return Err(BackupError::IntegrityError(
            "archive entry path is empty".to_string(),
        ));
    }

    let path = Path::new(path);
    if path.is_absolute() {
        return Err(BackupError::IntegrityError(format!(
            "absolute archive path rejected: {}",
            path.display()
        )));
    }

    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(BackupError::IntegrityError(format!(
                    "unsafe archive path rejected: {}",
                    path.display()
                )));
            }
        }
    }

    if sanitized.as_os_str().is_empty() {
        return Err(BackupError::IntegrityError(format!(
            "archive path resolves to empty target: {}",
            path.display()
        )));
    }

    Ok(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use tempfile::TempDir;

    use crate::export::{encode_archive, encrypt_archive};
    use crate::ManifestEntry;

    fn manifest_for(path: &str, data: &[u8]) -> BackupManifest {
        BackupManifest {
            version: CURRENT_BACKUP_FORMAT_VERSION.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            platform_version: "test".to_string(),
            entries: vec![ManifestEntry {
                path: path.to_string(),
                size: data.len() as u64,
                blake3_hash: blake3::hash(data).to_hex().to_string(),
            }],
        }
    }

    #[test]
    fn archive_with_trailing_bytes_is_rejected() {
        let mut files = BTreeMap::new();
        files.insert("data/file.txt".to_string(), b"ok".to_vec());
        let manifest = manifest_for("data/file.txt", b"ok");
        let mut raw = encode_archive(&manifest, &files).unwrap();
        raw.extend_from_slice(b"extra");

        let error = parse_archive_bytes(&raw).unwrap_err();
        assert!(matches!(error, BackupError::IntegrityError(_)));
    }

    #[test]
    fn import_rejects_path_traversal_entries() {
        let tmp = TempDir::new().unwrap();
        let archive = tmp.path().join("bad.ghost-backup");
        let restore_target = tmp.path().join("restore-target");

        let payload = b"escape";
        let mut files = BTreeMap::new();
        files.insert("../escape.txt".to_string(), payload.to_vec());
        let manifest = manifest_for("../escape.txt", payload);
        let raw = encode_archive(&manifest, &files).unwrap();
        let encrypted = encrypt_archive(&raw, "test-pass").unwrap();
        fs::write(&archive, encrypted).unwrap();

        let error = BackupImporter::new(&restore_target)
            .import(&archive, "test-pass")
            .unwrap_err();
        assert!(matches!(error, BackupError::IntegrityError(_)));
        assert!(!restore_target.exists());
    }
}
