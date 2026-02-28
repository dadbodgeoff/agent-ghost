pub mod agent;
pub mod caller;
pub mod cross_agent;
pub mod namespace;
pub mod provenance;

mod audit_entry;
mod causal_narrative;
mod compressed_memory;
mod consolidation_metrics;
mod consolidation_result;
mod contradiction;
mod decision_replay;
mod degradation_event;
mod drift_alert;
mod drift_snapshot;
mod embedding_info;
mod epistemic_status;
mod materialized_view;
mod generation_context;
mod health_report;
mod learning_result;
mod prediction_result;
mod retrieval_context;
mod session_context;
mod temporal_diff;
mod temporal_event;
mod temporal_query;
mod validation_result;
mod why_context;

pub use audit_entry::{AuditActor, AuditEntry, AuditOperation};
pub use causal_narrative::{CausalNarrative, NarrativeSection};
pub use compressed_memory::CompressedMemory;
pub use consolidation_metrics::ConsolidationMetrics;
pub use consolidation_result::ConsolidationResult;
pub use contradiction::{Contradiction, ContradictionType, DetectionStrategy};
pub use decision_replay::{CausalEdgeSnapshot, CausalGraphSnapshot, DecisionReplay, HindsightItem};
pub use degradation_event::DegradationEvent;
pub use drift_alert::{AlertSeverity, DriftAlert, DriftAlertCategory};
pub use drift_snapshot::{
    DriftSnapshot, GlobalDriftMetrics, ModuleDriftMetrics, TypeDriftMetrics,
};
pub use embedding_info::{EmbeddingModelInfo, EmbeddingModelStatus};
pub use epistemic_status::{AggregationStrategy, EpistemicStatus};
pub use materialized_view::MaterializedTemporalView;
pub use generation_context::{BudgetAllocation, GenerationContext};
pub use health_report::{HealthMetrics, HealthReport, HealthStatus, SubsystemHealth};
pub use learning_result::LearningResult;
pub use prediction_result::PredictionResult;
pub use retrieval_context::RetrievalContext;
pub use session_context::SessionContext;
pub use temporal_diff::{
    ConfidenceShift, DiffStats, MemoryModification, Reclassification, TemporalDiff,
};
pub use temporal_event::{
    EventActor, MemoryEvent, MemoryEventType, MemorySnapshot, SnapshotReason,
};
pub use temporal_query::{
    AsOfQuery, DecisionReplayQuery, DiffScope, MemoryFilter, TemporalCausalQuery,
    TemporalDiffQuery, TemporalRangeMode, TemporalRangeQuery, TraversalDirection,
};
pub use validation_result::{DimensionScores, HealingAction, HealingActionType, ValidationResult};
pub use why_context::{WhyContext, WhyEntry};

// Multi-agent types
pub use agent::{AgentId, AgentRegistration, AgentStatus, SpawnConfig};
pub use caller::CallerType;
pub use cross_agent::{
    AgentTrust, ContradictionResolution, CrossAgentContradiction, CrossAgentRelation,
    TrustEvidence,
};
pub use namespace::{
    MemoryProjection, NamespaceACL, NamespaceId, NamespacePermission, NamespaceScope,
    ProjectionFilter,
};
pub use provenance::{ProvenanceAction, ProvenanceHop, ProvenanceOrigin, ProvenanceRecord};
