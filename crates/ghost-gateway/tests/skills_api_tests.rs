mod common;

use cortex_storage::queries::external_skill_queries::{
    self, ExternalSkillInstallState, ExternalSkillQuarantineState, ExternalSkillVerificationStatus,
};
use ghost_gateway::agents::registry::{durable_agent_id, AgentLifecycleState, RegisteredAgent};
use reqwest::StatusCode;
use serde_json::json;

fn with_operation_headers(
    builder: reqwest::RequestBuilder,
    operation_id: &str,
    idempotency_key: &str,
) -> reqwest::RequestBuilder {
    builder
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
}

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

fn seed_external_skill(
    conn: &rusqlite::Connection,
    digest: &str,
    name: &str,
    verification: ExternalSkillVerificationStatus,
    quarantine: ExternalSkillQuarantineState,
    install: Option<ExternalSkillInstallState>,
) {
    external_skill_queries::upsert_external_skill_artifact(
        conn,
        digest,
        1,
        name,
        "1.0.0",
        "ghost-test",
        "external skill",
        "workspace",
        "wasm",
        "module.wasm",
        &format!("/source/{name}.ghostskill"),
        &format!("/managed/{digest}/artifact.ghostskill"),
        &format!("/managed/{digest}/module.wasm"),
        "{}",
        "[]",
        "[\"Pure WASM computation\"]",
        Some("key-1"),
        256,
    )
    .unwrap();
    external_skill_queries::upsert_external_skill_verification(
        conn,
        digest,
        verification,
        Some("key-1"),
        Some("ghost-test"),
        "{}",
    )
    .unwrap();
    external_skill_queries::upsert_external_skill_quarantine(
        conn,
        digest,
        quarantine,
        (quarantine == ExternalSkillQuarantineState::Quarantined).then_some("operator_quarantine"),
        (quarantine == ExternalSkillQuarantineState::Quarantined).then_some("manual quarantine"),
        Some("operator"),
    )
    .unwrap();
    if let Some(install) = install {
        external_skill_queries::upsert_external_skill_install_state(
            conn,
            digest,
            name,
            "1.0.0",
            install,
            Some("operator"),
        )
        .unwrap();
    }
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

    let uninstall = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/note_take/uninstall")),
        "018f0f23-8c65-7abc-9def-600000000001",
        "skills-uninstall-note-take",
    )
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

    let install = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/note_take/install")),
        "018f0f23-8c65-7abc-9def-600000000002",
        "skills-install-note-take",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(install.status(), StatusCode::OK);
    let install_body: serde_json::Value = install.json().await.unwrap();
    assert_eq!(install_body["state"], "installed");
}

#[tokio::test]
async fn execute_route_replays_transactional_skill_without_duplicate_side_effects() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;
    let agent_id =
        register_agent_with_allowlist(&gateway, "skills-test", Some(vec!["note_take".into()]));
    let session_id = uuid::Uuid::now_v7();
    let request = json!({
        "agent_id": agent_id,
        "session_id": session_id,
        "input": {
            "action": "create",
            "title": "catalog note",
            "content": "written through canonical executor"
        }
    });

    let response = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/note_take/execute"))
            .json(&request),
        "018f0f23-8c65-7abc-9def-600000000003",
        "skills-execute-note-take",
    )
    .send()
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let body: serde_json::Value = response.json().await.unwrap();
    let note_id = body["result"]["note_id"].as_str().unwrap();

    let replay = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/note_take/execute"))
            .json(&request),
        "018f0f23-8c65-7abc-9def-600000000003",
        "skills-execute-note-take",
    )
    .send()
    .await
    .unwrap();

    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );
    let replay_body: serde_json::Value = replay.json().await.unwrap();
    assert_eq!(replay_body["result"]["note_id"], note_id);

    let db = gateway.app_state.db.read().unwrap();
    let stored =
        cortex_storage::queries::note_queries::get_note(&db, note_id, &agent_id.to_string())
            .unwrap()
            .expect("stored note");
    let note_count =
        cortex_storage::queries::note_queries::count_notes(&db, &agent_id.to_string()).unwrap();
    let audit_row: (String, String, Option<String>) = db
        .query_row(
            "SELECT actor_id, idempotency_status, operation_id
             FROM audit_log
             WHERE event_type = 'execute_skill' AND idempotency_key = ?1
             LIMIT 1",
            ["skills-execute-note-take"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(stored.title, "catalog note");
    assert_eq!(note_count, 1);
    assert_eq!(audit_row.0, "anonymous");
    assert_eq!(audit_row.1, "executed");
    assert_eq!(
        audit_row.2.as_deref(),
        Some("018f0f23-8c65-7abc-9def-600000000003")
    );
}

#[tokio::test]
async fn mutating_skill_routes_require_caller_supplied_idempotency_keys() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;

    let install = gateway
        .client
        .post(gateway.url("/api/skills/note_take/uninstall"))
        .send()
        .await
        .unwrap();

    assert_eq!(install.status(), StatusCode::PRECONDITION_REQUIRED);
    let body: serde_json::Value = install.json().await.unwrap();
    assert_eq!(body["error"]["code"], "EXPLICIT_IDEMPOTENCY_KEY_REQUIRED");
}

#[tokio::test]
async fn execute_route_rejects_external_side_effect_skills() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;
    let agent_id =
        register_agent_with_allowlist(&gateway, "format-test", Some(vec!["format_code".into()]));
    let session_id = uuid::Uuid::now_v7();

    let response = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/format_code/execute"))
            .json(&json!({
                "agent_id": agent_id,
                "session_id": session_id,
                "input": {
                    "path": "/tmp/example.rs"
                }
            })),
        "018f0f23-8c65-7abc-9def-600000000004",
        "skills-execute-format-code",
    )
    .send()
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "NON_IDEMPOTENT_SKILL_UNSUPPORTED");
}

#[tokio::test]
async fn execute_route_rejects_disabled_skills() {
    let gateway = common::TestGateway::start_with_compiled_skills().await;
    let agent_id =
        register_agent_with_allowlist(&gateway, "skills-test", Some(vec!["note_take".into()]));
    let session_id = uuid::Uuid::now_v7();

    let uninstall = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/note_take/uninstall")),
        "018f0f23-8c65-7abc-9def-600000000005",
        "skills-disable-note-take",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(uninstall.status(), StatusCode::OK);

    let response = with_operation_headers(
        gateway
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
            })),
        "018f0f23-8c65-7abc-9def-600000000006",
        "skills-disabled-execute",
    )
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

#[tokio::test]
async fn list_and_install_routes_reflect_external_skill_truth() {
    let gateway = common::TestGateway::start().await;
    let writer = gateway.app_state.db.write().await;
    seed_external_skill(
        &writer,
        "digest-external",
        "echo",
        ExternalSkillVerificationStatus::Verified,
        ExternalSkillQuarantineState::Clear,
        None,
    );
    drop(writer);

    let list = gateway
        .client
        .get(gateway.url("/api/skills"))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body: serde_json::Value = list.json().await.unwrap();
    let external = body["available"]
        .as_array()
        .unwrap()
        .iter()
        .find(|skill| skill["id"] == "digest-external")
        .cloned()
        .expect("external skill in catalog");

    assert_eq!(external["name"], "echo");
    assert_eq!(external["state"], "verified");
    assert_eq!(external["install_state"], "not_installed");
    assert_eq!(external["verification_status"], "verified");
    assert_eq!(external["quarantine_state"], "clear");
    assert_eq!(external["runtime_visible"], false);
    assert_eq!(external["source"], "workspace");
    assert_eq!(external["policy_capability"], "skill:echo");

    let install = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/digest-external/install")),
        "018f0f23-8c65-7abc-9def-600000000007",
        "skills-install-external-echo",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(install.status(), StatusCode::OK);
    let install_body: serde_json::Value = install.json().await.unwrap();
    assert_eq!(install_body["id"], "digest-external");
    assert_eq!(install_body["state"], "installed");
    assert_eq!(install_body["install_state"], "installed");
    assert_eq!(install_body["runtime_visible"], false);
}

#[tokio::test]
async fn execute_route_cannot_bypass_catalog_for_installed_external_skills() {
    let gateway = common::TestGateway::start().await;
    let writer = gateway.app_state.db.write().await;
    seed_external_skill(
        &writer,
        "digest-runtime-dark",
        "echo",
        ExternalSkillVerificationStatus::Verified,
        ExternalSkillQuarantineState::Clear,
        Some(ExternalSkillInstallState::Installed),
    );
    drop(writer);

    let response = gateway
        .client
        .post(gateway.url("/api/skills/digest-runtime-dark/execute"))
        .json(&json!({
            "agent_id": uuid::Uuid::now_v7(),
            "session_id": uuid::Uuid::now_v7(),
            "input": {
                "message": "should stay blocked"
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body["error"]["message"],
        "Skill 'digest-runtime-dark' is verified but runtime execution is still gated off"
    );
}

#[tokio::test]
async fn quarantine_and_resolution_routes_require_fresh_revision_and_enforce_runtime_blocks() {
    let gateway = common::TestGateway::start().await;
    let writer = gateway.app_state.db.write().await;
    seed_external_skill(
        &writer,
        "digest-operator",
        "echo",
        ExternalSkillVerificationStatus::Verified,
        ExternalSkillQuarantineState::Clear,
        None,
    );
    drop(writer);

    let quarantine = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/digest-operator/quarantine"))
            .json(&json!({ "reason": "manual review" })),
        "018f0f23-8c65-7abc-9def-600000000008",
        "skills-quarantine-external-echo",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(quarantine.status(), StatusCode::OK);
    let quarantine_body: serde_json::Value = quarantine.json().await.unwrap();
    assert_eq!(quarantine_body["state"], "quarantined");
    assert_eq!(quarantine_body["quarantine_state"], "quarantined");
    assert_eq!(quarantine_body["quarantine_reason"], "manual review");
    assert_eq!(quarantine_body["quarantine_revision"], 2);

    let install_blocked = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/digest-operator/install")),
        "018f0f23-8c65-7abc-9def-600000000009",
        "skills-install-quarantined-external-echo",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(install_blocked.status(), StatusCode::CONFLICT);
    let install_error: serde_json::Value = install_blocked.json().await.unwrap();
    assert_eq!(
        install_error["error"]["message"],
        "Skill 'digest-operator' is quarantined: manual review"
    );

    let stale_resolve = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/digest-operator/quarantine/resolve"))
            .json(&json!({ "expected_quarantine_revision": 0 })),
        "018f0f23-8c65-7abc-9def-600000000010",
        "skills-resolve-quarantine-stale",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(stale_resolve.status(), StatusCode::CONFLICT);
    let stale_body: serde_json::Value = stale_resolve.json().await.unwrap();
    assert_eq!(stale_body["error"]["code"], "STALE_QUARANTINE_REVISION");
    assert_eq!(stale_body["error"]["details"]["expected_revision"], 0);
    assert_eq!(stale_body["error"]["details"]["actual_revision"], 2);

    let resolve = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/digest-operator/quarantine/resolve"))
            .json(&json!({ "expected_quarantine_revision": 2 })),
        "018f0f23-8c65-7abc-9def-600000000011",
        "skills-resolve-quarantine-fresh",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(resolve.status(), StatusCode::OK);
    let resolve_body: serde_json::Value = resolve.json().await.unwrap();
    assert_eq!(resolve_body["state"], "verified");
    assert_eq!(resolve_body["quarantine_state"], "clear");
    assert_eq!(resolve_body["quarantine_revision"], 3);

    let install = with_operation_headers(
        gateway
            .client
            .post(gateway.url("/api/skills/digest-operator/install")),
        "018f0f23-8c65-7abc-9def-600000000012",
        "skills-install-resolved-external-echo",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(install.status(), StatusCode::OK);
    let install_body: serde_json::Value = install.json().await.unwrap();
    assert_eq!(install_body["state"], "installed");
    assert_eq!(install_body["runtime_visible"], false);
}
