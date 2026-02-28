//! Backup export — collect, compress, encrypt, archive (Req 30 AC3).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{BackupError, BackupManifest, BackupResult, ManifestEntry};

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
    /// Collects: SQLite DB, identity files, skills, config, baselines,
    /// session history, signing keys.
    pub fn export(&self, output_path: &Path, passphrase: &str) -> BackupResult<BackupManifest> {
        let mut entries = Vec::new();
        let mut archive_data: BTreeMap<String, Vec<u8>> = BTreeMap::new();

        // Collect files from ghost directory
        let collect_dirs = ["data", "config", "agents"];
        for dir_name in &collect_dirs {
            let dir = self.ghost_dir.join(dir_name);
            if dir.exists() {
                self.collect_dir(&dir, dir_name, &mut entries, &mut archive_data)?;
            }
        }

        let manifest = BackupManifest {
            version: "1".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            platform_version: env!("CARGO_PKG_VERSION").to_string(),
            entries,
        };

        // Serialize manifest + data
        let manifest_json = serde_json::to_vec(&manifest)
            .map_err(|e| BackupError::SerializationError(e.to_string()))?;
        let data_json = serde_json::to_vec(&archive_data)
            .map_err(|e| BackupError::SerializationError(e.to_string()))?;

        // Combine: [manifest_len(4 bytes)][manifest][data]
        let manifest_len = (manifest_json.len() as u32).to_le_bytes();
        let mut raw = Vec::new();
        raw.extend_from_slice(&manifest_len);
        raw.extend_from_slice(&manifest_json);
        raw.extend_from_slice(&data_json);

        // Encrypt with passphrase (XOR-based placeholder — production would use age)
        let encrypted = Self::encrypt(&raw, passphrase);

        // Write to output
        let mut file = fs::File::create(output_path)?;
        file.write_all(&encrypted)?;

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
        entries: &mut Vec<ManifestEntry>,
        archive: &mut BTreeMap<String, Vec<u8>>,
    ) -> BackupResult<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
            if path.is_file() {
                let data = fs::read(&path)?;
                let hash = blake3::hash(&data).to_hex().to_string();
                entries.push(ManifestEntry {
                    path: rel.clone(),
                    size: data.len() as u64,
                    blake3_hash: hash,
                });
                archive.insert(rel, data);
            } else if path.is_dir() {
                self.collect_dir(&path, &rel, entries, archive)?;
            }
        }
        Ok(())
    }

    /// Simple XOR encryption placeholder. Production uses `age` crate.
    fn encrypt(data: &[u8], passphrase: &str) -> Vec<u8> {
        let key_bytes = blake3::hash(passphrase.as_bytes());
        let key = key_bytes.as_bytes();
        data.iter()
            .enumerate()
            .map(|(i, b)| b ^ key[i % 32])
            .collect()
    }
}
