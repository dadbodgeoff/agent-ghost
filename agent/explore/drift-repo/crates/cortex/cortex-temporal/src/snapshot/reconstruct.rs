//! State reconstruction: snapshot + replay algorithm.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_storage::pool::ReadPool;

use super::create::decompress_snapshot;
use super::lookup;
use crate::event_store;

/// Reconstruct a memory's state at a given point in time.
///
/// Algorithm:
/// 1. Find nearest snapshot before target_time
/// 2. If snapshot exists: decompress, replay events since snapshot
/// 3. If no snapshot: replay all events from the beginning
/// 4. Return reconstructed state (or None if no events exist)
pub fn reconstruct_at(
    readers: &Arc<ReadPool>,
    memory_id: &str,
    target_time: DateTime<Utc>,
) -> CortexResult<Option<BaseMemory>> {
    // Try to find a snapshot
    let snapshot = lookup::get_nearest_snapshot(readers, memory_id, target_time)?;

    match snapshot {
        Some(snap) => {
            // Decompress snapshot state
            let base_state = decompress_snapshot(&snap.state)?;

            // Get events after the snapshot's event_id up to target_time
            let events = event_store::query::get_events_after_id(
                readers,
                memory_id,
                snap.event_id,
                Some(target_time),
            )?;

            if events.is_empty() {
                Ok(Some(base_state))
            } else {
                Ok(Some(event_store::replay::replay_events(
                    &events, base_state,
                )))
            }
        }
        None => {
            // No snapshot — replay all events from the beginning
            let events = event_store::query::get_events(readers, memory_id, Some(target_time))?;

            if events.is_empty() {
                return Ok(None); // No events exist for this memory
            }

            // Create an empty shell and replay
            let shell = empty_memory_shell(memory_id);
            Ok(Some(event_store::replay::replay_events(&events, shell)))
        }
    }
}

/// Reconstruct all memories at a given point in time.
pub fn reconstruct_all_at(
    readers: &Arc<ReadPool>,
    target_time: DateTime<Utc>,
) -> CortexResult<Vec<BaseMemory>> {
    // Get all distinct memory_ids that had events before target_time,
    // but only for memories that actually exist in the memories table.
    // The INNER JOIN filters out orphaned events (events for deleted memories).
    let memory_ids = readers.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT me.memory_id FROM memory_events me \
                 INNER JOIN memories m ON me.memory_id = m.id \
                 WHERE me.recorded_at <= ?1",
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;

        let rows = stmt
            .query_map([target_time.to_rfc3339()], |row| row.get::<_, String>(0))
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))
    })?;

    let mut results = Vec::with_capacity(memory_ids.len());
    for mid in &memory_ids {
        if let Some(state) = reconstruct_at(readers, mid, target_time)? {
            if !state.archived {
                results.push(state);
            }
        }
    }
    Ok(results)
}

/// Create an empty BaseMemory shell for replay.
///
/// Uses `DateTime::UNIX_EPOCH` for timestamps instead of `Utc::now()` so that
/// reconstructed state doesn't leak the current wall-clock time. The Created
/// event's delta is expected to overwrite these sentinel values.
fn empty_memory_shell(memory_id: &str) -> BaseMemory {
    use chrono::DateTime;
    use cortex_core::memory::*;
    let epoch = DateTime::UNIX_EPOCH.with_timezone(&Utc);
    BaseMemory {
        id: memory_id.to_string(),
        memory_type: MemoryType::Episodic,
        content: TypedContent::Episodic(cortex_core::memory::types::EpisodicContent {
            interaction: String::new(),
            context: String::new(),
            outcome: None,
        }),
        summary: String::new(),
        transaction_time: epoch,
        valid_time: epoch,
        valid_until: None,
        confidence: Confidence::new(0.5),
        importance: Importance::Normal,
        last_accessed: epoch,
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
