//! Temporal correction — closing old records and creating corrected versions.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::errors::{CortexError, CortexResult};
use cortex_core::memory::BaseMemory;
use cortex_core::models::EventActor;
use cortex_storage::pool::WriteConnection;
use cortex_storage::queries::memory_crud;

/// Apply a temporal correction to a memory.
///
/// This creates a new version of the memory with corrected valid_time/valid_until,
/// while preserving the old version for historical queries.
///
/// Process:
/// 1. Close the old record by setting its valid_until to now
/// 2. Create a new record with corrected times
/// 3. Link them via supersedes/superseded_by
/// 4. Record both operations as events
pub async fn apply_temporal_correction(
    writer: &Arc<WriteConnection>,
    memory_id: &str,
    corrected_valid_time: DateTime<Utc>,
    corrected_valid_until: Option<DateTime<Utc>>,
    corrected_by: EventActor,
) -> CortexResult<BaseMemory> {
    let mid = memory_id.to_string();
    let cb = corrected_by.clone();

    let corrected_memory = writer
        .with_conn(move |conn| {
            // Get the current memory
            let old_memory = memory_crud::get_memory(conn, &mid)?.ok_or_else(|| {
                CortexError::StorageError(cortex_core::errors::StorageError::SqliteError {
                    message: format!("Memory not found: {}", mid),
                })
            })?;

            let now = Utc::now();
            let corrected_id = format!("{}-corrected-{}", mid, now.timestamp_millis());

            // Close the old record: set valid_until to now, superseded_by to new ID
            let mut closed_memory = old_memory.clone();
            closed_memory.valid_until = Some(now);
            closed_memory.superseded_by = Some(corrected_id.clone());
            memory_crud::update_memory(conn, &closed_memory)?;

            // Create the corrected record with new times
            let mut corrected = old_memory;
            corrected.id = corrected_id;
            corrected.valid_time = corrected_valid_time;
            corrected.valid_until = corrected_valid_until;
            corrected.transaction_time = now;
            corrected.supersedes = Some(mid.clone());
            corrected.superseded_by = None;
            memory_crud::insert_memory(conn, &corrected)?;

            // Emit events for both operations in the same transaction
            let (actor_type, actor_id) = match &cb {
                EventActor::User(id) => ("user", id.as_str()),
                EventActor::Agent(id) => ("agent", id.as_str()),
                EventActor::System(id) => ("system", id.as_str()),
            };

            cortex_storage::temporal_events::emit_event(
                conn,
                &mid,
                "superseded",
                &serde_json::json!({
                    "superseded_by": corrected.id,
                    "valid_until": now.to_rfc3339(),
                }),
                actor_type,
                actor_id,
            )?;

            cortex_storage::temporal_events::emit_event(
                conn,
                &corrected.id,
                "created",
                &serde_json::json!({
                    "corrected_from": mid,
                    "valid_time": corrected_valid_time.to_rfc3339(),
                    "valid_until": corrected_valid_until.map(|t| t.to_rfc3339()),
                }),
                actor_type,
                actor_id,
            )?;

            Ok(corrected)
        })
        .await?;

    Ok(corrected_memory)
}
