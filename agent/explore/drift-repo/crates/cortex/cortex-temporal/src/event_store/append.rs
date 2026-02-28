//! Event append (single + batch).

use std::sync::Arc;

use cortex_core::errors::{CortexResult, TemporalError};
use cortex_core::models::{EventActor, MemoryEvent};
use cortex_core::CortexError;
use cortex_storage::pool::WriteConnection;
use cortex_storage::queries::event_ops;

/// Maximum number of events allowed in a single batch append.
/// Batches exceeding this limit are rejected to prevent unbounded memory usage.
pub const MAX_BATCH_SIZE: usize = 5_000;

/// Append a single event via WriteConnection.
pub async fn append(writer: &Arc<WriteConnection>, event: &MemoryEvent) -> CortexResult<u64> {
    let (actor_type, actor_id) = actor_to_strings(&event.actor);
    let delta_str = event.delta.to_string();
    let recorded_at = event.recorded_at.to_rfc3339();
    let event_type = serde_json::to_string(&event.event_type)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    let caused_by = if event.caused_by.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&event.caused_by).unwrap_or_default())
    };
    let sv = event.schema_version;
    let memory_id = event.memory_id.clone();
    let et_for_err = event_type.clone();

    writer
        .with_conn(move |conn| {
            event_ops::insert_event(
                conn,
                &memory_id,
                &recorded_at,
                &event_type,
                &delta_str,
                &actor_type,
                &actor_id,
                caused_by.as_deref(),
                sv,
            )
            .map_err(|e| {
                CortexError::TemporalError(TemporalError::EventAppendFailed(format!(
                    "failed to insert event for memory_id='{}', event_type='{}': {}",
                    memory_id, et_for_err, e
                )))
            })
        })
        .await
}

/// Append a batch of events in a single transaction.
pub async fn append_batch(
    writer: &Arc<WriteConnection>,
    events: &[MemoryEvent],
) -> CortexResult<Vec<u64>> {
    if events.len() > MAX_BATCH_SIZE {
        return Err(CortexError::TemporalError(
            TemporalError::EventAppendFailed(format!(
                "batch size {} exceeds MAX_BATCH_SIZE ({}). \
                 Split into smaller batches to avoid unbounded memory usage.",
                events.len(),
                MAX_BATCH_SIZE,
            )),
        ));
    }

    let prepared: Vec<_> = events
        .iter()
        .map(|e| {
            let (at, ai) = actor_to_strings(&e.actor);
            let delta = e.delta.to_string();
            let recorded = e.recorded_at.to_rfc3339();
            let et = serde_json::to_string(&e.event_type)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let cb = if e.caused_by.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&e.caused_by).unwrap_or_default())
            };
            (
                e.memory_id.clone(),
                recorded,
                et,
                delta,
                at,
                ai,
                cb,
                e.schema_version,
            )
        })
        .collect();

    writer
        .with_conn(move |conn| {
            // Wrap in explicit transaction for atomicity (Issue 7 fix).
            // If any insert fails, the entire batch is rolled back.
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;

            let mut ids = Vec::with_capacity(prepared.len());
            for (idx, (mid, rec, et, delta, at, ai, cb, sv)) in prepared.iter().enumerate() {
                match event_ops::insert_event(
                    conn,
                    mid,
                    rec,
                    et,
                    delta,
                    at,
                    ai,
                    cb.as_deref(),
                    *sv,
                ) {
                    Ok(id) => ids.push(id),
                    Err(e) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        return Err(CortexError::TemporalError(
                            TemporalError::EventAppendFailed(format!(
                                "batch insert failed at index {} (memory_id='{}', event_type='{}'): {}",
                                idx, mid, et, e
                            )),
                        ));
                    }
                }
            }

            conn.execute_batch("COMMIT")
                .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;
            Ok(ids)
        })
        .await
}

fn actor_to_strings(actor: &EventActor) -> (String, String) {
    match actor {
        EventActor::User(id) => ("user".to_string(), id.clone()),
        EventActor::Agent(id) => ("agent".to_string(), id.clone()),
        EventActor::System(id) => ("system".to_string(), id.clone()),
    }
}
