mod common;

use ghost_gateway::agents::registry::{durable_agent_id, AgentLifecycleState, RegisteredAgent};
use reqwest::StatusCode;
use serde_json::json;

fn register_agent_with_allowlist(
    gateway: &common::TestGateway,
    name: &str,
    skills: Option<Vec<String>>,
) -> uuid::Uuid {
    let agent_id = durable_agent_id(name);
    gateway
        .app_state
        .agents
        .write()
        .unwrap()
        .register(RegisteredAgent {
            id: agent_id,
            name: name.to_string(),
            state: AgentLifecycleState::Ready,
            channel_bindings: Vec::new(),
            capabilities: Vec::new(),
            skills,
            spending_cap: 5.0,
            template: None,
        });
    agent_id
}

#[tokio::test]
async fn list_skills_reflects_always_on_and_install_state_truthfully() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;

    let response = gateway
        .client
        .get(gateway.url("/api/skills"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();

    assert!(body["installed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["name"] == "convergence_check" && skill["state"] == "always_on"));
    assert!(body["installed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["name"] == "note_take" && skill["state"] == "installed"));
    assert_eq!(body["available"], json!([]));
}

#[tokio::test]
async fn uninstall_and_install_change_runtime_visible_skill_state() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;

    let uninstall = gateway
        .client
        .post(gateway.url("/api/skills/note_take/uninstall"))
        .send()
        .await
        .unwrap();
    assert_eq!(uninstall.status(), StatusCode::OK);
    let uninstall_body: serde_json::Value = uninstall.json().await.unwrap();
    assert_eq!(uninstall_body["state"], "available");

    let after_uninstall = gateway
        .client
        .get(gateway.url("/api/skills"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert!(after_uninstall["available"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["name"] == "note_take" && skill["state"] == "available"));

    let install = gateway
        .client
        .post(gateway.url("/api/skills/note_take/install"))
        .send()
        .await
        .unwrap();
    assert_eq!(install.status(), StatusCode::OK);
    let install_body: serde_json::Value = install.json().await.unwrap();
    assert_eq!(install_body["state"], "installed");
}

#[tokio::test]
async fn execute_route_runs_write_capable_skill_through_catalog_executor() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;
    let agent_id =
        register_agent_with_allowlist(&gateway, "skills-test", Some(vec!["note_take".into()]));
    let session_id = uuid::Uuid::now_v7();

    let response = gateway
        .client
        .post(gateway.url("/api/skills/note_take/execute"))
        .json(&json!({
            "agent_id": agent_id,
            "session_id": session_id,
            "input": {
                "action": "create",
                "title": "catalog note",
                "content": "written through canonical executor"
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    let note_id = body["result"]["note_id"].as_str().unwrap();

    let db = gateway.app_state.db.read().unwrap();
    let stored =
        cortex_storage::queries::note_queries::get_note(&db, note_id, &agent_id.to_string())
            .unwrap()
            .expect("stored note");

    assert_eq!(stored.title, "catalog note");
}

#[tokio::test]
async fn execute_route_rejects_disabled_skills() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;
    let agent_id =
        register_agent_with_allowlist(&gateway, "skills-test", Some(vec!["note_take".into()]));
    let session_id = uuid::Uuid::now_v7();

    let uninstall = gateway
        .client
        .post(gateway.url("/api/skills/note_take/uninstall"))
        .send()
        .await
        .unwrap();
    assert_eq!(uninstall.status(), StatusCode::OK);

    let response = gateway
        .client
        .post(gateway.url("/api/skills/note_take/execute"))
        .json(&json!({
            "agent_id": agent_id,
            "session_id": session_id,
            "input": {
                "action": "create",
                "title": "disabled note",
                "content": "should fail"
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body["error"]["message"],
        "Skill 'note_take' is disabled and cannot be executed"
    );
}
