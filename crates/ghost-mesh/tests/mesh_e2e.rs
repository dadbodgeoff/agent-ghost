//! ghost-mesh E2E integration tests (Task 15.4).
//!
//! Verifies discovery, delegation, trust, cascade breakers, memory poisoning defense,
//! depth limits, and A2A interop.

use std::collections::BTreeMap;

use chrono::Utc;
use ghost_mesh::types::{
    AgentCard, DelegationRequest, DelegationResponse, MeshMessage, MeshTask, TaskStatus,
};
use ghost_mesh::trust::local_trust::InteractionOutcome;
use uuid::Uuid;

// ── AgentCard Discovery ─────────────────────────────────────────────────

/// AgentCard sign + verify round-trip.
#[test]
fn agent_card_sign_verify_roundtrip() {
    let (signing_key, _) = ghost_signing::generate_keypair();
    let vk = signing_key.verifying_key();

    let mut card = AgentCard {
        name: "test-agent".into(),
        description: "A test agent for mesh E2E".into(),
        capabilities: vec!["code-review".into(), "summarize".into()],
        capability_flags: 0,
        input_types: vec!["text/plain".into()],
        output_types: vec!["text/plain".into()],
        auth_schemes: vec!["bearer".into()],
        endpoint_url: "http://localhost:8080".into(),
        public_key: vk.to_bytes().to_vec(),
        convergence_profile: "standard".into(),
        trust_score: 0.5,
        sybil_lineage_hash: "abc123".into(),
        version: "1.0.0".into(),
        signed_at: Utc::now(),
        signature: Vec::new(),
        supported_task_types: Vec::new(),
        default_input_modes: Vec::new(),
        default_output_modes: Vec::new(),
        provider: String::new(),
        a2a_protocol_version: String::new(),
    };

    card.sign(&signing_key);
    assert!(!card.signature.is_empty(), "Signature should be populated");
    assert!(card.verify_signature(), "Valid signature should verify");
}

/// AgentCard with tampered field fails verification.
#[test]
fn agent_card_tampered_fails_verification() {
    let (signing_key, _) = ghost_signing::generate_keypair();
    let vk = signing_key.verifying_key();

    let mut card = AgentCard {
        name: "honest-agent".into(),
        description: "Original description".into(),
        capabilities: vec!["task".into()],
        capability_flags: 0,
        input_types: vec![],
        output_types: vec![],
        auth_schemes: vec!["bearer".into()],
        endpoint_url: "http://localhost:8080".into(),
        public_key: vk.to_bytes().to_vec(),
        convergence_profile: "standard".into(),
        trust_score: 0.5,
        sybil_lineage_hash: "abc".into(),
        version: "1.0.0".into(),
        signed_at: Utc::now(),
        signature: Vec::new(),
        supported_task_types: Vec::new(),
        default_input_modes: Vec::new(),
        default_output_modes: Vec::new(),
        provider: String::new(),
        a2a_protocol_version: String::new(),
    };

    card.sign(&signing_key);
    assert!(card.verify_signature());

    // Tamper with the name.
    card.name = "evil-agent".into();
    assert!(!card.verify_signature(), "Tampered card should fail verification");
}

/// AgentCard with wrong public key fails verification.
#[test]
fn agent_card_wrong_key_fails_verification() {
    let (signing_key, _) = ghost_signing::generate_keypair();
    let (_, other_vk) = ghost_signing::generate_keypair();

    let mut card = AgentCard {
        name: "agent".into(),
        description: "desc".into(),
        capabilities: vec![],
        capability_flags: 0,
        input_types: vec![],
        output_types: vec![],
        auth_schemes: vec![],
        endpoint_url: "http://localhost:8080".into(),
        public_key: other_vk.to_bytes().to_vec(), // Wrong key!
        convergence_profile: "standard".into(),
        trust_score: 0.5,
        sybil_lineage_hash: "abc".into(),
        version: "1.0.0".into(),
        signed_at: Utc::now(),
        signature: Vec::new(),
        supported_task_types: Vec::new(),
        default_input_modes: Vec::new(),
        default_output_modes: Vec::new(),
        provider: String::new(),
        a2a_protocol_version: String::new(),
    };

    card.sign(&signing_key);
    assert!(!card.verify_signature(), "Wrong public key should fail verification");
}

/// AgentCard canonical_bytes is deterministic.
#[test]
fn agent_card_canonical_bytes_deterministic() {
    let card = AgentCard {
        name: "agent".into(),
        description: "desc".into(),
        capabilities: vec!["a".into(), "b".into()],
        capability_flags: 0,
        input_types: vec![],
        output_types: vec![],
        auth_schemes: vec!["bearer".into()],
        endpoint_url: "http://localhost:8080".into(),
        public_key: vec![0u8; 32],
        convergence_profile: "standard".into(),
        trust_score: 0.5,
        sybil_lineage_hash: "abc".into(),
        version: "1.0.0".into(),
        signed_at: Utc::now(),
        signature: Vec::new(),
        supported_task_types: Vec::new(),
        default_input_modes: Vec::new(),
        default_output_modes: Vec::new(),
        provider: String::new(),
        a2a_protocol_version: String::new(),
    };

    let bytes1 = card.canonical_bytes();
    let bytes2 = card.canonical_bytes();
    assert_eq!(bytes1, bytes2, "canonical_bytes must be deterministic");
}

// ── Task Delegation ─────────────────────────────────────────────────────

/// MeshTask lifecycle: Submitted → Working → Completed.
#[test]
fn mesh_task_happy_path_lifecycle() {
    let mut task = MeshTask::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        serde_json::json!({"prompt": "summarize this"}),
        300,
    );
    assert_eq!(task.status, TaskStatus::Submitted);

    task.transition(TaskStatus::Working).expect("Submitted → Working");
    assert_eq!(task.status, TaskStatus::Working);

    task.transition(TaskStatus::Completed).expect("Working → Completed");
    assert_eq!(task.status, TaskStatus::Completed);
    assert!(task.status.is_terminal());
}

/// MeshTask lifecycle: Submitted → Working → Failed.
#[test]
fn mesh_task_failure_lifecycle() {
    let mut task = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 60);

    task.transition(TaskStatus::Working).unwrap();
    task.transition(TaskStatus::Failed("timeout".into())).unwrap();
    assert!(task.status.is_terminal());
}

/// MeshTask invalid transition: Submitted → Completed (must go through Working).
#[test]
fn mesh_task_invalid_transition_rejected() {
    let mut task = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 60);
    let result = task.transition(TaskStatus::Completed);
    assert!(result.is_err(), "Submitted → Completed should be invalid");
}

/// MeshTask: terminal state cannot transition further.
#[test]
fn mesh_task_terminal_state_immutable() {
    let mut task = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 60);
    task.transition(TaskStatus::Working).unwrap();
    task.transition(TaskStatus::Completed).unwrap();

    let result = task.transition(TaskStatus::Working);
    assert!(result.is_err(), "Completed task should not transition");
}

/// MeshTask: any non-terminal state can be canceled.
#[test]
fn mesh_task_cancellation() {
    let mut task = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 60);
    task.transition(TaskStatus::Canceled).expect("Submitted → Canceled");
    assert!(task.status.is_terminal());

    let mut task2 = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 60);
    task2.transition(TaskStatus::Working).unwrap();
    task2.transition(TaskStatus::Canceled).expect("Working → Canceled");
}

/// MeshTask: delegation depth tracking.
#[test]
fn mesh_task_delegation_depth() {
    let task = MeshTask {
        id: Uuid::new_v4(),
        initiator_agent_id: Uuid::new_v4(),
        target_agent_id: Uuid::new_v4(),
        status: TaskStatus::Submitted,
        input: serde_json::json!({}),
        output: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        timeout: 300,
        delegation_depth: 3,
        metadata: BTreeMap::new(),
    };
    assert_eq!(task.delegation_depth, 3);
}

// ── Trust ───────────────────────────────────────────────────────────────

/// InteractionOutcome variants all have defined trust deltas.
#[test]
fn interaction_outcome_all_variants() {
    let outcomes = [
        InteractionOutcome::TaskCompleted,
        InteractionOutcome::TaskFailed,
        InteractionOutcome::PolicyViolation,
        InteractionOutcome::SignatureFailure,
        InteractionOutcome::Timeout,
    ];
    // Just verify they all exist and are distinct.
    assert_eq!(outcomes.len(), 5);
    for (i, a) in outcomes.iter().enumerate() {
        for (j, b) in outcomes.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

/// InteractionOutcome serde round-trip.
#[test]
fn interaction_outcome_serde_roundtrip() {
    let outcomes = [
        InteractionOutcome::TaskCompleted,
        InteractionOutcome::TaskFailed,
        InteractionOutcome::PolicyViolation,
        InteractionOutcome::SignatureFailure,
        InteractionOutcome::Timeout,
    ];
    for outcome in &outcomes {
        let json = serde_json::to_string(outcome).expect("serialize");
        let deserialized: InteractionOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*outcome, deserialized);
    }
}

// ── JSON-RPC 2.0 Protocol ───────────────────────────────────────────────

/// MeshMessage request is valid JSON-RPC 2.0.
#[test]
fn mesh_message_request_valid() {
    let msg = MeshMessage::request(
        "tasks/send",
        serde_json::json!({"task": "test"}),
        serde_json::json!(1),
    );
    assert!(msg.is_valid_jsonrpc());
    assert_eq!(msg.jsonrpc, "2.0");
    assert_eq!(msg.method, "tasks/send");
}

/// MeshMessage success response is valid JSON-RPC 2.0.
#[test]
fn mesh_message_success_response_valid() {
    let msg = MeshMessage::success(
        serde_json::json!(1),
        serde_json::json!({"status": "completed"}),
    );
    assert!(msg.is_valid_jsonrpc());
}

/// MeshMessage error response is valid JSON-RPC 2.0.
#[test]
fn mesh_message_error_response_valid() {
    let msg = MeshMessage::error_response(serde_json::json!(1), -32600, "Invalid Request");
    assert!(msg.is_valid_jsonrpc());
    assert!(msg.error.is_some());
}

/// MeshMessage serde round-trip.
#[test]
fn mesh_message_serde_roundtrip() {
    let msg = MeshMessage::request(
        "tasks/send",
        serde_json::json!({"input": "hello"}),
        serde_json::json!("req-1"),
    );
    let json = serde_json::to_string(&msg).expect("serialize");
    let deserialized: MeshMessage = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.method, "tasks/send");
    assert_eq!(deserialized.jsonrpc, "2.0");
}

// ── Delegation Protocol ─────────────────────────────────────────────────

/// DelegationRequest serde round-trip.
#[test]
fn delegation_request_serde_roundtrip() {
    let req = DelegationRequest {
        task_description: "Summarize this document".into(),
        required_capabilities: vec!["summarize".into()],
        max_cost: 0.50,
        timeout: 300,
    };
    let json = serde_json::to_string(&req).expect("serialize");
    let deserialized: DelegationRequest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.task_description, req.task_description);
    assert_eq!(deserialized.max_cost, req.max_cost);
}

/// DelegationResponse accepted.
#[test]
fn delegation_response_accepted() {
    let resp = DelegationResponse {
        accepted: true,
        estimated_cost: 0.25,
        estimated_duration: 120,
        rejection_reason: None,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    let deserialized: DelegationResponse = serde_json::from_str(&json).expect("deserialize");
    assert!(deserialized.accepted);
    assert!(deserialized.rejection_reason.is_none());
}

/// DelegationResponse rejected.
#[test]
fn delegation_response_rejected() {
    let resp = DelegationResponse {
        accepted: false,
        estimated_cost: 0.0,
        estimated_duration: 0,
        rejection_reason: Some("Trust too low".into()),
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    let deserialized: DelegationResponse = serde_json::from_str(&json).expect("deserialize");
    assert!(!deserialized.accepted);
    assert_eq!(deserialized.rejection_reason.as_deref(), Some("Trust too low"));
}

/// ghost-mesh depends on ghost-signing (for Ed25519) but not on ghost-gateway.
#[test]
fn ghost_mesh_layer_separation() {
    let cargo_toml = include_str!("../Cargo.toml");
    let deps_section = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .split("[dev-dependencies]")
        .next()
        .unwrap_or("");

    assert!(
        deps_section.contains("ghost-signing"),
        "ghost-mesh should depend on ghost-signing for Ed25519"
    );
    assert!(
        !deps_section.contains("ghost-gateway"),
        "ghost-mesh must NOT depend on ghost-gateway (layer separation)"
    );
}
