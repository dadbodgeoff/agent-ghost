//! Stateless merge orchestrator with causal ordering validation.
//!
//! Coordinates merging two `MemoryCRDT` instances and computing/applying
//! deltas for efficient inter-agent sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::field_delta::FieldDelta;
use super::memory_crdt::MemoryCRDT;
use crate::clock::VectorClock;
use cortex_core::errors::{CortexError, CortexResult};

/// A set of field deltas representing changes to a single memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDelta {
    /// The memory this delta applies to.
    pub memory_id: String,
    /// The agent that produced this delta.
    pub source_agent: String,
    /// The vector clock at the time of delta creation.
    pub clock: VectorClock,
    /// Individual field changes.
    pub field_deltas: Vec<FieldDelta>,
    /// When this delta was created.
    pub timestamp: DateTime<Utc>,
}

/// Stateless merge orchestrator for `MemoryCRDT` instances.
///
/// Provides three operations:
/// - `merge_memories`: full merge of two CRDT states
/// - `compute_delta`: compute what the remote is missing
/// - `apply_delta`: apply a delta with causal ordering validation
pub struct MergeEngine;

impl MergeEngine {
    /// Merge two `MemoryCRDT` instances, returning the merged state.
    ///
    /// Per-field merge using each field's CRDT merge semantics.
    pub fn merge_memories(local: &MemoryCRDT, remote: &MemoryCRDT) -> MemoryCRDT {
        let mut merged = local.clone();
        merged.merge(remote);
        merged
    }

    /// Apply a set of field deltas to a local `MemoryCRDT`.
    ///
    /// Validates causal ordering before applying: the delta's clock must
    /// not be "from the future" relative to the local clock (i.e., the
    /// local clock must have seen all causal predecessors).
    pub fn apply_delta(local: &mut MemoryCRDT, delta: &MemoryDelta) -> CortexResult<()> {
        // Causal ordering validation: check that we have all predecessors.
        // The delta's clock (minus the source agent's entry) should be
        // dominated by or equal to our local clock.
        for agent in delta.clock.agents() {
            if agent == delta.source_agent.as_str() {
                continue; // Skip the source agent's own entry
            }
            let delta_val = delta.clock.get(agent);
            let local_val = local.clock.get(agent);
            if delta_val > local_val {
                return Err(CortexError::MultiAgentError(
                    cortex_core::errors::MultiAgentError::CausalOrderViolation {
                        expected: format!("{agent}:{local_val}"),
                        found: format!("{agent}:{delta_val}"),
                    },
                ));
            }
        }

        // Apply each field delta
        for field_delta in &delta.field_deltas {
            match field_delta {
                FieldDelta::ContentUpdated {
                    value,
                    lww_timestamp,
                    agent_id,
                } => {
                    local
                        .content
                        .set(value.clone(), *lww_timestamp, agent_id.clone());
                }
                FieldDelta::SummaryUpdated {
                    value,
                    lww_timestamp,
                    agent_id,
                } => {
                    local
                        .summary
                        .set(value.clone(), *lww_timestamp, agent_id.clone());
                }
                FieldDelta::ConfidenceBoosted {
                    value,
                    max_timestamp: _,
                } => {
                    local.base_confidence.set(*value);
                }
                FieldDelta::TagAdded { tag, unique_tag } => {
                    local
                        .tags
                        .add(tag.clone(), &unique_tag.agent_id, unique_tag.seq);
                }
                FieldDelta::TagRemoved { tag, .. } => {
                    local.tags.remove(tag);
                }
                FieldDelta::LinkAdded {
                    link_type,
                    target,
                    unique_tag,
                } => {
                    let set = match link_type.as_str() {
                        "pattern" => &mut local.linked_patterns,
                        "constraint" => &mut local.linked_constraints,
                        "file" => &mut local.linked_files,
                        "function" => &mut local.linked_functions,
                        _ => return Ok(()), // Unknown link type, skip
                    };
                    set.add(target.clone(), &unique_tag.agent_id, unique_tag.seq);
                }
                FieldDelta::LinkRemoved {
                    link_type, target, ..
                } => {
                    let set = match link_type.as_str() {
                        "pattern" => &mut local.linked_patterns,
                        "constraint" => &mut local.linked_constraints,
                        "file" => &mut local.linked_files,
                        "function" => &mut local.linked_functions,
                        _ => return Ok(()),
                    };
                    set.remove(target);
                }
                FieldDelta::AccessCountIncremented { agent, new_count: _ } => {
                    local.access_count.increment(agent);
                }
                FieldDelta::ImportanceChanged {
                    value,
                    lww_timestamp,
                    agent_id,
                } => {
                    local
                        .importance
                        .set(value.clone(), *lww_timestamp, agent_id.clone());
                }
                FieldDelta::ArchivedChanged {
                    value,
                    lww_timestamp,
                    agent_id,
                } => {
                    local
                        .archived
                        .set(*value, *lww_timestamp, agent_id.clone());
                }
                FieldDelta::ProvenanceHopAdded { hop } => {
                    let already_present = local.provenance.iter().any(|existing| {
                        existing.agent_id == hop.agent_id
                            && existing.timestamp == hop.timestamp
                            && existing.action == hop.action
                    });
                    if !already_present {
                        local.provenance.push(hop.clone());
                        local
                            .provenance
                            .sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                    }
                }
                FieldDelta::MemoryCreated { .. } => {
                    // Full state creation is handled separately
                }
                FieldDelta::NamespaceChanged {
                    namespace,
                    lww_timestamp,
                    agent_id,
                } => {
                    local
                        .namespace
                        .set(namespace.clone(), *lww_timestamp, agent_id.clone());
                }
            }
        }

        // Update local clock with delta's clock
        local.clock.merge(&delta.clock);

        Ok(())
    }

    /// Compute field deltas that the remote is missing based on clock comparison.
    ///
    /// Compares the local state against the remote's vector clock to determine
    /// which fields have been updated since the remote last synced.
    pub fn compute_delta(
        local: &MemoryCRDT,
        remote_clock: &VectorClock,
        agent_id: &str,
    ) -> MemoryDelta {
        let mut field_deltas = Vec::new();

        // For LWW fields, check if our timestamp is newer
        // Content
        if let Some(delta) = check_lww_newer(&local.content, remote_clock) {
            field_deltas.push(FieldDelta::ContentUpdated {
                value: delta.0,
                lww_timestamp: delta.1,
                agent_id: delta.2,
            });
        }
        // Summary
        if let Some(delta) = check_lww_newer(&local.summary, remote_clock) {
            field_deltas.push(FieldDelta::SummaryUpdated {
                value: delta.0,
                lww_timestamp: delta.1,
                agent_id: delta.2,
            });
        }
        // Importance
        if let Some(delta) = check_lww_newer(&local.importance, remote_clock) {
            field_deltas.push(FieldDelta::ImportanceChanged {
                value: delta.0,
                lww_timestamp: delta.1,
                agent_id: delta.2,
            });
        }
        // Namespace
        if let Some(delta) = check_lww_newer(&local.namespace, remote_clock) {
            field_deltas.push(FieldDelta::NamespaceChanged {
                namespace: delta.0,
                lww_timestamp: delta.1,
                agent_id: delta.2,
            });
        }

        MemoryDelta {
            memory_id: local.id.clone(),
            source_agent: agent_id.to_string(),
            clock: local.clock.clone(),
            field_deltas,
            timestamp: Utc::now(),
        }
    }
}

/// Helper: check if an LWW register has been updated since the remote clock.
/// Returns (value, timestamp, agent_id) if newer.
fn check_lww_newer<T: Clone>(
    register: &crate::primitives::LWWRegister<T>,
    _remote_clock: &VectorClock,
) -> Option<(T, DateTime<Utc>, String)> {
    // In a full implementation, we'd compare the register's agent_id entry
    // in the remote clock. For now, we include all fields in the delta
    // and let the LWW merge handle deduplication.
    Some((
        register.get().clone(),
        register.timestamp(),
        register.agent_id().to_string(),
    ))
}
