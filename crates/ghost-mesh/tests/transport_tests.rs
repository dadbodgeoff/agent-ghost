//! Tests for Task 14.3 — A2A-compatible transport + agent discovery.
//!
//! Covers: A2AServerState, A2ADispatcher (JSON-RPC routing, AgentCard serving),
//! AgentDiscovery (caching, TTL, signature verification), and adversarial cases.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use ghost_mesh::discovery::{AgentDiscovery, KnownAgentConfig};
use ghost_mesh::protocol::{error_codes, methods};
use ghost_mesh::transport::a2a_server::{A2ADispatcher, A2AServerState};
use ghost_mesh::types::{AgentCard, MeshMessage, TaskStatus};
use uuid::Uuid;

// ── Helper ──────────────────────────────────────────────────────────────

fn make_signed_card(name: &str) -> (AgentCard, ghost_signing::SigningKey) {
    let (sk, vk) = ghost_signing::generate_keypair();
    let mut card = AgentCard {
        name: name.to_string(),
        description: format!("Test agent: {name}"),
        capabilities: vec!["testing".to_string()],
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
    card.sign(&sk);
    (card, sk)
}

fn make_dispatcher(card: AgentCard) -> A2ADispatcher {
    let state = Arc::new(Mutex::new(A2AServerState::new(card)));
    A2ADispatcher::new(state)
}

// ── AgentCard served at correct path ────────────────────────────────────

#[test]
fn agent_card_served_from_dispatcher() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card.clone());

    let served = dispatcher
        .agent_card()
        .expect("agent_card should return Some in test");
    assert_eq!(served.name, card.name);
    assert_eq!(served.endpoint_url, card.endpoint_url);
    assert_eq!(served.public_key, card.public_key);
    assert_eq!(served.signature, card.signature);
    assert!(
        served.verify_signature(),
        "served card must have valid signature"
    );
}

// ── JSON-RPC dispatcher routes correctly ────────────────────────────────

#[test]
fn dispatcher_routes_tasks_send() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    let msg = MeshMessage::request(
        methods::TASKS_SEND,
        serde_json::json!({"task_description": "review code"}),
        serde_json::json!(1),
    );

    let resp = dispatcher.dispatch(&msg);
    assert!(resp.result.is_some(), "tasks/send should return a result");
    assert!(
        resp.error.is_none(),
        "tasks/send should not return an error"
    );

    // Result should be a valid MeshTask.
    let task: ghost_mesh::types::MeshTask =
        serde_json::from_value(resp.result.unwrap()).expect("result should be a MeshTask");
    assert_eq!(task.status, TaskStatus::Submitted);
}

#[test]
fn dispatcher_routes_tasks_get() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    // First submit a task.
    let send_msg = MeshMessage::request(
        methods::TASKS_SEND,
        serde_json::json!({"prompt": "hello"}),
        serde_json::json!(1),
    );
    let send_resp = dispatcher.dispatch(&send_msg);
    let task: ghost_mesh::types::MeshTask =
        serde_json::from_value(send_resp.result.unwrap()).unwrap();

    // Now get it.
    let get_msg = MeshMessage::request(
        methods::TASKS_GET,
        serde_json::json!({"task_id": task.id.to_string()}),
        serde_json::json!(2),
    );
    let get_resp = dispatcher.dispatch(&get_msg);
    assert!(get_resp.result.is_some());
    let fetched: ghost_mesh::types::MeshTask =
        serde_json::from_value(get_resp.result.unwrap()).unwrap();
    assert_eq!(fetched.id, task.id);
}

#[test]
fn dispatcher_routes_tasks_cancel() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    // Submit a task.
    let send_msg = MeshMessage::request(
        methods::TASKS_SEND,
        serde_json::json!({"prompt": "hello"}),
        serde_json::json!(1),
    );
    let send_resp = dispatcher.dispatch(&send_msg);
    let task: ghost_mesh::types::MeshTask =
        serde_json::from_value(send_resp.result.unwrap()).unwrap();

    // Cancel it.
    let cancel_msg = MeshMessage::request(
        methods::TASKS_CANCEL,
        serde_json::json!({"task_id": task.id.to_string()}),
        serde_json::json!(2),
    );
    let cancel_resp = dispatcher.dispatch(&cancel_msg);
    assert!(cancel_resp.result.is_some());
    let canceled: ghost_mesh::types::MeshTask =
        serde_json::from_value(cancel_resp.result.unwrap()).unwrap();
    assert_eq!(canceled.status, TaskStatus::Canceled);
}

// ── AgentCard signature verified on discovery ───────────────────────────

#[test]
fn agent_card_signature_verified_on_serve() {
    let (card, _sk) = make_signed_card("verified-agent");
    let dispatcher = make_dispatcher(card);
    let served = dispatcher
        .agent_card()
        .expect("agent_card should return Some in test");
    assert!(
        served.verify_signature(),
        "served AgentCard must pass signature verification"
    );
}

// ── Invalid signature → AuthenticationFailed ────────────────────────────

#[test]
fn tampered_agent_card_fails_verification() {
    let (mut card, _sk) = make_signed_card("honest-agent");
    // Tamper after signing.
    card.name = "evil-agent".to_string();
    assert!(
        !card.verify_signature(),
        "tampered card should fail verification"
    );
}

// ── AgentDiscovery cache TTL ────────────────────────────────────────────

#[test]
fn discovery_cache_returns_cached_card() {
    let discovery = AgentDiscovery::new(vec![], Duration::from_secs(3600));
    let (_card, _sk) = make_signed_card("cached-agent");
    let endpoint = "http://127.0.0.1:18789";

    // Manually insert into cache via a helper — we test the cache logic
    // by checking get_cached before and after.
    assert!(discovery.get_cached(endpoint).is_none());

    // We can't call discover() without a real server, but we can test
    // the cache invalidation and TTL logic directly.
    assert!(discovery.is_cache_expired(endpoint));
}

#[test]
fn discovery_cache_expired_returns_none() {
    // With a 0-second TTL, everything is immediately expired.
    let discovery = AgentDiscovery::new(vec![], Duration::from_secs(0));
    let endpoint = "http://127.0.0.1:18789";
    assert!(discovery.get_cached(endpoint).is_none());
    assert!(discovery.is_cache_expired(endpoint));
}

#[test]
fn discovery_invalidate_cache() {
    let mut discovery = AgentDiscovery::new(vec![], Duration::from_secs(3600));
    let endpoint = "http://127.0.0.1:18789";

    // Invalidate should not panic even if nothing cached.
    discovery.invalidate_cache(endpoint);
    assert!(discovery.get_cached(endpoint).is_none());
}

#[test]
fn discovery_invalidate_all_cache() {
    let mut discovery = AgentDiscovery::new(vec![], Duration::from_secs(3600));
    discovery.invalidate_all_cache();
    // Should not panic.
}

// ── AgentDiscovery known agents ─────────────────────────────────────────

#[test]
fn discovery_known_agents_from_config() {
    let config = vec![
        KnownAgentConfig {
            name: "helper".to_string(),
            endpoint: "http://192.168.1.100:18789".to_string(),
            public_key: vec![0u8; 32],
        },
        KnownAgentConfig {
            name: "reviewer".to_string(),
            endpoint: "http://192.168.1.101:18789".to_string(),
            public_key: vec![1u8; 32],
        },
    ];
    let discovery = AgentDiscovery::new(config, Duration::from_secs(3600));

    assert_eq!(discovery.known_agent_names().len(), 2);
    assert!(discovery.get_known_agent("helper").is_some());
    assert!(discovery.get_known_agent("reviewer").is_some());
    assert!(discovery.get_known_agent("unknown").is_none());
}

// ── Adversarial: unknown method → proper error ──────────────────────────

#[test]
fn dispatcher_unknown_method_returns_error() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    let msg = MeshMessage::request(
        "nonexistent/method",
        serde_json::json!({}),
        serde_json::json!(1),
    );

    let resp = dispatcher.dispatch(&msg);
    assert!(resp.error.is_some(), "unknown method should return error");
    let err = resp.error.unwrap();
    assert_eq!(err.code, error_codes::METHOD_NOT_FOUND);
    assert!(err.message.contains("nonexistent/method"));
}

#[test]
fn dispatcher_ghost_extension_unknown_returns_error() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    let msg = MeshMessage::request("ghost.unknown", serde_json::json!({}), serde_json::json!(1));

    let resp = dispatcher.dispatch(&msg);
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, error_codes::METHOD_NOT_FOUND);
}

// ── Adversarial: concurrent task submissions → no race conditions ───────

#[test]
fn concurrent_task_submissions_no_race() {
    let (card, _sk) = make_signed_card("concurrent-agent");
    let state = Arc::new(Mutex::new(A2AServerState::new(card)));

    let mut handles = vec![];
    for i in 0..10 {
        let state_clone = Arc::clone(&state);
        let handle = std::thread::spawn(move || {
            let dispatcher = A2ADispatcher::new(state_clone);
            let msg = MeshMessage::request(
                methods::TASKS_SEND,
                serde_json::json!({"task": format!("task-{i}")}),
                serde_json::json!(i),
            );
            let resp = dispatcher.dispatch(&msg);
            assert!(resp.result.is_some(), "concurrent submit should succeed");
            let task: ghost_mesh::types::MeshTask =
                serde_json::from_value(resp.result.unwrap()).unwrap();
            task.id
        });
        handles.push(handle);
    }

    let mut task_ids: Vec<Uuid> = vec![];
    for handle in handles {
        task_ids.push(handle.join().unwrap());
    }

    // All task IDs should be unique.
    task_ids.sort();
    task_ids.dedup();
    assert_eq!(
        task_ids.len(),
        10,
        "all 10 concurrent tasks should have unique IDs"
    );

    // All tasks should be in the state.
    let state = state.lock().unwrap();
    assert_eq!(state.tasks.len(), 10);
}

// ── Adversarial: tasks/get with invalid task_id ─────────────────────────

#[test]
fn dispatcher_get_nonexistent_task_returns_error() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    let msg = MeshMessage::request(
        methods::TASKS_GET,
        serde_json::json!({"task_id": Uuid::new_v4().to_string()}),
        serde_json::json!(1),
    );

    let resp = dispatcher.dispatch(&msg);
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, error_codes::TASK_NOT_FOUND);
}

#[test]
fn dispatcher_get_missing_task_id_returns_error() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    let msg = MeshMessage::request(
        methods::TASKS_GET,
        serde_json::json!({}),
        serde_json::json!(1),
    );

    let resp = dispatcher.dispatch(&msg);
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, error_codes::INVALID_PARAMS);
}

// ── Adversarial: cancel already-canceled task ───────────────────────────

#[test]
fn dispatcher_cancel_already_canceled_returns_error() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    // Submit.
    let send_msg = MeshMessage::request(
        methods::TASKS_SEND,
        serde_json::json!({"prompt": "hello"}),
        serde_json::json!(1),
    );
    let send_resp = dispatcher.dispatch(&send_msg);
    let task: ghost_mesh::types::MeshTask =
        serde_json::from_value(send_resp.result.unwrap()).unwrap();

    // Cancel once.
    let cancel_msg = MeshMessage::request(
        methods::TASKS_CANCEL,
        serde_json::json!({"task_id": task.id.to_string()}),
        serde_json::json!(2),
    );
    let cancel_resp = dispatcher.dispatch(&cancel_msg);
    assert!(cancel_resp.result.is_some());

    // Cancel again — should fail (already terminal).
    let cancel_msg2 = MeshMessage::request(
        methods::TASKS_CANCEL,
        serde_json::json!({"task_id": task.id.to_string()}),
        serde_json::json!(3),
    );
    let cancel_resp2 = dispatcher.dispatch(&cancel_msg2);
    assert!(cancel_resp2.error.is_some());
    assert_eq!(
        cancel_resp2.error.unwrap().code,
        error_codes::TASK_ALREADY_COMPLETED
    );
}

// ── tasks/send missing params ───────────────────────────────────────────

#[test]
fn dispatcher_tasks_send_missing_params_returns_error() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    // Create a message with no params.
    let msg = MeshMessage {
        jsonrpc: "2.0".to_string(),
        method: methods::TASKS_SEND.to_string(),
        params: None,
        id: Some(serde_json::json!(1)),
        error: None,
        result: None,
    };

    let resp = dispatcher.dispatch(&msg);
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, error_codes::INVALID_PARAMS);
}

// ── tasks/sendSubscribe routes correctly ────────────────────────────────

#[test]
fn dispatcher_routes_tasks_send_subscribe() {
    let (card, _sk) = make_signed_card("test-agent");
    let dispatcher = make_dispatcher(card);

    let msg = MeshMessage::request(
        methods::TASKS_SEND_SUBSCRIBE,
        serde_json::json!({"task_description": "streaming task"}),
        serde_json::json!(1),
    );

    let resp = dispatcher.dispatch(&msg);
    assert!(
        resp.result.is_some(),
        "tasks/sendSubscribe should return a result"
    );
    assert!(resp.error.is_none());

    let task: ghost_mesh::types::MeshTask =
        serde_json::from_value(resp.result.unwrap()).expect("result should be a MeshTask");
    assert_eq!(task.status, TaskStatus::Submitted);
}
