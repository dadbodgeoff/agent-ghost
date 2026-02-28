//! Event replay: apply events to reconstruct state.

use cortex_core::memory::{BaseMemory, Confidence, Importance, MemoryType};
use cortex_core::models::{MemoryEvent, MemoryEventType};

/// Replay a sequence of events onto an initial state.
pub fn replay_events(events: &[MemoryEvent], initial: BaseMemory) -> BaseMemory {
    let mut state = initial;
    for event in events {
        state = apply_event(state, event);
    }
    state
}

/// Apply a single event to a state, returning the modified state.
pub fn apply_event(mut state: BaseMemory, event: &MemoryEvent) -> BaseMemory {
    match &event.event_type {
        MemoryEventType::Created => {
            // Full initial state in delta — deserialize if possible.
            if let Ok(mem) = serde_json::from_value::<BaseMemory>(event.delta.clone()) {
                return mem;
            }
            state
        }
        MemoryEventType::ContentUpdated => {
            if let Some(s) = event.delta.get("new_summary").and_then(|v| v.as_str()) {
                state.summary = s.to_string();
            }
            if let Some(h) = event.delta.get("new_content_hash").and_then(|v| v.as_str()) {
                state.content_hash = h.to_string();
            }
            state
        }
        MemoryEventType::ConfidenceChanged => {
            if let Some(v) = event.delta.get("new").and_then(|v| v.as_f64()) {
                state.confidence = Confidence::new(v);
            }
            state
        }
        MemoryEventType::ImportanceChanged => {
            if let Some(v) = event.delta.get("new").and_then(|v| v.as_str()) {
                if let Ok(imp) = serde_json::from_str::<Importance>(&format!("\"{v}\"")) {
                    state.importance = imp;
                }
            }
            state
        }
        MemoryEventType::TagsModified => {
            if let Some(added) = event.delta.get("added").and_then(|v| v.as_array()) {
                for tag in added {
                    if let Some(t) = tag.as_str() {
                        if !state.tags.contains(&t.to_string()) {
                            state.tags.push(t.to_string());
                        }
                    }
                }
            }
            if let Some(removed) = event.delta.get("removed").and_then(|v| v.as_array()) {
                for tag in removed {
                    if let Some(t) = tag.as_str() {
                        state.tags.retain(|x| x != t);
                    }
                }
            }
            state
        }
        MemoryEventType::LinkAdded | MemoryEventType::LinkRemoved => {
            // Link changes are tracked but don't modify BaseMemory fields
            // in replay — links are loaded separately from link tables.
            state
        }
        MemoryEventType::RelationshipAdded
        | MemoryEventType::RelationshipRemoved
        | MemoryEventType::StrengthUpdated => {
            // Graph-level events — no BaseMemory field changes.
            state
        }
        MemoryEventType::Archived => {
            state.archived = true;
            state
        }
        MemoryEventType::Restored => {
            state.archived = false;
            state
        }
        MemoryEventType::Decayed => {
            if let Some(v) = event.delta.get("new_confidence").and_then(|v| v.as_f64()) {
                state.confidence = Confidence::new(v);
            }
            state
        }
        MemoryEventType::Validated => {
            // Validation metadata — no direct BaseMemory field change.
            state
        }
        MemoryEventType::Consolidated => {
            if let Some(into) = event.delta.get("merged_into").and_then(|v| v.as_str()) {
                state.superseded_by = Some(into.to_string());
            }
            state
        }
        MemoryEventType::Reclassified => {
            if let Some(v) = event.delta.get("new_type").and_then(|v| v.as_str()) {
                if let Ok(mt) = serde_json::from_str::<MemoryType>(&format!("\"{v}\"")) {
                    state.memory_type = mt;
                }
            }
            state
        }
        MemoryEventType::Superseded => {
            if let Some(by) = event.delta.get("superseded_by").and_then(|v| v.as_str()) {
                state.superseded_by = Some(by.to_string());
            }
            state
        }
    }
}
