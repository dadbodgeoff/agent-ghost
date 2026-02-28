//! Test helper functions shared across the workspace.

use cortex_temporal::hash_chain::{compute_event_hash, ChainEvent, GENESIS_HASH};

/// Build a valid hash chain from raw event data.
pub fn build_chain(events: &[(&str, &str, &str, &str)]) -> Vec<ChainEvent> {
    let mut chain = Vec::with_capacity(events.len());
    let mut prev_hash = GENESIS_HASH;

    for (event_type, delta_json, actor_id, recorded_at) in events {
        let event_hash =
            compute_event_hash(event_type, delta_json, actor_id, recorded_at, &prev_hash);

        chain.push(ChainEvent {
            event_type: event_type.to_string(),
            delta_json: delta_json.to_string(),
            actor_id: actor_id.to_string(),
            recorded_at: recorded_at.to_string(),
            event_hash,
            previous_hash: prev_hash,
        });

        prev_hash = event_hash;
    }

    chain
}

/// Assert that a value is in [0.0, 1.0].
pub fn assert_unit_range(value: f64, label: &str) {
    assert!(
        (0.0..=1.0).contains(&value),
        "{label} = {value} is outside [0.0, 1.0]"
    );
}

/// Assert that a value is >= 1.0 (for decay factor monotonicity).
pub fn assert_factor_monotonic(value: f64, label: &str) {
    assert!(
        value >= 1.0,
        "{label} = {value} is below 1.0 (monotonicity violation)"
    );
}
