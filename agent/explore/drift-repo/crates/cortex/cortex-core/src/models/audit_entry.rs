use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An entry in the append-only audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub memory_id: String,
    pub operation: AuditOperation,
    /// JSON details about the operation.
    pub details: serde_json::Value,
    pub actor: AuditActor,
    pub timestamp: DateTime<Utc>,
}

/// Operations tracked in the audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOperation {
    Create,
    Update,
    Archive,
    Restore,
    Link,
    Unlink,
    Decay,
    Validate,
    Consolidate,
    Reclassify,
}

/// Who performed the operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActor {
    System,
    User,
    Consolidation,
    Decay,
    Validation,
    Learning,
    Reclassification,
}
