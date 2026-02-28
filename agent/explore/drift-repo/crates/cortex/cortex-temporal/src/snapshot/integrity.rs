//! Snapshot integrity verification using blake3 state hashes.
//!
//! Computes a deterministic hash of a memory state for tamper detection.
//! Uses blake3 over canonical JSON (sorted keys) for determinism.

use cortex_core::memory::BaseMemory;
use serde::Serialize;

/// Compute a deterministic blake3 hash of a memory state.
///
/// Uses a canonical representation with sorted fields and sorted
/// collection items to ensure deterministic serialization.
pub fn compute_state_hash(memory: &BaseMemory) -> [u8; 32] {
    let canonical = serde_json::to_string(&CanonicalMemory::from(memory))
        .expect("Memory serialization should never fail");
    *blake3::hash(canonical.as_bytes()).as_bytes()
}

/// Verify a snapshot's integrity by comparing stored hash to recomputed hash.
///
/// Pre-v016 snapshots have no hash and pass by default.
pub fn verify_snapshot_integrity(
    stored_state_hash: Option<&[u8]>,
    reconstructed_state: &BaseMemory,
) -> bool {
    match stored_state_hash {
        Some(stored_hash) => {
            let recomputed = compute_state_hash(reconstructed_state);
            stored_hash == recomputed.as_slice()
        }
        None => true, // Pre-v016 snapshots have no hash — pass by default
    }
}

/// Wrapper that sorts all fields for deterministic serialization.
/// Fields in alphabetical order for deterministic JSON output.
#[derive(Serialize)]
struct CanonicalMemory {
    access_count: u64,
    archived: bool,
    confidence: f64,
    content_hash: String,
    id: String,
    importance: String,
    last_accessed: String,
    linked_constraints: Vec<String>,
    linked_files: Vec<String>,
    linked_functions: Vec<String>,
    linked_patterns: Vec<String>,
    memory_type: String,
    namespace: String,
    source_agent: String,
    summary: String,
    superseded_by: Option<String>,
    supersedes: Option<String>,
    tags: Vec<String>,
    transaction_time: String,
    valid_time: String,
    valid_until: Option<String>,
}

impl From<&BaseMemory> for CanonicalMemory {
    fn from(m: &BaseMemory) -> Self {
        let mut tags = m.tags.clone();
        tags.sort();

        let mut linked_patterns: Vec<String> = m
            .linked_patterns
            .iter()
            .map(|l| l.pattern_id.clone())
            .collect();
        linked_patterns.sort();

        let mut linked_constraints: Vec<String> = m
            .linked_constraints
            .iter()
            .map(|l| l.constraint_id.clone())
            .collect();
        linked_constraints.sort();

        let mut linked_files: Vec<String> =
            m.linked_files.iter().map(|l| l.file_path.clone()).collect();
        linked_files.sort();

        let mut linked_functions: Vec<String> = m
            .linked_functions
            .iter()
            .map(|l| l.function_name.clone())
            .collect();
        linked_functions.sort();

        CanonicalMemory {
            access_count: m.access_count,
            archived: m.archived,
            confidence: m.confidence.value(),
            content_hash: m.content_hash.clone(),
            id: m.id.clone(),
            importance: format!("{:?}", m.importance),
            last_accessed: m.last_accessed.to_rfc3339(),
            linked_constraints,
            linked_files,
            linked_functions,
            linked_patterns,
            memory_type: format!("{:?}", m.memory_type),
            namespace: m.namespace.to_uri(),
            source_agent: m.source_agent.0.clone(),
            summary: m.summary.clone(),
            superseded_by: m.superseded_by.clone(),
            supersedes: m.supersedes.clone(),
            tags,
            transaction_time: m.transaction_time.to_rfc3339(),
            valid_time: m.valid_time.to_rfc3339(),
            valid_until: m.valid_until.map(|t| t.to_rfc3339()),
        }
    }
}
