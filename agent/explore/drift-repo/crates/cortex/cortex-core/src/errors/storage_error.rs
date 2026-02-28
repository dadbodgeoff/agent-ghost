/// Storage-layer errors for SQLite operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("SQLite error: {message}")]
    SqliteError { message: String },

    #[error("migration failed at version {version}: {reason}")]
    MigrationFailed { version: u32, reason: String },

    #[error("database corruption detected: {details}")]
    CorruptionDetected { details: String },

    #[error("connection pool exhausted: {active_connections} active connections")]
    ConnectionPoolExhausted { active_connections: usize },
}
