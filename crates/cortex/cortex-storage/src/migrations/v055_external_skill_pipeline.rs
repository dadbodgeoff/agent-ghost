//! Migration v055: external skill artifacts, signer trust roots, verification,
//! quarantine, and version-aware install state.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS skill_signers (
            key_id             TEXT PRIMARY KEY,
            publisher          TEXT NOT NULL,
            public_key         BLOB NOT NULL,
            state              TEXT NOT NULL CHECK (state IN ('trusted', 'revoked')),
            updated_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_by         TEXT,
            revocation_reason  TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_skill_signers_state
            ON skill_signers(state);

        CREATE TABLE IF NOT EXISTS external_skill_artifacts (
            artifact_digest        TEXT PRIMARY KEY,
            artifact_schema_version INTEGER NOT NULL,
            skill_name             TEXT NOT NULL,
            skill_version          TEXT NOT NULL,
            publisher              TEXT NOT NULL,
            description            TEXT NOT NULL,
            source_kind            TEXT NOT NULL CHECK (source_kind IN ('user', 'workspace')),
            execution_mode         TEXT NOT NULL CHECK (execution_mode IN ('native', 'wasm')),
            entrypoint             TEXT NOT NULL,
            source_uri             TEXT NOT NULL,
            managed_artifact_path  TEXT NOT NULL,
            managed_entrypoint_path TEXT NOT NULL,
            manifest_json          TEXT NOT NULL,
            requested_capabilities TEXT NOT NULL,
            declared_privileges    TEXT NOT NULL,
            signer_key_id          TEXT,
            artifact_size_bytes    INTEGER NOT NULL,
            ingested_at            TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_external_skill_artifacts_name
            ON external_skill_artifacts(skill_name);
        CREATE INDEX IF NOT EXISTS idx_external_skill_artifacts_source_kind
            ON external_skill_artifacts(source_kind);

        CREATE TABLE IF NOT EXISTS external_skill_verifications (
            artifact_digest    TEXT PRIMARY KEY,
            status             TEXT NOT NULL CHECK (status IN (
                'verified',
                'validation_failed',
                'digest_mismatch',
                'missing_signature',
                'invalid_signature',
                'unknown_signer',
                'revoked_signer',
                'unsupported_capability',
                'unsupported_execution_mode'
            )),
            signer_key_id      TEXT,
            signer_publisher   TEXT,
            details_json       TEXT NOT NULL DEFAULT '{}',
            verified_at        TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (artifact_digest) REFERENCES external_skill_artifacts(artifact_digest)
        );

        CREATE TABLE IF NOT EXISTS external_skill_quarantine (
            artifact_digest    TEXT PRIMARY KEY,
            state              TEXT NOT NULL CHECK (state IN ('clear', 'quarantined')),
            reason_code        TEXT,
            reason_detail      TEXT,
            revision           INTEGER NOT NULL DEFAULT 1,
            updated_at         TEXT NOT NULL DEFAULT (datetime('now')),
            updated_by         TEXT,
            FOREIGN KEY (artifact_digest) REFERENCES external_skill_artifacts(artifact_digest)
        );
        CREATE INDEX IF NOT EXISTS idx_external_skill_quarantine_state
            ON external_skill_quarantine(state);

        CREATE TABLE IF NOT EXISTS external_skill_install_state (
            artifact_digest     TEXT PRIMARY KEY,
            skill_name          TEXT NOT NULL,
            skill_version       TEXT NOT NULL,
            state               TEXT NOT NULL CHECK (state IN ('installed', 'disabled')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_by          TEXT,
            FOREIGN KEY (artifact_digest) REFERENCES external_skill_artifacts(artifact_digest)
        );
        CREATE INDEX IF NOT EXISTS idx_external_skill_install_state_name
            ON external_skill_install_state(skill_name);
        CREATE INDEX IF NOT EXISTS idx_external_skill_install_state_state
            ON external_skill_install_state(state);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_external_skill_pipeline_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare(
                "SELECT name
                 FROM sqlite_master
                 WHERE type = 'table'
                   AND name IN (
                     'skill_signers',
                     'external_skill_artifacts',
                     'external_skill_verifications',
                     'external_skill_quarantine',
                     'external_skill_install_state'
                   )
                 ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(
            tables,
            vec![
                "external_skill_artifacts".to_string(),
                "external_skill_install_state".to_string(),
                "external_skill_quarantine".to_string(),
                "external_skill_verifications".to_string(),
                "skill_signers".to_string(),
            ]
        );
    }
}
