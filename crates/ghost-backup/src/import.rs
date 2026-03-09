//! Backup import — decrypt, verify, staged restore (Req 30 AC4).

#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
#[cfg(test)]
use std::io::Cursor;
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use age::secrecy::SecretString as AgeSecretString;
use age::Decryptor;
use uuid::Uuid;

use crate::export::{require_passphrase, verify_sqlite_snapshot};
use crate::{
    BackupError, BackupManifest, BackupResult, ManifestEntry, ARCHIVE_MAGIC,
    CURRENT_BACKUP_FORMAT_VERSION,
};

const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
const STREAM_BUFFER_BYTES: usize = 64 * 1024;

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
        let passphrase = require_passphrase(passphrase)?;
        let encrypted = fs::read(archive_path)?;
        with_decrypted_archive_reader(&encrypted, passphrase, |reader| {
            let mut sink = NoopSink;
            process_archive_reader(reader, &mut sink)
        })
    }

    /// Import from a backup archive into a fresh restore target.
    pub fn import(
        &self,
        archive_path: &std::path::Path,
        passphrase: &str,
    ) -> BackupResult<BackupManifest> {
        if self.ghost_dir.exists() {
            return Err(BackupError::InvalidRestoreTarget(format!(
                "restore target {} already exists; in-place restore is disabled",
                self.ghost_dir.display()
            )));
        }

        let passphrase = require_passphrase(passphrase)?;
        let encrypted = fs::read(archive_path)?;
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

        let restore_result = with_decrypted_archive_reader(&encrypted, passphrase, |reader| {
            let mut sink = RestoreSink::new(&stage_root);
            process_archive_reader(reader, &mut sink)
        });

        match restore_result {
            Ok(manifest) => finalize_restore_stage(&stage_root, &self.ghost_dir, manifest),
            Err(error) => {
                cleanup_restore_stage(&stage_root);
                Err(error)
            }
        }
    }
}

fn finalize_restore_stage(
    stage_root: &Path,
    restore_target: &Path,
    manifest: BackupManifest,
) -> BackupResult<BackupManifest> {
    match fs::rename(stage_root, restore_target) {
        Ok(()) => {
            tracing::info!(
                entries = manifest.entries.len(),
                target = %restore_target.display(),
                "Backup imported and restored into fresh target"
            );
            Ok(manifest)
        }
        Err(error) => {
            cleanup_restore_stage(stage_root);
            Err(BackupError::Io(error))
        }
    }
}

fn cleanup_restore_stage(stage_root: &Path) {
    match fs::remove_dir_all(stage_root) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                path = %stage_root.display(),
                error = %error,
                "failed to clean up restore staging directory"
            );
        }
    }
}

trait ArchiveEntrySink {
    type State;

    fn begin_entry(&mut self, entry: &ManifestEntry) -> BackupResult<Self::State>;
    fn write_chunk(&mut self, state: &mut Self::State, chunk: &[u8]) -> BackupResult<()>;
    fn finish_entry(&mut self, state: Self::State, entry: &ManifestEntry) -> BackupResult<()>;
}

struct NoopSink;

impl ArchiveEntrySink for NoopSink {
    type State = ();

    fn begin_entry(&mut self, _entry: &ManifestEntry) -> BackupResult<Self::State> {
        Ok(())
    }

    fn write_chunk(&mut self, _state: &mut Self::State, _chunk: &[u8]) -> BackupResult<()> {
        Ok(())
    }

    fn finish_entry(&mut self, _state: Self::State, _entry: &ManifestEntry) -> BackupResult<()> {
        Ok(())
    }
}

#[cfg(test)]
struct CollectSink {
    files: BTreeMap<String, Vec<u8>>,
}

#[cfg(test)]
impl CollectSink {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
impl ArchiveEntrySink for CollectSink {
    type State = Vec<u8>;

    fn begin_entry(&mut self, entry: &ManifestEntry) -> BackupResult<Self::State> {
        let capacity = usize::try_from(entry.size).map_err(|_| {
            BackupError::IntegrityError(format!("entry too large for memory: {}", entry.path))
        })?;
        Ok(Vec::with_capacity(capacity))
    }

    fn write_chunk(&mut self, state: &mut Self::State, chunk: &[u8]) -> BackupResult<()> {
        state.extend_from_slice(chunk);
        Ok(())
    }

    fn finish_entry(&mut self, state: Self::State, entry: &ManifestEntry) -> BackupResult<()> {
        self.files.insert(entry.path.clone(), state);
        Ok(())
    }
}

struct RestoreSink {
    stage_root: PathBuf,
}

struct RestoreEntryState {
    file: fs::File,
    target: PathBuf,
}

impl RestoreSink {
    fn new(stage_root: &Path) -> Self {
        Self {
            stage_root: stage_root.to_path_buf(),
        }
    }
}

impl ArchiveEntrySink for RestoreSink {
    type State = RestoreEntryState;

    fn begin_entry(&mut self, entry: &ManifestEntry) -> BackupResult<Self::State> {
        let sanitized = sanitize_entry_path(&entry.path)?;
        let target = self.stage_root.join(&sanitized);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(&target)?;
        Ok(RestoreEntryState { file, target })
    }

    fn write_chunk(&mut self, state: &mut Self::State, chunk: &[u8]) -> BackupResult<()> {
        state.file.write_all(chunk)?;
        Ok(())
    }

    fn finish_entry(&mut self, mut state: Self::State, entry: &ManifestEntry) -> BackupResult<()> {
        state.file.flush()?;
        state.file.sync_all()?;
        drop(state.file);

        if entry.path.starts_with("data/") && entry.path.ends_with(".db") {
            verify_sqlite_snapshot(&state.target, &state.target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn read_archive(
    archive_path: &Path,
    passphrase: &str,
) -> BackupResult<(BackupManifest, BTreeMap<String, Vec<u8>>)> {
    let passphrase = require_passphrase(passphrase)?;
    let encrypted = fs::read(archive_path)?;
    with_decrypted_archive_reader(&encrypted, passphrase, |reader| {
        let mut sink = CollectSink::new();
        let manifest = process_archive_reader(reader, &mut sink)?;
        Ok((manifest, sink.files))
    })
}

fn with_decrypted_archive_reader<T>(
    encrypted: &[u8],
    passphrase: &str,
    f: impl FnOnce(&mut dyn Read) -> BackupResult<T>,
) -> BackupResult<T> {
    let decryptor = Decryptor::new(encrypted).map_err(|error| {
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
    f(&mut reader)
}

fn process_archive_reader<S: ArchiveEntrySink>(
    reader: &mut dyn Read,
    sink: &mut S,
) -> BackupResult<BackupManifest> {
    let mut magic = vec![0u8; ARCHIVE_MAGIC.len()];
    reader.read_exact(&mut magic).map_err(|error| {
        BackupError::IntegrityError(format!("archive truncated while reading header: {error}"))
    })?;
    if magic != ARCHIVE_MAGIC {
        return Err(BackupError::UnsupportedArchive(
            "legacy or unknown backup archive format".to_string(),
        ));
    }

    let manifest_len = read_u64_from_reader(reader)?;
    let manifest_len = usize::try_from(manifest_len)
        .map_err(|_| BackupError::IntegrityError("manifest too large".to_string()))?;
    if manifest_len > MAX_MANIFEST_BYTES {
        return Err(BackupError::IntegrityError(format!(
            "manifest exceeds {} bytes",
            MAX_MANIFEST_BYTES
        )));
    }

    let mut manifest_json = vec![0u8; manifest_len];
    reader
        .read_exact(&mut manifest_json)
        .map_err(|error| BackupError::IntegrityError(format!("manifest truncated: {error}")))?;
    let manifest: BackupManifest = serde_json::from_slice(&manifest_json)
        .map_err(|error| BackupError::IntegrityError(format!("manifest parse: {error}")))?;

    if manifest.version != CURRENT_BACKUP_FORMAT_VERSION {
        return Err(BackupError::VersionMismatch {
            archive: manifest.version.clone(),
            current: CURRENT_BACKUP_FORMAT_VERSION.to_string(),
        });
    }

    let mut seen_paths = BTreeSet::new();
    let mut buffer = vec![0u8; STREAM_BUFFER_BYTES];

    for entry in &manifest.entries {
        if !seen_paths.insert(entry.path.clone()) {
            return Err(BackupError::IntegrityError(format!(
                "duplicate manifest entry: {}",
                entry.path
            )));
        }

        let file_len = read_u64_from_reader(reader)?;
        if file_len != entry.size {
            return Err(BackupError::IntegrityError(format!(
                "size mismatch for {}: manifest={}, actual={}",
                entry.path, entry.size, file_len
            )));
        }

        let mut sink_state = sink.begin_entry(entry)?;
        let mut remaining = usize::try_from(file_len)
            .map_err(|_| BackupError::IntegrityError(format!("entry too large: {}", entry.path)))?;
        let mut hasher = blake3::Hasher::new();

        while remaining > 0 {
            let chunk_len = remaining.min(buffer.len());
            reader
                .read_exact(&mut buffer[..chunk_len])
                .map_err(|error| {
                    BackupError::IntegrityError(format!(
                        "file data truncated for {}: {error}",
                        entry.path
                    ))
                })?;
            let chunk = &buffer[..chunk_len];
            hasher.update(chunk);
            sink.write_chunk(&mut sink_state, chunk)?;
            remaining -= chunk_len;
        }

        let hash = hasher.finalize().to_hex().to_string();
        if hash != entry.blake3_hash {
            return Err(BackupError::IntegrityError(format!(
                "hash mismatch for {}",
                entry.path
            )));
        }

        sink.finish_entry(sink_state, entry)?;
    }

    let mut trailing = [0u8; 1];
    if reader
        .read(&mut trailing)
        .map_err(|error| BackupError::IntegrityError(format!("archive trailing read: {error}")))?
        != 0
    {
        return Err(BackupError::IntegrityError(
            "archive contains unexpected trailing data".to_string(),
        ));
    }

    Ok(manifest)
}

#[cfg(test)]
pub(crate) fn parse_archive_bytes(
    raw: &[u8],
) -> BackupResult<(BackupManifest, BTreeMap<String, Vec<u8>>)> {
    let mut cursor = Cursor::new(raw);
    let mut sink = CollectSink::new();
    let manifest = process_archive_reader(&mut cursor, &mut sink)?;
    Ok((manifest, sink.files))
}

fn read_u64_from_reader(reader: &mut dyn Read) -> BackupResult<u64> {
    let mut len_bytes = [0u8; 8];
    reader.read_exact(&mut len_bytes).map_err(|error| {
        BackupError::IntegrityError(format!("archive truncated while reading length: {error}"))
    })?;
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

    use tempfile::TempDir;

    use crate::export::{encode_archive, encrypt_archive};

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

    #[test]
    fn import_rejects_invalid_sqlite_snapshot() {
        let tmp = TempDir::new().unwrap();
        let archive = tmp.path().join("bad-sqlite.ghost-backup");
        let restore_target = tmp.path().join("restore-target");

        let payload = b"not a sqlite database";
        let mut files = BTreeMap::new();
        files.insert("data/ghost.db".to_string(), payload.to_vec());
        let manifest = manifest_for("data/ghost.db", payload);
        let raw = encode_archive(&manifest, &files).unwrap();
        let encrypted = encrypt_archive(&raw, "test-pass").unwrap();
        fs::write(&archive, encrypted).unwrap();

        let error = BackupImporter::new(&restore_target)
            .import(&archive, "test-pass")
            .unwrap_err();
        assert!(matches!(error, BackupError::IntegrityError(_)));
        assert!(!restore_target.exists());
    }

    #[test]
    fn finalize_restore_stage_cleans_up_staging_on_rename_failure() {
        let tmp = TempDir::new().unwrap();
        let stage_root = tmp.path().join(".ghost-restore-stage");
        let restore_target = tmp.path().join("restore-target");
        fs::create_dir_all(stage_root.join("data")).unwrap();
        fs::write(stage_root.join("data/file.txt"), b"ok").unwrap();
        fs::create_dir_all(restore_target.join("occupied")).unwrap();
        fs::write(restore_target.join("occupied/existing.txt"), b"busy").unwrap();

        let manifest = manifest_for("data/file.txt", b"ok");
        let error = finalize_restore_stage(&stage_root, &restore_target, manifest).unwrap_err();

        assert!(matches!(error, BackupError::Io(_)));
        assert!(!stage_root.exists());
        assert!(restore_target.exists());
    }
}
