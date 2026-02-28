//! Per-field change descriptors for delta sync.
//!
//! Each variant describes a single field change that can be applied to a
//! [`MemoryCRDT`]. Used by the delta queue for efficient inter-agent sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::primitives::UniqueTag;
use cortex_core::models::provenance::ProvenanceHop;

/// Per-field change descriptor for delta sync.
///
/// Each variant represents a single field change that can be independently
/// applied to a `MemoryCRDT`. Uses `#[serde(tag = "type", content = "data")]`
/// for clean JSON representation in the delta_queue table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum FieldDelta {
    /// Content field updated (LWW).
    ContentUpdated {
        value: String,
        lww_timestamp: DateTime<Utc>,
        agent_id: String,
    },
    /// Summary field updated (LWW).
    SummaryUpdated {
        value: String,
        lww_timestamp: DateTime<Utc>,
        agent_id: String,
    },
    /// Confidence boosted (MaxRegister).
    ConfidenceBoosted {
        value: f64,
        max_timestamp: DateTime<Utc>,
    },
    /// Tag added (ORSet).
    TagAdded {
        tag: String,
        unique_tag: UniqueTag,
    },
    /// Tag removed (ORSet).
    TagRemoved {
        tag: String,
        removed_tags: HashSet<UniqueTag>,
    },
    /// Link added (ORSet) — link_type is one of "pattern", "constraint", "file", "function".
    LinkAdded {
        link_type: String,
        target: String,
        unique_tag: UniqueTag,
    },
    /// Link removed (ORSet).
    LinkRemoved {
        link_type: String,
        target: String,
        removed_tags: HashSet<UniqueTag>,
    },
    /// Access count incremented (GCounter).
    AccessCountIncremented {
        agent: String,
        new_count: u64,
    },
    /// Importance changed (LWW).
    ImportanceChanged {
        value: String,
        lww_timestamp: DateTime<Utc>,
        agent_id: String,
    },
    /// Archived flag changed (LWW).
    ArchivedChanged {
        value: bool,
        lww_timestamp: DateTime<Utc>,
        agent_id: String,
    },
    /// Provenance hop added (append-only).
    ProvenanceHopAdded {
        hop: ProvenanceHop,
    },
    /// Full memory state for initial creation.
    MemoryCreated {
        full_state: serde_json::Value,
    },
    /// Namespace changed (LWW).
    NamespaceChanged {
        namespace: String,
        lww_timestamp: DateTime<Utc>,
        agent_id: String,
    },
}
