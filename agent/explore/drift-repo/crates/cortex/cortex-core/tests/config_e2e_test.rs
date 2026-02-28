#![allow(clippy::field_reassign_with_default)]
//! E2E tests for config and temporal hardening (Phase F).
//!
//! Every test targets a specific production failure mode:
//! - F-06: Missing drift_detection_window_hours in serialized config → must default to 168
//! - F-06: Custom drift_detection_window_hours must round-trip through serde
//! - TemporalConfig serde resilience: missing optional fields → defaults, not error

use cortex_core::config::temporal_config::TemporalConfig;

// ═══════════════════════════════════════════════════════════════════════════
// F-06: drift_detection_window_hours serde resilience
// ═══════════════════════════════════════════════════════════════════════════

/// PRODUCTION BUG (pre-F-06): Config files without drift_detection_window_hours
/// would fail to deserialize or use an unexpected default. The field must have
/// a serde default so old config files still work.
#[test]
fn missing_drift_detection_window_hours_defaults_to_168() {
    // Simulate an old config JSON that doesn't have the new field
    let json = r#"{
        "snapshot_event_threshold": 50,
        "event_compaction_age_days": 180
    }"#;

    let config: TemporalConfig = serde_json::from_str(json).unwrap();
    assert_eq!(
        config.drift_detection_window_hours, 168,
        "missing drift_detection_window_hours should default to 168 (1 week)"
    );
}

/// Custom value should round-trip through serialize/deserialize.
#[test]
fn drift_detection_window_hours_roundtrip() {
    let mut config = TemporalConfig::default();
    config.drift_detection_window_hours = 720; // 30 days

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: TemporalConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.drift_detection_window_hours, 720);
}

/// Zero hours should be valid (disable drift detection).
#[test]
fn drift_detection_window_zero_valid() {
    let json = r#"{ "drift_detection_window_hours": 0 }"#;
    let config: TemporalConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.drift_detection_window_hours, 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// TemporalConfig: All fields have sensible defaults
// ═══════════════════════════════════════════════════════════════════════════

/// Default config should have all fields populated with reasonable values.
#[test]
fn temporal_config_default_all_fields_populated() {
    let config = TemporalConfig::default();

    // Verify key defaults exist and are reasonable
    assert!(config.snapshot_event_threshold > 0, "snapshot_event_threshold should be positive");
    assert!(config.event_compaction_age_days > 0, "event_compaction_age_days should be positive");
    assert_eq!(config.drift_detection_window_hours, 168, "default drift window should be 168 hours");
}

/// Partial JSON with only some fields should still deserialize (all others get defaults).
#[test]
fn partial_config_deserializes_with_defaults() {
    let json = r#"{ "event_compaction_age_days": 30 }"#;
    let config: TemporalConfig = serde_json::from_str(json).unwrap();

    assert_eq!(config.event_compaction_age_days, 30, "explicit field should be used");
    assert!(config.snapshot_event_threshold > 0, "missing field should get default");
    assert_eq!(config.drift_detection_window_hours, 168, "missing drift window should default");
}

/// Empty JSON object should deserialize to all defaults.
#[test]
fn empty_json_all_defaults() {
    let config: TemporalConfig = serde_json::from_str("{}").unwrap();

    let default_config = TemporalConfig::default();
    assert_eq!(
        serde_json::to_string(&config).unwrap(),
        serde_json::to_string(&default_config).unwrap(),
        "empty JSON should produce identical config to Default::default()"
    );
}
