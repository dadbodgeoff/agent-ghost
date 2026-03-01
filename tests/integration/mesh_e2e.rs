//! End-to-end: Mesh agent card → task lifecycle → delta encoding (Phase 15.3).
//!
//! Note: Full mesh flow (discover → submit → get → cancel) requires a
//! running gateway with mesh enabled. These tests exercise the core
//! mesh types and protocol logic without network I/O.

use std::sync::{Arc, Mutex};

use ghost_mesh::transport::a2a_server::{A2ADispatcher, A2AServerState};
use ghost_mesh::types::{AgentCard, MeshMessage, MeshTask, TaskStatus};
use ghost_signing::generate_keypair;
use uuid::Uuid;

fn make_signed_card() -> AgentCard {
    let (sk, _vk) = generate_keypair();
    let vk = sk.verifying_key();
    let mut card = AgentCard {
        name: "test-agent".into(),
        description: "Test agent for mesh e2e".into(),
        capabilities: vec!["code_execution".into(), "web_search".into()],
        capability_flags: AgentCard::capabilities_from_strings(&[
            "code_execution".into(),
            "web_search".into(),
        ]),
        input_types: vec!["text/plain".into()],
        output_types: vec!["text/plain".into()],
        auth_schemes: vec!["bearer".into()],
        endpoint_url: "http://localhost:8080".into(),
        public_key: vk.to_bytes().to_vec(),
        convergence_profile: "standard".into(),
        trust_score: 0.75,
        sybil_lineage_hash: "abc123".into(),
        version: "1.0.0".into(),
        signed_at: chrono::Utc::now(),
        signature: Vec::new(),
    };
    card.sign(&sk);
    card
}

/// Agent card sign + verify round-trip.
#[test]
fn agent_card_sign_verify_round_trip() {
    let card = make_signed_card();
    assert!(card.verify_signature(), "Signed card should verify");
}

/// Capability bitfield matching.
#[test]
fn capability_bitfield_matching() {
    let card = make_signed_card();
    // code_execution = bit 0, web_search = bit 1
    assert!(card.capabilities_match(0b01)); // code_execution
    assert!(card.capabilities_match(0b10)); // web_search
    assert!(card.capabilities_match(0b11)); // both
    assert!(!card.capabilities_match(0b100)); // file_operations — not set
}

/// A2A dispatcher: tasks/send → tasks/get → tasks/cancel lifecycle.
#[test]
fn a2a_task_lifecycle() {
    let card = make_signed_card();
    let state = Arc::new(Mutex::new(A2AServerState::new(card)));
    let dispatcher = A2ADispatcher::new(state);

    // 1. Submit a task via tasks/send.
    let send_msg = MeshMessage::request(
        "tasks/send",
        serde_json::json!({"task": "review code"}),
        serde_json::json!("req-1"),
    );
    let send_resp = dispatcher.dispatch(&send_msg);
    assert!(send_resp.error.is_none(), "tasks/send should succeed");
    let task_id = send_resp
        .result
        .as_ref()
        .unwrap()
        .get("id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // 2. Get task status via tasks/get.
    let get_msg = MeshMessage::request(
        "tasks/get",
        serde_json::json!({"task_id": task_id}),
        serde_json::json!("req-2"),
    );
    let get_resp = dispatcher.dispatch(&get_msg);
    assert!(get_resp.error.is_none(), "tasks/get should succeed");

    // 3. Cancel the task via tasks/cancel.
    let cancel_msg = MeshMessage::request(
        "tasks/cancel",
        serde_json::json!({"task_id": task_id}),
        serde_json::json!("req-3"),
    );
    let cancel_resp = dispatcher.dispatch(&cancel_msg);
    assert!(cancel_resp.error.is_none(), "tasks/cancel should succeed");
}

/// Unknown method returns JSON-RPC error.
#[test]
fn unknown_method_returns_error() {
    let card = make_signed_card();
    let state = Arc::new(Mutex::new(A2AServerState::new(card)));
    let dispatcher = A2ADispatcher::new(state);

    let msg = MeshMessage::request(
        "tasks/unknown",
        serde_json::json!({}),
        serde_json::json!("req-1"),
    );
    let resp = dispatcher.dispatch(&msg);
    assert!(resp.error.is_some(), "Unknown method should return error");
}

/// Delta encoding: compute_delta with no changes → all None.
#[test]
fn delta_no_changes_all_none() {
    let task = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 300);
    let delta = task.compute_delta(&task);
    assert!(delta.status.is_none());
    assert!(delta.output.is_none());
    assert!(delta.updated_at.is_none());
}

/// Delta encoding: compute + apply round-trip.
#[test]
fn delta_round_trip() {
    let task_a = MeshTask::new(Uuid::new_v4(), Uuid::new_v4(), serde_json::json!({}), 300);
    let mut task_b = task_a.clone();
    task_b.status = TaskStatus::Working;
    task_b.output = Some(serde_json::json!({"result": "done"}));
    task_b.updated_at = chrono::Utc::now();

    let delta = task_b.compute_delta(&task_a);
    assert!(delta.status.is_some());
    assert!(delta.output.is_some());

    let mut reconstructed = task_a.clone();
    reconstructed.apply_delta(&delta);
    assert_eq!(reconstructed.status, TaskStatus::Working);
    assert_eq!(reconstructed.output, task_b.output);
}

/// AgentCard TTL cache: get within TTL returns card, expired returns None.
#[test]
fn agent_card_cache_ttl() {
    use ghost_mesh::types::AgentCardCache;
    use std::time::Duration;

    let mut cache = AgentCardCache::new(Duration::from_millis(50));
    let card = make_signed_card();
    let agent_id = Uuid::new_v4();

    cache.put(agent_id, card.clone());
    assert!(cache.get(&agent_id).is_some(), "Should be cached");

    // Wait for TTL to expire.
    std::thread::sleep(Duration::from_millis(60));
    assert!(cache.get(&agent_id).is_none(), "Should be expired");
}
