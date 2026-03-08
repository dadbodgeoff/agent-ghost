use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};

const MANIFEST_SOURCE_FILE: &str = "skill.json";
pub const ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const SIGNATURE_ALGORITHM: &str = "ed25519";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactExecutionMode {
    Native,
    Wasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSourceKind {
    User,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSignatureEnvelope {
    pub key_id: String,
    pub algorithm: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillManifestSource {
    #[serde(default = "default_manifest_schema_version")]
    pub manifest_schema_version: u32,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub source_kind: ArtifactSourceKind,
    pub execution_mode: ArtifactExecutionMode,
    pub entrypoint: String,
    #[serde(default)]
    pub requested_capabilities: Vec<String>,
    pub declared_privileges: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillManifestV1 {
    pub manifest_schema_version: u32,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub source_kind: ArtifactSourceKind,
    pub execution_mode: ArtifactExecutionMode,
    pub entrypoint: String,
    pub requested_capabilities: Vec<String>,
    pub declared_privileges: Vec<String>,
    pub content_digests: BTreeMap<String, String>,
    pub signature: SkillSignatureEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillArtifactFile {
    pub logical_path: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillArtifact {
    #[serde(default = "default_artifact_schema_version")]
    pub artifact_schema_version: u32,
    pub manifest: SkillManifestV1,
    #[serde(default)]
    pub files: Vec<SkillArtifactFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedSkillArtifact {
    pub manifest: SkillManifestV1,
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("invalid artifact schema version {0}")]
    InvalidArtifactSchemaVersion(u32),
    #[error("invalid manifest schema version {0}")]
    InvalidManifestSchemaVersion(u32),
    #[error("missing required field '{0}'")]
    MissingField(&'static str),
    #[error("invalid logical path '{path}': {reason}")]
    InvalidLogicalPath { path: String, reason: String },
    #[error("duplicate logical path '{0}'")]
    DuplicateLogicalPath(String),
    #[error("entrypoint '{0}' is not present in the artifact payload")]
    MissingEntrypoint(String),
    #[error("unsupported signature algorithm '{0}'")]
    UnsupportedSignatureAlgorithm(String),
    #[error("signature missing")]
    MissingSignature,
    #[error("signature decoding failed: {0}")]
    SignatureDecoding(String),
    #[error("signer key id missing")]
    MissingSignerKeyId,
    #[error("base64 decoding failed for '{path}': {error}")]
    Base64Decode { path: String, error: String },
    #[error("content digest mismatch for '{path}'")]
    DigestMismatch { path: String },
    #[error("signature verification failed")]
    SignatureVerificationFailed,
    #[error("source manifest file '{0}' not found")]
    MissingSourceManifest(String),
    #[error("symlinks are not allowed in skill package sources: {0}")]
    SymlinkNotAllowed(String),
}

#[derive(Debug, Clone, Serialize)]
struct UnsignedSkillManifestV1<'a> {
    manifest_schema_version: u32,
    name: &'a str,
    version: &'a str,
    publisher: &'a str,
    description: &'a str,
    source_kind: ArtifactSourceKind,
    execution_mode: ArtifactExecutionMode,
    entrypoint: &'a str,
    requested_capabilities: &'a [String],
    declared_privileges: &'a [String],
    content_digests: &'a BTreeMap<String, String>,
}

impl SkillArtifact {
    pub fn build_from_directory(
        source_root: &Path,
        signing_key: &ghost_signing::SigningKey,
    ) -> Result<Self, ArtifactError> {
        let manifest_path = source_root.join(MANIFEST_SOURCE_FILE);
        if !manifest_path.exists() {
            return Err(ArtifactError::MissingSourceManifest(
                manifest_path.display().to_string(),
            ));
        }

        let manifest_source: SkillManifestSource = serde_json::from_slice(
            &fs::read(&manifest_path).map_err(|e| ArtifactError::Io(e.to_string()))?,
        )
        .map_err(|e| ArtifactError::Serialization(e.to_string()))?;

        let mut files = BTreeMap::new();
        collect_source_files(source_root, source_root, &mut files)?;
        files.remove(MANIFEST_SOURCE_FILE);

        Self::build(manifest_source, files, signing_key)
    }

    pub fn build(
        manifest_source: SkillManifestSource,
        files: BTreeMap<String, Vec<u8>>,
        signing_key: &ghost_signing::SigningKey,
    ) -> Result<Self, ArtifactError> {
        validate_manifest_source(&manifest_source)?;

        let mut content_digests = BTreeMap::new();
        let mut encoded_files = Vec::new();
        let mut seen_paths = BTreeSet::new();
        for (logical_path, bytes) in files {
            validate_logical_path(&logical_path)?;
            if !seen_paths.insert(logical_path.clone()) {
                return Err(ArtifactError::DuplicateLogicalPath(logical_path));
            }
            content_digests.insert(
                logical_path.clone(),
                blake3::hash(&bytes).to_hex().to_string(),
            );
            encoded_files.push(SkillArtifactFile {
                logical_path,
                content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            });
        }

        if !content_digests.contains_key(&manifest_source.entrypoint) {
            return Err(ArtifactError::MissingEntrypoint(
                manifest_source.entrypoint.clone(),
            ));
        }

        let key_id = blake3::hash(&signing_key.verifying_key().to_bytes())
            .to_hex()
            .to_string();
        let unsigned = UnsignedSkillManifestV1 {
            manifest_schema_version: MANIFEST_SCHEMA_VERSION,
            name: &manifest_source.name,
            version: &manifest_source.version,
            publisher: &manifest_source.publisher,
            description: &manifest_source.description,
            source_kind: manifest_source.source_kind,
            execution_mode: manifest_source.execution_mode,
            entrypoint: &manifest_source.entrypoint,
            requested_capabilities: &manifest_source.requested_capabilities,
            declared_privileges: &manifest_source.declared_privileges,
            content_digests: &content_digests,
        };
        let signing_bytes = serde_json::to_vec(&unsigned)
            .map_err(|e| ArtifactError::Serialization(e.to_string()))?;
        let signature = ghost_signing::sign(&signing_bytes, signing_key);

        let artifact = Self {
            artifact_schema_version: ARTIFACT_SCHEMA_VERSION,
            manifest: SkillManifestV1 {
                manifest_schema_version: MANIFEST_SCHEMA_VERSION,
                name: manifest_source.name,
                version: manifest_source.version,
                publisher: manifest_source.publisher,
                description: manifest_source.description,
                source_kind: manifest_source.source_kind,
                execution_mode: manifest_source.execution_mode,
                entrypoint: manifest_source.entrypoint,
                requested_capabilities: manifest_source.requested_capabilities,
                declared_privileges: manifest_source.declared_privileges,
                content_digests,
                signature: SkillSignatureEnvelope {
                    key_id,
                    algorithm: SIGNATURE_ALGORITHM.to_string(),
                    signature: base64::engine::general_purpose::STANDARD
                        .encode(signature.to_bytes()),
                },
            },
            files: encoded_files,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn read_from_path(path: &Path) -> Result<Self, ArtifactError> {
        let bytes = fs::read(path).map_err(|e| ArtifactError::Io(e.to_string()))?;
        Self::read_from_bytes(&bytes)
    }

    pub fn read_from_bytes(bytes: &[u8]) -> Result<Self, ArtifactError> {
        serde_json::from_slice(bytes).map_err(|e| ArtifactError::Serialization(e.to_string()))
    }

    pub fn write_to_path(&self, path: &Path) -> Result<(), ArtifactError> {
        let bytes = self.canonical_bytes()?;
        fs::write(path, bytes).map_err(|e| ArtifactError::Io(e.to_string()))
    }

    pub fn artifact_digest(&self) -> Result<String, ArtifactError> {
        Ok(blake3::hash(&self.canonical_bytes()?).to_hex().to_string())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        let mut files = self.files.clone();
        files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
        serde_json::to_vec(&SkillArtifact {
            artifact_schema_version: self.artifact_schema_version,
            manifest: self.manifest.clone(),
            files,
        })
        .map_err(|e| ArtifactError::Serialization(e.to_string()))
    }

    pub fn validate(&self) -> Result<DecodedSkillArtifact, ArtifactError> {
        if self.artifact_schema_version != ARTIFACT_SCHEMA_VERSION {
            return Err(ArtifactError::InvalidArtifactSchemaVersion(
                self.artifact_schema_version,
            ));
        }
        validate_manifest(&self.manifest)?;

        let mut seen_paths = BTreeSet::new();
        let mut decoded = BTreeMap::new();
        for file in &self.files {
            validate_logical_path(&file.logical_path)?;
            if !seen_paths.insert(file.logical_path.clone()) {
                return Err(ArtifactError::DuplicateLogicalPath(
                    file.logical_path.clone(),
                ));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&file.content_base64)
                .map_err(|e| ArtifactError::Base64Decode {
                    path: file.logical_path.clone(),
                    error: e.to_string(),
                })?;
            let actual = blake3::hash(&bytes).to_hex().to_string();
            let expected = self
                .manifest
                .content_digests
                .get(&file.logical_path)
                .ok_or_else(|| ArtifactError::DigestMismatch {
                    path: file.logical_path.clone(),
                })?;
            if actual != *expected {
                return Err(ArtifactError::DigestMismatch {
                    path: file.logical_path.clone(),
                });
            }
            decoded.insert(file.logical_path.clone(), bytes);
        }

        let expected_paths: BTreeSet<_> = self.manifest.content_digests.keys().cloned().collect();
        if expected_paths != seen_paths {
            let missing = expected_paths
                .difference(&seen_paths)
                .next()
                .cloned()
                .unwrap_or_else(|| self.manifest.entrypoint.clone());
            return Err(ArtifactError::DigestMismatch { path: missing });
        }
        if !decoded.contains_key(&self.manifest.entrypoint) {
            return Err(ArtifactError::MissingEntrypoint(
                self.manifest.entrypoint.clone(),
            ));
        }

        Ok(DecodedSkillArtifact {
            manifest: self.manifest.clone(),
            files: decoded,
        })
    }

    pub fn verify_signature(
        &self,
        verifying_key: &ghost_signing::VerifyingKey,
    ) -> Result<(), ArtifactError> {
        let decoded = self.validate()?;
        let signature = decode_signature(&decoded.manifest.signature)?;
        let unsigned = UnsignedSkillManifestV1 {
            manifest_schema_version: decoded.manifest.manifest_schema_version,
            name: &decoded.manifest.name,
            version: &decoded.manifest.version,
            publisher: &decoded.manifest.publisher,
            description: &decoded.manifest.description,
            source_kind: decoded.manifest.source_kind,
            execution_mode: decoded.manifest.execution_mode,
            entrypoint: &decoded.manifest.entrypoint,
            requested_capabilities: &decoded.manifest.requested_capabilities,
            declared_privileges: &decoded.manifest.declared_privileges,
            content_digests: &decoded.manifest.content_digests,
        };
        let signing_bytes = serde_json::to_vec(&unsigned)
            .map_err(|e| ArtifactError::Serialization(e.to_string()))?;
        if ghost_signing::verify(&signing_bytes, &signature, verifying_key) {
            Ok(())
        } else {
            Err(ArtifactError::SignatureVerificationFailed)
        }
    }
}

fn decode_signature(
    envelope: &SkillSignatureEnvelope,
) -> Result<ghost_signing::Signature, ArtifactError> {
    if envelope.key_id.trim().is_empty() {
        return Err(ArtifactError::MissingSignerKeyId);
    }
    if envelope.algorithm != SIGNATURE_ALGORITHM {
        return Err(ArtifactError::UnsupportedSignatureAlgorithm(
            envelope.algorithm.clone(),
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&envelope.signature)
        .map_err(|e| ArtifactError::SignatureDecoding(e.to_string()))?;
    ghost_signing::Signature::from_bytes(&bytes)
        .ok_or_else(|| ArtifactError::SignatureDecoding("expected 64-byte signature".to_string()))
}

fn collect_source_files(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), ArtifactError> {
    let mut entries = fs::read_dir(current)
        .map_err(|e| ArtifactError::Io(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ArtifactError::Io(e.to_string()))?;
    entries.sort_by_key(|left| left.path());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| ArtifactError::Io(e.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(ArtifactError::SymlinkNotAllowed(path.display().to_string()));
        }
        if metadata.is_dir() {
            collect_source_files(root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }

        let logical = logical_path(root, &path)?;
        if files.contains_key(&logical) {
            return Err(ArtifactError::DuplicateLogicalPath(logical));
        }
        let bytes = fs::read(&path).map_err(|e| ArtifactError::Io(e.to_string()))?;
        files.insert(logical, bytes);
    }

    Ok(())
}

fn logical_path(root: &Path, path: &Path) -> Result<String, ArtifactError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|e| ArtifactError::Io(e.to_string()))?;
    let mut normalized = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            _ => {
                return Err(ArtifactError::InvalidLogicalPath {
                    path: relative.display().to_string(),
                    reason: "path must stay within the source root".to_string(),
                });
            }
        }
    }
    let logical = normalized
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    validate_logical_path(&logical)?;
    Ok(logical)
}

fn validate_manifest_source(manifest: &SkillManifestSource) -> Result<(), ArtifactError> {
    if manifest.manifest_schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(ArtifactError::InvalidManifestSchemaVersion(
            manifest.manifest_schema_version,
        ));
    }
    require_field("name", &manifest.name)?;
    require_field("version", &manifest.version)?;
    require_field("publisher", &manifest.publisher)?;
    require_field("description", &manifest.description)?;
    require_field("entrypoint", &manifest.entrypoint)?;
    validate_logical_path(&manifest.entrypoint)?;
    if manifest.declared_privileges.is_empty() {
        return Err(ArtifactError::MissingField("declared_privileges"));
    }
    for privilege in &manifest.declared_privileges {
        require_field("declared_privileges[]", privilege)?;
    }
    for capability in &manifest.requested_capabilities {
        require_field("requested_capabilities[]", capability)?;
    }
    Ok(())
}

fn validate_manifest(manifest: &SkillManifestV1) -> Result<(), ArtifactError> {
    validate_manifest_source(&SkillManifestSource {
        manifest_schema_version: manifest.manifest_schema_version,
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        publisher: manifest.publisher.clone(),
        description: manifest.description.clone(),
        source_kind: manifest.source_kind,
        execution_mode: manifest.execution_mode,
        entrypoint: manifest.entrypoint.clone(),
        requested_capabilities: manifest.requested_capabilities.clone(),
        declared_privileges: manifest.declared_privileges.clone(),
    })?;
    if manifest.signature.signature.trim().is_empty() {
        return Err(ArtifactError::MissingSignature);
    }
    decode_signature(&manifest.signature)?;
    for logical_path in manifest.content_digests.keys() {
        validate_logical_path(logical_path)?;
    }
    Ok(())
}

fn validate_logical_path(path: &str) -> Result<(), ArtifactError> {
    if path.trim().is_empty() {
        return Err(ArtifactError::MissingField("logical_path"));
    }
    let raw = Path::new(path);
    let mut saw_segment = false;
    for component in raw.components() {
        match component {
            Component::Normal(segment) => {
                saw_segment = true;
                if segment.is_empty() {
                    return Err(ArtifactError::InvalidLogicalPath {
                        path: path.to_string(),
                        reason: "empty segment".to_string(),
                    });
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(ArtifactError::InvalidLogicalPath {
                    path: path.to_string(),
                    reason: "parent-directory traversal is not allowed".to_string(),
                });
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ArtifactError::InvalidLogicalPath {
                    path: path.to_string(),
                    reason: "absolute paths are not allowed".to_string(),
                });
            }
        }
    }
    if !saw_segment {
        return Err(ArtifactError::InvalidLogicalPath {
            path: path.to_string(),
            reason: "path must contain at least one normal segment".to_string(),
        });
    }
    Ok(())
}

fn require_field(name: &'static str, value: &str) -> Result<(), ArtifactError> {
    if value.trim().is_empty() {
        return Err(ArtifactError::MissingField(name));
    }
    Ok(())
}

fn default_artifact_schema_version() -> u32 {
    ARTIFACT_SCHEMA_VERSION
}

fn default_manifest_schema_version() -> u32 {
    MANIFEST_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> SkillManifestSource {
        SkillManifestSource {
            manifest_schema_version: MANIFEST_SCHEMA_VERSION,
            name: "echo".to_string(),
            version: "1.0.0".to_string(),
            publisher: "ghost-test".to_string(),
            description: "Echoes JSON".to_string(),
            source_kind: ArtifactSourceKind::Workspace,
            execution_mode: ArtifactExecutionMode::Wasm,
            entrypoint: "module.wasm".to_string(),
            requested_capabilities: Vec::new(),
            declared_privileges: vec![
                "Pure WASM computation over caller-provided JSON input without host access"
                    .to_string(),
            ],
        }
    }

    fn sample_files() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("module.wasm".to_string(), b"\0asmfake".to_vec()),
            ("README.md".to_string(), b"docs".to_vec()),
        ])
    }

    #[test]
    fn digest_is_stable_across_repeated_builds() {
        let (signing_key, _) = ghost_signing::generate_keypair();
        let artifact_a =
            SkillArtifact::build(sample_manifest(), sample_files(), &signing_key).unwrap();
        let artifact_b =
            SkillArtifact::build(sample_manifest(), sample_files(), &signing_key).unwrap();

        assert_eq!(
            artifact_a.canonical_bytes().unwrap(),
            artifact_b.canonical_bytes().unwrap()
        );
        assert_eq!(
            artifact_a.artifact_digest().unwrap(),
            artifact_b.artifact_digest().unwrap()
        );
    }

    #[test]
    fn unknown_manifest_schema_version_fails_closed() {
        let (signing_key, _) = ghost_signing::generate_keypair();
        let mut manifest = sample_manifest();
        manifest.manifest_schema_version = 999;

        let error = SkillArtifact::build(manifest, sample_files(), &signing_key).unwrap_err();
        assert!(matches!(
            error,
            ArtifactError::InvalidManifestSchemaVersion(999)
        ));
    }

    #[test]
    fn duplicate_logical_paths_are_rejected() {
        let (signing_key, _) = ghost_signing::generate_keypair();
        let artifact =
            SkillArtifact::build(sample_manifest(), sample_files(), &signing_key).unwrap();
        let mut tampered = artifact.clone();
        tampered.files.push(tampered.files[0].clone());

        let error = tampered.validate().unwrap_err();
        assert!(matches!(error, ArtifactError::DuplicateLogicalPath(_)));
    }

    #[test]
    fn path_traversal_is_rejected() {
        let (signing_key, _) = ghost_signing::generate_keypair();
        let files = BTreeMap::from([("../escape.wasm".to_string(), b"bad".to_vec())]);
        let error = SkillArtifact::build(sample_manifest(), files, &signing_key).unwrap_err();
        assert!(matches!(error, ArtifactError::InvalidLogicalPath { .. }));
    }

    #[test]
    fn tampering_after_signing_breaks_validation() {
        let (signing_key, verifying_key) = ghost_signing::generate_keypair();
        let artifact =
            SkillArtifact::build(sample_manifest(), sample_files(), &signing_key).unwrap();
        let mut tampered = artifact.clone();
        tampered.files[0].content_base64 =
            base64::engine::general_purpose::STANDARD.encode(b"different module");

        let error = tampered.verify_signature(&verifying_key).unwrap_err();
        assert!(matches!(error, ArtifactError::DigestMismatch { .. }));
    }

    #[test]
    fn verification_rejects_wrong_key() {
        let (signing_key, _) = ghost_signing::generate_keypair();
        let (_, wrong_verifying_key) = ghost_signing::generate_keypair();
        let artifact =
            SkillArtifact::build(sample_manifest(), sample_files(), &signing_key).unwrap();

        let error = artifact.verify_signature(&wrong_verifying_key).unwrap_err();
        assert!(matches!(error, ArtifactError::SignatureVerificationFailed));
    }

    #[test]
    fn source_directory_packaging_rejects_symlinks() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join(MANIFEST_SOURCE_FILE),
            serde_json::to_vec(&sample_manifest()).unwrap(),
        )
        .unwrap();
        fs::write(temp_dir.path().join("module.wasm"), b"wasm").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/tmp", temp_dir.path().join("evil")).unwrap();
            let (signing_key, _) = ghost_signing::generate_keypair();
            let error =
                SkillArtifact::build_from_directory(temp_dir.path(), &signing_key).unwrap_err();
            assert!(matches!(error, ArtifactError::SymlinkNotAllowed(_)));
        }
    }
}
