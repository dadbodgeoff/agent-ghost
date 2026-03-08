use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use cortex_storage::queries::external_skill_queries::{self, ExternalSkillQuarantineState};
use ghost_agent_loop::tools::skill_bridge::{
    ExecutionContext, SkillHandler, SkillHandlerEnvironment,
};
use ghost_audit::query_engine::{AuditEntry, AuditQueryEngine};
use ghost_skills::artifact::{ArtifactExecutionMode, SkillArtifact};
use ghost_skills::sandbox::wasm_sandbox::{ExecutionResult, WasmSandbox, WasmSandboxConfig};
use ghost_skills::skill::SkillError;

use crate::db_pool::DbPool;

pub(crate) struct ExternalWasmSkillHandler {
    artifact_digest: String,
    skill_name: String,
    description: String,
    managed_artifact_path: String,
    db: Arc<DbPool>,
    sandbox: WasmSandbox,
}

impl ExternalWasmSkillHandler {
    pub(crate) fn new(
        artifact_digest: String,
        skill_name: String,
        description: String,
        managed_artifact_path: String,
        db: Arc<DbPool>,
        sandbox_config: WasmSandboxConfig,
    ) -> Self {
        Self {
            artifact_digest,
            skill_name,
            description,
            managed_artifact_path,
            db,
            sandbox: WasmSandbox::new(sandbox_config),
        }
    }

    fn load_wasm_bytes(&self, actor: &str) -> Result<Vec<u8>, SkillError> {
        let artifact_bytes =
            read_no_follow(Path::new(&self.managed_artifact_path)).map_err(|error| {
                self.quarantine_if_needed("managed_artifact_unreadable", &error, actor);
                SkillError::AuthorizationDenied(format!(
                    "external skill '{}' managed artifact is unreadable and has been quarantined",
                    self.skill_name
                ))
            })?;
        let artifact = SkillArtifact::read_from_bytes(&artifact_bytes).map_err(|error| {
            self.quarantine_if_needed("managed_artifact_invalid", &error.to_string(), actor);
            SkillError::AuthorizationDenied(format!(
                "external skill '{}' managed artifact is invalid and has been quarantined",
                self.skill_name
            ))
        })?;
        let decoded = artifact.validate().map_err(|error| {
            self.quarantine_if_needed("managed_artifact_invalid", &error.to_string(), actor);
            SkillError::AuthorizationDenied(format!(
                "external skill '{}' managed artifact failed validation and has been quarantined",
                self.skill_name
            ))
        })?;
        let actual_digest = artifact.artifact_digest().map_err(|error| {
            self.quarantine_if_needed("managed_artifact_invalid", &error.to_string(), actor);
            SkillError::AuthorizationDenied(format!(
                "external skill '{}' managed artifact digest could not be computed",
                self.skill_name
            ))
        })?;
        if actual_digest != self.artifact_digest {
            self.quarantine_if_needed(
                "managed_artifact_digest_mismatch",
                &format!(
                    "managed artifact digest '{}' does not match persisted digest '{}'",
                    actual_digest, self.artifact_digest
                ),
                actor,
            );
            return Err(SkillError::AuthorizationDenied(format!(
                "external skill '{}' managed artifact digest mismatched and has been quarantined",
                self.skill_name
            )));
        }
        if decoded.manifest.execution_mode != ArtifactExecutionMode::Wasm {
            self.quarantine_if_needed(
                "unsupported_execution_mode",
                "managed artifact is not a wasm skill",
                actor,
            );
            return Err(SkillError::AuthorizationDenied(format!(
                "external skill '{}' is no longer executable as wasm",
                self.skill_name
            )));
        }
        if !decoded.manifest.requested_capabilities.is_empty() {
            self.quarantine_if_needed(
                "unsupported_capability",
                "external wasm skills currently execute without host capabilities",
                actor,
            );
            return Err(SkillError::AuthorizationDenied(format!(
                "external skill '{}' requested unsupported host capabilities",
                self.skill_name
            )));
        }

        decoded
            .files
            .get(&decoded.manifest.entrypoint)
            .cloned()
            .ok_or_else(|| {
                self.quarantine_if_needed(
                    "managed_artifact_invalid",
                    "managed artifact entrypoint payload is missing",
                    actor,
                );
                SkillError::AuthorizationDenied(format!(
                    "external skill '{}' entrypoint payload is missing",
                    self.skill_name
                ))
            })
    }

    fn quarantine_if_needed(&self, reason_code: &str, reason_detail: &str, actor: &str) {
        let conn = match self.db.legacy_connection() {
            Ok(conn) => conn,
            Err(error) => {
                tracing::warn!(
                    skill_name = %self.skill_name,
                    artifact_digest = %self.artifact_digest,
                    error = %error,
                    "failed to open legacy connection for external skill quarantine"
                );
                return;
            }
        };
        let conn = match conn.lock() {
            Ok(conn) => conn,
            Err(_) => {
                tracing::warn!(
                    skill_name = %self.skill_name,
                    artifact_digest = %self.artifact_digest,
                    "failed to lock legacy connection for external skill quarantine"
                );
                return;
            }
        };

        let already_quarantined =
            external_skill_queries::get_external_skill_quarantine(&conn, &self.artifact_digest)
                .ok()
                .flatten()
                .is_some_and(|row| {
                    row.state == ExternalSkillQuarantineState::Quarantined
                        && row.reason_code.as_deref() == Some(reason_code)
                        && row.reason_detail.as_deref() == Some(reason_detail)
                });
        if already_quarantined {
            return;
        }

        if let Err(error) = external_skill_queries::upsert_external_skill_quarantine(
            &conn,
            &self.artifact_digest,
            ExternalSkillQuarantineState::Quarantined,
            Some(reason_code),
            Some(reason_detail),
            Some(actor),
        ) {
            tracing::warn!(
                skill_name = %self.skill_name,
                artifact_digest = %self.artifact_digest,
                error = %error,
                "failed to persist external skill quarantine"
            );
            return;
        }

        if let Err(error) = write_runtime_skill_audit(
            &conn,
            &self.skill_name,
            &self.artifact_digest,
            reason_code,
            reason_detail,
            actor,
        ) {
            tracing::warn!(
                skill_name = %self.skill_name,
                artifact_digest = %self.artifact_digest,
                error = %error,
                "failed to persist external skill runtime quarantine audit entry"
            );
        }
    }
}

impl SkillHandler for ExternalWasmSkillHandler {
    fn description(&self) -> String {
        self.description.clone()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": true
        })
    }

    fn removable(&self) -> bool {
        true
    }

    fn execute(
        &self,
        _env: &SkillHandlerEnvironment,
        input: &serde_json::Value,
        exec_ctx: &ExecutionContext,
    ) -> Result<serde_json::Value, SkillError> {
        let actor = format!("agent:{}", exec_ctx.agent_id);
        let wasm_bytes = self.load_wasm_bytes(&actor)?;

        match self.sandbox.execute(
            &wasm_bytes,
            input.clone(),
            exec_ctx.agent_id,
            &self.skill_name,
        ) {
            ExecutionResult::Success { output, .. } => Ok(output),
            ExecutionResult::Timeout { elapsed } => Err(SkillError::ExecutionTimedOut(format!(
                "external wasm skill '{}' timed out after {} ms",
                self.skill_name,
                elapsed.as_millis()
            ))),
            ExecutionResult::FuelExhausted { consumed, limit } => {
                Err(SkillError::ResourceExhausted(format!(
                    "external wasm skill '{}' exhausted fuel budget ({consumed}/{limit})",
                    self.skill_name
                )))
            }
            ExecutionResult::MemoryExceeded {
                used_bytes,
                limit_bytes,
            } => Err(SkillError::ResourceExhausted(format!(
                "external wasm skill '{}' exceeded memory limit (used {}, limit {})",
                self.skill_name, used_bytes, limit_bytes
            ))),
            ExecutionResult::EscapeDetected(attempt) => {
                self.quarantine_if_needed("sandbox_escape", &attempt.details, &actor);
                Err(SkillError::SandboxViolation(format!(
                    "external wasm skill '{}' violated the sandbox and has been quarantined",
                    self.skill_name
                )))
            }
            ExecutionResult::Error(message) => Err(SkillError::Internal(message)),
        }
    }
}

fn write_runtime_skill_audit(
    conn: &rusqlite::Connection,
    skill_name: &str,
    artifact_digest: &str,
    reason_code: &str,
    reason_detail: &str,
    actor: &str,
) -> Result<(), ghost_audit::query_engine::AuditError> {
    let engine = AuditQueryEngine::new(conn);
    engine.insert(&AuditEntry {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: format!("skill:{skill_name}"),
        event_type: "skill_runtime_quarantined".to_string(),
        severity: "warn".to_string(),
        tool_name: Some(format!("skill_{skill_name}")),
        details: serde_json::json!({
            "actor": actor,
            "details": {
                "artifact_digest": artifact_digest,
                "reason_code": reason_code,
                "reason_detail": reason_detail,
            }
        })
        .to_string(),
        session_id: None,
        actor_id: Some(actor.to_string()),
        operation_id: None,
        request_id: None,
        idempotency_key: None,
        idempotency_status: None,
    })
}

fn read_no_follow(path: &Path) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err(format!(
                "managed artifact path is not a file: {}",
                path.display()
            ));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        return Ok(bytes);
    }

    #[cfg(not(unix))]
    {
        let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "managed artifact symlinks are not allowed: {}",
                path.display()
            ));
        }
        if !metadata.is_file() {
            return Err(format!(
                "managed artifact path is not a file: {}",
                path.display()
            ));
        }
        fs::read(path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use cortex_storage::queries::external_skill_queries;
    use ghost_signing::generate_keypair;
    use ghost_skills::artifact::{ArtifactSourceKind, SkillManifestSource};
    use tempfile::TempDir;
    use wat::parse_str;

    use super::*;

    #[tokio::test]
    async fn executes_pure_wasm_from_managed_artifact() {
        let harness = Harness::new().await;
        let (artifact_path, artifact, digest) = harness.write_artifact("echo", &echo_module());
        let handler = harness.seed_handler(
            "echo",
            &digest,
            &artifact_path,
            &artifact.manifest.requested_capabilities,
        );

        let output = handler
            .execute(
                &harness.env,
                &serde_json::json!({"message": "hello"}),
                &harness.exec_ctx,
            )
            .unwrap();
        assert_eq!(output, serde_json::json!({"message": "hello"}));

        let quarantine = external_skill_queries::get_external_skill_quarantine(
            &harness.db.read().unwrap(),
            &digest,
        )
        .unwrap();
        assert!(quarantine.is_none());
    }

    #[tokio::test]
    async fn sandbox_violation_quarantines_hidden_import_probe() {
        let harness = Harness::new().await;
        let (artifact_path, artifact, digest) =
            harness.write_artifact("evil", &env_import_module());
        let handler = harness.seed_handler(
            "evil",
            &digest,
            &artifact_path,
            &artifact.manifest.requested_capabilities,
        );

        let error = handler
            .execute(&harness.env, &serde_json::json!({}), &harness.exec_ctx)
            .unwrap_err();
        assert!(matches!(error, SkillError::SandboxViolation(_)));

        let db = harness.db.read().unwrap();
        let quarantine = external_skill_queries::get_external_skill_quarantine(&db, &digest)
            .unwrap()
            .unwrap();
        assert_eq!(quarantine.state, ExternalSkillQuarantineState::Quarantined);
        assert_eq!(quarantine.reason_code.as_deref(), Some("sandbox_escape"));
        assert!(quarantine
            .reason_detail
            .as_deref()
            .unwrap_or_default()
            .contains("environ_get"));
        let audit_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE event_type = 'skill_runtime_quarantined' AND details LIKE ?1",
                [format!("%{digest}%")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 1);
    }

    #[tokio::test]
    async fn tampered_managed_artifact_is_quarantined_before_execution() {
        let harness = Harness::new().await;
        let (artifact_path, _artifact, digest) = harness.write_artifact("echo", &echo_module());
        let (_, replacement, _) = harness.write_artifact("echo", &different_echo_module());
        replacement.write_to_path(&artifact_path).unwrap();

        let handler = harness.seed_handler("echo", &digest, &artifact_path, &[]);
        let error = handler
            .execute(
                &harness.env,
                &serde_json::json!({"message": "tamper"}),
                &harness.exec_ctx,
            )
            .unwrap_err();
        assert!(matches!(error, SkillError::AuthorizationDenied(_)));

        let quarantine = external_skill_queries::get_external_skill_quarantine(
            &harness.db.read().unwrap(),
            &digest,
        )
        .unwrap()
        .unwrap();
        assert_eq!(quarantine.state, ExternalSkillQuarantineState::Quarantined);
        assert_eq!(
            quarantine.reason_code.as_deref(),
            Some("managed_artifact_digest_mismatch")
        );
    }

    #[tokio::test]
    async fn resource_exhaustion_fails_closed_without_quarantining_the_skill() {
        let harness = Harness::new().await;
        let (artifact_path, artifact, digest) =
            harness.write_artifact("spin", &infinite_loop_module());
        let handler = ExternalWasmSkillHandler::new(
            digest.clone(),
            "spin".to_string(),
            "spin".to_string(),
            artifact_path.display().to_string(),
            Arc::clone(&harness.db),
            WasmSandboxConfig {
                timeout: Duration::from_millis(20),
                fuel_limit: u64::MAX / 4,
                ..Default::default()
            },
        );
        harness.seed_artifact_row(
            "spin",
            &digest,
            &artifact_path,
            &artifact.manifest.requested_capabilities,
        );

        let error = handler
            .execute(&harness.env, &serde_json::json!({}), &harness.exec_ctx)
            .unwrap_err();
        eprintln!("resource exhaustion error: {error:?}");
        assert!(matches!(
            error,
            SkillError::ExecutionTimedOut(_) | SkillError::ResourceExhausted(_)
        ));
        let quarantine = external_skill_queries::get_external_skill_quarantine(
            &harness.db.read().unwrap(),
            &digest,
        )
        .unwrap();
        assert!(quarantine.is_none());
    }

    struct Harness {
        _temp_dir: TempDir,
        db: Arc<DbPool>,
        env: SkillHandlerEnvironment,
        exec_ctx: ExecutionContext,
    }

    impl Harness {
        async fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            let db_path = temp_dir.path().join("runtime.db");
            let db = crate::db_pool::create_pool(db_path).unwrap();
            {
                let writer = db.writer_for_migrations().await;
                cortex_storage::migrations::run_migrations(&writer).unwrap();
            }
            let env = SkillHandlerEnvironment {
                db: db.legacy_connection().unwrap(),
                convergence_profile: "standard".to_string(),
            };
            let exec_ctx = ExecutionContext {
                agent_id: uuid::Uuid::now_v7(),
                session_id: uuid::Uuid::now_v7(),
                intervention_level: 0,
                session_duration: Duration::ZERO,
                session_reflection_count: 0,
                is_compaction_flush: false,
            };
            Self {
                _temp_dir: temp_dir,
                db,
                env,
                exec_ctx,
            }
        }

        fn write_artifact(
            &self,
            skill_name: &str,
            wasm_bytes: &[u8],
        ) -> (std::path::PathBuf, SkillArtifact, String) {
            let artifact_root = self._temp_dir.path().join("managed");
            fs::create_dir_all(&artifact_root).unwrap();
            let (signing_key, _) = generate_keypair();
            let artifact = SkillArtifact::build(
                SkillManifestSource {
                    manifest_schema_version: ghost_skills::artifact::MANIFEST_SCHEMA_VERSION,
                    name: skill_name.to_string(),
                    version: "1.0.0".to_string(),
                    publisher: "ghost-test".to_string(),
                    description: format!("external {skill_name}"),
                    source_kind: ArtifactSourceKind::Workspace,
                    execution_mode: ArtifactExecutionMode::Wasm,
                    entrypoint: "module.wasm".to_string(),
                    requested_capabilities: Vec::new(),
                    declared_privileges: vec!["Pure WASM computation".to_string()],
                },
                BTreeMap::from([("module.wasm".to_string(), wasm_bytes.to_vec())]),
                &signing_key,
            )
            .unwrap();
            let digest = artifact.artifact_digest().unwrap();
            let artifact_path = artifact_root.join(format!("{digest}.ghostskill"));
            artifact.write_to_path(&artifact_path).unwrap();
            (artifact_path, artifact, digest)
        }

        fn seed_artifact_row(
            &self,
            skill_name: &str,
            digest: &str,
            artifact_path: &Path,
            requested_capabilities: &[String],
        ) {
            let legacy = self.db.legacy_connection().unwrap();
            let conn = legacy.lock().unwrap();
            let artifact_path = artifact_path.display().to_string();
            external_skill_queries::upsert_external_skill_artifact(
                &conn,
                digest,
                1,
                skill_name,
                "1.0.0",
                "ghost-test",
                &format!("external {skill_name}"),
                "workspace",
                "wasm",
                "module.wasm",
                &artifact_path,
                &artifact_path,
                &artifact_path,
                "{}",
                &serde_json::to_string(requested_capabilities).unwrap(),
                "[\"Pure WASM computation\"]",
                Some("key-1"),
                512,
            )
            .unwrap();
        }

        fn seed_handler(
            &self,
            skill_name: &str,
            digest: &str,
            artifact_path: &Path,
            requested_capabilities: &[String],
        ) -> ExternalWasmSkillHandler {
            self.seed_artifact_row(skill_name, digest, artifact_path, requested_capabilities);
            ExternalWasmSkillHandler::new(
                digest.to_string(),
                skill_name.to_string(),
                format!("external {skill_name}"),
                artifact_path.display().to_string(),
                Arc::clone(&self.db),
                WasmSandboxConfig::default(),
            )
        }
    }

    fn echo_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (memory (export "memory") 2)
              (global $heap (mut i32) (i32.const 1024))
              (func (export "alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
              (func (export "run") (param $input_ptr i32) (param $input_len i32) (result i64)
                local.get $input_ptr
                i64.extend_i32_u
                i64.const 32
                i64.shl
                local.get $input_len
                i64.extend_i32_u
                i64.or))
            "#,
        )
        .unwrap()
    }

    fn different_echo_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (memory (export "memory") 2)
              (global $heap (mut i32) (i32.const 2048))
              (func (export "alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
              (func (export "run") (param $input_ptr i32) (param $input_len i32) (result i64)
                local.get $input_ptr
                i64.extend_i32_u
                i64.const 32
                i64.shl
                local.get $input_len
                i64.extend_i32_u
                i64.or))
            "#,
        )
        .unwrap()
    }

    fn infinite_loop_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (func (export "alloc") (param i32) (result i32)
                i32.const 0)
              (func (export "run") (param i32 i32) (result i64)
                (loop
                  br 0)
                i64.const 0))
            "#,
        )
        .unwrap()
    }

    fn env_import_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (import "wasi_snapshot_preview1" "environ_get" (func $environ_get (param i32 i32) (result i32)))
              (memory (export "memory") 1)
              (func (export "alloc") (param i32) (result i32)
                i32.const 0)
              (func (export "run") (param i32 i32) (result i64)
                i64.const 0))
            "#,
        )
        .unwrap()
    }
}
