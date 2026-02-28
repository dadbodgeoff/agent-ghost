//! AS OF query execution — point-in-time knowledge reconstruction.

use std::sync::Arc;

use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::models::AsOfQuery;
use cortex_storage::pool::ReadPool;
use cortex_storage::queries::temporal_ops;
use rusqlite::Connection;

use super::integrity::enforce_temporal_integrity;

/// Warning threshold for AS OF query result size.
const MAX_RESULT_MEMORIES: usize = 10_000;

/// Execute an AS OF query to reconstruct knowledge state at a specific point in time.
///
/// Uses bitemporal semantics:
/// - `system_time`: what was recorded by this time (transaction time)
/// - `valid_time`: what was true at this time (valid time)
///
/// Critical invariant: `query_as_of(now())` must equal current state.
///
/// For past-time queries, memories that were modified between the query time and now
/// are reconstructed from events to recover their historical state.
pub fn execute_as_of(conn: &Connection, query: &AsOfQuery) -> CortexResult<Vec<BaseMemory>> {
    // Get memories that were valid at the query time via direct SQL
    let memories = temporal_ops::get_memories_valid_at(conn, query.valid_time, query.system_time)?;

    // For memories that were modified after the query time, we need to check
    // if their current state differs from their state at query time.
    // We do this by checking for events after the query time — if a memory
    // has events after system_time, its current DB row may not reflect the
    // historical state. In that case, we reconstruct from events.
    let memories = reconstruct_modified_memories(conn, memories, query.system_time)?;

    // Apply temporal integrity filter
    let memories = enforce_temporal_integrity(memories, query.valid_time)?;

    // Apply optional filter
    let memories = if let Some(filter) = &query.filter {
        apply_filter(memories, filter)
    } else {
        memories
    };

    if memories.len() > MAX_RESULT_MEMORIES {
        tracing::warn!(
            result_count = memories.len(),
            limit = MAX_RESULT_MEMORIES,
            system_time = %query.system_time,
            valid_time = %query.valid_time,
            "AS OF query returned {} memories, exceeding {} threshold. \
             Consider adding a filter or narrowing the time window.",
            memories.len(),
            MAX_RESULT_MEMORIES,
        );
    }

    Ok(memories)
}

/// Execute an AS OF query using a ReadPool (for engine-level calls that need reconstruction).
///
/// This variant uses the ReadPool to access the event store for full reconstruction
/// of memories that were modified after the query time.
pub fn execute_as_of_with_pool(
    readers: &Arc<ReadPool>,
    query: &AsOfQuery,
) -> CortexResult<Vec<BaseMemory>> {
    readers.with_conn(|conn| execute_as_of(conn, query))
}

/// For memories that have events after the query's system_time, reconstruct
/// their state at that time by replaying events up to system_time.
///
/// This handles the case where a memory existed at the query time but has
/// since been modified — the current DB row reflects the latest state, not
/// the historical state.
///
/// Key subtlety: `memory_crud::insert_memory` emits a "created" event with
/// `recorded_at = Utc::now()` (wall clock), NOT the memory's `transaction_time`.
/// So a memory inserted with `transaction_time` in the past will always have a
/// "created" event whose `recorded_at` is later than `transaction_time`.
/// This does NOT mean the memory was modified — the DB row already reflects
/// the correct historical state. We only need to reconstruct when there are
/// actual modification events (not just "created") after `system_time`.
fn reconstruct_modified_memories(
    conn: &Connection,
    memories: Vec<BaseMemory>,
    system_time: chrono::DateTime<chrono::Utc>,
) -> CortexResult<Vec<BaseMemory>> {
    let system_time_str = system_time.to_rfc3339();

    let mut result = Vec::with_capacity(memories.len());

    for memory in memories {
        // Check if this memory has non-"created" events after system_time.
        // The "created" event is always emitted at wall-clock time by insert_memory,
        // so its recorded_at may be after system_time even though the memory's
        // transaction_time is before system_time. That's not a real modification.
        let has_modification_events: bool = conn
            .prepare(
                "SELECT EXISTS(SELECT 1 FROM memory_events \
                 WHERE memory_id = ?1 AND recorded_at > ?2 AND event_type != 'created')",
            )
            .and_then(|mut stmt| {
                stmt.query_row(rusqlite::params![memory.id, system_time_str], |row| {
                    row.get(0)
                })
            })
            .unwrap_or(false);

        if has_modification_events {
            // Real modifications exist after system_time — reconstruct from events
            if let Some(reconstructed) = reconstruct_memory_at(conn, &memory.id, system_time)? {
                result.push(reconstructed);
            }
            // If reconstruction returns None, the memory didn't exist at that time
            // (shouldn't happen since we got it from get_memories_valid_at, but safe)
        } else {
            // No modification events after system_time — current DB row IS the
            // correct historical state (the only later event is the "created" event
            // emitted at wall-clock time, which doesn't represent a state change)
            result.push(memory);
        }
    }

    Ok(result)
}

/// Reconstruct a single memory's state at a given time using events only.
/// This is a connection-level reconstruction (no ReadPool needed).
fn reconstruct_memory_at(
    conn: &Connection,
    memory_id: &str,
    target_time: chrono::DateTime<chrono::Utc>,
) -> CortexResult<Option<BaseMemory>> {
    use cortex_storage::queries::event_ops;

    // Get all events for this memory up to target_time
    let target_str = target_time.to_rfc3339();
    let raw_events = event_ops::get_events_for_memory(conn, memory_id, Some(&target_str))?;

    if raw_events.is_empty() {
        return Ok(None);
    }

    // Convert raw events to MemoryEvents
    let events: Vec<cortex_core::models::MemoryEvent> = raw_events
        .into_iter()
        .filter_map(|raw| crate::event_store::query::raw_to_event(raw).ok())
        .collect();

    // Create empty shell and replay
    let shell = empty_memory_shell(memory_id);
    Ok(Some(crate::event_store::replay::replay_events(
        &events, shell,
    )))
}

/// Create an empty BaseMemory shell for replay.
fn empty_memory_shell(memory_id: &str) -> BaseMemory {
    use cortex_core::memory::*;
    BaseMemory {
        id: memory_id.to_string(),
        memory_type: MemoryType::Episodic,
        content: TypedContent::Episodic(cortex_core::memory::types::EpisodicContent {
            interaction: String::new(),
            context: String::new(),
            outcome: None,
        }),
        summary: String::new(),
        transaction_time: chrono::Utc::now(),
        valid_time: chrono::Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.5),
        importance: Importance::Normal,
        last_accessed: chrono::Utc::now(),
        access_count: 0,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec![],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: String::new(),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

fn apply_filter(
    memories: Vec<BaseMemory>,
    filter: &cortex_core::models::MemoryFilter,
) -> Vec<BaseMemory> {
    memories
        .into_iter()
        .filter(|m| {
            // Filter by memory type
            if let Some(types) = &filter.memory_types {
                if !types.contains(&m.memory_type) {
                    return false;
                }
            }

            // Filter by tags
            if let Some(tags) = &filter.tags {
                if !tags.iter().any(|t| m.tags.contains(t)) {
                    return false;
                }
            }

            // Filter by linked files (check file paths)
            if let Some(files) = &filter.linked_files {
                if !files
                    .iter()
                    .any(|f| m.linked_files.iter().any(|link| &link.file_path == f))
                {
                    return false;
                }
            }

            true
        })
        .collect()
}
