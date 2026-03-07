//! Tests for Task 14.1 — ghost-mesh core types.
//!
//! Covers: serde round-trips, AgentCard signing/verification,
//! TaskStatus transitions, MeshMessage JSON-RPC 2.0 conformance,
//! proptest invariants, and adversarial edge cases.

use chrono::Utc;
use ghost_mesh::error::MeshError;
use ghost_mesh::types::*;
use uuid::Uuid;

// ── Helper: build a test AgentCard ──────────────────────────────────────

fn make_agent_card(signing_key: &ghost_signing::SigningKey) -> AgentCard {
    let vk = signing_key.verifying_key();
    let mut card = AgentCard {
        name: "test-agent".to_string(),
        description: "A test agent for unit tests".to_string(),
        capabilities: vec!["code-review".to_string(), "testing".to_string()],
        capability_flags: 0,
        input_types: vec!["text/plain".to_string()],
        output_types: vec!["application/json".to_string()],
        auth_schemes: vec!["ed25519".to_string()],
        endpoint_url: "http://127.0.0.1:18789".to_string(),
        public_key: vk.to_bytes().to_vec(),
        convergence_profile: "standard".to_string(),
        trust_score: 0.5,
        sybil_lineage_hash: "abc123".to_string(),
        version: "1.0.0".to_string(),
        signed_at: Utc::now(),
        signature: vec![],
        supported_task_types: Vec::new(),
        default_input_modes: Vec::new(),
        default_output_modes: Vec::new(),
        provider: String::new(),
        a2a_protocol_version: String::new(),
    };
    card.sign(signing_key);
    card
}

// ── Serde round-trip tests ──────────────────────────────────────────────

#[test]
fn agent_card_serde_round_trip() {
    let (sk, _) = ghost_signing::generate_keypair();
    let card = make_agent_card(&sk);
    let json = serde_json::to_string(&card).unwrap();
    let deserialized: AgentCard = serde_json::from_str(&json).unwrap();
    assert_eq!(card.name, deserialized.name);
    assert_eq!(card.endpoint_url, deserialized.endpoint_url);
    assert_eq!(card.public_key, deserialized.public_key);
    assert_eq!(card.signature, deserialized.signature);
}

#[test]
fn mesh_task_serde_round_trip() {
    let task = MeshTask::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        serde_json::json!({"prompt": "hello"}),
        60,
    );
    let json = serde_json::to_string(&task).unwrap();
    let deserialized: MeshTask = serde_json::from_str(&json).unwrap();
    assert_eq!(task.id, deserialized.id);
    assert_eq!(task.input, deserialized.input);
}

#[test]
fn task_status_serde_round_trip() {
    let statuses = vec![
        TaskStatus::Submitted,
        TaskStatus::Working,
        TaskStatus::InputRequired("need more info".to_string()),
        TaskStatus::Completed,
        TaskStatus::Failed("timeout".to_string()),
        TaskStatus::Canceled,
    ];
    for status in statuses {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }
}

#[test]
fn mesh_message_serde_round_trip() {
    let msg = MeshMessage::request(
        "tasks/send",
        serde_json::json!({"task": "review code"}),
        serde_json::json!(1),
    );
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: MeshMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg.jsonrpc, deserialized.jsonrpc);
    assert_eq!(msg.method, deserialized.method);
}

#[test]
fn delegation_request_serde_round_trip() {
    let req = DelegationRequest {
        task_description: "Review PR #42".to_string(),
        required_capabilities: vec!["code-review".to_string()],
        max_cost: 0.5,
        timeout: 300,
    };
    let json = serde_json::to_string(&req).unwrap();
    let deserialized: DelegationRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.task_description, deserialized.task_description);
}

#[test]
fn delegation_response_serde_round_trip() {
    let resp = DelegationResponse {
        accepted: true,
        estimated_cost: 0.3,
        estimated_duration: 120,
        rejection_reason: None,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: DelegationResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.accepted, deserialized.accepted);
}

#[test]
fn payment_stubs_serde_round_trip() {
    let payment = MeshPayment {
        id: Uuid::new_v4(),
        from_agent: Uuid::new_v4(),
        to_agent: Uuid::new_v4(),
        amount: 1.5,
        currency: "USD".to_string(),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&payment).unwrap();
    let _: MeshPayment = serde_json::from_str(&json).unwrap();

    let invoice = MeshInvoice {
        id: Uuid::new_v4(),
        task_id: Uuid::new_v4(),
        amount: 1.5,
        currency: "USD".to_string(),
        issued_at: Utc::now(),
        due_at: Utc::now(),
    };
    let json = serde_json::to_string(&invoice).unwrap();
    let _: MeshInvoice = serde_json::from_str(&json).unwrap();

    let settlement = MeshSettlement {
        id: Uuid::new_v4(),
        invoice_id: Uuid::new_v4(),
        payment_id: Uuid::new_v4(),
        settled_at: Utc::now(),
    };
    let json = serde_json::to_string(&settlement).unwrap();
    let _: MeshSettlement = serde_json::from_str(&json).unwrap();
}

// ── AgentCard signature tests ───────────────────────────────────────────

#[test]
fn agent_card_signature_verification() {
    let (sk, _) = ghost_signing::generate_keypair();
    let card = make_agent_card(&sk);
    assert!(card.verify_signature(), "valid signature should verify");
}

#[test]
fn agent_card_tampered_fields_fail_verification() {
    let (sk, _) = ghost_signing::generate_keypair();
    let mut card = make_agent_card(&sk);
    // Tamper with the name after signing.
    card.name = "evil-agent".to_string();
    assert!(
        !card.verify_signature(),
        "tampered card should fail verification"
    );
}

#[test]
fn agent_card_wrong_key_fails_verification() {
    let (sk1, _) = ghost_signing::generate_keypair();
    let (_, vk2) = ghost_signing::generate_keypair();
    let mut card = make_agent_card(&sk1);
    // Replace public key with a different key.
    card.public_key = vk2.to_bytes().to_vec();
    assert!(
        !card.verify_signature(),
        "wrong public key should fail verification"
    );
}

#[test]
fn agent_card_empty_signature_fails() {
    let (sk, _) = ghost_signing::generate_keypair();
    let mut card = make_agent_card(&sk);
    card.signature = vec![];
    assert!(!card.verify_signature());
}

#[test]
fn agent_card_invalid_public_key_fails() {
    let (sk, _) = ghost_signing::generate_keypair();
    let mut card = make_agent_card(&sk);
    card.public_key = vec![0u8; 31]; // Wrong length.
    assert!(!card.verify_signature());
}

// ── TaskStatus transition tests ─────────────────────────────────────────

#[test]
fn valid_task_status_transitions() {
    // Submitted → Working
    assert!(TaskStatus::Submitted.can_transition_to(&TaskStatus::Working));
    // Working → Completed
    assert!(TaskStatus::Working.can_transition_to(&TaskStatus::Completed));
    // Working → Failed
    assert!(TaskStatus::Working.can_transition_to(&TaskStatus::Failed("err".into())));
    // Working → InputRequired
    assert!(TaskStatus::Working.can_transition_to(&TaskStatus::InputRequired("need info".into())));
    // InputRequired → Working
    assert!(TaskStatus::InputRequired("x".into()).can_transition_to(&TaskStatus::Working));
    // Any → Canceled
    assert!(TaskStatus::Submitted.can_transition_to(&TaskStatus::Canceled));
    assert!(TaskStatus::Working.can_transition_to(&TaskStatus::Canceled));
    assert!(TaskStatus::InputRequired("x".into()).can_transition_to(&TaskStatus::Canceled));
}

#[test]
fn invalid_task_status_transitions() {
    // Completed → Working (invalid)
    assert!(!TaskStatus::Completed.can_transition_to(&TaskStatus::Working));
    // Failed → Working (invalid)
    assert!(!TaskStatus::Failed("err".into()).can_transition_to(&TaskStatus::Working));
    // Canceled → Working (invalid)
    assert!(!TaskStatus::Canceled.can_transition_to(&TaskStatus::Working));
    // Completed → Submitted (invalid)
    assert!(!TaskStatus::Completed.can_transition_to(&TaskStatus::Submitted));
    // Submitted → Completed (must go through Working)
    assert!(!TaskStatus::Submitted.can_transition_to(&TaskStatus::Completed));
}

#[test]
fn task_status_transition_returns_error_on_invalid() {
    let result = TaskStatus::Completed.transition_to(TaskStatus::Working);
    assert!(result.is_err());
    match result.unwrap_err() {
        MeshError::InvalidTransition { from, to } => {
            assert!(from.contains("Completed"));
            assert!(to.contains("Working"));
        }
        _ => panic!("expected InvalidTransition error"),
    }
}

#[test]
fn task_status_terminal_states() {
    assert!(TaskStatus::Completed.is_terminal());
    assert!(TaskStatus::Failed("err".into()).is_terminal());
    assert!(TaskStatus::Canceled.is_terminal());
    assert!(!TaskStatus::Submitted.is_terminal());
    assert!(!TaskStatus::Working.is_terminal());
    assert!(!TaskStatus::InputRequired("x".into()).is_terminal());
}

// ── MeshTask transition tests ───────────────────────────────────────────

#[test]
fn mesh_task_lifecycle() {
    let mut task = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 60);
    assert_eq!(task.status, TaskStatus::Submitted);

    task.transition(TaskStatus::Working).unwrap();
    assert_eq!(task.status, TaskStatus::Working);

    task.transition(TaskStatus::Completed).unwrap();
    assert_eq!(task.status, TaskStatus::Completed);
}

#[test]
fn mesh_task_invalid_transition_error() {
    let mut task = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 60);
    task.transition(TaskStatus::Working).unwrap();
    task.transition(TaskStatus::Completed).unwrap();

    // Completed → Working should fail.
    let result = task.transition(TaskStatus::Working);
    assert!(result.is_err());
}

// ── MeshMessage JSON-RPC 2.0 tests ─────────────────────────────────────

#[test]
fn mesh_message_request_conforms_to_jsonrpc() {
    let msg = MeshMessage::request(
        "tasks/send",
        serde_json::json!({"task": "test"}),
        serde_json::json!(1),
    );
    assert_eq!(msg.jsonrpc, "2.0");
    assert!(msg.is_valid_jsonrpc());
    assert!(msg.id.is_some());
    assert!(msg.params.is_some());
    assert!(!msg.method.is_empty());
}

#[test]
fn mesh_message_success_response_conforms() {
    let msg = MeshMessage::success(serde_json::json!(1), serde_json::json!({"status": "ok"}));
    assert_eq!(msg.jsonrpc, "2.0");
    assert!(msg.is_valid_jsonrpc());
    assert!(msg.result.is_some());
    assert!(msg.error.is_none());
}

#[test]
fn mesh_message_error_response_conforms() {
    let msg = MeshMessage::error_response(serde_json::json!(1), -32601, "Method not found");
    assert_eq!(msg.jsonrpc, "2.0");
    assert!(msg.is_valid_jsonrpc());
    assert!(msg.error.is_some());
    assert!(msg.result.is_none());
}

#[test]
fn mesh_message_missing_required_fields_deserialization_error() {
    // Missing jsonrpc field — serde should reject this.
    let json = r#"{"method": "tasks/send", "params": {}, "id": 1}"#;
    let result: Result<MeshMessage, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "missing jsonrpc field should cause deserialization error"
    );
}
