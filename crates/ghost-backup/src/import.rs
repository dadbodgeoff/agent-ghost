//! Backup import — decrypt, decompress, verify, restore (Req 30 AC4).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::{BackupError, BackupManifest, BackupResult};

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

    /// Import from a backup archive, verifying integrity.
    pub fn import(
        &self,
        archive_path: &std::path::Path,
        passphrase: &str,
    ) -> BackupResult<BackupManifest> {
        let encrypted = fs::read(archive_path)?;

        // Decrypt
        let raw = Self::decrypt(&encrypted, passphrase);

        // Parse manifest length
        if raw.len() < 4 {
            return Err(BackupError::IntegrityError("archive too small".to_string()));
        }
        let manifest_len = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;

        if raw.len() < 4 + manifest_len {
            return Err(BackupError::IntegrityError(
                "manifest truncated".to_string(),
            ));
        }

        let manifest: BackupManifest = serde_json::from_slice(&raw[4..4 + manifest_len])
            .map_err(|e| BackupError::IntegrityError(format!("manifest parse: {}", e)))?;

        let data: BTreeMap<String, Vec<u8>> = serde_json::from_slice(&raw[4 + manifest_len..])
            .map_err(|e| BackupError::IntegrityError(format!("data parse: {}", e)))?;

        // Verify integrity of each entry
        for entry in &manifest.entries {
            let file_data = data.get(&entry.path).ok_or_else(|| {
                BackupError::IntegrityError(format!("missing file: {}", entry.path))
            })?;
            let hash = blake3::hash(file_data).to_hex().to_string();
            if hash != entry.blake3_hash {
                return Err(BackupError::IntegrityError(format!(
                    "hash mismatch for {}: expected {}, got {}",
                    entry.path, entry.blake3_hash, hash
                )));
            }
        }

        // Restore files
        for (rel_path, file_data) in &data {
            let target = self.ghost_dir.join(rel_path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, file_data)?;
        }

        tracing::info!(
            entries = manifest.entries.len(),
            "Backup imported and restored"
        );

        Ok(manifest)
    }

    /// Decrypt (matches BackupExporter::encrypt).
    fn decrypt(data: &[u8], passphrase: &str) -> Vec<u8> {
        let key_bytes = blake3::hash(passphrase.as_bytes());
        let key = key_bytes.as_bytes();
        data.iter()
            .enumerate()
            .map(|(i, b)| b ^ key[i % 32])
            .collect()
    }
}
