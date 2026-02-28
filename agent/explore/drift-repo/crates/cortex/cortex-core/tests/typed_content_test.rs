/// Serde roundtrip for each of the 23 typed content structs.
use cortex_core::memory::base::TypedContent;
use cortex_core::memory::types::*;

fn roundtrip(content: &TypedContent) -> TypedContent {
    let json = serde_json::to_string(content).unwrap();
    serde_json::from_str(&json).unwrap()
}

#[test]
fn core_content_roundtrip() {
    let c = TypedContent::Core(CoreContent {
        project_name: "drift".into(),
        description: "AI IDE".into(),
        metadata: serde_json::json!({"lang": "rust"}),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn tribal_content_roundtrip() {
    let c = TypedContent::Tribal(TribalContent {
        knowledge: "Never deploy on Friday".into(),
        severity: "high".into(),
        warnings: vec!["Outage risk".into()],
        consequences: vec!["Weekend oncall".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn procedural_content_roundtrip() {
    let c = TypedContent::Procedural(ProceduralContent {
        title: "Deploy to prod".into(),
        steps: vec![ProceduralStep {
            order: 1,
            instruction: "Run tests".into(),
            completed: false,
        }],
        prerequisites: vec!["CI green".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn semantic_content_roundtrip() {
    let c = TypedContent::Semantic(SemanticContent {
        knowledge: "Auth uses JWT".into(),
        source_episodes: vec!["ep-1".into()],
        consolidation_confidence: 0.9,
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn episodic_content_roundtrip() {
    let c = TypedContent::Episodic(EpisodicContent {
        interaction: "User asked about auth".into(),
        context: "code review".into(),
        outcome: Some("Explained JWT flow".into()),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn decision_content_roundtrip() {
    let c = TypedContent::Decision(DecisionContent {
        decision: "Use SQLite".into(),
        rationale: "Single-file, embedded".into(),
        alternatives: vec![Alternative {
            description: "PostgreSQL".into(),
            reason_rejected: "External dependency".into(),
        }],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn insight_content_roundtrip() {
    let c = TypedContent::Insight(InsightContent {
        observation: "Batch ops are 10x faster".into(),
        evidence: vec!["benchmark results".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn reference_content_roundtrip() {
    let c = TypedContent::Reference(ReferenceContent {
        title: "SQLite WAL docs".into(),
        url: Some("https://sqlite.org/wal.html".into()),
        citation: "SQLite documentation".into(),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn preference_content_roundtrip() {
    let c = TypedContent::Preference(PreferenceContent {
        preference: "tabs over spaces".into(),
        scope: "workspace".into(),
        value: serde_json::json!({"indent": "tabs"}),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn pattern_rationale_content_roundtrip() {
    let c = TypedContent::PatternRationale(PatternRationaleContent {
        pattern_name: "Repository pattern".into(),
        rationale: "Decouples data access".into(),
        business_context: "Testability".into(),
        examples: vec!["UserRepository".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn constraint_override_content_roundtrip() {
    let c = TypedContent::ConstraintOverride(ConstraintOverrideContent {
        constraint_name: "no-unsafe".into(),
        override_reason: "FFI boundary".into(),
        approved_by: "tech-lead".into(),
        scope: "cortex-napi".into(),
        expiry: None,
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn decision_context_content_roundtrip() {
    let c = TypedContent::DecisionContext(DecisionContextContent {
        decision: "Use moka for caching".into(),
        context: "Need TinyLFU".into(),
        adr_link: Some("docs/adr/003.md".into()),
        trade_offs: vec!["Extra dependency".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn code_smell_content_roundtrip() {
    let c = TypedContent::CodeSmell(CodeSmellContent {
        smell_name: "God object".into(),
        description: "Class does too much".into(),
        bad_example: "class App { ... 2000 lines }".into(),
        good_example: "Split into focused modules".into(),
        severity: "high".into(),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn agent_spawn_content_roundtrip() {
    let c = TypedContent::AgentSpawn(AgentSpawnContent {
        agent_name: "code-reviewer".into(),
        configuration: serde_json::json!({"model": "claude"}),
        purpose: "Automated code review".into(),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn entity_content_roundtrip() {
    let c = TypedContent::Entity(EntityContent {
        entity_name: "Cortex".into(),
        entity_type: "system".into(),
        description: "Memory system".into(),
        attributes: serde_json::json!({"language": "rust"}),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn goal_content_roundtrip() {
    let c = TypedContent::Goal(GoalContent {
        title: "Ship v2".into(),
        description: "Complete Rust rewrite".into(),
        progress: 0.15,
        milestones: vec!["Phase 1 done".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn feedback_content_roundtrip() {
    let c = TypedContent::Feedback(FeedbackContent {
        feedback: "Retrieval was slow".into(),
        category: "performance".into(),
        source: "user".into(),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn workflow_content_roundtrip() {
    let c = TypedContent::Workflow(WorkflowContent {
        name: "Release process".into(),
        steps: vec![WorkflowStep {
            order: 1,
            action: "Run tests".into(),
            condition: None,
        }],
        trigger: Some("tag push".into()),
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn conversation_content_roundtrip() {
    let c = TypedContent::Conversation(ConversationContent {
        summary: "Discussed auth approach".into(),
        participants: vec!["alice".into(), "bob".into()],
        key_points: vec!["Use OAuth2".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn incident_content_roundtrip() {
    let c = TypedContent::Incident(IncidentContent {
        title: "Prod outage 2026-01-15".into(),
        root_cause: "DB connection leak".into(),
        impact: "30min downtime".into(),
        resolution: "Fixed connection pool".into(),
        lessons_learned: vec!["Add connection monitoring".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn meeting_content_roundtrip() {
    let c = TypedContent::Meeting(MeetingContent {
        title: "Sprint planning".into(),
        attendees: vec!["team".into()],
        notes: "Prioritized P1".into(),
        action_items: vec!["Start cortex-core".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn skill_content_roundtrip() {
    let c = TypedContent::Skill(SkillContent {
        skill_name: "Rust".into(),
        proficiency: "advanced".into(),
        domain: "systems programming".into(),
        evidence: vec!["Built cortex".into()],
    });
    assert_eq!(roundtrip(&c), c);
}

#[test]
fn environment_content_roundtrip() {
    let c = TypedContent::Environment(EnvironmentContent {
        name: "production".into(),
        config: serde_json::json!({"region": "us-east-1"}),
        platform: Some("linux".into()),
    });
    assert_eq!(roundtrip(&c), c);
}
