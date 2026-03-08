//! Queries for the external skill ingestion, verification, quarantine, and
//! version-aware install lifecycle tables.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSignerState {
    Trusted,
    Revoked,
}

impl SkillSignerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Revoked => "revoked",
        }
    }

    fn from_db(value: &str) -> CortexResult<Self> {
        match value {
            "trusted" => Ok(Self::Trusted),
            "revoked" => Ok(Self::Revoked),
            other => Err(to_storage_err(format!(
                "unknown skill_signer state '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalSkillVerificationStatus {
    Verified,
    ValidationFailed,
    DigestMismatch,
    MissingSignature,
    InvalidSignature,
    UnknownSigner,
    RevokedSigner,
    UnsupportedCapability,
    UnsupportedExecutionMode,
}

impl ExternalSkillVerificationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::ValidationFailed => "validation_failed",
            Self::DigestMismatch => "digest_mismatch",
            Self::MissingSignature => "missing_signature",
            Self::InvalidSignature => "invalid_signature",
            Self::UnknownSigner => "unknown_signer",
            Self::RevokedSigner => "revoked_signer",
            Self::UnsupportedCapability => "unsupported_capability",
            Self::UnsupportedExecutionMode => "unsupported_execution_mode",
        }
    }

    fn from_db(value: &str) -> CortexResult<Self> {
        match value {
            "verified" => Ok(Self::Verified),
            "validation_failed" => Ok(Self::ValidationFailed),
            "digest_mismatch" => Ok(Self::DigestMismatch),
            "missing_signature" => Ok(Self::MissingSignature),
            "invalid_signature" => Ok(Self::InvalidSignature),
            "unknown_signer" => Ok(Self::UnknownSigner),
            "revoked_signer" => Ok(Self::RevokedSigner),
            "unsupported_capability" => Ok(Self::UnsupportedCapability),
            "unsupported_execution_mode" => Ok(Self::UnsupportedExecutionMode),
            other => Err(to_storage_err(format!(
                "unknown external_skill_verification status '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalSkillQuarantineState {
    Clear,
    Quarantined,
}

impl ExternalSkillQuarantineState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Quarantined => "quarantined",
        }
    }

    fn from_db(value: &str) -> CortexResult<Self> {
        match value {
            "clear" => Ok(Self::Clear),
            "quarantined" => Ok(Self::Quarantined),
            other => Err(to_storage_err(format!(
                "unknown external_skill_quarantine state '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalSkillInstallState {
    Installed,
    Disabled,
}

impl ExternalSkillInstallState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Disabled => "disabled",
        }
    }

    fn from_db(value: &str) -> CortexResult<Self> {
        match value {
            "installed" => Ok(Self::Installed),
            "disabled" => Ok(Self::Disabled),
            other => Err(to_storage_err(format!(
                "unknown external_skill_install_state value '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSignerRow {
    pub key_id: String,
    pub publisher: String,
    pub public_key: Vec<u8>,
    pub state: SkillSignerState,
    pub updated_at: String,
    pub updated_by: Option<String>,
    pub revocation_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSkillArtifactRow {
    pub artifact_digest: String,
    pub artifact_schema_version: u32,
    pub skill_name: String,
    pub skill_version: String,
    pub publisher: String,
    pub description: String,
    pub source_kind: String,
    pub execution_mode: String,
    pub entrypoint: String,
    pub source_uri: String,
    pub managed_artifact_path: String,
    pub managed_entrypoint_path: String,
    pub manifest_json: String,
    pub requested_capabilities: String,
    pub declared_privileges: String,
    pub signer_key_id: Option<String>,
    pub artifact_size_bytes: i64,
    pub ingested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSkillVerificationRow {
    pub artifact_digest: String,
    pub status: ExternalSkillVerificationStatus,
    pub signer_key_id: Option<String>,
    pub signer_publisher: Option<String>,
    pub details_json: String,
    pub verified_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSkillQuarantineRow {
    pub artifact_digest: String,
    pub state: ExternalSkillQuarantineState,
    pub reason_code: Option<String>,
    pub reason_detail: Option<String>,
    pub revision: i64,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSkillInstallRow {
    pub artifact_digest: String,
    pub skill_name: String,
    pub skill_version: String,
    pub state: ExternalSkillInstallState,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

pub fn upsert_skill_signer(
    conn: &Connection,
    key_id: &str,
    publisher: &str,
    public_key: &[u8],
    state: SkillSignerState,
    updated_by: Option<&str>,
    revocation_reason: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO skill_signers (
            key_id, publisher, public_key, state, updated_by, revocation_reason
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(key_id) DO UPDATE SET
            publisher = excluded.publisher,
            public_key = excluded.public_key,
            state = excluded.state,
            updated_at = datetime('now'),
            updated_by = excluded.updated_by,
            revocation_reason = excluded.revocation_reason",
        params![
            key_id,
            publisher,
            public_key,
            state.as_str(),
            updated_by,
            revocation_reason
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_skill_signer(conn: &Connection, key_id: &str) -> CortexResult<Option<SkillSignerRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT key_id, publisher, public_key, state, updated_at, updated_by, revocation_reason
             FROM skill_signers
             WHERE key_id = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let mut rows = stmt
        .query_map(params![key_id], map_signer_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    match rows.next() {
        Some(row) => row.map(Some).map_err(|e| to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn list_skill_signers(conn: &Connection) -> CortexResult<Vec<SkillSignerRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT key_id, publisher, public_key, state, updated_at, updated_by, revocation_reason
             FROM skill_signers
             ORDER BY publisher ASC, key_id ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let rows = stmt
        .query_map([], map_signer_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_external_skill_artifact(
    conn: &Connection,
    artifact_digest: &str,
    artifact_schema_version: u32,
    skill_name: &str,
    skill_version: &str,
    publisher: &str,
    description: &str,
    source_kind: &str,
    execution_mode: &str,
    entrypoint: &str,
    source_uri: &str,
    managed_artifact_path: &str,
    managed_entrypoint_path: &str,
    manifest_json: &str,
    requested_capabilities: &str,
    declared_privileges: &str,
    signer_key_id: Option<&str>,
    artifact_size_bytes: i64,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO external_skill_artifacts (
            artifact_digest,
            artifact_schema_version,
            skill_name,
            skill_version,
            publisher,
            description,
            source_kind,
            execution_mode,
            entrypoint,
            source_uri,
            managed_artifact_path,
            managed_entrypoint_path,
            manifest_json,
            requested_capabilities,
            declared_privileges,
            signer_key_id,
            artifact_size_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         ON CONFLICT(artifact_digest) DO UPDATE SET
            artifact_schema_version = excluded.artifact_schema_version,
            skill_name = excluded.skill_name,
            skill_version = excluded.skill_version,
            publisher = excluded.publisher,
            description = excluded.description,
            source_kind = excluded.source_kind,
            execution_mode = excluded.execution_mode,
            entrypoint = excluded.entrypoint,
            source_uri = excluded.source_uri,
            managed_artifact_path = excluded.managed_artifact_path,
            managed_entrypoint_path = excluded.managed_entrypoint_path,
            manifest_json = excluded.manifest_json,
            requested_capabilities = excluded.requested_capabilities,
            declared_privileges = excluded.declared_privileges,
            signer_key_id = excluded.signer_key_id,
            artifact_size_bytes = excluded.artifact_size_bytes,
            ingested_at = datetime('now')",
        params![
            artifact_digest,
            artifact_schema_version,
            skill_name,
            skill_version,
            publisher,
            description,
            source_kind,
            execution_mode,
            entrypoint,
            source_uri,
            managed_artifact_path,
            managed_entrypoint_path,
            manifest_json,
            requested_capabilities,
            declared_privileges,
            signer_key_id,
            artifact_size_bytes
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn list_external_skill_artifacts(
    conn: &Connection,
) -> CortexResult<Vec<ExternalSkillArtifactRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, artifact_schema_version, skill_name, skill_version, publisher,
                    description, source_kind, execution_mode, entrypoint, source_uri,
                    managed_artifact_path, managed_entrypoint_path, manifest_json,
                    requested_capabilities, declared_privileges, signer_key_id,
                    artifact_size_bytes, ingested_at
             FROM external_skill_artifacts
             ORDER BY skill_name ASC, skill_version ASC, ingested_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let rows = stmt
        .query_map([], map_artifact_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))
}

pub fn get_external_skill_artifact(
    conn: &Connection,
    artifact_digest: &str,
) -> CortexResult<Option<ExternalSkillArtifactRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, artifact_schema_version, skill_name, skill_version, publisher,
                    description, source_kind, execution_mode, entrypoint, source_uri,
                    managed_artifact_path, managed_entrypoint_path, manifest_json,
                    requested_capabilities, declared_privileges, signer_key_id,
                    artifact_size_bytes, ingested_at
             FROM external_skill_artifacts
             WHERE artifact_digest = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let mut rows = stmt
        .query_map(params![artifact_digest], map_artifact_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    match rows.next() {
        Some(row) => row.map(Some).map_err(|e| to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn upsert_external_skill_verification(
    conn: &Connection,
    artifact_digest: &str,
    status: ExternalSkillVerificationStatus,
    signer_key_id: Option<&str>,
    signer_publisher: Option<&str>,
    details_json: &str,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO external_skill_verifications (
            artifact_digest, status, signer_key_id, signer_publisher, details_json
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artifact_digest) DO UPDATE SET
            status = excluded.status,
            signer_key_id = excluded.signer_key_id,
            signer_publisher = excluded.signer_publisher,
            details_json = excluded.details_json,
            verified_at = datetime('now')",
        params![
            artifact_digest,
            status.as_str(),
            signer_key_id,
            signer_publisher,
            details_json
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_external_skill_verification(
    conn: &Connection,
    artifact_digest: &str,
) -> CortexResult<Option<ExternalSkillVerificationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, status, signer_key_id, signer_publisher, details_json, verified_at
             FROM external_skill_verifications
             WHERE artifact_digest = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let mut rows = stmt
        .query_map(params![artifact_digest], map_verification_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    match rows.next() {
        Some(row) => row.map(Some).map_err(|e| to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn list_external_skill_verifications(
    conn: &Connection,
) -> CortexResult<Vec<ExternalSkillVerificationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, status, signer_key_id, signer_publisher, details_json, verified_at
             FROM external_skill_verifications
             ORDER BY artifact_digest ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let rows = stmt
        .query_map([], map_verification_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))
}

pub fn upsert_external_skill_quarantine(
    conn: &Connection,
    artifact_digest: &str,
    state: ExternalSkillQuarantineState,
    reason_code: Option<&str>,
    reason_detail: Option<&str>,
    updated_by: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO external_skill_quarantine (
            artifact_digest, state, reason_code, reason_detail, updated_by
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artifact_digest) DO UPDATE SET
            state = excluded.state,
            reason_code = excluded.reason_code,
            reason_detail = excluded.reason_detail,
            revision = external_skill_quarantine.revision + 1,
            updated_at = datetime('now'),
            updated_by = excluded.updated_by",
        params![
            artifact_digest,
            state.as_str(),
            reason_code,
            reason_detail,
            updated_by
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn clear_external_skill_quarantine(
    conn: &Connection,
    artifact_digest: &str,
    expected_revision: i64,
    updated_by: Option<&str>,
) -> CortexResult<bool> {
    let changed = conn
        .execute(
            "UPDATE external_skill_quarantine
             SET state = 'clear',
                 reason_code = NULL,
                 reason_detail = NULL,
                 revision = revision + 1,
                 updated_at = datetime('now'),
                 updated_by = ?3
             WHERE artifact_digest = ?1
               AND state = 'quarantined'
               AND revision = ?2",
            params![artifact_digest, expected_revision, updated_by],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(changed == 1)
}

pub fn get_external_skill_quarantine(
    conn: &Connection,
    artifact_digest: &str,
) -> CortexResult<Option<ExternalSkillQuarantineRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, state, reason_code, reason_detail, revision, updated_at, updated_by
             FROM external_skill_quarantine
             WHERE artifact_digest = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let mut rows = stmt
        .query_map(params![artifact_digest], map_quarantine_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    match rows.next() {
        Some(row) => row.map(Some).map_err(|e| to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn list_external_skill_quarantine(
    conn: &Connection,
) -> CortexResult<Vec<ExternalSkillQuarantineRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, state, reason_code, reason_detail, revision, updated_at, updated_by
             FROM external_skill_quarantine
             ORDER BY artifact_digest ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let rows = stmt
        .query_map([], map_quarantine_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))
}

pub fn upsert_external_skill_install_state(
    conn: &Connection,
    artifact_digest: &str,
    skill_name: &str,
    skill_version: &str,
    state: ExternalSkillInstallState,
    updated_by: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO external_skill_install_state (
            artifact_digest, skill_name, skill_version, state, updated_by
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artifact_digest) DO UPDATE SET
            skill_name = excluded.skill_name,
            skill_version = excluded.skill_version,
            state = excluded.state,
            updated_at = datetime('now'),
            updated_by = excluded.updated_by",
        params![
            artifact_digest,
            skill_name,
            skill_version,
            state.as_str(),
            updated_by
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn disable_other_installed_external_versions(
    conn: &Connection,
    skill_name: &str,
    keep_artifact_digest: &str,
    updated_by: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "UPDATE external_skill_install_state
         SET state = 'disabled',
             updated_at = datetime('now'),
             updated_by = ?3
         WHERE skill_name = ?1
           AND artifact_digest <> ?2
           AND state = 'installed'",
        params![skill_name, keep_artifact_digest, updated_by],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_external_skill_install_state(
    conn: &Connection,
    artifact_digest: &str,
) -> CortexResult<Option<ExternalSkillInstallRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, skill_name, skill_version, state, updated_at, updated_by
             FROM external_skill_install_state
             WHERE artifact_digest = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let mut rows = stmt
        .query_map(params![artifact_digest], map_install_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    match rows.next() {
        Some(row) => row.map(Some).map_err(|e| to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn list_external_skill_install_states(
    conn: &Connection,
) -> CortexResult<Vec<ExternalSkillInstallRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT artifact_digest, skill_name, skill_version, state, updated_at, updated_by
             FROM external_skill_install_state
             ORDER BY skill_name ASC, skill_version ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    let rows = stmt
        .query_map([], map_install_row)
        .map_err(|e| to_storage_err(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))
}

fn map_signer_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillSignerRow> {
    let state_raw: String = row.get(3)?;
    let state = SkillSignerState::from_db(&state_raw).map_err(storage_value_error)?;
    Ok(SkillSignerRow {
        key_id: row.get(0)?,
        publisher: row.get(1)?,
        public_key: row.get(2)?,
        state,
        updated_at: row.get(4)?,
        updated_by: row.get(5)?,
        revocation_reason: row.get(6)?,
    })
}

fn map_artifact_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExternalSkillArtifactRow> {
    Ok(ExternalSkillArtifactRow {
        artifact_digest: row.get(0)?,
        artifact_schema_version: row.get(1)?,
        skill_name: row.get(2)?,
        skill_version: row.get(3)?,
        publisher: row.get(4)?,
        description: row.get(5)?,
        source_kind: row.get(6)?,
        execution_mode: row.get(7)?,
        entrypoint: row.get(8)?,
        source_uri: row.get(9)?,
        managed_artifact_path: row.get(10)?,
        managed_entrypoint_path: row.get(11)?,
        manifest_json: row.get(12)?,
        requested_capabilities: row.get(13)?,
        declared_privileges: row.get(14)?,
        signer_key_id: row.get(15)?,
        artifact_size_bytes: row.get(16)?,
        ingested_at: row.get(17)?,
    })
}

fn map_verification_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExternalSkillVerificationRow> {
    let raw: String = row.get(1)?;
    let status = ExternalSkillVerificationStatus::from_db(&raw).map_err(storage_value_error)?;
    Ok(ExternalSkillVerificationRow {
        artifact_digest: row.get(0)?,
        status,
        signer_key_id: row.get(2)?,
        signer_publisher: row.get(3)?,
        details_json: row.get(4)?,
        verified_at: row.get(5)?,
    })
}

fn map_quarantine_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExternalSkillQuarantineRow> {
    let raw: String = row.get(1)?;
    let state = ExternalSkillQuarantineState::from_db(&raw).map_err(storage_value_error)?;
    Ok(ExternalSkillQuarantineRow {
        artifact_digest: row.get(0)?,
        state,
        reason_code: row.get(2)?,
        reason_detail: row.get(3)?,
        revision: row.get(4)?,
        updated_at: row.get(5)?,
        updated_by: row.get(6)?,
    })
}

fn map_install_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExternalSkillInstallRow> {
    let raw: String = row.get(3)?;
    let state = ExternalSkillInstallState::from_db(&raw).map_err(storage_value_error)?;
    Ok(ExternalSkillInstallRow {
        artifact_digest: row.get(0)?,
        skill_name: row.get(1)?,
        skill_version: row.get(2)?,
        state,
        updated_at: row.get(4)?,
        updated_by: row.get(5)?,
    })
}

fn storage_value_error(error: cortex_core::models::error::CortexError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn insert_artifact(conn: &Connection, digest: &str, skill_name: &str, version: &str) {
        upsert_external_skill_artifact(
            conn,
            digest,
            1,
            skill_name,
            version,
            "publisher",
            "description",
            "workspace",
            "wasm",
            "module.wasm",
            "/input/path.ghostskill",
            "/managed/artifact.ghostskill",
            "/managed/module.wasm",
            "{}",
            "[]",
            "[\"Pure compute\"]",
            Some("key-1"),
            128,
        )
        .unwrap();
    }

    #[test]
    fn signer_round_trip_preserves_revocation_state() {
        let conn = test_db();
        upsert_skill_signer(
            &conn,
            "key-1",
            "publisher",
            &[1, 2, 3, 4],
            SkillSignerState::Revoked,
            Some("operator"),
            Some("compromised"),
        )
        .unwrap();

        let row = get_skill_signer(&conn, "key-1").unwrap().unwrap();
        assert_eq!(row.state, SkillSignerState::Revoked);
        assert_eq!(row.revocation_reason.as_deref(), Some("compromised"));
    }

    #[test]
    fn install_state_disables_other_versions_for_same_skill() {
        let conn = test_db();
        insert_artifact(&conn, "digest-a", "echo", "1.0.0");
        insert_artifact(&conn, "digest-b", "echo", "1.1.0");
        upsert_external_skill_install_state(
            &conn,
            "digest-a",
            "echo",
            "1.0.0",
            ExternalSkillInstallState::Installed,
            Some("system"),
        )
        .unwrap();
        upsert_external_skill_install_state(
            &conn,
            "digest-b",
            "echo",
            "1.1.0",
            ExternalSkillInstallState::Installed,
            Some("system"),
        )
        .unwrap();

        disable_other_installed_external_versions(&conn, "echo", "digest-b", Some("operator"))
            .unwrap();

        let rows = list_external_skill_install_states(&conn).unwrap();
        assert_eq!(
            rows.iter()
                .find(|row| row.artifact_digest == "digest-a")
                .unwrap()
                .state,
            ExternalSkillInstallState::Disabled
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.artifact_digest == "digest-b")
                .unwrap()
                .state,
            ExternalSkillInstallState::Installed
        );
    }

    #[test]
    fn quarantine_clear_requires_matching_revision() {
        let conn = test_db();
        insert_artifact(&conn, "digest-a", "echo", "1.0.0");
        upsert_external_skill_quarantine(
            &conn,
            "digest-a",
            ExternalSkillQuarantineState::Quarantined,
            Some("invalid_signature"),
            Some("bad sig"),
            Some("system"),
        )
        .unwrap();
        let original = get_external_skill_quarantine(&conn, "digest-a")
            .unwrap()
            .unwrap();

        assert!(
            !clear_external_skill_quarantine(&conn, "digest-a", 999, Some("operator")).unwrap()
        );
        assert!(clear_external_skill_quarantine(
            &conn,
            "digest-a",
            original.revision,
            Some("operator")
        )
        .unwrap());

        let cleared = get_external_skill_quarantine(&conn, "digest-a")
            .unwrap()
            .unwrap();
        assert_eq!(cleared.state, ExternalSkillQuarantineState::Clear);
        assert!(cleared.revision > original.revision);
    }
}
