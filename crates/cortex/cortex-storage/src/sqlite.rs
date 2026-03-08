use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

pub const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;

pub fn apply_writer_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", SQLITE_BUSY_TIMEOUT_MS as i64)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}

pub fn apply_reader_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "busy_timeout", SQLITE_BUSY_TIMEOUT_MS as i64)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

pub fn maintenance_lock_path(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.maintenance.lock", db_path.display()))
}

#[derive(Debug, Error)]
pub enum MaintenanceLockError {
    #[error("database maintenance lock is held at {path}")]
    Held { path: PathBuf },
    #[error("failed to inspect maintenance lock at {path}: {source}")]
    Inspect {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to acquire maintenance lock at {path}: {source}")]
    Acquire {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write maintenance lock metadata at {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub struct MaintenanceLockGuard {
    path: PathBuf,
}

impl Drop for MaintenanceLockGuard {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %self.path.display(),
                    error = %error,
                    "failed to remove maintenance lock"
                );
            }
        }
    }
}

pub fn ensure_maintenance_lock_absent(db_path: &Path) -> Result<(), MaintenanceLockError> {
    let lock_path = maintenance_lock_path(db_path);
    match fs::metadata(&lock_path) {
        Ok(_) => Err(MaintenanceLockError::Held { path: lock_path }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(MaintenanceLockError::Inspect {
            path: lock_path,
            source: error,
        }),
    }
}

pub fn acquire_maintenance_lock(
    db_path: &Path,
) -> Result<MaintenanceLockGuard, MaintenanceLockError> {
    let lock_path = maintenance_lock_path(db_path);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).map_err(|error| MaintenanceLockError::Acquire {
            path: lock_path.clone(),
            source: error,
        })?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                MaintenanceLockError::Held {
                    path: lock_path.clone(),
                }
            } else {
                MaintenanceLockError::Acquire {
                    path: lock_path.clone(),
                    source: error,
                }
            }
        })?;

    let metadata = serde_json::json!({
        "pid": std::process::id(),
        "acquired_at": chrono::Utc::now().to_rfc3339(),
    });
    file.write_all(metadata.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| MaintenanceLockError::Write {
            path: lock_path.clone(),
            source: error,
        })?;

    Ok(MaintenanceLockGuard { path: lock_path })
}
