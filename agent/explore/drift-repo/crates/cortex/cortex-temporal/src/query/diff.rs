//! Temporal diff execution with event-range optimization.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::models::{
    ConfidenceShift, DiffScope, DiffStats, MemoryModification, Reclassification, TemporalDiff,
    TemporalDiffQuery,
};
use cortex_storage::pool::ReadPool;
use cortex_storage::queries::temporal_ops;

use crate::snapshot;

/// Filtered diff components returned by `apply_scope_filter`.
type FilteredDiff = (
    Vec<BaseMemory>,
    Vec<BaseMemory>,
    Vec<MemoryModification>,
    Vec<ConfidenceShift>,
    Vec<Reclassification>,
);

/// Execute a temporal diff query using full state reconstruction.
///
/// Uses `reconstruct_all_at` to get the true historical state at each time point,
/// ensuring that confidence values and other fields reflect what they were at that
/// time — not their current DB values.
///
/// Critical invariants:
/// - diff(T, T) must return empty diff
/// - diff(A, B).created == diff(B, A).archived (symmetry)
pub fn execute_diff_reconstructed(
    readers: &Arc<ReadPool>,
    query: &TemporalDiffQuery,
) -> CortexResult<TemporalDiff> {
    // Identity check: diff(T, T) = empty
    if query.time_a == query.time_b {
        return Ok(empty_diff());
    }

    // Determine if the query is reversed (time_a > time_b)
    let reversed = query.time_a > query.time_b;

    // Ensure earlier < later for internal computation
    let (earlier, later) = if reversed {
        (query.time_b, query.time_a)
    } else {
        (query.time_a, query.time_b)
    };

    // Get memories modified between the two times (event-range optimization)
    let modified_ids = readers.with_conn(|conn| {
        temporal_ops::get_memories_modified_between(conn, earlier, later)
    })?;

    // Reconstruct true historical state at both time points
    let state_a = snapshot::reconstruct::reconstruct_all_at(readers, earlier)?;
    let state_b = snapshot::reconstruct::reconstruct_all_at(readers, later)?;

    // Build maps for efficient lookup
    let map_a: HashMap<String, BaseMemory> =
        state_a.into_iter().map(|m| (m.id.clone(), m)).collect();
    let map_b: HashMap<String, BaseMemory> =
        state_b.into_iter().map(|m| (m.id.clone(), m)).collect();

    // Classify changes (always computed as earlier→later internally)
    let mut created = Vec::new();
    let mut archived = Vec::new();
    let mut modified = Vec::new();
    let mut confidence_shifts = Vec::new();
    let mut reclassifications = Vec::new();

    // Find created memories (in later but not in earlier)
    for (id, memory_b) in &map_b {
        if !map_a.contains_key(id) {
            created.push(memory_b.clone());
        }
    }

    // Find archived memories (in earlier but not in later)
    for (id, memory_a) in &map_a {
        if !map_b.contains_key(id) {
            archived.push(memory_a.clone());
        }
    }

    // Find modified memories (in both, but changed)
    let modified_set: HashSet<String> = modified_ids.into_iter().collect();
    for id in modified_set {
        if let (Some(memory_a), Some(memory_b)) = (map_a.get(&id), map_b.get(&id)) {
            // Check for modifications by comparing reconstructed states
            let mods = detect_modifications(memory_a, memory_b, later);
            modified.extend(mods);

            // Check for confidence shifts (delta > 0.2)
            let delta = memory_b.confidence.value() - memory_a.confidence.value();
            if delta.abs() > 0.2 {
                confidence_shifts.push(ConfidenceShift {
                    memory_id: id.clone(),
                    old_confidence: memory_a.confidence.value(),
                    new_confidence: memory_b.confidence.value(),
                    delta,
                });
            }

            // Also check for confidence shifts from events between the two times.
            if delta.abs() <= 0.2 {
                if let Ok(shifts) = readers.with_conn(|conn| {
                    extract_confidence_shifts_from_events(conn, &id, earlier, later)
                }) {
                    confidence_shifts.extend(shifts);
                }
            }

            // Check for reclassifications
            if memory_a.memory_type != memory_b.memory_type {
                reclassifications.push(Reclassification {
                    memory_id: id.clone(),
                    old_type: format!("{:?}", memory_a.memory_type),
                    new_type: format!("{:?}", memory_b.memory_type),
                    confidence: memory_b.confidence.value(),
                    reclassified_at: later,
                });
            }
        }
    }

    // If the query was reversed (time_a > time_b), swap created/archived
    // to maintain the invariant: diff(A,B).created == diff(B,A).archived
    if reversed {
        std::mem::swap(&mut created, &mut archived);
    }

    // Apply scope filter (DiffScope::All, Types, Files, Namespace)
    let (created, archived, modified, confidence_shifts, reclassifications) = apply_scope_filter(
        &query.scope,
        created,
        archived,
        modified,
        confidence_shifts,
        reclassifications,
    );

    // Compute stats — use the maps oriented to the query direction
    let stats = if reversed {
        compute_stats(&map_b, &map_a, &created, &archived)
    } else {
        compute_stats(&map_a, &map_b, &created, &archived)
    };

    Ok(TemporalDiff {
        created,
        archived,
        modified,
        confidence_shifts,
        new_contradictions: Vec::new(),
        resolved_contradictions: Vec::new(),
        reclassifications,
        stats,
    })
}

/// Execute a temporal diff query using direct DB queries (connection-level).
///
/// This is the backward-compatible version that works with a raw `&Connection`.
/// It queries the current DB state, which may not reflect true historical values.
/// Prefer `execute_diff_reconstructed` when a `ReadPool` is available.
///
/// Critical invariants:
/// - diff(T, T) must return empty diff
/// - diff(A, B).created == diff(B, A).archived (symmetry)
pub fn execute_diff(conn: &Connection, query: &TemporalDiffQuery) -> CortexResult<TemporalDiff> {
    // Identity check: diff(T, T) = empty
    if query.time_a == query.time_b {
        return Ok(empty_diff());
    }

    // Determine if the query is reversed (time_a > time_b)
    let reversed = query.time_a > query.time_b;

    // Ensure earlier < later for internal computation
    let (earlier, later) = if reversed {
        (query.time_b, query.time_a)
    } else {
        (query.time_a, query.time_b)
    };

    // Get memories modified between the two times (event-range optimization)
    let modified_ids = temporal_ops::get_memories_modified_between(conn, earlier, later)?;

    // Use direct DB queries for state at both times
    let state_a = temporal_ops::get_memories_valid_at(conn, earlier, earlier)?;
    let state_b = temporal_ops::get_memories_valid_at(conn, later, later)?;

    // Build maps for efficient lookup
    let map_a: HashMap<String, BaseMemory> = state_a.into_iter().map(|m| (m.id.clone(), m)).collect();
    let map_b: HashMap<String, BaseMemory> = state_b.into_iter().map(|m| (m.id.clone(), m)).collect();

    // Classify changes (always computed as earlier→later internally)
    let mut created = Vec::new();
    let mut archived = Vec::new();
    let mut modified = Vec::new();
    let mut confidence_shifts = Vec::new();
    let mut reclassifications = Vec::new();

    // Find created memories (in later but not in earlier)
    for (id, memory_b) in &map_b {
        if !map_a.contains_key(id) {
            created.push(memory_b.clone());
        }
    }

    // Find archived memories (in earlier but not in later)
    for (id, memory_a) in &map_a {
        if !map_b.contains_key(id) {
            archived.push(memory_a.clone());
        }
    }

    // Find modified memories (in both, but changed)
    let modified_set: HashSet<String> = modified_ids.into_iter().collect();
    for id in modified_set {
        if let (Some(memory_a), Some(memory_b)) = (map_a.get(&id), map_b.get(&id)) {
            let mods = detect_modifications(memory_a, memory_b, later);
            modified.extend(mods);

            let delta = memory_b.confidence.value() - memory_a.confidence.value();
            if delta.abs() > 0.2 {
                confidence_shifts.push(ConfidenceShift {
                    memory_id: id.clone(),
                    old_confidence: memory_a.confidence.value(),
                    new_confidence: memory_b.confidence.value(),
                    delta,
                });
            }

            if delta.abs() <= 0.2 {
                if let Ok(shifts) = extract_confidence_shifts_from_events(conn, &id, earlier, later) {
                    confidence_shifts.extend(shifts);
                }
            }

            if memory_a.memory_type != memory_b.memory_type {
                reclassifications.push(Reclassification {
                    memory_id: id.clone(),
                    old_type: format!("{:?}", memory_a.memory_type),
                    new_type: format!("{:?}", memory_b.memory_type),
                    confidence: memory_b.confidence.value(),
                    reclassified_at: later,
                });
            }
        }
    }

    if reversed {
        std::mem::swap(&mut created, &mut archived);
    }

    let (created, archived, modified, confidence_shifts, reclassifications) =
        apply_scope_filter(
            &query.scope,
            created,
            archived,
            modified,
            confidence_shifts,
            reclassifications,
        );

    let stats = if reversed {
        compute_stats(&map_b, &map_a, &created, &archived)
    } else {
        compute_stats(&map_a, &map_b, &created, &archived)
    };

    Ok(TemporalDiff {
        created,
        archived,
        modified,
        confidence_shifts,
        new_contradictions: Vec::new(),
        resolved_contradictions: Vec::new(),
        reclassifications,
        stats,
    })
}

/// Extract confidence shifts from events between two times for a specific memory.
fn extract_confidence_shifts_from_events(
    conn: &Connection,
    memory_id: &str,
    earlier: DateTime<Utc>,
    later: DateTime<Utc>,
) -> CortexResult<Vec<ConfidenceShift>> {
    let earlier_str = earlier.to_rfc3339();
    let later_str = later.to_rfc3339();

    let mut stmt = conn
        .prepare(
            "SELECT delta FROM memory_events \
             WHERE memory_id = ?1 AND recorded_at > ?2 AND recorded_at <= ?3 \
             AND event_type = 'confidence_changed' \
             ORDER BY recorded_at ASC",
        )
        .map_err(|e| {
            cortex_core::CortexError::TemporalError(
                cortex_core::errors::TemporalError::EventAppendFailed(e.to_string()),
            )
        })?;

    let rows = stmt
        .query_map(
            rusqlite::params![memory_id, earlier_str, later_str],
            |row| row.get::<_, String>(0),
        )
        .map_err(|e| {
            cortex_core::CortexError::TemporalError(
                cortex_core::errors::TemporalError::EventAppendFailed(e.to_string()),
            )
        })?;

    let mut shifts = Vec::new();
    for delta_str in rows.flatten() {
        if let Ok(delta) = serde_json::from_str::<serde_json::Value>(&delta_str) {
            let old = delta.get("old").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let new = delta.get("new").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let d = new - old;
            shifts.push(ConfidenceShift {
                memory_id: memory_id.to_string(),
                old_confidence: old,
                new_confidence: new,
                delta: d,
            });
        }
    }

    Ok(shifts)
}

fn empty_diff() -> TemporalDiff {
    TemporalDiff {
        created: Vec::new(),
        archived: Vec::new(),
        modified: Vec::new(),
        confidence_shifts: Vec::new(),
        new_contradictions: Vec::new(),
        resolved_contradictions: Vec::new(),
        reclassifications: Vec::new(),
        stats: DiffStats {
            memories_at_a: 0,
            memories_at_b: 0,
            net_change: 0,
            avg_confidence_at_a: 0.0,
            avg_confidence_at_b: 0.0,
            confidence_trend: 0.0,
            knowledge_churn_rate: 0.0,
        },
    }
}

fn detect_modifications(
    memory_a: &BaseMemory,
    memory_b: &BaseMemory,
    modified_at: DateTime<Utc>,
) -> Vec<MemoryModification> {
    let mut mods = Vec::new();

    if memory_a.content != memory_b.content {
        mods.push(MemoryModification {
            memory_id: memory_a.id.clone(),
            field: "content".to_string(),
            old_value: serde_json::json!(memory_a.content),
            new_value: serde_json::json!(memory_b.content),
            modified_at,
        });
    }

    if memory_a.summary != memory_b.summary {
        mods.push(MemoryModification {
            memory_id: memory_a.id.clone(),
            field: "summary".to_string(),
            old_value: serde_json::json!(memory_a.summary),
            new_value: serde_json::json!(memory_b.summary),
            modified_at,
        });
    }

    if memory_a.importance != memory_b.importance {
        mods.push(MemoryModification {
            memory_id: memory_a.id.clone(),
            field: "importance".to_string(),
            old_value: serde_json::json!(format!("{:?}", memory_a.importance)),
            new_value: serde_json::json!(format!("{:?}", memory_b.importance)),
            modified_at,
        });
    }

    if memory_a.tags != memory_b.tags {
        mods.push(MemoryModification {
            memory_id: memory_a.id.clone(),
            field: "tags".to_string(),
            old_value: serde_json::json!(memory_a.tags),
            new_value: serde_json::json!(memory_b.tags),
            modified_at,
        });
    }

    mods
}

fn compute_stats(
    map_a: &HashMap<String, BaseMemory>,
    map_b: &HashMap<String, BaseMemory>,
    created: &[BaseMemory],
    archived: &[BaseMemory],
) -> DiffStats {
    let memories_at_a = map_a.len();
    let memories_at_b = map_b.len();
    let net_change = memories_at_b as i64 - memories_at_a as i64;

    let avg_confidence_at_a = if memories_at_a > 0 {
        map_a.values().map(|m| m.confidence.value()).sum::<f64>() / memories_at_a as f64
    } else {
        0.0
    };

    let avg_confidence_at_b = if memories_at_b > 0 {
        map_b.values().map(|m| m.confidence.value()).sum::<f64>() / memories_at_b as f64
    } else {
        0.0
    };

    let confidence_trend = avg_confidence_at_b - avg_confidence_at_a;

    let knowledge_churn_rate = if memories_at_a > 0 {
        (created.len() + archived.len()) as f64 / memories_at_a as f64
    } else {
        0.0
    };

    DiffStats {
        memories_at_a,
        memories_at_b,
        net_change,
        avg_confidence_at_a,
        avg_confidence_at_b,
        confidence_trend,
        knowledge_churn_rate,
    }
}

/// Apply DiffScope filter to restrict diff results.
fn apply_scope_filter(
    scope: &DiffScope,
    created: Vec<BaseMemory>,
    archived: Vec<BaseMemory>,
    modified: Vec<MemoryModification>,
    confidence_shifts: Vec<ConfidenceShift>,
    reclassifications: Vec<Reclassification>,
) -> FilteredDiff {
    match scope {
        DiffScope::All => (
            created,
            archived,
            modified,
            confidence_shifts,
            reclassifications,
        ),
        DiffScope::Types(types) => {
            let created = created
                .into_iter()
                .filter(|m| types.contains(&m.memory_type))
                .collect();
            let archived = archived
                .into_iter()
                .filter(|m| types.contains(&m.memory_type))
                .collect();
            (created, archived, modified, confidence_shifts, reclassifications)
        }
        DiffScope::Files(files) => {
            let has_file = |m: &BaseMemory| {
                m.linked_files
                    .iter()
                    .any(|link| files.contains(&link.file_path))
            };
            let created = created.into_iter().filter(|m| has_file(m)).collect();
            let archived = archived.into_iter().filter(|m| has_file(m)).collect();
            (
                created,
                archived,
                modified,
                confidence_shifts,
                reclassifications,
            )
        }
        DiffScope::Namespace(ns) => {
            // A-03: Filter by namespace — only include memories belonging to this namespace.
            // DiffScope::Namespace holds a string; compare against namespace name or URI.
            let in_namespace = |m: &BaseMemory| m.namespace.name == *ns || m.namespace.to_uri() == *ns;
            let created: Vec<BaseMemory> = created.into_iter().filter(|m| in_namespace(m)).collect();
            let archived: Vec<BaseMemory> = archived.into_iter().filter(|m| in_namespace(m)).collect();
            // Build a set of memory IDs that belong to this namespace for filtering
            // modifications, confidence shifts, and reclassifications.
            let mut ns_memory_ids: HashSet<String> = HashSet::new();
            for m in &created {
                ns_memory_ids.insert(m.id.clone());
            }
            for m in &archived {
                ns_memory_ids.insert(m.id.clone());
            }
            let modified = modified
                .into_iter()
                .filter(|m| ns_memory_ids.contains(&m.memory_id))
                .collect();
            let confidence_shifts = confidence_shifts
                .into_iter()
                .filter(|s| ns_memory_ids.contains(&s.memory_id))
                .collect();
            let reclassifications = reclassifications
                .into_iter()
                .filter(|r| ns_memory_ids.contains(&r.memory_id))
                .collect();
            (created, archived, modified, confidence_shifts, reclassifications)
        }
    }
}
