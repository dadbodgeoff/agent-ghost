//! Golden dataset loaders for deterministic test baselines.

use serde_json::Value;

/// Load a golden convergence trajectory (normal pattern).
pub fn normal_trajectory() -> Vec<f64> {
    vec![
        0.05, 0.08, 0.06, 0.10, 0.07, 0.09, 0.11, 0.08, 0.06, 0.10, 0.12, 0.09, 0.07, 0.11, 0.08,
        0.10, 0.09, 0.07, 0.06, 0.08,
    ]
}

/// Load a golden convergence trajectory (escalating pattern).
pub fn escalating_trajectory() -> Vec<f64> {
    vec![
        0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50, 0.55, 0.60, 0.65, 0.70, 0.75, 0.80,
        0.85, 0.88, 0.90, 0.92, 0.95,
    ]
}

/// Load a golden intervention sequence.
pub fn intervention_sequence() -> Vec<(f64, u8)> {
    vec![
        (0.10, 0), // Normal
        (0.25, 0), // Still normal
        (0.35, 1), // Level 1
        (0.55, 2), // Level 2
        (0.75, 3), // Level 3
        (0.90, 4), // Level 4
    ]
}

/// Create a minimal valid ghost.yml config as JSON Value.
pub fn minimal_config() -> Value {
    serde_json::json!({
        "agents": [{
            "name": "test-agent",
            "model": "gpt-4",
            "channel": "cli"
        }],
        "convergence": {
            "profile": "standard",
            "calibration_sessions": 10
        }
    })
}
