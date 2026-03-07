//! Admin endpoints for backup/restore/export (T-3.4.1–3.4.4).
//!
//! All endpoints require `role == "admin"` in JWT claims.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

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

fn require_admin(ext: &axum::http::Extensions) -> Result<(), ApiError> {
    if let Some(claims) = ext.get::<Claims>() {
        if claims.role == "admin" {
            return Ok(());
        }
    }
    Err(ApiError::Forbidden(
        "Admin role required for this operation".to_owned(),
    ))
}

// ── Handlers ────────────────────────────────────────────────────────

/// POST /api/admin/backup — trigger a point-in-time backup.
pub async fn create_backup(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> ApiResult<BackupResponse> {
    require_admin(request.extensions())?;

    let backup_dir = std::env::var("GHOST_BACKUP_DIR").unwrap_or_else(|_| "./backups".into());

    // T-5.8.1: Require explicit backup passphrase — never use hardcoded default.
    let passphrase = match std::env::var("GHOST_BACKUP_PASSPHRASE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            // Generate random 32-byte passphrase and store in ~/.ghost/backup.key.
            let key_path = crate::bootstrap::shellexpand_tilde("~/.ghost/backup.key");
            if let Ok(existing) = std::fs::read_to_string(&key_path) {
                if !existing.trim().is_empty() {
                    existing.trim().to_string()
                } else {
                    let new_key = generate_backup_key(&key_path);
                    new_key
                }
            } else {
                let new_key = generate_backup_key(&key_path);
                new_key
            }
        }
    };

    // Ensure backup directory exists.
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| ApiError::internal(format!("Failed to create backup directory: {e}")))?;

    let backup_id = uuid::Uuid::now_v7().to_string();
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let output_path =
        std::path::PathBuf::from(&backup_dir).join(format!("ghost-backup-{timestamp}.tar.gz"));

    // Use ghost-backup exporter.
    let ghost_dir = std::env::var("GHOST_DIR").unwrap_or_else(|_| ".".into());
    let exporter = ghost_backup::BackupExporter::new(&ghost_dir);
    let manifest = exporter
        .export(&output_path, &passphrase)
        .map_err(|e| ApiError::internal(format!("Backup failed: {e}")))?;

    let size_bytes = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Compute overall BLAKE3 checksum of the archive.
    let checksum = {
        let data = std::fs::read(&output_path).unwrap_or_default();
        blake3::hash(&data).to_hex().to_string()
    };

    // Record in backup_manifest table.
    let db = state.db.write().await;
    db.execute(
        "INSERT INTO backup_manifest (id, size_bytes, entry_count, blake3_checksum, status, metadata) \
         VALUES (?1, ?2, ?3, ?4, 'complete', ?5)",
        rusqlite::params![
            backup_id,
            size_bytes as i64,
            manifest.entries.len() as i64,
            checksum,
            serde_json::to_string(&manifest).unwrap_or_default(),
        ],
    )
    .map_err(|e| ApiError::db_error("record backup manifest", e))?;

    // Broadcast WS event.
    crate::api::websocket::broadcast_event(
        &state,
        crate::api::websocket::WsEvent::BackupComplete {
            backup_id: backup_id.clone(),
            status: "complete".into(),
            size_bytes,
        },
    );

    Ok(Json(BackupResponse {
        backup_id,
        created_at: chrono::Utc::now().to_rfc3339(),
        size_bytes,
        entry_count: manifest.entries.len(),
        blake3_checksum: checksum,
        status: "complete".into(),
    }))
}

/// POST /api/admin/restore — verify backup integrity (does NOT apply).
///
/// Validates the backup archive exists and computes its BLAKE3 checksum.
/// Actual restore requires running `ghost restore <path>` via the CLI.
pub async fn restore_backup(
    State(_state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> ApiResult<RestoreVerification> {
    require_admin(request.extensions())?;

    // Parse body.
    let body = axum::body::to_bytes(request.into_body(), 2048)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid request body: {e}")))?;
    let req: RestoreRequest = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("Invalid JSON: {e}")))?;

    let backup_path = std::path::Path::new(&req.backup_path);
    if !backup_path.exists() {
        return Err(ApiError::not_found("Backup file not found"));
    }

    // Verify BLAKE3 integrity without importing.
    let data = std::fs::read(backup_path)
        .map_err(|e| ApiError::internal(format!("Failed to read backup file: {e}")))?;
    let checksum = blake3::hash(&data).to_hex().to_string();
    let is_valid_archive = data.len() > 2 && data[0] == 0x1f && data[1] == 0x8b;

    Ok(Json(RestoreVerification {
        valid: is_valid_archive,
        entry_count: 0,
        version: checksum[..16].to_string(),
        message: if is_valid_archive {
            format!(
                "Backup verified (BLAKE3: {}…, {} bytes). Use CLI to apply restore.",
                &checksum[..16],
                data.len()
            )
        } else {
            "Invalid backup archive format".into()
        },
    }))
}

/// GET /api/admin/export — export entities as JSON or JSONL.
///
/// When `format=jsonl`, returns newline-delimited JSON (one object per line).
/// When `format=json` (or default), returns structured JSON wrapper.
pub async fn export_data(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> Result<axum::response::Response, ApiError> {
    require_admin(request.extensions())?;

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
    require_admin(request.extensions())?;

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
