//! Property-based tests for cortex-core (Task 1.2).
//!
//! Proptest: For 100 random Proposal structs, serialize then deserialize
//! produces an identical struct.

use chrono::{TimeZone, Utc};
use proptest::prelude::*;
use uuid::Uuid;

use cortex_core::memory::types::MemoryType;
use cortex_core::models::proposal::ProposalOperation;
use cortex_core::traits::convergence::{CallerType, Proposal};

// ── Strategies ──────────────────────────────────────────────────────────

fn arb_uuid() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(|b| Uuid::from_bytes(b))
}

fn arb_operation() -> impl Strategy<Value = ProposalOperation> {
    prop_oneof![
        Just(ProposalOperation::GoalChange),
        Just(ProposalOperation::ReflectionWrite),
        Just(ProposalOperation::MemoryWrite),
        Just(ProposalOperation::MemoryDelete),
    ]
}

fn arb_memory_type() -> impl Strategy<Value = MemoryType> {
    prop_oneof![
        Just(MemoryType::AgentGoal),
        Just(MemoryType::AgentReflection),
        Just(MemoryType::Conversation),
        Just(MemoryType::Feedback),
        Just(MemoryType::ProposalRecord),
        Just(MemoryType::Core),
        Just(MemoryType::Insight),
        Just(MemoryType::SimulationResult),
    ]
}

fn arb_caller_type() -> impl Strategy<Value = CallerType> {
    prop_oneof![
        Just(CallerType::Platform),
        arb_uuid().prop_map(|id| CallerType::Agent { agent_id: id }),
        "[a-z]{1,8}".prop_map(|s| CallerType::Human { user_id: s }),
    ]
}

fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::json!(null)),
        any::<bool>().prop_map(|b| serde_json::json!(b)),
        any::<i64>().prop_map(|n| serde_json::json!(n)),
        "[a-zA-Z0-9 ]{0,64}".prop_map(|s| serde_json::json!(s)),
    ]
}

fn arb_timestamp() -> impl Strategy<Value = chrono::DateTime<Utc>> {
    // Range: 2020-01-01 to 2030-01-01 (seconds since epoch).
    (1_577_836_800i64..1_893_456_000i64).prop_map(|secs| {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    })
}

fn arb_proposal() -> impl Strategy<Value = Proposal> {
    (
        arb_uuid(),
        arb_caller_type(),
        arb_operation(),
        arb_memory_type(),
        arb_json_value(),
        proptest::collection::vec(arb_uuid(), 0..5),
        arb_uuid(),
        arb_timestamp(),
    )
        .prop_map(
            |(id, proposer, operation, target_type, content, cited, session_id, timestamp)| {
                Proposal {
                    id,
                    proposer,
                    operation,
                    target_type,
                    content,
                    cited_memory_ids: cited,
                    session_id,
                    timestamp,
                }
            },
        )
}

// ── Property tests ──────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// For 100 random Proposal structs, serialize then deserialize produces
    /// an identical struct (field-by-field comparison).
    #[test]
    fn proposal_serde_round_trip(proposal in arb_proposal()) {
        let json = serde_json::to_string(&proposal).unwrap();
        let deserialized: Proposal = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(&proposal.id, &deserialized.id);
        prop_assert_eq!(&proposal.proposer, &deserialized.proposer);
        prop_assert_eq!(&proposal.operation, &deserialized.operation);
        prop_assert_eq!(&proposal.target_type, &deserialized.target_type);
        prop_assert_eq!(&proposal.content, &deserialized.content);
        prop_assert_eq!(&proposal.cited_memory_ids, &deserialized.cited_memory_ids);
        prop_assert_eq!(&proposal.session_id, &deserialized.session_id);
        prop_assert_eq!(&proposal.timestamp, &deserialized.timestamp);
    }

    /// CallerType::Agent can never create platform-restricted types,
    /// regardless of agent_id.
    #[test]
    fn agent_never_creates_restricted_types(agent_id in arb_uuid()) {
        let caller = CallerType::Agent { agent_id };
        let restricted = [
            MemoryType::Core,
            MemoryType::ConvergenceEvent,
            MemoryType::BoundaryViolation,
            MemoryType::InterventionPlan,
        ];
        for mt in &restricted {
            prop_assert!(!caller.can_create_type(mt));
        }
    }
}
