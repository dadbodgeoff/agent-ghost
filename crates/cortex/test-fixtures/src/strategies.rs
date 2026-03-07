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

/// 8 signals each in [0.0, 1.0].
pub fn signal_array_strategy() -> impl Strategy<Value = [f64; 8]> {
    [
        convergence_score_strategy(),
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
            let event_hash = compute_event_hash(
                &event_type,
                &delta_json,
                &actor_id,
                &recorded_at,
                &prev_hash,
            );

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

/// All 11 TriggerEvent variants with random payloads.
pub fn trigger_event_strategy() -> impl Strategy<Value = TriggerEvent> {
    prop_oneof![
        // T1: SoulDrift
        (
            uuid_strategy(),
            convergence_score_strategy(),
            datetime_strategy()
        )
            .prop_map(
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
        (uuid_strategy(), 0u32..1000, datetime_strategy()).prop_map(
            |(agent_id, total_cents, detected_at)| {
                let total = total_cents as f64;
                TriggerEvent::SpendingCapExceeded {
                    agent_id,
                    daily_total: total,
                    cap: 50.0,
                    overage: (total - 50.0).max(0.0),
                    detected_at,
                }
            }
        ),
        // T3: PolicyDenialThreshold
        (
            uuid_strategy(),
            uuid_strategy(),
            5u32..20,
            datetime_strategy()
        )
            .prop_map(|(agent_id, session_id, count, detected_at)| {
                TriggerEvent::PolicyDenialThreshold {
                    agent_id,
                    session_id,
                    denial_count: count,
                    denied_tools: vec!["tool_a".into()],
                    denied_reasons: vec!["policy".into()],
                    detected_at,
                }
            }),
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
        (
            uuid_strategy(),
            convergence_score_strategy(),
            datetime_strategy()
        )
            .prop_map(|(agent_id, score, detected_at)| {
                TriggerEvent::MemoryHealthCritical {
                    agent_id,
                    health_score: score,
                    threshold: 0.3,
                    sub_scores: BTreeMap::new(),
                    detected_at,
                }
            }),
        // T8: NetworkEgressViolation (Phase 11)
        (uuid_strategy(), 1u32..20, datetime_strategy()).prop_map(
            |(agent_id, count, detected_at)| TriggerEvent::NetworkEgressViolation {
                agent_id,
                domain: "evil.example.com".into(),
                policy_mode: "allowlist".into(),
                violation_count: count,
                threshold: 5,
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
        .prop_map(
            |(id, memory_type, importance, confidence, created_at)| BaseMemory {
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
            },
        )
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

// ── Post-v1 strategies (Phase 15) ───────────────────────────────────────

use ghost_egress::config::{AgentEgressConfig, EgressPolicyMode};
use ghost_mesh::trust::local_trust::InteractionOutcome;
use ghost_mesh::types::{AgentCard, MeshTask, TaskStatus};
use ghost_oauth::types::OAuthRefId;
use secrecy::SecretString;

/// Random AgentEgressConfig with valid domain patterns.
pub fn egress_config_strategy() -> impl Strategy<Value = AgentEgressConfig> {
    (
        egress_policy_mode_strategy(),
        proptest::collection::vec(domain_pattern_strategy(), 0..8),
        proptest::collection::vec(domain_pattern_strategy(), 0..4),
        proptest::bool::ANY,
        proptest::bool::ANY,
        1u32..20,
        1u32..60,
    )
        .prop_map(
            |(policy, allowed, blocked, log_violations, alert, threshold, window)| {
                AgentEgressConfig {
                    policy,
                    allowed_domains: allowed,
                    blocked_domains: blocked,
                    log_violations,
                    alert_on_violation: alert,
                    violation_threshold: threshold,
                    violation_window_minutes: window,
                }
            },
        )
}

/// Random EgressPolicyMode.
fn egress_policy_mode_strategy() -> impl Strategy<Value = EgressPolicyMode> {
    prop_oneof![
        Just(EgressPolicyMode::Allowlist),
        Just(EgressPolicyMode::Blocklist),
        Just(EgressPolicyMode::Unrestricted),
    ]
}

/// Random domain string or wildcard pattern.
pub fn domain_pattern_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{3,10}\\.[a-z]{2,4}".prop_map(|s| s),
        "[a-z]{3,8}\\.[a-z]{3,8}\\.[a-z]{2,4}".prop_map(|s| s),
        "[a-z]{3,10}\\.[a-z]{2,4}".prop_map(|s| format!("*.{s}")),
    ]
}

/// Random OAuthRefId (UUID-based).
pub fn oauth_ref_id_strategy() -> impl Strategy<Value = OAuthRefId> {
    uuid_strategy().prop_map(OAuthRefId::from_uuid)
}

/// Random TokenSet with valid expiry and scopes.
///
/// Note: TokenSet contains `SecretString` which is not `Arbitrary`.
/// We generate deterministic test tokens (never real credentials).
pub fn token_set_strategy() -> impl Strategy<Value = ghost_oauth::types::TokenSet> {
    (
        "[a-zA-Z0-9]{32,64}",                            // access_token
        proptest::option::of("[a-zA-Z0-9]{32,64}"),      // refresh_token
        datetime_strategy(),                             // expires_at
        proptest::collection::vec("[a-z.]{4,20}", 0..5), // scopes
    )
        .prop_map(
            |(access, refresh, expires_at, scopes)| ghost_oauth::types::TokenSet {
                access_token: SecretString::from(access),
                refresh_token: refresh.map(SecretString::from),
                expires_at,
                scopes,
            },
        )
}

/// Random AgentCard with valid Ed25519 signature.
pub fn agent_card_strategy() -> impl Strategy<Value = AgentCard> {
    (
        "[a-z]{3,12}",                                                // name
        "[a-zA-Z0-9 ]{10,50}",                                        // description
        proptest::collection::vec("[a-z_]{3,15}", 1..5),              // capabilities
        proptest::collection::vec("[a-z/]{5,20}", 0..3),              // input_types
        proptest::collection::vec("[a-z/]{5,20}", 0..3),              // output_types
        "[a-z]{5,15}".prop_map(|s| format!("http://{s}.local:8080")), // endpoint_url
        prop_oneof![
            Just("standard".to_string()),
            Just("research".to_string()),
            Just("companion".to_string()),
        ],
        convergence_score_strategy(), // trust_score
        "[a-f0-9]{16}",               // sybil_lineage_hash
        "[0-9]\\.[0-9]\\.[0-9]",      // version
        datetime_strategy(),          // signed_at
    )
        .prop_map(
            |(
                name,
                description,
                capabilities,
                input_types,
                output_types,
                endpoint_url,
                profile,
                trust_score,
                lineage,
                version,
                signed_at,
            )| {
                let (signing_key, _) = ghost_signing::generate_keypair();
                let vk = signing_key.verifying_key();
                let public_key = vk.to_bytes().to_vec();

                let mut card = AgentCard {
                    name,
                    description,
                    capabilities,
                    capability_flags: 0,
                    input_types,
                    output_types,
                    auth_schemes: vec!["bearer".to_string()],
                    endpoint_url,
                    public_key,
                    convergence_profile: profile,
                    trust_score,
                    sybil_lineage_hash: lineage,
                    version,
                    signed_at,
                    signature: Vec::new(),
                    supported_task_types: Vec::new(),
                    default_input_modes: Vec::new(),
                    default_output_modes: Vec::new(),
                    provider: String::new(),
                    a2a_protocol_version: String::new(),
                };
                card.sign(&signing_key);
                card
            },
        )
}

/// Random MeshTask with valid status.
pub fn mesh_task_strategy() -> impl Strategy<Value = MeshTask> {
    (
        uuid_strategy(), // initiator
        uuid_strategy(), // target
        datetime_strategy(),
        datetime_strategy(),
        0u64..3600, // timeout
        0u32..4,    // delegation_depth
    )
        .prop_map(
            |(initiator, target, created, updated, timeout, depth)| MeshTask {
                id: Uuid::new_v4(),
                initiator_agent_id: initiator,
                target_agent_id: target,
                status: TaskStatus::Submitted,
                input: serde_json::json!({"task": "test"}),
                output: None,
                created_at: created,
                updated_at: updated,
                timeout,
                delegation_depth: depth,
                metadata: BTreeMap::new(),
            },
        )
}

/// Random InteractionOutcome.
pub fn interaction_outcome_strategy() -> impl Strategy<Value = InteractionOutcome> {
    prop_oneof![
        Just(InteractionOutcome::TaskCompleted),
        Just(InteractionOutcome::TaskFailed),
        Just(InteractionOutcome::PolicyViolation),
        Just(InteractionOutcome::SignatureFailure),
        Just(InteractionOutcome::Timeout),
    ]
}

/// Random ToolCallPlan (sequence of tool calls).
pub fn tool_call_plan_strategy(
) -> impl Strategy<Value = ghost_agent_loop::tools::plan_validator::ToolCallPlan> {
    proptest::collection::vec(
        (
            "[a-z_]{3,15}", // id
            "[a-z_]{3,20}", // name
        ),
        0..8,
    )
    .prop_map(|calls| {
        let llm_calls: Vec<ghost_llm::provider::LLMToolCall> = calls
            .into_iter()
            .map(|(id, name)| ghost_llm::provider::LLMToolCall {
                id,
                name,
                arguments: serde_json::json!({}),
            })
            .collect();
        ghost_agent_loop::tools::plan_validator::ToolCallPlan::new(llm_calls)
    })
}

/// Random SpotlightingConfig.
pub fn spotlighting_config_strategy(
) -> impl Strategy<Value = ghost_agent_loop::context::spotlighting::SpotlightingConfig> {
    use ghost_agent_loop::context::spotlighting::{SpotlightMode, SpotlightingConfig};

    (
        proptest::bool::ANY,
        prop_oneof![Just('^'), Just('~'), Just('|'), Just('#')],
        proptest::collection::vec(0u8..10, 0..4),
        prop_oneof![
            Just(SpotlightMode::Datamarking),
            Just(SpotlightMode::Delimiting),
            Just(SpotlightMode::Off),
        ],
    )
        .prop_map(|(enabled, marker, layers, mode)| SpotlightingConfig {
            enabled,
            marker,
            layers,
            mode,
        })
}

/// 8 signals each in [0.0, 1.0] (updated from 7 for post-v1 behavioral anomaly S8).
pub fn signal_array_8_strategy() -> impl Strategy<Value = [f64; 8]> {
    signal_array_strategy()
}

/// Random local trust values for N agents (trust matrix).
pub fn trust_matrix_strategy() -> impl Strategy<Value = BTreeMap<(Uuid, Uuid), f64>> {
    proptest::collection::vec(
        (
            uuid_strategy(),
            uuid_strategy(),
            convergence_score_strategy(),
        ),
        0..20,
    )
    .prop_map(|entries| {
        entries
            .into_iter()
            .filter(|(a, b, _)| a != b) // No self-trust.
            .map(|(a, b, v)| ((a, b), v))
            .collect()
    })
}
