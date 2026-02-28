/// Cloud sync errors.
#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("sync conflict on memory {memory_id}: local version {local_version}, remote version {remote_version}")]
    SyncConflict {
        memory_id: String,
        local_version: u64,
        remote_version: u64,
    },

    #[error("network error: {reason}")]
    NetworkError { reason: String },

    #[error("quota exceeded: {resource} usage {used}/{limit}")]
    QuotaExceeded {
        resource: String,
        used: u64,
        limit: u64,
    },

    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
}
