use cortex_core::intent::{weights, Intent};
use cortex_core::memory::MemoryType;

#[test]
fn intent_has_22_variants() {
    assert_eq!(Intent::COUNT, 22);
    assert_eq!(Intent::ALL.len(), 22);
}

#[test]
fn intent_categories() {
    assert_eq!(Intent::Create.category(), "domain_agnostic");
    assert_eq!(Intent::FixBug.category(), "code_specific");
    assert_eq!(Intent::SpawnAgent.category(), "universal");
    assert_eq!(Intent::MonitorConvergence.category(), "convergence");
}

#[test]
fn intent_serde_roundtrip() {
    for intent in Intent::ALL {
        let json = serde_json::to_string(&intent).unwrap();
        let deserialized: Intent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, intent);
    }
}

#[test]
fn default_weights_return_boosts_for_known_pairings() {
    // FixBug + CodeSmell should be boosted
    let w = weights::default_weight(Intent::FixBug, MemoryType::CodeSmell);
    assert!(w > 1.0, "FixBug + CodeSmell should be boosted, got {}", w);

    // Recall + Semantic should be boosted
    let w = weights::default_weight(Intent::Recall, MemoryType::Semantic);
    assert!(w > 1.0);

    // Random pairing should be 1.0
    let w = weights::default_weight(Intent::Summarize, MemoryType::Environment);
    assert_eq!(w, 1.0);
}

#[test]
fn weight_overrides_load_from_map() {
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("create:core".to_string(), 3.0);
    overrides.insert("invalid:bogus".to_string(), 99.0); // should be ignored

    let map = weights::load_weight_overrides(&overrides);
    assert_eq!(map.get(&(Intent::Create, MemoryType::Core)), Some(&3.0));
    assert_eq!(map.len(), 1); // invalid entry ignored
}
