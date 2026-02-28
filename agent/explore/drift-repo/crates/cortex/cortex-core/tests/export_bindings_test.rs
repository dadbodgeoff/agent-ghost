//! Test that generates TypeScript bindings from Rust types via ts-rs.
//!
//! Run with: cargo test -p cortex-core export_bindings
//! Generated files appear in cortex-core/bindings/*.ts
//!
//! CI should run this and then `git diff --exit-code` to catch drift.

#[test]
fn export_bindings() {
    // ts-rs generates .ts files automatically for every type with #[ts(export)].
    // This test simply ensures all types compile with their TS derive.
    // The actual file generation happens via the #[ts(export)] attribute
    // when `cargo test` runs — ts-rs writes to `TS_RS_EXPORT_DIR` or
    // `<crate>/bindings/` by default.
    //
    // To verify: check that cortex-core/bindings/ contains .ts files after running.

    use cortex_core::intent::Intent;
    use cortex_core::memory::types::{
        AgentSpawnContent, Alternative, CodeSmellContent, ConstraintOverrideContent,
        ConversationContent, CoreContent, DecisionContent, DecisionContextContent, EntityContent,
        EnvironmentContent, EpisodicContent, FeedbackContent, GoalContent, IncidentContent,
        InsightContent, MeetingContent, PatternRationaleContent, PreferenceContent,
        ProceduralContent, ProceduralStep, ReferenceContent, SemanticContent, SkillContent,
        TribalContent, WorkflowContent, WorkflowStep,
    };
    use cortex_core::memory::{
        BaseMemory, Confidence, ConstraintLink, FileLink, FunctionLink, Importance, MemoryType,
        PatternLink, RelationshipEdge, RelationshipType, TypedContent,
    };
    use cortex_core::models::{
        BudgetAllocation, CausalNarrative, CompressedMemory, ConsolidationMetrics,
        ConsolidationResult, DegradationEvent, DimensionScores, GenerationContext, HealingAction,
        HealingActionType, HealthMetrics, HealthReport, HealthStatus, LearningResult,
        NarrativeSection, PredictionResult, RetrievalContext, SessionContext, SubsystemHealth,
        ValidationResult, WhyContext, WhyEntry,
    };

    // ts-rs export is triggered by the derive macro at compile time.
    // This test just validates all types are importable and TS-derivable.
    let _ = std::any::type_name::<BaseMemory>();
    let _ = std::any::type_name::<TypedContent>();
    let _ = std::any::type_name::<MemoryType>();
    let _ = std::any::type_name::<Importance>();
    let _ = std::any::type_name::<Confidence>();
    let _ = std::any::type_name::<Intent>();
    let _ = std::any::type_name::<RelationshipType>();
    let _ = std::any::type_name::<RelationshipEdge>();
    let _ = std::any::type_name::<PatternLink>();
    let _ = std::any::type_name::<ConstraintLink>();
    let _ = std::any::type_name::<FileLink>();
    let _ = std::any::type_name::<FunctionLink>();
    let _ = std::any::type_name::<RetrievalContext>();
    let _ = std::any::type_name::<CompressedMemory>();
    let _ = std::any::type_name::<CausalNarrative>();
    let _ = std::any::type_name::<NarrativeSection>();
    let _ = std::any::type_name::<ConsolidationResult>();
    let _ = std::any::type_name::<ConsolidationMetrics>();
    let _ = std::any::type_name::<HealthReport>();
    let _ = std::any::type_name::<HealthStatus>();
    let _ = std::any::type_name::<SubsystemHealth>();
    let _ = std::any::type_name::<HealthMetrics>();
    let _ = std::any::type_name::<DegradationEvent>();
    let _ = std::any::type_name::<ValidationResult>();
    let _ = std::any::type_name::<DimensionScores>();
    let _ = std::any::type_name::<HealingAction>();
    let _ = std::any::type_name::<HealingActionType>();
    let _ = std::any::type_name::<LearningResult>();
    let _ = std::any::type_name::<PredictionResult>();
    let _ = std::any::type_name::<GenerationContext>();
    let _ = std::any::type_name::<BudgetAllocation>();
    let _ = std::any::type_name::<WhyContext>();
    let _ = std::any::type_name::<WhyEntry>();
    let _ = std::any::type_name::<SessionContext>();
    let _ = std::any::type_name::<CoreContent>();
    let _ = std::any::type_name::<TribalContent>();
    let _ = std::any::type_name::<ProceduralContent>();
    let _ = std::any::type_name::<ProceduralStep>();
    let _ = std::any::type_name::<SemanticContent>();
    let _ = std::any::type_name::<EpisodicContent>();
    let _ = std::any::type_name::<DecisionContent>();
    let _ = std::any::type_name::<Alternative>();
    let _ = std::any::type_name::<InsightContent>();
    let _ = std::any::type_name::<ReferenceContent>();
    let _ = std::any::type_name::<PreferenceContent>();
    let _ = std::any::type_name::<PatternRationaleContent>();
    let _ = std::any::type_name::<ConstraintOverrideContent>();
    let _ = std::any::type_name::<DecisionContextContent>();
    let _ = std::any::type_name::<CodeSmellContent>();
    let _ = std::any::type_name::<AgentSpawnContent>();
    let _ = std::any::type_name::<EntityContent>();
    let _ = std::any::type_name::<GoalContent>();
    let _ = std::any::type_name::<FeedbackContent>();
    let _ = std::any::type_name::<WorkflowContent>();
    let _ = std::any::type_name::<WorkflowStep>();
    let _ = std::any::type_name::<ConversationContent>();
    let _ = std::any::type_name::<IncidentContent>();
    let _ = std::any::type_name::<MeetingContent>();
    let _ = std::any::type_name::<SkillContent>();
    let _ = std::any::type_name::<EnvironmentContent>();
}
