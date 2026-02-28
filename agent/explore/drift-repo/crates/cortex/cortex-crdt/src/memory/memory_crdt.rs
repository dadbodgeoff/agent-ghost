//! Per-field CRDT wrapper for `BaseMemory`.
//!
//! Every field of `BaseMemory` is wrapped in the appropriate CRDT type.
//! This is the central data structure of the multi-agent system.
//!
//! # Field-to-CRDT Mapping
//!
//! | BaseMemory Field    | CRDT Type      | Merge Semantics                    |
//! |---------------------|----------------|------------------------------------|
//! | `id`                | Immutable      | First-write wins (UUID)            |
//! | `memory_type`       | LWW-Register   | Last reclassification wins         |
//! | `content`           | LWW-Register   | Last edit wins                     |
//! | `summary`           | LWW-Register   | Last edit wins                     |
//! | `transaction_time`  | Immutable      | Set at creation                    |
//! | `valid_time`        | LWW-Register   | Can be corrected                   |
//! | `valid_until`       | LWW-Register   | Can be extended/shortened          |
//! | `confidence`        | MaxRegister    | Only explicit boosts propagate     |
//! | `importance`        | LWW-Register   | Last reclassification wins         |
//! | `last_accessed`     | MaxRegister    | Most recent access wins            |
//! | `access_count`      | GCounter       | Per-agent counters, sum for total  |
//! | `linked_*`          | ORSet          | Add wins over concurrent remove    |
//! | `tags`              | ORSet          | Add wins over concurrent remove    |
//! | `archived`          | LWW-Register   | Explicit archive/restore           |
//! | `superseded_by`     | LWW-Register   | Explicit supersession              |
//! | `supersedes`        | ORSet          | Can supersede multiple memories    |
//! | `namespace`         | LWW-Register   | Explicit promote/move              |
//! | `source_agent`      | Immutable      | Set at creation                    |
//! | `provenance`        | Append-only    | Union of all provenance hops       |
//! | `content_hash`      | Derived        | Recomputed from content            |

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::clock::VectorClock;
use crate::primitives::{GCounter, LWWRegister, MaxRegister, ORSet};
use cortex_core::memory::base::{BaseMemory, TypedContent};
use cortex_core::memory::confidence::Confidence;
use cortex_core::memory::importance::Importance;
use cortex_core::memory::links::{ConstraintLink, FileLink, FunctionLink, PatternLink};
use cortex_core::memory::types::MemoryType;
use cortex_core::models::agent::AgentId;
use cortex_core::models::namespace::NamespaceId;
use cortex_core::models::provenance::ProvenanceHop;

/// Per-field CRDT wrapper for a single `BaseMemory`.
///
/// Every mutable field is wrapped in the appropriate CRDT type. Immutable
/// fields (`id`, `transaction_time`, `source_agent`) are stored directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCRDT {
    // === Immutable fields (set once, never change) ===
    /// UUID v4 identifier.
    pub id: String,
    /// When we learned this (bitemporal: transaction time).
    pub transaction_time: DateTime<Utc>,
    /// Agent that created this memory.
    pub source_agent: AgentId,

    // === LWW-Register fields ===
    /// The type of this memory.
    pub memory_type: LWWRegister<String>,
    /// Typed content — serialized TypedContent.
    pub content: LWWRegister<String>,
    /// ~20 token summary.
    pub summary: LWWRegister<String>,
    /// When this was/is true (bitemporal: valid time).
    pub valid_time: LWWRegister<DateTime<Utc>>,
    /// Optional expiry.
    pub valid_until: LWWRegister<Option<DateTime<Utc>>>,
    /// Importance level — serialized Importance.
    pub importance: LWWRegister<String>,
    /// Whether this memory has been archived.
    pub archived: LWWRegister<bool>,
    /// ID of the memory that supersedes this one.
    pub superseded_by: LWWRegister<Option<String>>,
    /// Namespace — serialized NamespaceId.
    pub namespace: LWWRegister<String>,

    // === MaxRegister fields ===
    /// Base confidence (explicit boosts only; decay is local).
    pub base_confidence: MaxRegister<f64>,
    /// Last time this memory was accessed.
    pub last_accessed: MaxRegister<DateTime<Utc>>,

    // === GCounter fields ===
    /// Number of times accessed (per-agent counters).
    pub access_count: GCounter,

    // === ORSet fields ===
    /// Linked code patterns (serialized PatternLink).
    pub linked_patterns: ORSet<String>,
    /// Linked constraints (serialized ConstraintLink).
    pub linked_constraints: ORSet<String>,
    /// Linked files (serialized FileLink).
    pub linked_files: ORSet<String>,
    /// Linked functions (serialized FunctionLink).
    pub linked_functions: ORSet<String>,
    /// Free-form tags.
    pub tags: ORSet<String>,
    /// IDs of memories this one supersedes.
    pub supersedes: ORSet<String>,

    // === Append-only ===
    /// Provenance chain (union of all hops).
    pub provenance: Vec<ProvenanceHop>,

    // === Causal context ===
    /// Vector clock for causal ordering.
    pub clock: VectorClock,
}

impl MemoryCRDT {
    /// Wrap an existing `BaseMemory` in CRDT wrappers.
    ///
    /// All LWW fields are initialized with the given timestamp and agent_id.
    /// ORSet fields are populated with unique tags from the agent.
    pub fn from_base_memory(memory: &BaseMemory, agent_id: &str) -> Self {
        let now = Utc::now();
        let aid = agent_id.to_string();

        // Serialize complex types to JSON strings for CRDT storage
        let content_json =
            serde_json::to_string(&memory.content).unwrap_or_default();
        let memory_type_json =
            serde_json::to_string(&memory.memory_type).unwrap_or_default();
        let importance_json =
            serde_json::to_string(&memory.importance).unwrap_or_default();
        let namespace_json =
            serde_json::to_string(&memory.namespace).unwrap_or_default();

        // Build ORSets with unique tags
        let mut linked_patterns = ORSet::new();
        for (seq, link) in memory.linked_patterns.iter().enumerate() {
            let json = serde_json::to_string(link).unwrap_or_default();
            linked_patterns.add(json, agent_id, seq as u64);
        }

        let mut linked_constraints = ORSet::new();
        for (seq, link) in memory.linked_constraints.iter().enumerate() {
            let json = serde_json::to_string(link).unwrap_or_default();
            linked_constraints.add(json, agent_id, seq as u64);
        }

        let mut linked_files = ORSet::new();
        for (seq, link) in memory.linked_files.iter().enumerate() {
            let json = serde_json::to_string(link).unwrap_or_default();
            linked_files.add(json, agent_id, seq as u64);
        }

        let mut linked_functions = ORSet::new();
        for (seq, link) in memory.linked_functions.iter().enumerate() {
            let json = serde_json::to_string(link).unwrap_or_default();
            linked_functions.add(json, agent_id, seq as u64);
        }

        let mut tags = ORSet::new();
        for (seq, tag) in memory.tags.iter().enumerate() {
            tags.add(tag.clone(), agent_id, seq as u64);
        }

        let mut supersedes = ORSet::new();
        if let Some(ref s) = memory.supersedes {
            supersedes.add(s.clone(), agent_id, 0);
        }

        let mut clock = VectorClock::new();
        clock.increment(agent_id);

        Self {
            id: memory.id.clone(),
            transaction_time: memory.transaction_time,
            source_agent: memory.source_agent.clone(),
            memory_type: LWWRegister::new(memory_type_json, now, aid.clone()),
            content: LWWRegister::new(content_json, now, aid.clone()),
            summary: LWWRegister::new(memory.summary.clone(), now, aid.clone()),
            valid_time: LWWRegister::new(memory.valid_time, now, aid.clone()),
            valid_until: LWWRegister::new(memory.valid_until, now, aid.clone()),
            importance: LWWRegister::new(importance_json, now, aid.clone()),
            archived: LWWRegister::new(memory.archived, now, aid.clone()),
            superseded_by: LWWRegister::new(memory.superseded_by.clone(), now, aid.clone()),
            namespace: LWWRegister::new(namespace_json, now, aid.clone()),
            base_confidence: MaxRegister::new(memory.confidence.value(), now),
            last_accessed: MaxRegister::new(memory.last_accessed, now),
            access_count: {
                let mut gc = GCounter::new();
                // Initialize with 1 for the creating agent. The base access_count
                // is a snapshot, not per-agent attribution. If we attributed the
                // full base count to each agent, merging N agents' CRDTs would
                // inflate the total to N × base_count.
                gc.increment(agent_id);
                gc
            },
            linked_patterns,
            linked_constraints,
            linked_files,
            linked_functions,
            tags,
            supersedes,
            provenance: Vec::new(),
            clock,
        }
    }

    /// Materialize current CRDT state into a `BaseMemory`.
    ///
    /// Deserializes JSON-encoded fields back into their typed representations.
    /// Recomputes `content_hash` from the materialized content.
    pub fn to_base_memory(&self) -> BaseMemory {
        let memory_type: MemoryType =
            serde_json::from_str(self.memory_type.get()).unwrap_or(MemoryType::Core);
        let content: TypedContent = serde_json::from_str(self.content.get())
            .unwrap_or_else(|_| {
                TypedContent::Core(cortex_core::memory::types::CoreContent {
                    project_name: String::new(),
                    description: self.content.get().clone(),
                    metadata: serde_json::Value::Null,
                })
            });
        let importance: Importance =
            serde_json::from_str(self.importance.get()).unwrap_or_default();
        let namespace: NamespaceId =
            serde_json::from_str(self.namespace.get()).unwrap_or_default();

        let linked_patterns: Vec<PatternLink> = self
            .linked_patterns
            .elements()
            .iter()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect();
        let linked_constraints: Vec<ConstraintLink> = self
            .linked_constraints
            .elements()
            .iter()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect();
        let linked_files: Vec<FileLink> = self
            .linked_files
            .elements()
            .iter()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect();
        let linked_functions: Vec<FunctionLink> = self
            .linked_functions
            .elements()
            .iter()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect();

        let tags: Vec<String> = self
            .tags
            .elements()
            .into_iter()
            .cloned()
            .collect();

        // Supersedes: collect all present elements
        let supersedes: Option<String> = {
            let elems = self.supersedes.elements();
            elems.into_iter().next().cloned()
        };

        let content_hash = BaseMemory::compute_content_hash(&content)
            .unwrap_or_else(|_| "hash-error".to_string());

        BaseMemory {
            id: self.id.clone(),
            memory_type,
            content,
            summary: self.summary.get().clone(),
            transaction_time: self.transaction_time,
            valid_time: *self.valid_time.get(),
            valid_until: *self.valid_until.get(),
            confidence: Confidence::new(*self.base_confidence.get()),
            importance,
            last_accessed: *self.last_accessed.get(),
            access_count: self.access_count.value(),
            linked_patterns,
            linked_constraints,
            linked_files,
            linked_functions,
            tags,
            archived: *self.archived.get(),
            superseded_by: self.superseded_by.get().clone(),
            supersedes,
            content_hash,
            namespace,
            source_agent: self.source_agent.clone(),
        }
    }

    /// Per-field merge using each field's CRDT merge semantics.
    ///
    /// After merge, both replicas converge to the same state regardless
    /// of merge order (commutativity).
    pub fn merge(&mut self, other: &Self) {
        // Immutable fields: id, transaction_time, source_agent — no merge needed.
        // They must be identical (same memory).

        // LWW fields
        self.memory_type.merge(&other.memory_type);
        self.content.merge(&other.content);
        self.summary.merge(&other.summary);
        self.valid_time.merge(&other.valid_time);
        self.valid_until.merge(&other.valid_until);
        self.importance.merge(&other.importance);
        self.archived.merge(&other.archived);
        self.superseded_by.merge(&other.superseded_by);
        self.namespace.merge(&other.namespace);

        // MaxRegister fields
        self.base_confidence.merge(&other.base_confidence);
        self.last_accessed.merge(&other.last_accessed);

        // GCounter fields
        self.access_count.merge(&other.access_count);

        // ORSet fields
        self.linked_patterns.merge(&other.linked_patterns);
        self.linked_constraints.merge(&other.linked_constraints);
        self.linked_files.merge(&other.linked_files);
        self.linked_functions.merge(&other.linked_functions);
        self.tags.merge(&other.tags);
        self.supersedes.merge(&other.supersedes);

        // Append-only provenance: union of hops (dedup by timestamp + agent_id)
        for hop in &other.provenance {
            let already_present = self.provenance.iter().any(|existing| {
                existing.agent_id == hop.agent_id
                    && existing.timestamp == hop.timestamp
                    && existing.action == hop.action
            });
            if !already_present {
                self.provenance.push(hop.clone());
            }
        }
        // Sort provenance by timestamp for consistent ordering
        self.provenance
            .sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        // Vector clock: component-wise max
        self.clock.merge(&other.clock);
    }

    /// Recompute the blake3 content hash from the current content field.
    pub fn content_hash(&self) -> String {
        // Deserialize content and use BaseMemory's hash function
        let content: TypedContent = serde_json::from_str(self.content.get())
            .unwrap_or_else(|_| {
                TypedContent::Core(cortex_core::memory::types::CoreContent {
                    project_name: String::new(),
                    description: self.content.get().clone(),
                    metadata: serde_json::Value::Null,
                })
            });
        BaseMemory::compute_content_hash(&content)
            .unwrap_or_else(|_| "hash-error".to_string())
    }
}
