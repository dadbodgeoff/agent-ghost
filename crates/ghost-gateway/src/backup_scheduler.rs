//! Scheduled backup background task (T-3.4.3).
//!
//! Runs daily backups at the configured cron time and prunes old backups.

use std::sync::Arc;

use chrono::Timelike;

use crate::api::websocket::WsEvent;
use crate::state::AppState;

/// Default backup time: 3 AM daily.
const DEFAULT_BACKUP_HOUR: u32 = 3;

/// Default retention: 30 days.
const DEFAULT_RETENTION_DAYS: u64 = 30;

/// Start the backup scheduler background task.
///
/// When using `GatewayRuntime`, prefer `backup_scheduler_task()` with
/// `runtime.spawn_tracked()` instead of this function.
pub fn spawn_backup_scheduler(state: Arc<AppState>) {
    tokio::spawn(backup_scheduler_task(state));
}

/// The backup scheduler loop as a standalone future.
/// Designed to be wrapped by `GatewayRuntime::spawn_tracked()` which
/// adds cancellation handling.
pub async fn backup_scheduler_task(state: Arc<AppState>) {
    // Check if backups are enabled.
        let backup_dir = std::env::var("GHOST_BACKUP_DIR").unwrap_or_else(|_| "./backups".into());
        // T-5.8.1: Require explicit passphrase — never use hardcoded default.
        let passphrase = match std::env::var("GHOST_BACKUP_PASSPHRASE") {
            Ok(p) if !p.is_empty() => p,
            _ => {
                let key_path = crate::bootstrap::shellexpand_tilde("~/.ghost/backup.key");
                match std::fs::read_to_string(&key_path) {
                    Ok(existing) if !existing.trim().is_empty() => existing.trim().to_string(),
                    _ => {
                        tracing::error!(
                            "No backup passphrase configured (GHOST_BACKUP_PASSPHRASE) \
                             and no key file at {key_path} — scheduled backups disabled"
                        );
                        return;
                    }
                }
            }
        };
        let retention_days: u64 = std::env::var("GHOST_BACKUP_RETENTION_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_RETENTION_DAYS);
        let ghost_dir = std::env::var("GHOST_DIR").unwrap_or_else(|_| ".".into());

        // Compute time until next backup hour, then sleep precisely.
        loop {
            let now = chrono::Utc::now();
            let current_hour = now.hour();
            let hours_until = if current_hour < DEFAULT_BACKUP_HOUR {
                DEFAULT_BACKUP_HOUR - current_hour
            } else {
                24 - current_hour + DEFAULT_BACKUP_HOUR
            };
            let next_run = now
                + chrono::Duration::hours(hours_until as i64)
                - chrono::Duration::minutes(now.minute() as i64)
                - chrono::Duration::seconds(now.second() as i64);
            let delay_secs = (next_run - now).num_seconds().max(60) as u64;
            tracing::debug!(delay_secs, "Backup scheduler sleeping until next run");
            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

            let now = chrono::Utc::now();

            tracing::info!("Scheduled backup starting");

            if let Err(e) = std::fs::create_dir_all(&backup_dir) {
                tracing::error!(error = %e, "Failed to create backup directory");
                continue;
            }

            let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
            let output_path = std::path::PathBuf::from(&backup_dir)
                .join(format!("ghost-backup-{timestamp}.tar.gz"));

            let exporter = ghost_backup::BackupExporter::new(&ghost_dir);
            match exporter.export(&output_path, &passphrase) {
                Ok(manifest) => {
                    let size = std::fs::metadata(&output_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    let backup_id = uuid::Uuid::now_v7().to_string();

                    // T-5.3.8: Stream BLAKE3 checksum in 64KB chunks instead of
                    // loading entire file into memory.
                    let checksum = {
                        let mut hasher = blake3::Hasher::new();
                        match std::fs::File::open(&output_path) {
                            Ok(mut file) => {
                                let mut buf = [0u8; 65536];
                                loop {
                                    use std::io::Read;
                                    match file.read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => hasher.update(&buf[..n]),
                                        Err(e) => {
                                            tracing::warn!(error = %e, "Error reading backup for checksum");
                                            break;
                                        }
                                    };
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to open backup for checksum");
                            }
                        }
                        hasher.finalize().to_hex().to_string()
                    };
                    {
                        let db = state.db.write().await;
                        let _ = db.execute(
                            "INSERT INTO backup_manifest (id, size_bytes, entry_count, blake3_checksum, status) \
                             VALUES (?1, ?2, ?3, ?4, 'complete')",
                            rusqlite::params![
                                backup_id,
                                size as i64,
                                manifest.entries.len() as i64,
                                checksum,
                            ],
                        );
                    }

                    crate::api::websocket::broadcast_event(&state, WsEvent::BackupComplete {
                        backup_id,
                        status: "complete".into(),
                        size_bytes: size,
                    });

                    tracing::info!(
                        entries = manifest.entries.len(),
                        size_bytes = size,
                        "Scheduled backup complete"
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "Scheduled backup failed");
                }
            }

            // Prune old backups.
            prune_old_backups(&backup_dir, retention_days);

            // Prune old stream event log entries (>24h).
            prune_stream_events(&state).await;

            // WP9-D: Session lifecycle cleanup.
            prune_inactive_sessions(&state).await;
        }
}

/// Delete stream event log entries older than 24 hours.
/// These are only needed for short-term SSE recovery, not long-term storage.
///
/// Uses the writer connection — DELETE is a write operation and will fail
/// on read-only connections (SQLITE_OPEN_READ_ONLY).
async fn prune_stream_events(state: &Arc<AppState>) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let conn = state.db.write().await;
    match cortex_storage::queries::stream_event_queries::delete_events_before(&conn, &cutoff) {
        Ok(count) if count > 0 => {
            tracing::info!(deleted = count, "Pruned old stream event log entries");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "Failed to prune stream event log");
        }
    }
}

/// WP9-D: Soft-delete sessions inactive beyond TTL, hard-delete beyond 2x TTL.
///
/// Uses the writer connection — UPDATE/DELETE are write operations.
async fn prune_inactive_sessions(state: &Arc<AppState>) {
    let ttl_days = state.session_ttl_days;
    if ttl_days == 0 {
        return; // TTL disabled
    }

    let now = chrono::Utc::now();

    // Soft-delete: sessions inactive for > TTL days.
    let soft_cutoff = (now - chrono::Duration::days(ttl_days as i64))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    // Hard-delete: sessions soft-deleted for > TTL days (i.e., deleted_at older than TTL ago).
    let hard_cutoff = (now - chrono::Duration::days(ttl_days as i64 * 2))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let conn = state.db.write().await;

    // Hard-delete first (removes sessions + messages + audits).
    match cortex_storage::queries::studio_chat_queries::hard_delete_old_sessions(
        &conn, &hard_cutoff,
    ) {
        Ok(count) if count > 0 => {
            tracing::info!(
                deleted = count,
                cutoff = %hard_cutoff,
                "Hard-deleted expired sessions (2x TTL)"
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "Failed to hard-delete expired sessions");
        }
    }

    // Soft-delete inactive sessions.
    match cortex_storage::queries::studio_chat_queries::soft_delete_inactive_sessions(
        &conn, &soft_cutoff,
    ) {
        Ok(count) if count > 0 => {
            tracing::info!(
                soft_deleted = count,
                cutoff = %soft_cutoff,
                ttl_days = ttl_days,
                "Soft-deleted inactive sessions"
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "Failed to soft-delete inactive sessions");
        }
    }
}

fn prune_old_backups(backup_dir: &str, retention_days: u64) {
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(retention_days * 86400);

    let Ok(entries) = std::fs::read_dir(backup_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("ghost-backup-") && n.ends_with(".tar.gz"))
            .unwrap_or(false)
        {
            continue;
        }

        let Ok(metadata) = path.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified < cutoff {
            tracing::info!(path = %path.display(), "Pruning old backup");
            let _ = std::fs::remove_file(&path);
        }
    }
}
