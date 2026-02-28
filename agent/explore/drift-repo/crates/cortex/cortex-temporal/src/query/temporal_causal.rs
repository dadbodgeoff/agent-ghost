//! Temporal causal query execution — "At the time we adopted Pattern X,
//! what was the causal chain?"
//!
//! Delegates to cortex-causal's temporal graph reconstruction, then runs
//! traversal on the historical graph.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_causal::graph::temporal_graph;
use cortex_causal::TraversalResult;
use cortex_core::errors::CortexResult;
use cortex_core::models::{CausalGraphSnapshot, MemoryEvent, MemoryEventType, TemporalCausalQuery};
use cortex_storage::pool::ReadPool;

/// Execute a temporal causal query.
///
/// Fetches all relationship events up to `query.as_of`, delegates graph
/// reconstruction to cortex-causal, then runs traversal on the historical graph.
///
/// Critical invariant: temporal causal at current time must equal current
/// graph traversal (same pattern as AS OF current == current state).
pub fn execute_temporal_causal(
    readers: &Arc<ReadPool>,
    query: &TemporalCausalQuery,
) -> CortexResult<TraversalResult> {
    // Fetch all relationship events up to as_of
    let events = fetch_relationship_events(readers, query.as_of)?;

    // Delegate to cortex-causal's temporal traversal
    let result = temporal_graph::temporal_traversal(
        &events,
        &query.memory_id,
        query.as_of,
        query.direction,
        query.max_depth,
    );

    Ok(result)
}

/// Reconstruct the causal graph snapshot at a given time.
///
/// Used by decision replay to capture the causal state at decision time.
pub fn reconstruct_causal_snapshot(
    readers: &Arc<ReadPool>,
    as_of: DateTime<Utc>,
) -> CortexResult<CausalGraphSnapshot> {
    let events = fetch_relationship_events(readers, as_of)?;
    let graph = temporal_graph::reconstruct_graph_at(&events, as_of);
    Ok(temporal_graph::graph_to_snapshot(&graph))
}

/// Fetch all relationship events (Added, Removed, StrengthUpdated) up to a time.
fn fetch_relationship_events(
    readers: &Arc<ReadPool>,
    before: DateTime<Utc>,
) -> CortexResult<Vec<MemoryEvent>> {
    let before_str = before.to_rfc3339();

    readers.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT event_id, memory_id, recorded_at, event_type, delta, \
                        actor_type, actor_id, caused_by, schema_version \
                 FROM memory_events \
                 WHERE recorded_at <= ?1 \
                   AND event_type IN ('relationship_added', 'relationship_removed', 'strength_updated') \
                 ORDER BY event_id ASC",
            )
            .map_err(|e| {
                cortex_core::CortexError::TemporalError(
                    cortex_core::errors::TemporalError::QueryFailed(e.to_string()),
                )
            })?;

        let rows = stmt
            .query_map(rusqlite::params![before_str], |row| {
                Ok(RawRelEvent {
                    event_id: row.get(0)?,
                    memory_id: row.get(1)?,
                    recorded_at: row.get(2)?,
                    event_type: row.get(3)?,
                    delta: row.get(4)?,
                    actor_type: row.get(5)?,
                    actor_id: row.get(6)?,
                    caused_by: row.get(7)?,
                    schema_version: row.get(8)?,
                })
            })
            .map_err(|e| {
                cortex_core::CortexError::TemporalError(
                    cortex_core::errors::TemporalError::QueryFailed(e.to_string()),
                )
            })?;

        let mut events = Vec::new();
        for raw in rows.flatten() {
            if let Ok(event) = raw_to_memory_event(raw) {
                events.push(event);
            }
        }

        Ok(events)
    })
}

/// Raw row from the relationship events query.
struct RawRelEvent {
    event_id: u64,
    memory_id: String,
    recorded_at: String,
    event_type: String,
    delta: String,
    actor_type: String,
    actor_id: String,
    caused_by: Option<String>,
    schema_version: u16,
}

/// Convert a raw row to a MemoryEvent.
fn raw_to_memory_event(raw: RawRelEvent) -> CortexResult<MemoryEvent> {
    use cortex_core::models::EventActor;

    let event_type: MemoryEventType = serde_json::from_str(&format!("\"{}\"", raw.event_type))
        .map_err(|e| {
            cortex_core::CortexError::TemporalError(
                cortex_core::errors::TemporalError::QueryFailed(format!(
                    "parse event_type '{}': {}",
                    raw.event_type, e
                )),
            )
        })?;

    let actor = match raw.actor_type.as_str() {
        "user" => EventActor::User(raw.actor_id),
        "agent" => EventActor::Agent(raw.actor_id),
        _ => EventActor::System(raw.actor_id),
    };

    let delta: serde_json::Value = serde_json::from_str(&raw.delta).unwrap_or_default();

    let caused_by: Vec<u64> = raw
        .caused_by
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let recorded_at = chrono::DateTime::parse_from_rfc3339(&raw.recorded_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(MemoryEvent {
        event_id: raw.event_id,
        memory_id: raw.memory_id,
        recorded_at,
        event_type,
        delta,
        actor,
        caused_by,
        schema_version: raw.schema_version,
    })
}
