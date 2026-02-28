use super::types::MemoryType;

/// Half-life in days for each memory type.
/// `None` means infinite (never decays).
pub fn half_life_days(memory_type: MemoryType) -> Option<u64> {
    match memory_type {
        // Domain-agnostic
        MemoryType::Core => None, // ∞
        MemoryType::Tribal => Some(365),
        MemoryType::Procedural => Some(180),
        MemoryType::Semantic => Some(90),
        MemoryType::Episodic => Some(7),
        MemoryType::Decision => Some(180),
        MemoryType::Insight => Some(90),
        MemoryType::Reference => Some(60),
        MemoryType::Preference => Some(120),
        // Code-specific
        MemoryType::PatternRationale => Some(180),
        MemoryType::ConstraintOverride => Some(90),
        MemoryType::DecisionContext => Some(180),
        MemoryType::CodeSmell => Some(90),
        // Universal V2
        MemoryType::AgentSpawn => Some(365),
        MemoryType::Entity => Some(180),
        MemoryType::Goal => Some(90),
        MemoryType::Feedback => Some(120),
        MemoryType::Workflow => Some(180),
        MemoryType::Conversation => Some(30),
        MemoryType::Incident => Some(365),
        MemoryType::Meeting => Some(60),
        MemoryType::Skill => Some(180),
        MemoryType::Environment => Some(90),
        // Convergence (Phase 2A)
        MemoryType::AgentGoal => Some(90),
        MemoryType::AgentReflection => Some(30),
        MemoryType::ConvergenceEvent => None,     // ∞
        MemoryType::BoundaryViolation => None,     // ∞
        MemoryType::ProposalRecord => Some(365),
        MemoryType::SimulationResult => Some(60),
        MemoryType::InterventionPlan => Some(180),
        MemoryType::AttachmentIndicator => Some(120),
    }
}
