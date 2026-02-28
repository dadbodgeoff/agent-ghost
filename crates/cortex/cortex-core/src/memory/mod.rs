//! Memory model: types, half-lives, and base structures.

pub mod types;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use types::MemoryType;

/// The fundamental memory record shared across all Cortex subsystems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseMemory {
    pub id: Uuid,
    pub memory_type: MemoryType,
    pub content: serde_json::Value,
    pub summary: String,
    pub importance: Importance,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub last_accessed: Option<DateTime<Utc>>,
    pub access_count: u64,
    pub tags: Vec<String>,
    pub archived: bool,
}

/// Memory importance level, used for decay weighting and access control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Importance {
    Trivial,
    Low,
    Normal,
    High,
    Critical,
}
