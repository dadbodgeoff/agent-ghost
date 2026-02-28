//! Event query operations using ReadPool.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::errors::CortexResult;
use cortex_core::models::{EventActor, MemoryEvent, MemoryEventType};
use cortex_storage::pool::ReadPool;
use cortex_storage::queries::event_ops::{self, RawEvent};

/// Maximum number of events returned from a single replay query.
/// Queries exceeding this limit still succeed but emit a warning for monitoring.
pub const MAX_REPLAY_EVENTS: usize = 10_000;

/// Get all events for a memory, optionally before a timestamp.
pub fn get_events(
    readers: &Arc<ReadPool>,
    memory_id: &str,
    before: Option<DateTime<Utc>>,
) -> CortexResult<Vec<MemoryEvent>> {
    let mid = memory_id.to_string();
    let before_str = before.map(|t| t.to_rfc3339());
    readers.with_conn(|conn| {
        let raw = event_ops::get_events_for_memory(conn, &mid, before_str.as_deref())?;
        let events: Vec<MemoryEvent> =
            raw.into_iter().filter_map(|r| raw_to_event(r).ok()).collect();
        if events.len() > MAX_REPLAY_EVENTS {
            tracing::warn!(
                memory_id = %mid,
                event_count = events.len(),
                limit = MAX_REPLAY_EVENTS,
                "get_events returned {} events, exceeding MAX_REPLAY_EVENTS ({}). \
                 Consider compacting old events or narrowing the query window.",
                events.len(),
                MAX_REPLAY_EVENTS,
            );
        }
        Ok(events)
    })
}

/// Get events in a time range across all memories.
pub fn get_events_in_range(
    readers: &Arc<ReadPool>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> CortexResult<Vec<MemoryEvent>> {
    let from_str = from.to_rfc3339();
    let to_str = to.to_rfc3339();
    readers.with_conn(|conn| {
        let raw = event_ops::get_events_in_range(conn, &from_str, &to_str)?;
        Ok(raw
            .into_iter()
            .filter_map(|r| raw_to_event(r).ok())
            .collect())
    })
}

/// Get events by type, optionally before a timestamp.
pub fn get_events_by_type(
    readers: &Arc<ReadPool>,
    event_type: &MemoryEventType,
    before: Option<DateTime<Utc>>,
) -> CortexResult<Vec<MemoryEvent>> {
    let et = serde_json::to_string(event_type)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    let before_str = before.map(|t| t.to_rfc3339());
    readers.with_conn(|conn| {
        let raw = event_ops::get_events_by_type(conn, &et, before_str.as_deref())?;
        Ok(raw
            .into_iter()
            .filter_map(|r| raw_to_event(r).ok())
            .collect())
    })
}

/// Get event count for a memory.
pub fn get_event_count(readers: &Arc<ReadPool>, memory_id: &str) -> CortexResult<u64> {
    let mid = memory_id.to_string();
    readers.with_conn(|conn| event_ops::get_event_count(conn, &mid))
}

/// Get events for a memory after a specific event_id.
pub fn get_events_after_id(
    readers: &Arc<ReadPool>,
    memory_id: &str,
    after_event_id: u64,
    before: Option<DateTime<Utc>>,
) -> CortexResult<Vec<MemoryEvent>> {
    let mid = memory_id.to_string();
    let before_str = before.map(|t| t.to_rfc3339());
    readers.with_conn(|conn| {
        let raw =
            event_ops::get_events_after_id(conn, &mid, after_event_id, before_str.as_deref())?;
        let events: Vec<MemoryEvent> =
            raw.into_iter().filter_map(|r| raw_to_event(r).ok()).collect();
        if events.len() > MAX_REPLAY_EVENTS {
            tracing::warn!(
                memory_id = %mid,
                after_event_id = after_event_id,
                event_count = events.len(),
                limit = MAX_REPLAY_EVENTS,
                "get_events_after_id returned {} events, exceeding MAX_REPLAY_EVENTS ({}). \
                 Consider compacting old events or narrowing the query window.",
                events.len(),
                MAX_REPLAY_EVENTS,
            );
        }
        Ok(events)
    })
}

/// Convert a RawEvent to a MemoryEvent.
pub(crate) fn raw_to_event(raw: RawEvent) -> CortexResult<MemoryEvent> {
    let event_type: MemoryEventType = serde_json::from_str(&format!("\"{}\"", raw.event_type))
        .map_err(|e| {
            cortex_core::CortexError::TemporalError(
                cortex_core::errors::TemporalError::EventAppendFailed(format!(
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
