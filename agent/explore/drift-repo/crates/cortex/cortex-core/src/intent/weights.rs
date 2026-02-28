use std::collections::HashMap;

use crate::memory::MemoryType;

use super::taxonomy::Intent;

/// Default boost weights for Intent → MemoryType.
/// Returns a multiplier (1.0 = no boost, >1.0 = boosted).
pub fn default_weight(intent: Intent, memory_type: MemoryType) -> f64 {
    // High-value pairings get a 2.0x boost, moderate get 1.5x, rest 1.0x.
    match (intent, memory_type) {
        // Create intent boosts procedural and workflow
        (Intent::Create, MemoryType::Procedural | MemoryType::Workflow) => 2.0,
        (Intent::Create, MemoryType::PatternRationale) => 1.5,

        // Investigate boosts episodic, incident, causal
        (Intent::Investigate, MemoryType::Episodic | MemoryType::Incident) => 2.0,
        (Intent::Investigate, MemoryType::Decision | MemoryType::DecisionContext) => 1.5,

        // Decide boosts decisions
        (Intent::Decide, MemoryType::Decision | MemoryType::DecisionContext) => 2.0,
        (Intent::Decide, MemoryType::ConstraintOverride) => 1.5,

        // Recall boosts semantic and tribal
        (Intent::Recall, MemoryType::Semantic | MemoryType::Tribal) => 2.0,
        (Intent::Recall, MemoryType::Core) => 1.5,

        // Learn boosts feedback and insight
        (Intent::Learn, MemoryType::Feedback | MemoryType::Insight) => 2.0,
        (Intent::Learn, MemoryType::Skill) => 1.5,

        // FixBug boosts code smells and incidents
        (Intent::FixBug, MemoryType::CodeSmell | MemoryType::Incident) => 2.0,
        (Intent::FixBug, MemoryType::PatternRationale) => 1.5,

        // Refactor boosts patterns and constraints
        (Intent::Refactor, MemoryType::PatternRationale | MemoryType::ConstraintOverride) => 2.0,
        (Intent::Refactor, MemoryType::CodeSmell) => 1.5,

        // SecurityAudit boosts constraints and tribal
        (Intent::SecurityAudit, MemoryType::ConstraintOverride | MemoryType::Tribal) => 2.0,

        // UnderstandCode boosts patterns and decisions
        (Intent::UnderstandCode, MemoryType::PatternRationale | MemoryType::DecisionContext) => 2.0,
        (Intent::UnderstandCode, MemoryType::Tribal) => 1.5,

        // ReviewCode boosts code smells and patterns
        (Intent::ReviewCode, MemoryType::CodeSmell | MemoryType::PatternRationale) => 2.0,

        // Default: no boost
        _ => 1.0,
    }
}

/// Load weight overrides from a TOML table.
/// Keys are "intent:memory_type", values are f64 multipliers.
pub fn load_weight_overrides(
    overrides: &HashMap<String, f64>,
) -> HashMap<(Intent, MemoryType), f64> {
    let mut map = HashMap::new();
    for (key, &value) in overrides {
        if let Some((intent_str, type_str)) = key.split_once(':') {
            if let (Ok(intent), Ok(memory_type)) = (
                serde_json::from_str::<Intent>(&format!("\"{}\"", intent_str)),
                serde_json::from_str::<MemoryType>(&format!("\"{}\"", type_str)),
            ) {
                map.insert((intent, memory_type), value);
            }
        }
    }
    map
}
