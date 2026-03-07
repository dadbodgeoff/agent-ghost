//! Read/write separated SQLite connection pool for WAL mode.
//!
//! Architecture:
//!   - 1 writer connection (serialized via TokioMutex, safe across .await)
//!   - N reader connections (lock-free pool via ArrayQueue)
//!   - WAL mode enables concurrent readers + single writer
//!
//! Invariants:
//!   - Writer connection is NEVER used for reads (prevents writer starvation)
//!   - Reader connections are opened with SQLITE_OPEN_READ_ONLY
//!   - busy_timeout = 5000ms on all connections
//!   - All connections share the same WAL file

use crossbeam_queue::ArrayQueue;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Semaphore;

/// Maximum number of overflow connections allowed beyond the pool.
const MAX_OVERFLOW_CONNECTIONS: usize = 4;
/// Timeout for acquiring an overflow permit.
const OVERFLOW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Read/write separated connection pool for SQLite WAL mode.
pub struct DbPool {
    writer: TokioMutex<Connection>,
    readers: ArrayQueue<Connection>,
    db_path: PathBuf,
    _pool_size: usize,
    /// Semaphore limiting overflow connections to prevent file descriptor exhaustion.
    overflow_semaphore: Arc<Semaphore>,
}

/// RAII guard that returns a reader connection to the pool on drop.
pub struct ReadConn<'a> {
    conn: Option<Connection>,
    pool: &'a DbPool,
    /// Holds an overflow semaphore permit when this is an overflow connection.
    _overflow_permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl DbPool {
    /// Create pool with 1 writer + `pool_size` readers.
    /// Recommended pool_size: `min(num_cpus, 8)`, minimum 2.
    pub fn open(db_path: PathBuf, pool_size: usize) -> Result<Self, DbPoolError> {
        let pool_size = pool_size.max(2);

        // Writer: read-write, WAL mode, busy_timeout 5000ms
        let writer = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        writer.pragma_update(None, "journal_mode", "WAL")?;
        writer.pragma_update(None, "busy_timeout", 5000)?;
        writer.pragma_update(None, "synchronous", "NORMAL")?;
        writer.pragma_update(None, "foreign_keys", "ON")?;

        // Readers: read-only, same pragmas minus journal_mode (inherited via WAL)
        let readers = ArrayQueue::new(pool_size);
        for _ in 0..pool_size {
            let r = Connection::open_with_flags(
                &db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            r.pragma_update(None, "busy_timeout", 5000)?;
            readers
                .push(r)
                .map_err(|_| DbPoolError::PoolFull)?;
        }

        Ok(Self {
            writer: TokioMutex::new(writer),
            readers,
            db_path,
            _pool_size: pool_size,
            overflow_semaphore: Arc::new(Semaphore::new(MAX_OVERFLOW_CONNECTIONS)),
        })
    }

    /// Acquire write connection. Holds TokioMutex — safe across .await.
    pub async fn write(&self) -> tokio::sync::MutexGuard<'_, Connection> {
        self.writer.lock().await
    }

    /// Acquire read connection from pool. Returns RAII guard.
    /// If pool is empty, opens a temporary overflow read-only connection
    /// (capped at MAX_OVERFLOW_CONNECTIONS to prevent file descriptor exhaustion).
    pub fn read(&self) -> Result<ReadConn<'_>, DbPoolError> {
        match self.readers.pop() {
            Some(conn) => Ok(ReadConn {
                conn: Some(conn),
                pool: self,
                _overflow_permit: None,
            }),
            None => {
                // Pool exhausted — try to acquire overflow permit.
                let permit = match self.overflow_semaphore.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        // All overflow slots taken — return error instead of creating unbounded connections.
                        tracing::warn!(
                            max_overflow = MAX_OVERFLOW_CONNECTIONS,
                            "db_pool: reader pool AND overflow slots exhausted"
                        );
                        return Err(DbPoolError::PoolExhausted);
                    }
                };
                tracing::debug!("db_pool: reader pool exhausted, opening overflow connection");
                let conn = Connection::open_with_flags(
                    &self.db_path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                conn.pragma_update(None, "busy_timeout", 5000)?;
                Ok(ReadConn {
                    conn: Some(conn),
                    pool: self,
                    _overflow_permit: Some(permit),
                })
            }
        }
    }

    /// Access the writer connection directly for migrations (bootstrap only).
    /// This bypasses the pool and should NOT be used by API handlers.
    pub async fn writer_for_migrations(&self) -> tokio::sync::MutexGuard<'_, Connection> {
        self.writer.lock().await
    }

    /// WAL checkpoint (call during shutdown or scheduled maintenance).
    /// Logs checkpoint results including busy/log/checkpointed page counts.
    pub async fn checkpoint(&self) -> Result<(), DbPoolError> {
        let w = self.writer.lock().await;
        let result: (i32, i32, i32) = w.pragma_query_value(
            None,
            "wal_checkpoint(TRUNCATE)",
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let (busy, log_pages, checkpointed) = result;
        if busy != 0 {
            tracing::warn!(
                busy, log_pages, checkpointed,
                "WAL checkpoint was partial — readers may have been active"
            );
        } else {
            tracing::info!(log_pages, checkpointed, "WAL checkpoint completed successfully");
        }
        Ok(())
    }

    /// Create a standalone `Arc<Mutex<Connection>>` for components that still
    /// use the legacy API (e.g. `ghost-agent-loop`'s `AgentRunner.db` and
    /// `SkillBridge`). Opens a new read-write connection to the same database.
    ///
    /// This should be used sparingly — prefer `read()` / `write()` where possible.
    pub fn legacy_connection(&self) -> Result<std::sync::Arc<std::sync::Mutex<Connection>>, DbPoolError> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(std::sync::Arc::new(std::sync::Mutex::new(conn)))
    }
}

impl<'a> std::ops::Deref for ReadConn<'a> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("ReadConn used after drop")
    }
}

impl<'a> Drop for ReadConn<'a> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            // Try to return to pool; if full, connection is simply dropped.
            let _ = self.pool.readers.push(conn);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DbPoolError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Connection pool is full")]
    PoolFull,
    #[error("Database pool exhausted — all reader and overflow connections in use")]
    PoolExhausted,
}

/// Checkpoint mode for WAL operations.
pub enum CheckpointMode {
    /// Does not block readers or writers. Best for periodic maintenance.
    Passive,
    /// Blocks new writers, waits for existing readers. Used at shutdown.
    Truncate,
}

impl DbPool {
    /// Run a WAL checkpoint with the specified mode.
    /// Returns (busy, log_pages, checkpointed) on success.
    pub async fn checkpoint_with_mode(&self, mode: CheckpointMode) -> Result<(i32, i32, i32), DbPoolError> {
        let w = self.writer.lock().await;
        let pragma = match mode {
            CheckpointMode::Passive => "wal_checkpoint(PASSIVE)",
            CheckpointMode::Truncate => "wal_checkpoint(TRUNCATE)",
        };
        let result: (i32, i32, i32) = w.pragma_query_value(
            None,
            pragma,
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok(result)
    }

    /// Get the DB file path (for WAL size checks).
    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }
}

/// Spawn a background task that runs `PRAGMA wal_checkpoint(PASSIVE)` every 5 minutes.
/// Logs at debug level on success, warns if WAL exceeds 100MB.
pub async fn wal_checkpoint_task(db: Arc<DbPool>) {
    const CHECKPOINT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(300);
    const WAL_SIZE_WARN_BYTES: u64 = 100 * 1024 * 1024; // 100MB

    let mut interval = tokio::time::interval(CHECKPOINT_INTERVAL);
    interval.tick().await; // skip immediate first tick

    loop {
        interval.tick().await;

        // Check WAL file size before checkpoint.
        let wal_path = db.db_path().with_extension("db-wal");
        let wal_size = std::fs::metadata(&wal_path)
            .map(|m| m.len())
            .unwrap_or(0);

        if wal_size > WAL_SIZE_WARN_BYTES {
            tracing::warn!(
                wal_size_mb = wal_size / (1024 * 1024),
                "WAL file exceeds 100MB — checkpoint may be falling behind"
            );
        }

        match db.checkpoint_with_mode(CheckpointMode::Passive).await {
            Ok((busy, log_pages, checkpointed)) => {
                if busy != 0 {
                    tracing::debug!(
                        busy, log_pages, checkpointed,
                        "periodic WAL checkpoint partial (readers active)"
                    );
                } else {
                    tracing::debug!(
                        log_pages, checkpointed,
                        "periodic WAL checkpoint completed"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "periodic WAL checkpoint failed");
            }
        }
    }
}

/// Create a DbPool wrapped in Arc, ready for AppState.
pub fn create_pool(db_path: PathBuf) -> Result<Arc<DbPool>, DbPoolError> {
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let pool_size = num_cpus.min(8).max(2);
    let pool = DbPool::open(db_path, pool_size)?;
    Ok(Arc::new(pool))
}
