mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn paused_agents_are_blocked_on_agent_chat_and_studio_message_routes() {
    let gateway = common::TestGateway::start().await;
    let agent_id = uuid::Uuid::now_v7();

    {
        let mut registry = gateway.app_state.agents.write().unwrap();
        registry.register(ghost_gateway::agents::registry::RegisteredAgent {
            id: agent_id,
            name: "paused-agent".into(),
            state: ghost_gateway::agents::registry::AgentLifecycleState::Ready,
            channel_bindings: Vec::new(),
            capabilities: Vec::new(),
            skills: None,
            spending_cap: 5.0,
            template: None,
        });
    }

    gateway.app_state.kill_switch.activate_agent(
        agent_id,
        ghost_gateway::safety::kill_switch::KillLevel::Pause,
        &cortex_core::safety::trigger::TriggerEvent::ManualPause {
            agent_id,
            reason: "release-test".into(),
            initiated_by: "test".into(),
        },
    );

    let agent_chat = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&json!({
          "agent_id": agent_id.to_string(),
          "message": "hello",
        }))
        .send()
        .await
        .unwrap();
    let agent_chat_status = agent_chat.status();
    let agent_chat_text = agent_chat.text().await.unwrap();
    assert_eq!(
        agent_chat_status,
        StatusCode::LOCKED,
        "agent chat body: {agent_chat_text}"
    );
    let agent_chat_body: serde_json::Value = serde_json::from_str(&agent_chat_text).unwrap();
    assert_eq!(agent_chat_body["error"]["code"], "AGENT_PAUSED");

    let create_session = gateway
        .client
        .post(gateway.url("/api/studio/sessions"))
        .json(&json!({
          "agent_id": agent_id.to_string(),
          "title": "Paused agent",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_session.status(), StatusCode::CREATED);
    let session: serde_json::Value = create_session.json().await.unwrap();
    let session_id = session["id"].as_str().unwrap();

    let studio_message = gateway
        .client
        .post(gateway.url(&format!("/api/studio/sessions/{session_id}/messages")))
        .json(&json!({ "content": "hello" }))
        .send()
        .await
        .unwrap();
    let studio_status = studio_message.status();
    let studio_text = studio_message.text().await.unwrap();
    assert_eq!(
        studio_status,
        StatusCode::LOCKED,
        "studio body: {studio_text}"
    );
    let studio_body: serde_json::Value = serde_json::from_str(&studio_text).unwrap();
    assert_eq!(studio_body["error"]["code"], "AGENT_PAUSED");

    gateway.stop().await;
}
