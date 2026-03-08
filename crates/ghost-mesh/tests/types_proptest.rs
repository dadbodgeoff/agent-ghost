//! Property tests for Task 14.1 — ghost-mesh core types.

use chrono::Utc;
use ghost_mesh::types::*;
use proptest::prelude::*;
use uuid::Uuid;

// ── Strategies ──────────────────────────────────────────────────────────

fn arbitrary_uuid() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(Uuid::from_bytes)
}

fn arbitrary_task_status() -> impl Strategy<Value = TaskStatus> {
    prop_oneof![
        Just(TaskStatus::Submitted),
        Just(TaskStatus::Working),
        ".*".prop_map(TaskStatus::InputRequired),
        Just(TaskStatus::Completed),
        ".*".prop_map(TaskStatus::Failed),
        Just(TaskStatus::Canceled),
    ]
}

fn arbitrary_agent_card() -> impl Strategy<Value = AgentCard> {
    (
        "[-a-z0-9]{1,20}",
        ".*",
        prop::collection::vec("[-a-z]{1,10}", 0..5),
        "[-a-z/]{1,20}",
    )
        .prop_map(|(name, desc, caps, endpoint)| {
            let (sk, vk) = ghost_signing::generate_keypair();
            let mut card = AgentCard {
                name,
                description: desc,
                capabilities: caps,
                capability_flags: 0,
                input_types: vec!["text/plain".to_string()],
                output_types: vec!["application/json".to_string()],
                auth_schemes: vec!["ed25519".to_string()],
                endpoint_url: format!("http://127.0.0.1:18789/{endpoint}"),
                public_key: vk.to_bytes().to_vec(),
                convergence_profile: "standard".to_string(),
                trust_score: 0.5,
                sybil_lineage_hash: "test".to_string(),
                version: "1.0.0".to_string(),
                signed_at: Utc::now(),
                signature: vec![],
                supported_task_types: Vec::new(),
                default_input_modes: Vec::new(),
                default_output_modes: Vec::new(),
                provider: String::new(),
                a2a_protocol_version: String::new(),
            };
            card.sign(&sk);
            card
        })
}

fn arbitrary_mesh_task() -> impl Strategy<Value = MeshTask> {
    (arbitrary_uuid(), arbitrary_uuid(), 0..3600u64).prop_map(|(initiator, target, timeout)| {
        MeshTask::new(
            initiator,
            target,
            serde_json::json!({"test": true}),
            timeout,
        )
    })
}

// ── Proptest: AgentCard sign-then-verify ────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn agent_card_sign_verify_round_trip(card in arbitrary_agent_card()) {
        prop_assert!(card.verify_signature(), "signed card must verify");
    }

    #[test]
    fn mesh_task_serde_round_trip(task in arbitrary_mesh_task()) {
        let json = serde_json::to_string(&task).unwrap();
        let deserialized: MeshTask = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(task.id, deserialized.id);
        prop_assert_eq!(task.initiator_agent_id, deserialized.initiator_agent_id);
        prop_assert_eq!(task.target_agent_id, deserialized.target_agent_id);
    }

    #[test]
    fn task_status_serde_round_trip(status in arbitrary_task_status()) {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(status, deserialized);
    }

    /// Task 22.2: For 500 random MeshTask pairs, delta round-trip produces identical task.
    #[test]
    fn delta_round_trip_preserves_task(
        task_a in arbitrary_mesh_task(),
        new_status in arbitrary_task_status(),
    ) {
        let mut task_b = task_a.clone();
        task_b.status = new_status;
        task_b.updated_at = Utc::now();

        let delta = task_b.compute_delta(&task_a);
        let mut reconstructed = task_a.clone();
        reconstructed.apply_delta(&delta);

        prop_assert_eq!(reconstructed.status, task_b.status);
        prop_assert_eq!(reconstructed.updated_at, task_b.updated_at);
    }

    /// Task 22.2: For 500 random capability sets, bitfield round-trip preserves all capabilities.
    #[test]
    fn capability_bitfield_round_trip(
        caps in prop::collection::vec(
            prop_oneof![
                Just("code_execution".to_string()),
                Just("web_search".to_string()),
                Just("file_operations".to_string()),
                Just("api_calls".to_string()),
                Just("data_analysis".to_string()),
                Just("image_generation".to_string()),
            ],
            0..6,
        ),
    ) {
        let flags = AgentCard::capabilities_from_strings(&caps);
        // Every capability in the input should be matched by the bitfield.
        for cap in &caps {
            let single_flag = AgentCard::capabilities_from_strings(std::slice::from_ref(cap));
            prop_assert!(
                (flags & single_flag) == single_flag,
                "capability '{}' not preserved in bitfield {:#b}",
                cap,
                flags,
            );
        }
    }
}
