//! Temporal event types: MemoryEvent, MemoryEventType, EventActor,
//! MemorySnapshot, SnapshotReason.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The atomic unit of temporal tracking. Every mutation to any memory
/// produces exactly one MemoryEvent. Events are immutable once written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvent {
    /// Monotonically increasing, gap-free.
    pub event_id: u64,
    /// Which memory was affected.
    pub memory_id: String,
    /// Transaction time — immutable once written.
    pub recorded_at: DateTime<Utc>,
    /// One of 17 variants.
    pub event_type: MemoryEventType,
    /// Field-level diff, NOT full state.
    pub delta: serde_json::Value,
    /// Who caused this mutation.
    pub actor: EventActor,
    /// Causal predecessors for ordering.
    pub caused_by: Vec<u64>,
    /// Starts at 1, incremented on schema changes.
    pub schema_version: u16,
}

/// 17 variants covering every mutation path in Cortex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEventType {
    Created,
    ContentUpdated,
    ConfidenceChanged,
    ImportanceChanged,
    TagsModified,
    LinkAdded,
    LinkRemoved,
    RelationshipAdded,
    RelationshipRemoved,
    StrengthUpdated,
    Archived,
    Restored,
    Decayed,
    Validated,
    Consolidated,
    Reclassified,
    Superseded,
}

/// Who caused the mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventActor {
    User(String),
    Agent(String),
    System(String),
}

/// Periodic state capture for O(k) reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub snapshot_id: u64,
    pub memory_id: String,
    pub snapshot_at: DateTime<Utc>,
    /// zstd-compressed JSON of full BaseMemory.
    pub state: Vec<u8>,
    /// Snapshot is valid up to this event.
    pub event_id: u64,
    pub snapshot_reason: SnapshotReason,
}

/// Why a snapshot was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotReason {
    /// Memory exceeded 50 events since last snapshot.
    EventThreshold,
    /// Weekly full-database snapshot.
    Periodic,
    /// Before consolidation or major mutation.
    PreOperation,
    /// User requested materialized view.
    OnDemand,
}
