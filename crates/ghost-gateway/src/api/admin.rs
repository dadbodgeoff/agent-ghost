//! Admin endpoints for backup/restore/export (T-3.4.1–3.4.4).
//!
//! All endpoints require `role == "admin"` in JWT claims.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::OperationContext;
use crate::state::AppState;

const CREATE_BACKUP_ROUTE_TEMPLATE: &str = "/api/admin/backup";
const RESTORE_BACKUP_ROUTE_TEMPLATE: &str = "/api/admin/restore";

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub backup_id: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub entry_count: usize,
    pub blake3_checksum: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    pub backup_path: String,
}

#[derive(Debug, Serialize)]
pub struct RestoreVerification {
    pub valid: bool,
    pub entry_count: usize,
    pub version: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ExportParams {
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "jsonl".into()
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub format: String,
    pub entities: Vec<ExportEntity>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ExportEntity {
    pub entity_type: String,
    pub count: usize,
    pub data: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct BackupListResponse {
    pub backups: Vec<BackupResponse>,
}

// ── Auth helper ─────────────────────────────────────────────────────

fn require_role(claims: Option<&Claims>, role: &str) -> Result<(), ApiError> {
    if let Some(claims) = claims {
        if claims.role == role {
            return Ok(());
        }
    }
    Err(ApiError::Forbidden(format!(
        "{} role required for this operation",
        role.to_ascii_uppercase()
    )))
}

fn backup_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown-admin")
}

fn resolve_backup_passphrase() -> String {
    match std::env::var("GHOST_BACKUP_PASSPHRASE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            let key_path = crate::bootstrap::shellexpand_tilde("~/.ghost/backup.key");
            if let Ok(existing) = std::fs::read_to_string(&key_path) {
                if !existing.trim().is_empty() {
                    existing.trim().to_string()
                } else {
                    generate_backup_key(&key_path)
                }
            } else {
                generate_backup_key(&key_path)
            }
        }
    }
}

fn backup_paths(backup_dir: &str, backup_id: &str) -> (PathBuf, PathBuf) {
    let final_path =
        PathBuf::from(backup_dir).join(format!("ghost-backup-{backup_id}.ghost-backup"));
    let temp_path = PathBuf::from(backup_dir).join(format!(".ghost-backup-{backup_id}.tmp"));
    (final_path, temp_path)
}

fn checksum_file(path: &Path) -> Result<(String, u64), ApiError> {
    let data =
        std::fs::read(path).map_err(|e| ApiError::internal(format!("read backup archive: {e}")))?;
    Ok((blake3::hash(&data).to_hex().to_string(), data.len() as u64))
}

fn verify_backup_archive(
    backup_path: &Path,
    passphrase: &str,
) -> Result<(ghost_backup::BackupManifest, String, u64), ApiError> {
    if !backup_path.exists() {
        return Err(ApiError::not_found("Backup file not found"));
    }

    let verify_dir = tempfile::tempdir()
        .map_err(|e| ApiError::internal(format!("create restore verification dir: {e}")))?;
    let importer = ghost_backup::BackupImporter::new(verify_dir.path());
    let manifest = importer
        .import(backup_path, passphrase)
        .map_err(|e| ApiError::internal(format!("Backup verification failed: {e}")))?;
    let (checksum, size_bytes) = checksum_file(backup_path)?;
    Ok((manifest, checksum, size_bytes))
}

// ── Handlers ────────────────────────────────────────────────────────

/// POST /api/admin/backup — trigger a platform-state archive export.
pub async fn create_backup(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
) -> Response {
    let claims = claims.as_ref().map(|claims| &claims.0);
    if let Err(error) = require_role(claims, "admin") {
        return error_response_with_idempotency(error);
    }
    let actor = backup_actor(claims);
    let backup_dir = std::env::var("GHOST_BACKUP_DIR").unwrap_or_else(|_| "./backups".into());
    let ghost_dir = std::env::var("GHOST_DIR").unwrap_or_else(|_| ".".into());
    let passphrase = resolve_backup_passphrase();
    let backup_id = operation_context
        .operation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let request_body = serde_json::json!({
        "backup_id": backup_id,
        "backup_dir": backup_dir,
        "ghost_dir": ghost_dir,
    });

    let db = state.db.write().await;
    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_BACKUP_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            std::fs::create_dir_all(&backup_dir).map_err(|e| {
                ApiError::internal(format!("Failed to create backup directory: {e}"))
            })?;

            let (output_path, temp_path) = backup_paths(&backup_dir, &backup_id);
            let _ = std::fs::remove_file(&temp_path);

            let exporter = ghost_backup::BackupExporter::new(&ghost_dir);
            let manifest = exporter
                .export(&temp_path, &passphrase)
                .map_err(|e| ApiError::internal(format!("Backup failed: {e}")))?;

            let temp_file = std::fs::OpenOptions::new()
                .read(true)
                .open(&temp_path)
                .map_err(|e| ApiError::internal(format!("open backup temp file: {e}")))?;
            temp_file
                .sync_all()
                .map_err(|e| ApiError::internal(format!("fsync backup temp file: {e}")))?;

            if output_path.exists() {
                std::fs::remove_file(&output_path).map_err(|e| {
                    ApiError::internal(format!("replace prior backup archive: {e}"))
                })?;
            }
            std::fs::rename(&temp_path, &output_path)
                .map_err(|e| ApiError::internal(format!("finalize backup archive: {e}")))?;

            let (checksum, size_bytes) = checksum_file(&output_path)?;
            let created_at = chrono::Utc::now().to_rfc3339();
            let metadata = serde_json::json!({
                "manifest": manifest,
                "archive_path": output_path,
                "ghost_dir": ghost_dir,
            });

            conn.execute(
                "INSERT OR REPLACE INTO backup_manifest (id, created_at, size_bytes, entry_count, blake3_checksum, status, metadata) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'complete', ?6)",
                rusqlite::params![
                    backup_id,
                    created_at,
                    size_bytes as i64,
                    metadata["manifest"]["entries"].as_array().map(|entries| entries.len()).unwrap_or(0) as i64,
                    checksum,
                    metadata.to_string(),
                ],
            )
            .map_err(|e| ApiError::db_error("record backup manifest", e))?;

            Ok((
                axum::http::StatusCode::OK,
                serde_json::json!(BackupResponse {
                    backup_id: backup_id.clone(),
                    created_at,
                    size_bytes,
                    entry_count: metadata["manifest"]["entries"]
                        .as_array()
                        .map(|entries| entries.len())
                        .unwrap_or(0),
                    blake3_checksum: checksum,
                    status: "complete".into(),
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            if outcome.idempotency_status
                == crate::api::operation_context::IdempotencyStatus::Executed
            {
                crate::api::websocket::broadcast_event(
                    &state,
                    crate::api::websocket::WsEvent::BackupComplete {
                        backup_id: backup_id.clone(),
                        status: "complete".into(),
                        size_bytes: outcome.body["size_bytes"].as_u64().unwrap_or(0),
                    },
                );
            }

            write_mutation_audit_entry(
                &db,
                "platform",
                "create_backup",
                "high",
                actor,
                "complete",
                serde_json::json!({
                    "backup_id": backup_id,
                    "size_bytes": outcome.body["size_bytes"],
                    "entry_count": outcome.body["entry_count"],
                    "backup_dir": backup_dir,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/admin/restore — verify archive integrity (does NOT apply).
///
/// Validates the backup archive exists and computes its BLAKE3 checksum.
/// Actual restore requires running `ghost restore <path>` via the CLI.
pub async fn restore_backup(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<RestoreRequest>,
) -> Response {
    let claims = claims.as_ref().map(|claims| &claims.0);
    if let Err(error) = require_role(claims, "superadmin") {
        return error_response_with_idempotency(error);
    }
    let actor = backup_actor(claims);
    let request_body = serde_json::json!({
        "backup_path": req.backup_path,
    });
    let passphrase = resolve_backup_passphrase();
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        RESTORE_BACKUP_ROUTE_TEMPLATE,
        &request_body,
        |_conn| {
            let backup_path = Path::new(&req.backup_path);
            let (manifest, checksum, size_bytes) = verify_backup_archive(backup_path, &passphrase)?;
            let response = RestoreVerification {
                valid: true,
                entry_count: manifest.entries.len(),
                version: manifest.version.clone(),
                message: format!(
                    "Archive verified (BLAKE3: {}…, {} bytes). This checks the export package only, not migration rollback safety.",
                    &checksum[..16],
                    size_bytes
                ),
            };
            Ok((
                axum::http::StatusCode::OK,
                serde_json::to_value(&response).map_err(|e| {
                    ApiError::internal(format!("serialize restore verification: {e}"))
                })?,
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "restore_backup_verification",
                "critical",
                actor,
                "verified",
                serde_json::json!({
                    "backup_path": req.backup_path,
                    "entry_count": outcome.body["entry_count"],
                    "version": outcome.body["version"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/admin/export — export entities as JSON or JSONL.
///
/// When `format=jsonl`, returns newline-delimited JSON (one object per line).
/// When `format=json` (or default), returns structured JSON wrapper.
pub async fn export_data(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> Result<axum::response::Response, ApiError> {
    require_role(request.extensions().get::<Claims>(), "admin")?;

    // Parse format from query string manually.
    let format = request
        .uri()
        .query()
        .and_then(|q| {
            q.split('&').find_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                if parts.next() == Some("format") {
                    parts.next().map(|v| v.to_string())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "jsonl".into());

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("export_data", e))?;

    let table_queries: &[(&str, &str)] = &[
        ("agents", "SELECT id, name FROM agents LIMIT 10000"),
        (
            "memories",
            "SELECT id, memory_type, summary FROM memory_snapshots LIMIT 10000",
        ),
        (
            "proposals",
            "SELECT id, operation, decision FROM goal_proposals LIMIT 10000",
        ),
        (
            "audit",
            "SELECT id, event_type, severity FROM audit_log LIMIT 10000",
        ),
    ];

    let mut entities = Vec::new();
    for &(entity_type, query) in table_queries {
        let data = export_table(&db, entity_type, query);
        entities.push(ExportEntity {
            entity_type: entity_type.into(),
            count: data.len(),
            data,
        });
    }

    if format == "jsonl" {
        // Build newline-delimited JSON: each entity row is one line with _type prefix.
        let mut lines = String::new();
        for entity in &entities {
            for row in &entity.data {
                let mut obj = row.as_object().cloned().unwrap_or_default();
                obj.insert(
                    "_type".into(),
                    serde_json::Value::String(entity.entity_type.clone()),
                );
                lines.push_str(
                    &serde_json::to_string(&serde_json::Value::Object(obj)).unwrap_or_default(),
                );
                lines.push('\n');
            }
        }

        use axum::response::IntoResponse;
        Ok((
            [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
            lines,
        )
            .into_response())
    } else {
        let total: usize = entities.iter().map(|e| e.count).sum();
        use axum::response::IntoResponse;
        Ok(Json(ExportResponse {
            format,
            entities,
            total,
        })
        .into_response())
    }
}

/// GET /api/admin/backups — list existing backup manifests.
pub async fn list_backups(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> ApiResult<BackupListResponse> {
    require_role(request.extensions().get::<Claims>(), "admin")?;

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_backups", e))?;
    let mut stmt = db
        .prepare(
            "SELECT id, created_at, size_bytes, entry_count, blake3_checksum, status \
             FROM backup_manifest ORDER BY created_at DESC LIMIT 100",
        )
        .map_err(|e| ApiError::db_error("prepare backup list", e))?;

    let backups: Vec<BackupResponse> = stmt
        .query_map([], |row| {
            Ok(BackupResponse {
                backup_id: row.get(0)?,
                created_at: row.get(1)?,
                size_bytes: row.get::<_, i64>(2).unwrap_or(0) as u64,
                entry_count: row.get::<_, i64>(3).unwrap_or(0) as usize,
                blake3_checksum: row.get(4)?,
                status: row.get(5)?,
            })
        })
        .map_err(|e| ApiError::db_error("query backups", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(BackupListResponse { backups }))
}

// ── Helpers ─────────────────────────────────────────────────────────

/// T-5.8.1: Generate a random 32-byte backup key and persist to the given path.
fn generate_backup_key(key_path: &str) -> String {
    use std::io::Write;
    // Use two UUIDv4 (128-bit random each) to produce 256 bits of randomness.
    let key_hex = format!(
        "{}{}",
        uuid::Uuid::new_v4().as_simple(),
        uuid::Uuid::new_v4().as_simple(),
    );
    // Ensure parent directory exists.
    if let Some(parent) = std::path::Path::new(key_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Write with restrictive permissions (0600).
    match std::fs::File::create(key_path) {
        Ok(mut f) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            let _ = f.write_all(key_hex.as_bytes());
            tracing::info!(path = key_path, "Generated new backup encryption key");
        }
        Err(e) => {
            tracing::error!(path = key_path, error = %e, "Failed to persist backup key");
        }
    }
    key_hex
}

fn export_table(
    db: &rusqlite::Connection,
    _table_name: &str,
    query: &str,
) -> Vec<serde_json::Value> {
    let Ok(mut stmt) = db.prepare(query) else {
        return Vec::new();
    };

    let col_count = stmt.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let Ok(rows) = stmt.query_map([], |row| {
        let mut obj = serde_json::Map::new();
        for (i, name) in col_names.iter().enumerate() {
            let val: String = row.get::<_, String>(i).unwrap_or_default();
            obj.insert(name.clone(), serde_json::Value::String(val));
        }
        Ok(serde_json::Value::Object(obj))
    }) else {
        return Vec::new();
    };

    rows.filter_map(|r| r.ok()).collect()
}
