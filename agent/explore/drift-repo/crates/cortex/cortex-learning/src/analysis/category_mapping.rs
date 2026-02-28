//! Category → MemoryType mapping.
//!
//! Maps correction categories to the appropriate memory type for storage:
//! - pattern_violation → PatternRationale
//! - tribal_miss → Tribal
//! - constraint_violation → ConstraintOverride
//! - style_preference → Preference
//! - naming_convention → Preference
//! - architecture_mismatch → Decision
//! - security_issue → Tribal (critical importance)
//! - performance_issue → CodeSmell
//! - api_misuse → Reference
//! - other → Insight

use cortex_core::errors::CortexResult;
use cortex_core::memory::types::*;
use cortex_core::memory::{Importance, MemoryType, TypedContent};

use super::categorizer::CorrectionCategory;

/// Mapping result: memory type + importance override.
#[derive(Debug, Clone, Copy)]
pub struct CategoryMapping {
    pub memory_type: MemoryType,
    pub importance: Importance,
}

/// Map a correction category to a memory type and importance.
pub fn map_category(category: CorrectionCategory) -> CategoryMapping {
    match category {
        CorrectionCategory::PatternViolation => CategoryMapping {
            memory_type: MemoryType::PatternRationale,
            importance: Importance::High,
        },
        CorrectionCategory::TribalMiss => CategoryMapping {
            memory_type: MemoryType::Tribal,
            importance: Importance::High,
        },
        CorrectionCategory::ConstraintViolation => CategoryMapping {
            memory_type: MemoryType::ConstraintOverride,
            importance: Importance::High,
        },
        CorrectionCategory::StylePreference => CategoryMapping {
            memory_type: MemoryType::Preference,
            importance: Importance::Normal,
        },
        CorrectionCategory::NamingConvention => CategoryMapping {
            memory_type: MemoryType::Preference,
            importance: Importance::Normal,
        },
        CorrectionCategory::ArchitectureMismatch => CategoryMapping {
            memory_type: MemoryType::Decision,
            importance: Importance::High,
        },
        CorrectionCategory::SecurityIssue => CategoryMapping {
            memory_type: MemoryType::Tribal,
            importance: Importance::Critical,
        },
        CorrectionCategory::PerformanceIssue => CategoryMapping {
            memory_type: MemoryType::CodeSmell,
            importance: Importance::Normal,
        },
        CorrectionCategory::ApiMisuse => CategoryMapping {
            memory_type: MemoryType::Reference,
            importance: Importance::Normal,
        },
        CorrectionCategory::Other => CategoryMapping {
            memory_type: MemoryType::Insight,
            importance: Importance::Normal,
        },
    }
}

/// Build the appropriate `TypedContent` variant for a given memory type.
///
/// Uses the summary/principle text to populate the type-specific content fields.
pub fn build_typed_content(memory_type: MemoryType, summary: &str) -> CortexResult<TypedContent> {
    Ok(match memory_type {
        MemoryType::Insight => TypedContent::Insight(InsightContent {
            observation: summary.to_string(),
            evidence: vec![],
        }),
        MemoryType::Tribal => TypedContent::Tribal(TribalContent {
            knowledge: summary.to_string(),
            severity: "medium".to_string(),
            warnings: vec![],
            consequences: vec![],
        }),
        MemoryType::Decision => TypedContent::Decision(DecisionContent {
            decision: summary.to_string(),
            rationale: String::new(),
            alternatives: vec![],
        }),
        MemoryType::Preference => TypedContent::Preference(PreferenceContent {
            preference: summary.to_string(),
            scope: "project".to_string(),
            value: serde_json::Value::Null,
        }),
        MemoryType::PatternRationale => TypedContent::PatternRationale(PatternRationaleContent {
            pattern_name: String::new(),
            rationale: summary.to_string(),
            business_context: String::new(),
            examples: vec![],
        }),
        MemoryType::ConstraintOverride => {
            TypedContent::ConstraintOverride(ConstraintOverrideContent {
                constraint_name: String::new(),
                override_reason: summary.to_string(),
                approved_by: String::new(),
                scope: "project".to_string(),
                expiry: None,
            })
        }
        MemoryType::CodeSmell => TypedContent::CodeSmell(CodeSmellContent {
            smell_name: String::new(),
            description: summary.to_string(),
            bad_example: String::new(),
            good_example: String::new(),
            severity: "medium".to_string(),
        }),
        MemoryType::Reference => TypedContent::Reference(ReferenceContent {
            title: summary.to_string(),
            url: None,
            citation: String::new(),
        }),
        // Fallback: use Insight for any other type.
        _ => TypedContent::Insight(InsightContent {
            observation: summary.to_string(),
            evidence: vec![],
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_maps_to_tribal_critical() {
        let mapping = map_category(CorrectionCategory::SecurityIssue);
        assert_eq!(mapping.memory_type, MemoryType::Tribal);
        assert_eq!(mapping.importance, Importance::Critical);
    }

    #[test]
    fn pattern_violation_maps_to_pattern_rationale() {
        let mapping = map_category(CorrectionCategory::PatternViolation);
        assert_eq!(mapping.memory_type, MemoryType::PatternRationale);
        assert_eq!(mapping.importance, Importance::High);
    }

    #[test]
    fn performance_maps_to_code_smell() {
        let mapping = map_category(CorrectionCategory::PerformanceIssue);
        assert_eq!(mapping.memory_type, MemoryType::CodeSmell);
    }

    #[test]
    fn other_maps_to_insight() {
        let mapping = map_category(CorrectionCategory::Other);
        assert_eq!(mapping.memory_type, MemoryType::Insight);
    }

    #[test]
    fn all_categories_have_mappings() {
        for cat in CorrectionCategory::ALL {
            let mapping = map_category(cat);
            // Just verify it doesn't panic.
            let _ = mapping.memory_type;
        }
    }
}
