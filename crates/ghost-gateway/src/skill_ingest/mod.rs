use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use cortex_storage::queries::external_skill_queries::{
    self, ExternalSkillQuarantineState, ExternalSkillVerificationStatus, SkillSignerState,
};
use ghost_audit::query_engine::AuditError;
use ghost_skills::artifact::{
    ArtifactError, ArtifactExecutionMode, DecodedSkillArtifact, SkillArtifact,
};
use thiserror::Error;

use crate::bootstrap::shellexpand_tilde;
use crate::config::{
    ExternalSkillRootConfig, ExternalSkillSourceConfig, ExternalSkillsConfig,
    TrustedSkillSignerConfig,
};
use crate::db_pool::DbPool;

#[derive(Debug, Error)]
pub enum SkillIngestError {
    #[error("external skills are disabled")]
    Disabled,
    #[error("I/O error: {0}")]
    Io(String),
    #[error("artifact error: {0}")]
    Artifact(#[from] ArtifactError),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("invalid signer '{0}'")]
    InvalidSigner(String),
}

#[derive(Debug, Clone)]
pub struct IngestedSkillRecord {
    pub artifact_digest: String,
    pub skill_name: String,
    pub skill_version: String,
}

#[derive(Debug, Clone)]
pub struct SkillIngestFailure {
    pub path: PathBuf,
    pub source: ExternalSkillSourceConfig,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScanApprovedRootsReport {
    pub discovered: Vec<IngestedSkillRecord>,
    pub failures: Vec<SkillIngestFailure>,
}

const OPERATOR_QUARANTINE_REASON_CODE: &str = "operator_quarantine";

#[derive(Clone)]
pub struct SkillIngestService {
    db: Arc<DbPool>,
    config: ExternalSkillsConfig,
}

impl SkillIngestService {
    pub fn new(db: Arc<DbPool>, config: ExternalSkillsConfig) -> Self {
        Self { db, config }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn execution_enabled(&self) -> bool {
        self.config.enabled && self.config.execution_enabled
    }

    pub fn managed_storage_root(&self) -> PathBuf {
        PathBuf::from(shellexpand_tilde(&self.config.managed_storage_path))
    }

    pub async fn seed_trusted_signers(&self) -> Result<(), SkillIngestError> {
        if !self.config.enabled {
            return Ok(());
        }

        let db = self.db.write().await;
        for signer in &self.config.trusted_signers {
            let public_key = decode_signer_public_key(signer)?;
            external_skill_queries::upsert_skill_signer(
                &db,
                &signer.key_id,
                &signer.publisher,
                &public_key,
                if signer.revoked {
                    SkillSignerState::Revoked
                } else {
                    SkillSignerState::Trusted
                },
                Some("bootstrap"),
                signer.revoked.then_some("revoked in gateway configuration"),
            )
            .map_err(|e| SkillIngestError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn scan_approved_roots(
        &self,
        actor: &str,
    ) -> Result<ScanApprovedRootsReport, SkillIngestError> {
        if !self.config.enabled {
            return Ok(ScanApprovedRootsReport::default());
        }

        let mut report = ScanApprovedRootsReport::default();
        for root in &self.config.approved_roots {
            let root_path = PathBuf::from(shellexpand_tilde(&root.path));
            if !root_path.exists() {
                continue;
            }
            let mut artifact_paths = Vec::new();
            if let Err(error) = collect_artifacts_in_root(&root_path, &mut artifact_paths) {
                self.record_scan_failure(actor, &root_path, root.source, &error.to_string())
                    .await;
                report.failures.push(SkillIngestFailure {
                    path: root_path.clone(),
                    source: root.source,
                    error: error.to_string(),
                });
                continue;
            }
            for artifact_path in artifact_paths {
                match self
                    .ingest_artifact_from_path(&artifact_path, root, actor)
                    .await
                {
                    Ok(record) => report.discovered.push(record),
                    Err(error) => {
                        tracing::warn!(
                            path = %artifact_path.display(),
                            source = ?root.source,
                            error = %error,
                            "external skill ingestion failed"
                        );
                        self.record_scan_failure(
                            actor,
                            &artifact_path,
                            root.source,
                            &error.to_string(),
                        )
                        .await;
                        report.failures.push(SkillIngestFailure {
                            path: artifact_path,
                            source: root.source,
                            error: error.to_string(),
                        });
                    }
                }
            }
        }

        Ok(report)
    }

    pub async fn ingest_artifact_from_path(
        &self,
        artifact_path: &Path,
        root: &ExternalSkillRootConfig,
        actor: &str,
    ) -> Result<IngestedSkillRecord, SkillIngestError> {
        if !self.config.enabled {
            return Err(SkillIngestError::Disabled);
        }

        ensure_artifact_path_within_root(
            artifact_path,
            &PathBuf::from(shellexpand_tilde(&root.path)),
        )?;
        let artifact_bytes = read_artifact_bytes_no_follow(artifact_path)?;
        let artifact = SkillArtifact::read_from_bytes(&artifact_bytes)?;
        let decoded = artifact.validate()?;
        let artifact_digest = artifact.artifact_digest()?;
        let managed_dir = self.managed_storage_root().join(&artifact_digest);
        let artifact_output_path = managed_dir.join("artifact.ghostskill");
        let entrypoint_output_path = managed_dir.join(&decoded.manifest.entrypoint);

        fs::create_dir_all(&managed_dir).map_err(|e| SkillIngestError::Io(e.to_string()))?;
        artifact
            .write_to_path(&artifact_output_path)
            .map_err(SkillIngestError::Artifact)?;
        write_managed_payload(&managed_dir, &decoded)?;

        let verification = self
            .verify_ingested_artifact(&artifact, &decoded)
            .await
            .map_err(SkillIngestError::Artifact)?;

        let db = self.db.write().await;
        external_skill_queries::upsert_external_skill_artifact(
            &db,
            &artifact_digest,
            artifact.artifact_schema_version,
            &decoded.manifest.name,
            &decoded.manifest.version,
            &decoded.manifest.publisher,
            &decoded.manifest.description,
            match root.source {
                ExternalSkillSourceConfig::User => "user",
                ExternalSkillSourceConfig::Workspace => "workspace",
            },
            match decoded.manifest.execution_mode {
                ArtifactExecutionMode::Native => "native",
                ArtifactExecutionMode::Wasm => "wasm",
            },
            &decoded.manifest.entrypoint,
            &artifact_path.display().to_string(),
            &artifact_output_path.display().to_string(),
            &entrypoint_output_path.display().to_string(),
            &serde_json::to_string(&decoded.manifest)
                .map_err(|e| SkillIngestError::Io(e.to_string()))?,
            &serde_json::to_string(&decoded.manifest.requested_capabilities)
                .map_err(|e| SkillIngestError::Io(e.to_string()))?,
            &serde_json::to_string(&decoded.manifest.declared_privileges)
                .map_err(|e| SkillIngestError::Io(e.to_string()))?,
            Some(&decoded.manifest.signature.key_id),
            artifact_bytes.len() as i64,
        )
        .map_err(|e| SkillIngestError::Storage(e.to_string()))?;

        drop(db);
        self.persist_verification_state(&artifact_digest, &verification, actor)
            .await?;

        let audit_db = self.db.write().await;
        if let Err(error) = write_skill_audit(
            &audit_db,
            &format!("skill:{}", decoded.manifest.name),
            "skill_artifact_ingested",
            actor,
            serde_json::json!({
                "artifact_digest": artifact_digest,
                "publisher": decoded.manifest.publisher,
                "version": decoded.manifest.version,
                "source_kind": root.source,
                "verification_status": verification.status.as_str(),
                "quarantine_state": verification.quarantine_state.as_str(),
            }),
        ) {
            tracing::warn!(
                skill_name = %decoded.manifest.name,
                error = %error,
                "failed to persist external skill ingest audit entry"
            );
        }

        Ok(IngestedSkillRecord {
            artifact_digest,
            skill_name: decoded.manifest.name,
            skill_version: decoded.manifest.version,
        })
    }

    pub async fn reverify_managed_artifact(
        &self,
        artifact_digest: &str,
        actor: &str,
    ) -> Result<IngestedSkillRecord, SkillIngestError> {
        if !self.config.enabled {
            return Err(SkillIngestError::Disabled);
        }

        let artifact_row = {
            let db = self
                .db
                .read()
                .map_err(|error| SkillIngestError::Storage(error.to_string()))?;
            external_skill_queries::get_external_skill_artifact(&db, artifact_digest)
                .map_err(|error| SkillIngestError::Storage(error.to_string()))?
                .ok_or_else(|| {
                    SkillIngestError::Storage(format!(
                        "external skill artifact '{artifact_digest}' not found"
                    ))
                })?
        };

        let verification = self.reverify_outcome_for_artifact(&artifact_row).await;
        let quarantine_preserved = self
            .persist_verification_state(artifact_digest, &verification, actor)
            .await?;

        let audit_db = self.db.write().await;
        if let Err(error) = write_skill_audit(
            &audit_db,
            &format!("skill:{}", artifact_row.skill_name),
            "skill_artifact_reverified",
            actor,
            serde_json::json!({
                "artifact_digest": artifact_row.artifact_digest,
                "publisher": artifact_row.publisher,
                "version": artifact_row.skill_version,
                "verification_status": verification.status.as_str(),
                "quarantine_state": verification.quarantine_state.as_str(),
                "quarantine_preserved_operator_override": quarantine_preserved,
            }),
        ) {
            tracing::warn!(
                skill_name = %artifact_row.skill_name,
                error = %error,
                "failed to persist external skill reverify audit entry"
            );
        }

        Ok(IngestedSkillRecord {
            artifact_digest: artifact_row.artifact_digest,
            skill_name: artifact_row.skill_name,
            skill_version: artifact_row.skill_version,
        })
    }

    async fn verify_ingested_artifact(
        &self,
        artifact: &SkillArtifact,
        decoded: &DecodedSkillArtifact,
    ) -> Result<VerificationOutcome, ArtifactError> {
        let db = self
            .db
            .read()
            .map_err(|e| ArtifactError::Io(e.to_string()))?;
        let signer =
            external_skill_queries::get_skill_signer(&db, &decoded.manifest.signature.key_id)
                .map_err(|e| ArtifactError::Io(e.to_string()))?;

        let Some(signer) = signer else {
            return Ok(VerificationOutcome::quarantined(
                ExternalSkillVerificationStatus::UnknownSigner,
                None,
                None,
                "unknown_signer",
                "signer key id is not trusted by the gateway",
            ));
        };

        if signer.state == SkillSignerState::Revoked {
            return Ok(VerificationOutcome::quarantined(
                ExternalSkillVerificationStatus::RevokedSigner,
                Some(signer.key_id),
                Some(signer.publisher),
                "revoked_signer",
                signer
                    .revocation_reason
                    .as_deref()
                    .unwrap_or("signer has been revoked"),
            ));
        }

        let verifying_key = verifying_key_from_db(&signer.public_key)
            .map_err(|_| ArtifactError::SignatureVerificationFailed)?;
        if let Err(error) = artifact.verify_signature(&verifying_key) {
            return Ok(VerificationOutcome::quarantined(
                match error {
                    ArtifactError::MissingSignature => {
                        ExternalSkillVerificationStatus::MissingSignature
                    }
                    ArtifactError::DigestMismatch { .. } => {
                        ExternalSkillVerificationStatus::DigestMismatch
                    }
                    _ => ExternalSkillVerificationStatus::InvalidSignature,
                },
                Some(signer.key_id),
                Some(signer.publisher),
                "invalid_signature",
                &error.to_string(),
            ));
        }

        if decoded.manifest.execution_mode != ArtifactExecutionMode::Wasm {
            return Ok(VerificationOutcome::quarantined(
                ExternalSkillVerificationStatus::UnsupportedExecutionMode,
                Some(signer.key_id),
                Some(signer.publisher),
                "unsupported_execution_mode",
                "only external wasm execution is supported in this phase",
            ));
        }

        if !decoded.manifest.requested_capabilities.is_empty() {
            return Ok(VerificationOutcome::quarantined(
                ExternalSkillVerificationStatus::UnsupportedCapability,
                Some(signer.key_id),
                Some(signer.publisher),
                "unsupported_capability",
                "external wasm skills currently run without host capabilities",
            ));
        }

        Ok(VerificationOutcome {
            status: ExternalSkillVerificationStatus::Verified,
            signer_key_id: Some(signer.key_id),
            signer_publisher: Some(signer.publisher),
            details_json: "{}".to_string(),
            quarantine_state: ExternalSkillQuarantineState::Clear,
            quarantine_reason_code: None,
            quarantine_reason_detail: None,
        })
    }

    async fn record_scan_failure(
        &self,
        actor: &str,
        path: &Path,
        source: ExternalSkillSourceConfig,
        error: &str,
    ) {
        let db = self.db.write().await;
        if let Err(audit_error) = write_skill_audit(
            &db,
            "skill:external-ingest",
            "skill_artifact_ingest_failed",
            actor,
            serde_json::json!({
                "path": path.display().to_string(),
                "source_kind": source,
                "error": error,
            }),
        ) {
            tracing::warn!(
                path = %path.display(),
                error = %audit_error,
                "failed to persist external skill ingest failure audit entry"
            );
        }
    }

    async fn reverify_outcome_for_artifact(
        &self,
        artifact_row: &external_skill_queries::ExternalSkillArtifactRow,
    ) -> VerificationOutcome {
        let artifact_bytes =
            match read_artifact_bytes_no_follow(Path::new(&artifact_row.managed_artifact_path)) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return VerificationOutcome::quarantined(
                        ExternalSkillVerificationStatus::ValidationFailed,
                        artifact_row.signer_key_id.clone(),
                        None,
                        "managed_artifact_unreadable",
                        &error.to_string(),
                    );
                }
            };

        let artifact = match SkillArtifact::read_from_bytes(&artifact_bytes) {
            Ok(artifact) => artifact,
            Err(error) => {
                return verification_outcome_from_artifact_error(
                    artifact_row.signer_key_id.clone(),
                    None,
                    &error,
                    "managed_artifact_invalid",
                );
            }
        };
        let decoded = match artifact.validate() {
            Ok(decoded) => decoded,
            Err(error) => {
                return verification_outcome_from_artifact_error(
                    artifact_row.signer_key_id.clone(),
                    None,
                    &error,
                    "managed_artifact_invalid",
                );
            }
        };
        let actual_digest = match artifact.artifact_digest() {
            Ok(actual_digest) => actual_digest,
            Err(error) => {
                return verification_outcome_from_artifact_error(
                    artifact_row.signer_key_id.clone(),
                    None,
                    &error,
                    "managed_artifact_invalid",
                );
            }
        };

        if actual_digest != artifact_row.artifact_digest {
            return VerificationOutcome::quarantined(
                ExternalSkillVerificationStatus::DigestMismatch,
                Some(decoded.manifest.signature.key_id.clone()),
                None,
                "managed_artifact_digest_mismatch",
                &format!(
                    "managed artifact digest '{}' does not match persisted artifact digest '{}'",
                    actual_digest, artifact_row.artifact_digest
                ),
            );
        }

        match self.verify_ingested_artifact(&artifact, &decoded).await {
            Ok(verification) => verification,
            Err(error) => verification_outcome_from_artifact_error(
                Some(decoded.manifest.signature.key_id),
                None,
                &error,
                "managed_artifact_invalid",
            ),
        }
    }

    async fn persist_verification_state(
        &self,
        artifact_digest: &str,
        verification: &VerificationOutcome,
        actor: &str,
    ) -> Result<bool, SkillIngestError> {
        let db = self.db.write().await;
        external_skill_queries::upsert_external_skill_verification(
            &db,
            artifact_digest,
            verification.status,
            verification.signer_key_id.as_deref(),
            verification.signer_publisher.as_deref(),
            &verification.details_json,
        )
        .map_err(|e| SkillIngestError::Storage(e.to_string()))?;

        let existing_quarantine =
            external_skill_queries::get_external_skill_quarantine(&db, artifact_digest)
                .map_err(|e| SkillIngestError::Storage(e.to_string()))?;
        let preserve_operator_quarantine = existing_quarantine.as_ref().is_some_and(|row| {
            row.state == ExternalSkillQuarantineState::Quarantined
                && row.reason_code.as_deref() == Some(OPERATOR_QUARANTINE_REASON_CODE)
                && verification.quarantine_state == ExternalSkillQuarantineState::Clear
        });

        if !preserve_operator_quarantine {
            external_skill_queries::upsert_external_skill_quarantine(
                &db,
                artifact_digest,
                verification.quarantine_state,
                verification.quarantine_reason_code.as_deref(),
                verification.quarantine_reason_detail.as_deref(),
                Some(actor),
            )
            .map_err(|e| SkillIngestError::Storage(e.to_string()))?;
        }

        Ok(preserve_operator_quarantine)
    }
}

struct VerificationOutcome {
    status: ExternalSkillVerificationStatus,
    signer_key_id: Option<String>,
    signer_publisher: Option<String>,
    details_json: String,
    quarantine_state: ExternalSkillQuarantineState,
    quarantine_reason_code: Option<String>,
    quarantine_reason_detail: Option<String>,
}

impl VerificationOutcome {
    fn quarantined(
        status: ExternalSkillVerificationStatus,
        signer_key_id: Option<String>,
        signer_publisher: Option<String>,
        reason_code: &str,
        reason_detail: &str,
    ) -> Self {
        Self {
            status,
            signer_key_id,
            signer_publisher,
            details_json: serde_json::json!({ "reason": reason_detail }).to_string(),
            quarantine_state: ExternalSkillQuarantineState::Quarantined,
            quarantine_reason_code: Some(reason_code.to_string()),
            quarantine_reason_detail: Some(reason_detail.to_string()),
        }
    }
}

fn verification_outcome_from_artifact_error(
    signer_key_id: Option<String>,
    signer_publisher: Option<String>,
    error: &ArtifactError,
    fallback_reason_code: &str,
) -> VerificationOutcome {
    match error {
        ArtifactError::MissingSignature => VerificationOutcome::quarantined(
            ExternalSkillVerificationStatus::MissingSignature,
            signer_key_id,
            signer_publisher,
            "missing_signature",
            &error.to_string(),
        ),
        ArtifactError::DigestMismatch { .. } => VerificationOutcome::quarantined(
            ExternalSkillVerificationStatus::DigestMismatch,
            signer_key_id,
            signer_publisher,
            "digest_mismatch",
            &error.to_string(),
        ),
        _ => VerificationOutcome::quarantined(
            ExternalSkillVerificationStatus::ValidationFailed,
            signer_key_id,
            signer_publisher,
            fallback_reason_code,
            &error.to_string(),
        ),
    }
}

fn write_managed_payload(
    managed_dir: &Path,
    decoded: &DecodedSkillArtifact,
) -> Result<(), SkillIngestError> {
    for (logical_path, bytes) in &decoded.files {
        let destination = managed_dir.join(logical_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| SkillIngestError::Io(e.to_string()))?;
        }
        fs::write(&destination, bytes).map_err(|e| SkillIngestError::Io(e.to_string()))?;
    }
    Ok(())
}

fn ensure_artifact_path_within_root(
    artifact_path: &Path,
    configured_root: &Path,
) -> Result<(), SkillIngestError> {
    let canonical_root =
        fs::canonicalize(configured_root).map_err(|e| SkillIngestError::Io(e.to_string()))?;
    let parent = artifact_path.parent().ok_or_else(|| {
        SkillIngestError::Io(format!(
            "artifact path has no parent directory: {}",
            artifact_path.display()
        ))
    })?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|e| SkillIngestError::Io(e.to_string()))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(SkillIngestError::Io(format!(
            "artifact path is outside approved root: {}",
            artifact_path.display()
        )));
    }
    Ok(())
}

fn read_artifact_bytes_no_follow(path: &Path) -> Result<Vec<u8>, SkillIngestError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| SkillIngestError::Io(e.to_string()))?;
        let metadata = file
            .metadata()
            .map_err(|e| SkillIngestError::Io(e.to_string()))?;
        if !metadata.is_file() {
            return Err(SkillIngestError::Io(format!(
                "artifact path is not a file: {}",
                path.display()
            )));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|e| SkillIngestError::Io(e.to_string()))?;
        Ok(bytes)
    }

    #[cfg(not(unix))]
    {
        let metadata =
            fs::symlink_metadata(path).map_err(|e| SkillIngestError::Io(e.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(SkillIngestError::Io(format!(
                "symlink inputs are not allowed: {}",
                path.display()
            )));
        }
        if !metadata.is_file() {
            return Err(SkillIngestError::Io(format!(
                "artifact path is not a file: {}",
                path.display()
            )));
        }
        fs::read(path).map_err(|e| SkillIngestError::Io(e.to_string()))
    }
}

fn collect_artifacts_in_root(
    root: &Path,
    artifact_paths: &mut Vec<PathBuf>,
) -> Result<(), SkillIngestError> {
    let metadata = fs::symlink_metadata(root).map_err(|e| SkillIngestError::Io(e.to_string()))?;
    if metadata.file_type().is_symlink() {
        return Err(SkillIngestError::Io(format!(
            "approved root cannot be a symlink: {}",
            root.display()
        )));
    }
    if metadata.is_file() {
        if root.extension().and_then(|ext| ext.to_str()) == Some("ghostskill") {
            artifact_paths.push(root.to_path_buf());
        }
        return Ok(());
    }

    let mut entries = fs::read_dir(root)
        .map_err(|e| SkillIngestError::Io(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| SkillIngestError::Io(e.to_string()))?;
    entries.sort_by_key(|left| left.path());

    for entry in entries {
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|e| SkillIngestError::Io(e.to_string()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_artifacts_in_root(&path, artifact_paths)?;
        } else if metadata.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("ghostskill")
        {
            artifact_paths.push(path);
        }
    }
    Ok(())
}

fn decode_signer_public_key(
    signer: &TrustedSkillSignerConfig,
) -> Result<Vec<u8>, SkillIngestError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&signer.public_key)
        .map_err(|e| SkillIngestError::InvalidSigner(format!("{}: {}", signer.key_id, e)))?;
    if bytes.len() != 32 {
        return Err(SkillIngestError::InvalidSigner(format!(
            "{}: expected 32-byte Ed25519 public key",
            signer.key_id
        )));
    }
    Ok(bytes)
}

fn verifying_key_from_db(bytes: &[u8]) -> Result<ghost_signing::VerifyingKey, SkillIngestError> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SkillIngestError::InvalidSigner("invalid signer public key length".into()))?;
    ghost_signing::VerifyingKey::from_bytes(&arr)
        .ok_or_else(|| SkillIngestError::InvalidSigner("invalid signer public key bytes".into()))
}

fn write_skill_audit(
    conn: &rusqlite::Connection,
    agent_id: &str,
    event_type: &str,
    actor: &str,
    details: serde_json::Value,
) -> Result<(), AuditError> {
    let engine = ghost_audit::AuditQueryEngine::new(conn);
    let entry = ghost_audit::AuditEntry {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.to_string(),
        event_type: event_type.to_string(),
        severity: "info".to_string(),
        tool_name: None,
        details: serde_json::json!({
            "actor": actor,
            "details": details,
        })
        .to_string(),
        session_id: None,
        actor_id: Some(actor.to_string()),
        operation_id: None,
        request_id: None,
        idempotency_key: None,
        idempotency_status: None,
    };
    engine.insert(&entry)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::Engine;
    use cortex_storage::queries::external_skill_queries::{
        self, ExternalSkillQuarantineState, ExternalSkillVerificationStatus, SkillSignerState,
    };
    use ghost_audit::{AuditFilter, AuditQueryEngine};
    use ghost_signing::generate_keypair;
    use ghost_skills::artifact::{ArtifactSourceKind, SkillManifestSource};

    use super::*;
    use crate::db_pool::create_pool;

    #[tokio::test]
    async fn scan_approved_roots_reports_failures_and_audits_them() {
        let tmp = tempfile::tempdir().unwrap();
        let approved_root = tmp.path().join("approved");
        let managed_root = tmp.path().join("managed");
        fs::create_dir_all(&approved_root).unwrap();
        fs::write(approved_root.join("broken.ghostskill"), b"{not-json").unwrap();

        let db = test_db_pool(tmp.path()).await;
        let service = SkillIngestService::new(
            Arc::clone(&db),
            ExternalSkillsConfig {
                enabled: true,
                managed_storage_path: managed_root.display().to_string(),
                approved_roots: vec![ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                }],
                ..ExternalSkillsConfig::default()
            },
        );

        let report = service.scan_approved_roots("tester").await.unwrap();
        assert!(report.discovered.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert_eq!(
            report.failures[0].path,
            approved_root.join("broken.ghostskill")
        );

        let read = db.read().unwrap();
        let mut filter = AuditFilter::new();
        filter.event_type = Some("skill_artifact_ingest_failed".to_string());
        let audits = AuditQueryEngine::new(&read).query(&filter).unwrap();
        assert_eq!(audits.total, 1);
        assert_eq!(audits.items[0].idempotency_status, None);
    }

    #[tokio::test]
    async fn ingest_rejects_paths_outside_approved_root() {
        let tmp = tempfile::tempdir().unwrap();
        let approved_root = tmp.path().join("approved");
        let outside_root = tmp.path().join("outside");
        let managed_root = tmp.path().join("managed");
        fs::create_dir_all(&approved_root).unwrap();
        fs::create_dir_all(&outside_root).unwrap();
        fs::write(outside_root.join("outside.ghostskill"), b"{}").unwrap();

        let db = test_db_pool(tmp.path()).await;
        let service = SkillIngestService::new(
            Arc::clone(&db),
            ExternalSkillsConfig {
                enabled: true,
                managed_storage_path: managed_root.display().to_string(),
                approved_roots: vec![ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                }],
                ..ExternalSkillsConfig::default()
            },
        );

        let error = service
            .ingest_artifact_from_path(
                &outside_root.join("outside.ghostskill"),
                &ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                },
                "tester",
            )
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("artifact path is outside approved root"));
    }

    #[tokio::test]
    async fn successful_ingest_audit_has_no_idempotency_status() {
        let tmp = tempfile::tempdir().unwrap();
        let approved_root = tmp.path().join("approved");
        let managed_root = tmp.path().join("managed");
        fs::create_dir_all(&approved_root).unwrap();

        let (signing_key, verifying_key) = generate_keypair();
        let artifact = SkillArtifact::build(
            SkillManifestSource {
                manifest_schema_version: ghost_skills::artifact::MANIFEST_SCHEMA_VERSION,
                name: "external-demo".to_string(),
                version: "1.0.0".to_string(),
                publisher: "Acme".to_string(),
                description: "demo".to_string(),
                source_kind: ArtifactSourceKind::Workspace,
                execution_mode: ArtifactExecutionMode::Wasm,
                entrypoint: "main.wasm".to_string(),
                requested_capabilities: Vec::new(),
                declared_privileges: vec!["wasm:execute".to_string()],
            },
            BTreeMap::from([("main.wasm".to_string(), b"\0asm\x01\0\0\0".to_vec())]),
            &signing_key,
        )
        .unwrap();
        let artifact_path = approved_root.join("external-demo.ghostskill");
        artifact.write_to_path(&artifact_path).unwrap();

        let db = test_db_pool(tmp.path()).await;
        let service = SkillIngestService::new(
            Arc::clone(&db),
            ExternalSkillsConfig {
                enabled: true,
                managed_storage_path: managed_root.display().to_string(),
                approved_roots: vec![ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                }],
                trusted_signers: vec![TrustedSkillSignerConfig {
                    key_id: blake3::hash(&verifying_key.to_bytes()).to_hex().to_string(),
                    publisher: "Acme".to_string(),
                    public_key: base64::engine::general_purpose::STANDARD
                        .encode(verifying_key.to_bytes()),
                    revoked: false,
                }],
                ..ExternalSkillsConfig::default()
            },
        );
        service.seed_trusted_signers().await.unwrap();

        let ingested = service
            .ingest_artifact_from_path(
                &artifact_path,
                &ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                },
                "tester",
            )
            .await
            .unwrap();

        assert_eq!(ingested.skill_name, "external-demo");

        let read = db.read().unwrap();
        let mut filter = AuditFilter::new();
        filter.event_type = Some("skill_artifact_ingested".to_string());
        let audits = AuditQueryEngine::new(&read).query(&filter).unwrap();
        assert_eq!(audits.total, 1);
        assert_eq!(audits.items[0].agent_id, "skill:external-demo");
        assert_eq!(audits.items[0].idempotency_status, None);
    }

    #[tokio::test]
    async fn reverify_preserves_manual_operator_quarantine_for_verified_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let approved_root = tmp.path().join("approved");
        let managed_root = tmp.path().join("managed");
        fs::create_dir_all(&approved_root).unwrap();

        let (signing_key, verifying_key) = generate_keypair();
        let artifact = build_test_wasm_artifact("external-demo", &signing_key);
        let artifact_path = approved_root.join("external-demo.ghostskill");
        artifact.write_to_path(&artifact_path).unwrap();

        let db = test_db_pool(tmp.path()).await;
        let service = configured_ingest_service(
            Arc::clone(&db),
            &approved_root,
            &managed_root,
            &verifying_key,
        );
        service.seed_trusted_signers().await.unwrap();

        let ingested = service
            .ingest_artifact_from_path(
                &artifact_path,
                &ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                },
                "tester",
            )
            .await
            .unwrap();

        {
            let writer = db.write().await;
            external_skill_queries::upsert_external_skill_quarantine(
                &writer,
                &ingested.artifact_digest,
                ExternalSkillQuarantineState::Quarantined,
                Some("operator_quarantine"),
                Some("manual review"),
                Some("operator"),
            )
            .unwrap();
        }

        service
            .reverify_managed_artifact(&ingested.artifact_digest, "verifier")
            .await
            .unwrap();

        let read = db.read().unwrap();
        let quarantine =
            external_skill_queries::get_external_skill_quarantine(&read, &ingested.artifact_digest)
                .unwrap()
                .expect("quarantine row");
        let verification = external_skill_queries::get_external_skill_verification(
            &read,
            &ingested.artifact_digest,
        )
        .unwrap()
        .expect("verification row");

        assert_eq!(
            verification.status,
            ExternalSkillVerificationStatus::Verified
        );
        assert_eq!(quarantine.state, ExternalSkillQuarantineState::Quarantined);
        assert_eq!(
            quarantine.reason_code.as_deref(),
            Some("operator_quarantine")
        );
        assert_eq!(quarantine.reason_detail.as_deref(), Some("manual review"));
        assert_eq!(quarantine.revision, 2);
    }

    #[tokio::test]
    async fn reverify_quarantines_tampered_managed_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let approved_root = tmp.path().join("approved");
        let managed_root = tmp.path().join("managed");
        fs::create_dir_all(&approved_root).unwrap();

        let (signing_key, verifying_key) = generate_keypair();
        let artifact = build_test_wasm_artifact("external-demo", &signing_key);
        let artifact_path = approved_root.join("external-demo.ghostskill");
        artifact.write_to_path(&artifact_path).unwrap();

        let db = test_db_pool(tmp.path()).await;
        let service = configured_ingest_service(
            Arc::clone(&db),
            &approved_root,
            &managed_root,
            &verifying_key,
        );
        service.seed_trusted_signers().await.unwrap();

        let ingested = service
            .ingest_artifact_from_path(
                &artifact_path,
                &ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                },
                "tester",
            )
            .await
            .unwrap();

        let managed_artifact_path = managed_root
            .join(&ingested.artifact_digest)
            .join("artifact.ghostskill");
        fs::write(&managed_artifact_path, b"{not-json").unwrap();

        service
            .reverify_managed_artifact(&ingested.artifact_digest, "verifier")
            .await
            .unwrap();

        let read = db.read().unwrap();
        let quarantine =
            external_skill_queries::get_external_skill_quarantine(&read, &ingested.artifact_digest)
                .unwrap()
                .expect("quarantine row");
        let verification = external_skill_queries::get_external_skill_verification(
            &read,
            &ingested.artifact_digest,
        )
        .unwrap()
        .expect("verification row");

        assert_eq!(
            verification.status,
            ExternalSkillVerificationStatus::ValidationFailed
        );
        assert_eq!(quarantine.state, ExternalSkillQuarantineState::Quarantined);
        assert_eq!(
            quarantine.reason_code.as_deref(),
            Some("managed_artifact_invalid")
        );
    }

    #[tokio::test]
    async fn reverify_quarantines_artifacts_signed_by_newly_revoked_signers() {
        let tmp = tempfile::tempdir().unwrap();
        let approved_root = tmp.path().join("approved");
        let managed_root = tmp.path().join("managed");
        fs::create_dir_all(&approved_root).unwrap();

        let (signing_key, verifying_key) = generate_keypair();
        let artifact = build_test_wasm_artifact("external-demo", &signing_key);
        let artifact_path = approved_root.join("external-demo.ghostskill");
        artifact.write_to_path(&artifact_path).unwrap();

        let db = test_db_pool(tmp.path()).await;
        let service = configured_ingest_service(
            Arc::clone(&db),
            &approved_root,
            &managed_root,
            &verifying_key,
        );
        service.seed_trusted_signers().await.unwrap();

        let ingested = service
            .ingest_artifact_from_path(
                &artifact_path,
                &ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                },
                "tester",
            )
            .await
            .unwrap();

        {
            let writer = db.write().await;
            external_skill_queries::upsert_skill_signer(
                &writer,
                &signer_key_id(&verifying_key),
                "Acme",
                &verifying_key.to_bytes(),
                SkillSignerState::Revoked,
                Some("security"),
                Some("revoked during incident response"),
            )
            .unwrap();
        }

        service
            .reverify_managed_artifact(&ingested.artifact_digest, "security")
            .await
            .unwrap();

        let read = db.read().unwrap();
        let quarantine =
            external_skill_queries::get_external_skill_quarantine(&read, &ingested.artifact_digest)
                .unwrap()
                .expect("quarantine row");
        let verification = external_skill_queries::get_external_skill_verification(
            &read,
            &ingested.artifact_digest,
        )
        .unwrap()
        .expect("verification row");

        assert_eq!(
            verification.status,
            ExternalSkillVerificationStatus::RevokedSigner
        );
        assert_eq!(quarantine.state, ExternalSkillQuarantineState::Quarantined);
        assert_eq!(quarantine.reason_code.as_deref(), Some("revoked_signer"));
    }

    async fn test_db_pool(root: &Path) -> Arc<DbPool> {
        let db = create_pool(root.join("test.db")).unwrap();
        let writer = db.writer_for_migrations().await;
        cortex_storage::migrations::run_migrations(&writer).unwrap();
        drop(writer);
        db
    }

    fn configured_ingest_service(
        db: Arc<DbPool>,
        approved_root: &Path,
        managed_root: &Path,
        verifying_key: &ghost_signing::VerifyingKey,
    ) -> SkillIngestService {
        SkillIngestService::new(
            db,
            ExternalSkillsConfig {
                enabled: true,
                managed_storage_path: managed_root.display().to_string(),
                approved_roots: vec![ExternalSkillRootConfig {
                    source: ExternalSkillSourceConfig::Workspace,
                    path: approved_root.display().to_string(),
                }],
                trusted_signers: vec![TrustedSkillSignerConfig {
                    key_id: signer_key_id(verifying_key),
                    publisher: "Acme".to_string(),
                    public_key: base64::engine::general_purpose::STANDARD
                        .encode(verifying_key.to_bytes()),
                    revoked: false,
                }],
                ..ExternalSkillsConfig::default()
            },
        )
    }

    fn build_test_wasm_artifact(
        name: &str,
        signing_key: &ghost_signing::SigningKey,
    ) -> SkillArtifact {
        SkillArtifact::build(
            SkillManifestSource {
                manifest_schema_version: ghost_skills::artifact::MANIFEST_SCHEMA_VERSION,
                name: name.to_string(),
                version: "1.0.0".to_string(),
                publisher: "Acme".to_string(),
                description: "demo".to_string(),
                source_kind: ArtifactSourceKind::Workspace,
                execution_mode: ArtifactExecutionMode::Wasm,
                entrypoint: "main.wasm".to_string(),
                requested_capabilities: Vec::new(),
                declared_privileges: vec!["wasm:execute".to_string()],
            },
            BTreeMap::from([("main.wasm".to_string(), b"\0asm\x01\0\0\0".to_vec())]),
            signing_key,
        )
        .unwrap()
    }

    fn signer_key_id(verifying_key: &ghost_signing::VerifyingKey) -> String {
        blake3::hash(&verifying_key.to_bytes()).to_hex().to_string()
    }
}
