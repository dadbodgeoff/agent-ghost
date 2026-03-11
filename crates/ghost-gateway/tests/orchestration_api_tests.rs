mod common;

use axum::{routing::get, Json, Router};
use base64::Engine;
use ghost_gateway::agents::registry::{durable_agent_id, AgentLifecycleState, RegisteredAgent};

fn registered_agent(name: &str) -> RegisteredAgent {
    RegisteredAgent {
        id: durable_agent_id(name),
        name: name.to_string(),
        state: AgentLifecycleState::Ready,
        channel_bindings: vec![format!("cli:{name}")],
        isolation: ghost_gateway::config::IsolationMode::InProcess,
        full_access: false,
        capabilities: vec!["delegate".into()],
        skills: None,
        baseline_capabilities: vec!["delegate".into()],
        baseline_skills: None,
        access_pullback_active: false,
        spending_cap: 5.0,
        template: None,
    }
}

async fn register_agents(gateway: &common::TestGateway, names: &[&str]) {
    let mut registry = gateway.app_state.agents.write().unwrap();
    for name in names {
        registry.register(registered_agent(name));
    }
}

async fn seed_delegation(
    gateway: &common::TestGateway,
    delegation_id: &str,
    sender_id: &str,
    recipient_id: &str,
    state: &str,
) {
    let db = gateway.app_state.db.write().await;
    cortex_storage::queries::delegation_state_queries::insert_delegation(
        &db,
        &format!("row-{delegation_id}"),
        delegation_id,
        sender_id,
        recipient_id,
        "delegate work",
        &format!("msg-{delegation_id}"),
        &[1; 32],
        &[0; 32],
    )
    .unwrap();
    if state != "Offered" {
        cortex_storage::queries::delegation_state_queries::transition_by_delegation_id(
            &db,
            delegation_id,
            state,
            if matches!(state, "Accepted" | "Completed") {
                Some("accept-msg")
            } else {
                None
            },
            if state == "Completed" {
                Some("complete-msg")
            } else {
                None
            },
            if state == "Completed" {
                Some("{\"status\":\"done\"}")
            } else {
                None
            },
            None,
        )
        .unwrap();
    }
}

async fn seed_proposal(
    gateway: &common::TestGateway,
    id: &str,
    agent_id: &str,
    session_id: &str,
    created_at: &str,
    subject_key: &str,
) {
    let db = gateway.app_state.db.write().await;
    let content = format!(
        "{{\"goal_text\":\"Reduce drift\",\"subject_key\":\"{subject_key}\",\"reviewed_revision\":\"rev-1\"}}"
    );
    cortex_storage::queries::goal_proposal_queries::insert_proposal_record(
        &db,
        &cortex_storage::queries::goal_proposal_queries::NewProposalRecord {
            id,
            agent_id,
            session_id,
            proposer_type: "agent",
            operation: "GoalChange",
            target_type: "goal",
            content: &content,
            cited_memory_ids: "[]",
            decision: "HumanReviewRequired",
            event_hash: &[2; 32],
            previous_hash: &[1; 32],
            created_at: Some(created_at),
            operation_id: None,
            request_id: None,
        },
    )
    .unwrap();
}

async fn start_signed_agent_card_server(name: &str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base_url = format!("http://{}", address);
    let (signing_key, verifying_key) = ghost_signing::generate_keypair();

    let mut card = ghost_mesh::types::AgentCard {
        name: name.into(),
        description: "Signed remote planner".into(),
        capabilities: vec!["planning".into()],
        capability_flags: 0,
        input_types: vec!["application/json".into()],
        output_types: vec!["application/json".into()],
        auth_schemes: vec!["ed25519".into()],
        endpoint_url: base_url.clone(),
        public_key: verifying_key.to_bytes().to_vec(),
        convergence_profile: "standard".into(),
        trust_score: 1.0,
        sybil_lineage_hash: String::new(),
        version: "1.0.0".into(),
        signed_at: chrono::Utc::now(),
        signature: Vec::new(),
        supported_task_types: vec!["analysis".into()],
        default_input_modes: vec!["application/json".into()],
        default_output_modes: vec!["application/json".into()],
        provider: "ghost-platform".into(),
        a2a_protocol_version: "0.2.0".into(),
    };
    card.sign(&signing_key);
    let card = std::sync::Arc::new(card);

    let router = Router::new().route(
        "/.well-known/agent.json",
        get({
            let card = std::sync::Arc::clone(&card);
            move || {
                let card = std::sync::Arc::clone(&card);
                async move { Json((*card).clone()) }
            }
        }),
    );

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (base_url, handle)
}

fn write_mesh_peer_config(
    gateway: &common::TestGateway,
    name: &str,
    endpoint: &str,
    public_key: &str,
) {
    let path = gateway.app_state.config_path.clone();
    let mut config = ghost_gateway::config::GhostConfig::load(&path).unwrap();
    config.mesh.enabled = true;
    config.mesh.known_agents = vec![ghost_gateway::config::KnownAgent {
        name: name.to_string(),
        endpoint: endpoint.to_string(),
        public_key: public_key.to_string(),
    }];
    std::fs::write(path, serde_yaml::to_string(&config).unwrap()).unwrap();
}

#[tokio::test]
async fn trust_graph_endpoint_derives_edges_from_real_delegations() {
    let gateway = common::TestGateway::start().await;
    register_agents(&gateway, &["alpha", "beta"]).await;

    seed_delegation(
        &gateway,
        "del-1",
        &durable_agent_id("alpha").to_string(),
        &durable_agent_id("beta").to_string(),
        "Completed",
    )
    .await;

    let response = gateway
        .client
        .get(gateway.url("/api/mesh/trust-graph"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let nodes = body["nodes"].as_array().unwrap();
    let edges = body["edges"].as_array().unwrap();

    assert_eq!(nodes.len(), 2);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["source"], durable_agent_id("alpha").to_string());
    assert_eq!(edges[0]["target"], durable_agent_id("beta").to_string());
    assert_eq!(edges[0]["trust_score"], 1.0);

    gateway.stop().await;
}

#[tokio::test]
async fn consensus_endpoint_reports_transition_backed_status() {
    let gateway = common::TestGateway::start().await;
    register_agents(&gateway, &["alpha"]).await;

    let agent_id = durable_agent_id("alpha").to_string();
    seed_proposal(
        &gateway,
        "proposal-pending",
        &agent_id,
        "session-1",
        "2026-03-11T00:00:00Z",
        "goal:reduce-drift:pending",
    )
    .await;
    seed_proposal(
        &gateway,
        "proposal-approved",
        &agent_id,
        "session-2",
        "2026-03-11T01:00:00Z",
        "goal:reduce-drift:approved",
    )
    .await;

    {
        let db = gateway.app_state.db.write().await;
        cortex_storage::queries::goal_proposal_queries::resolve_proposal(
            &db,
            "proposal-approved",
            "approved",
            "tester",
            "2026-03-11T01:05:00Z",
        )
        .unwrap();
    }

    let response = gateway
        .client
        .get(gateway.url("/api/mesh/consensus"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let rounds = body["rounds"].as_array().unwrap();

    let approved = rounds
        .iter()
        .find(|round| round["proposal_id"] == "proposal-approved")
        .unwrap();
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["approvals"], 1);
    assert_eq!(approved["rejections"], 0);
    assert_eq!(approved["threshold"], 1);

    let pending = rounds
        .iter()
        .find(|round| round["proposal_id"] == "proposal-pending")
        .unwrap();
    assert_eq!(pending["status"], "pending_review");
    assert_eq!(pending["approvals"], 0);
    assert_eq!(pending["rejections"], 0);

    gateway.stop().await;
}

#[tokio::test]
async fn delegations_endpoint_computes_chain_depth_from_active_graph() {
    let gateway = common::TestGateway::start().await;
    let agent_a = durable_agent_id("alpha").to_string();
    let agent_b = durable_agent_id("beta").to_string();
    let agent_c = durable_agent_id("gamma").to_string();

    seed_delegation(&gateway, "del-a-b", &agent_a, &agent_b, "Accepted").await;
    seed_delegation(&gateway, "del-b-c", &agent_b, &agent_c, "Offered").await;

    let response = gateway
        .client
        .get(gateway.url("/api/mesh/delegations"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let metrics = &body["sybil_metrics"];

    assert_eq!(metrics["total_delegations"], 2);
    assert_eq!(metrics["unique_delegators"], 2);
    assert_eq!(metrics["max_chain_depth"], 2);

    gateway.stop().await;
}

#[tokio::test]
async fn delegations_endpoint_excludes_completed_rows_from_live_sybil_depth() {
    let gateway = common::TestGateway::start().await;
    let agent_a = durable_agent_id("alpha").to_string();
    let agent_b = durable_agent_id("beta").to_string();
    let agent_c = durable_agent_id("gamma").to_string();

    seed_delegation(&gateway, "del-a-b", &agent_a, &agent_b, "Completed").await;
    seed_delegation(&gateway, "del-b-c", &agent_b, &agent_c, "Accepted").await;

    let response = gateway
        .client
        .get(gateway.url("/api/mesh/delegations"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let metrics = &body["sybil_metrics"];

    assert_eq!(metrics["total_delegations"], 1);
    assert_eq!(metrics["unique_delegators"], 1);
    assert_eq!(metrics["max_chain_depth"], 1);

    gateway.stop().await;
}

#[tokio::test]
async fn discover_agents_probes_mesh_config_peers_without_preseeded_db_rows() {
    let gateway = common::TestGateway::start().await;
    let (endpoint, server_handle) = start_signed_agent_card_server("Remote Planner").await;
    let public_key = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);

    write_mesh_peer_config(&gateway, "remote-planner", &endpoint, &public_key);

    let response = gateway
        .client
        .get(gateway.url("/api/a2a/discover"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let agents = body["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["name"], "Remote Planner");
    assert_eq!(agents[0]["endpoint_url"], endpoint);
    assert_eq!(agents[0]["reachable"], true);
    assert_eq!(agents[0]["verified"], true);

    server_handle.abort();
    gateway.stop().await;
}
