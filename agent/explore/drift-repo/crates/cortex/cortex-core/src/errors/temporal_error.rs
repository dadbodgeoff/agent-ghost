/// Temporal subsystem errors.
#[derive(Debug, thiserror::Error)]
pub enum TemporalError {
    #[error("event append failed: {0}")]
    EventAppendFailed(String),

    #[error("snapshot creation failed: {0}")]
    SnapshotCreationFailed(String),

    #[error("reconstruction failed: {0}")]
    ReconstructionFailed(String),

    #[error("query failed: {0}")]
    QueryFailed(String),

    #[error("invalid temporal bounds: {0}")]
    InvalidTemporalBounds(String),

    #[error("immutable field violation: {0}")]
    ImmutableFieldViolation(String),

    #[error("schema version mismatch: expected {expected}, found {found}")]
    SchemaVersionMismatch { expected: u16, found: u16 },

    #[error("compaction failed: {0}")]
    CompactionFailed(String),

    #[error("invalid epistemic transition: {from} → {to}")]
    InvalidEpistemicTransition { from: String, to: String },
}
