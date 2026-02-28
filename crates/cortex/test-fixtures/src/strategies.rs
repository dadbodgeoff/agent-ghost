//! Proptest strategy library for GHOST platform correctness properties.
//!
//! Provides concrete strategies for all domain types used in property tests
//! across the workspace. Each strategy generates valid instances suitable
//! for round-trip, invariant, and adversarial testing.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
use cortex_core::models::proposal::ProposalOperation;
use cortex_core::safety::trigger::{ExfilType, TriggerEvent};
use cortex_core::traits::convergence::{CallerType, Proposal};
use cortex_temporal::hash_chain::{compute_event_hash, ChainEvent, GENESIS_HASH};
use proptest::prelude::*;
use uuid::Uuid;

// ── Primitive strategies ────────────────────────────────────────────────

/// All MemoryType variants.
pub fn memory_type_strategy() -> impl Strategy<Value = MemoryType> {
    prop_oneof![
        Just(MemoryType::Core),
        Just(MemoryType::Tribal),
        Just(MemoryType::Procedural),
        Just(MemoryType::Semantic),
        Just(MemoryType::Episodic),
        Just(MemoryType::Decision),
        Just(MemoryType::Insight),
        Just(MemoryType::Reference),
        Just(MemoryType::Preference),
        Just(MemoryType::Conversation),
        Just(MemoryType::Feedback),
        Just(MemoryType::Skill),
        Just(MemoryType::Goal),
        Just(MemoryType::Relationship),
        Just(MemoryType::Context),
        Just(MemoryType::Observation),
        Just(MemoryType::Hypothesis),
        Just(MemoryType::Experiment),
        Just(MemoryType::Lesson),
        Just(MemoryType::PatternRationale),
        Just(MemoryType::ConstraintOverride),
        Just(MemoryType::DecisionContext),
        Just(MemoryType::CodeSmell),
        Just(MemoryType::AgentGoal),
        Just(MemoryType::AgentReflection),
        Just(MemoryType::ConvergenceEvent),
        Just(MemoryType::BoundaryViolation),
        Just(MemoryType::ProposalRecord),
        Just(MemoryType::SimulationResult),
        Just(MemoryType::InterventionPlan),
        Just(MemoryType::AttachmentIndicator),
    ]
}

/// All Importance variants.
pub fn importance_strategy() -> impl Strategy<Value = Importance> {
    prop_oneof![
        Just(Importance::Trivial),
        Just(Importance::Low),
        Just(Importance::Normal),
        Just(Importance::High),
        Just(Importance::Critical),
    ]
}

/// Convergence score in [0.0, 1.0].
pub fn convergence_score_strategy() -> impl Strategy<Value = f64> {
    (0u32..=10000).prop_map(|v| v as f64 / 10000.0)
}

/// 7 signals each in [0.0, 1.0].
pub fn signal_array_strategy() -> impl Strategy<Value = [f64; 7]> {
    [
        convergence_score_strategy(),
        convergence_score_strategy(),
        convergence_score_strategy(),
        convergence_score_strategy(),
        convergence_score_strategy(),
        convergence_score_strategy(),
        convergence_score_strategy(),
    ]
}

// ── Composite strategies ────────────────────────────────────────────────

/// Random valid UUID (v4).
fn uuid_strategy() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(|bytes| Uuid::from_bytes(bytes))
}

/// Random DateTime<Utc> within a reasonable range (2024–2027).
fn datetime_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    (1704067200i64..1798761600i64).prop_map(|ts| Utc.timestamp_opt(ts, 0).unwrap())
}

/// Random event chain with valid hash linkage.
///
/// Each event's hash is correctly computed from its fields and the
/// previous event's hash, starting from GENESIS_HASH.
pub fn event_chain_strategy(
    min_len: usize,
    max_len: usize,
) -> impl Strategy<Value = Vec<ChainEvent>> {
    proptest::collection::vec(
        (
            "[a-z]{3,10}",       // event_type
            "[a-zA-Z0-9]{5,50}", // delta_json
            "[a-z]{3,8}",        // actor_id
        ),
        min_len..=max_len,
    )
    .prop_map(|raw_events| {
        let mut chain = Vec::with_capacity(raw_events.len());
        let mut prev_hash = GENESIS_HASH;

        for (event_type, delta_json, actor_id) in raw_events {
            let recorded_at = Utc::now().to_rfc3339();
            let event_hash =
                compute_event_hash(&event_type, &delta_json, &actor_id, &recorded_at, &prev_hash);

            chain.push(ChainEvent {
                event_type,
                delta_json,
                actor_id,
                recorded_at,
                event_hash,
                previous_hash: prev_hash,
            });

            prev_hash = event_hash;
        }

        chain
    })
}

/// Convergence score trajectory (sequence of scores for escalation/de-escalation testing).
pub fn convergence_trajectory_strategy() -> impl Strategy<Value = Vec<f64>> {
    proptest::collection::vec(convergence_score_strategy(), 1..100)
}

/// Random Proposal with valid UUIDv7, CallerType, and content.
pub fn proposal_strategy() -> impl Strategy<Value = Proposal> {
    (
        uuid_strategy(),
        caller_type_strategy(),
        proposal_operation_strategy(),
        memory_type_strategy(),
        uuid_strategy(),
        datetime_strategy(),
        proptest::collection::vec(uuid_strategy(), 0..5),
    )
        .prop_map(
            |(id, proposer, operation, target_type, session_id, timestamp, cited)| Proposal {
                id,
                proposer,
                operation,
                target_type,
                content: serde_json::json!({"test": true}),
                cited_memory_ids: cited,
                session_id,
                timestamp,
            },
        )
}

/// CallerType strategy.
pub fn caller_type_strategy() -> impl Strategy<Value = CallerType> {
    prop_oneof![
        Just(CallerType::Platform),
        uuid_strategy().prop_map(|id| CallerType::Agent { agent_id: id }),
        "[a-z]{5,10}".prop_map(|id| CallerType::Human { user_id: id }),
    ]
}

/// ProposalOperation strategy.
pub fn proposal_operation_strategy() -> impl Strategy<Value = ProposalOperation> {
    prop_oneof![
        Just(ProposalOperation::GoalChange),
        Just(ProposalOperation::ReflectionWrite),
        Just(ProposalOperation::MemoryWrite),
        Just(ProposalOperation::MemoryDelete),
    ]
}

/// All 10 TriggerEvent variants with random payloads.
pub fn trigger_event_strategy() -> impl Strategy<Value = TriggerEvent> {
    prop_oneof![
        // T1: SoulDrift
        (uuid_strategy(), convergence_score_strategy(), datetime_strategy()).prop_map(
            |(agent_id, drift_score, detected_at)| TriggerEvent::SoulDrift {
                agent_id,
                drift_score,
                threshold: 0.25,
                baseline_hash: "baseline".into(),
                current_hash: "current".into(),
                detected_at,
            }
        ),
        // T2: SpendingCapExceeded
        // Use simple integer values to ensure JSON round-trip fidelity.
        (uuid_strategy(), 0u32..1000, datetime_strategy()).prop_map(
            |(agent_id, total_int, detected_at)| {
                let total = total_int as f64;
                let cap = 50.0;
                let overage = if total > cap { total - cap } else { 0.0 };
                TriggerEvent::SpendingCapExceeded {
                    agent_id,
                    daily_total: total,
                    cap,
                    overage,
                    detected_at,
                }
            }
        ),
        // T3: PolicyDenialThreshold
        (uuid_strategy(), uuid_strategy(), 5u32..20, datetime_strategy()).prop_map(
            |(agent_id, session_id, count, detected_at)| TriggerEvent::PolicyDenialThreshold {
                agent_id,
                session_id,
                denial_count: count,
                denied_tools: vec!["tool_a".into()],
                denied_reasons: vec!["policy".into()],
                detected_at,
            }
        ),
        // T4: SandboxEscape
        (uuid_strategy(), datetime_strategy()).prop_map(|(agent_id, detected_at)| {
            TriggerEvent::SandboxEscape {
                agent_id,
                skill_name: "malicious_skill".into(),
                escape_attempt: "fs_write".into(),
                detected_at,
            }
        }),
        // T5: CredentialExfiltration
        (uuid_strategy(), datetime_strategy()).prop_map(|(agent_id, detected_at)| {
            TriggerEvent::CredentialExfiltration {
                agent_id,
                skill_name: Some("leaky_skill".into()),
                exfil_type: ExfilType::OutputLeakage,
                credential_id: "cred-001".into(),
                detected_at,
            }
        }),
        // T6: MultiAgentQuarantine
        (
            proptest::collection::vec(uuid_strategy(), 3..6),
            datetime_strategy()
        )
            .prop_map(|(agents, detected_at)| {
                let count = agents.len();
                TriggerEvent::MultiAgentQuarantine {
                    quarantined_agents: agents,
                    quarantine_reasons: vec!["reason".into()],
                    count,
                    threshold: 3,
                    detected_at,
                }
            }),
        // T7: MemoryHealthCritical
        (uuid_strategy(), convergence_score_strategy(), datetime_strategy()).prop_map(
            |(agent_id, score, detected_at)| TriggerEvent::MemoryHealthCritical {
                agent_id,
                health_score: score,
                threshold: 0.3,
                sub_scores: BTreeMap::new(),
                detected_at,
            }
        ),
        // Manual triggers
        (uuid_strategy()).prop_map(|agent_id| TriggerEvent::ManualPause {
            agent_id,
            reason: "test".into(),
            initiated_by: "owner".into(),
        }),
        (uuid_strategy()).prop_map(|agent_id| TriggerEvent::ManualQuarantine {
            agent_id,
            reason: "test".into(),
            initiated_by: "owner".into(),
        }),
        Just(TriggerEvent::ManualKillAll {
            reason: "test".into(),
            initiated_by: "owner".into(),
        }),
    ]
}

/// Random BaseMemory for snapshot/filtering tests.
pub fn base_memory_strategy() -> impl Strategy<Value = BaseMemory> {
    (
        uuid_strategy(),
        memory_type_strategy(),
        importance_strategy(),
        convergence_score_strategy(),
        datetime_strategy(),
    )
        .prop_map(|(id, memory_type, importance, confidence, created_at)| BaseMemory {
            id,
            memory_type,
            content: serde_json::json!({"data": "test"}),
            summary: "test memory".into(),
            importance,
            confidence,
            created_at,
            last_accessed: None,
            access_count: 0,
            tags: vec![],
            archived: false,
        })
}

/// Random session history (Vec of message-like tuples) for compaction tests.
pub fn session_history_strategy(
    min_len: usize,
    max_len: usize,
) -> impl Strategy<Value = Vec<(String, String, usize)>> {
    proptest::collection::vec(
        (
            prop_oneof![Just("human".to_string()), Just("agent".to_string())],
            "[a-zA-Z0-9 ]{10,200}",
            10usize..500,
        ),
        min_len..=max_len,
    )
}

/// Random KillSwitchState-like structure for persistence roundtrip tests.
pub fn kill_state_strategy() -> impl Strategy<Value = (u8, BTreeMap<String, u8>)> {
    (
        0u8..4,
        proptest::collection::btree_map("[a-f0-9]{8}", 0u8..4, 0..5),
    )
}

/// Random gateway state transition sequences for FSM validation.
pub fn gateway_state_transition_strategy() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(0u8..6, 1..20)
}
