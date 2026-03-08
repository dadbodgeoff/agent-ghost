mod common;

use reqwest::StatusCode;
use serde_json::json;

const CLIENT_NAME_HEADER: &str = "x-ghost-client-name";
const CLIENT_VERSION_HEADER: &str = "x-ghost-client-version";

#[tokio::test]
async fn compatibility_endpoint_advertises_supported_clients() {
    let gateway = common::TestGateway::start().await;

    let response = gateway
        .client
        .get(gateway.url("/api/compatibility"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["gateway_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        body["policy_a_writes_require_explicit_client_identity"],
        true
    );
    assert!(body["supported_clients"]
        .as_array()
        .unwrap()
        .iter()
        .any(|client| client["client_name"] == "dashboard"));

    gateway.stop().await;
}

#[tokio::test]
async fn integrity_writes_fail_closed_for_missing_and_old_client_versions() {
    let gateway = common::TestGateway::start().await;
    let path = gateway.url("/api/goals/test-goal/approve");
    let body = json!({
        "expected_state": "pending_review",
        "expected_lineage_id": "lineage-1",
        "expected_subject_key": "goal:test",
        "expected_reviewed_revision": "rev-1"
    });
    let bare_client = reqwest::Client::new();

    let missing_headers = bare_client.post(&path).json(&body).send().await.unwrap();
    let missing_status = missing_headers.status();
    let missing_body: serde_json::Value = missing_headers.json().await.unwrap();
    assert_eq!(missing_status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(
        missing_body["error"]["code"],
        "CLIENT_COMPATIBILITY_REQUIRED"
    );

    for version in ["0.0.98", "0.0.99"] {
        let response = bare_client
            .post(&path)
            .header(CLIENT_NAME_HEADER, "dashboard")
            .header(CLIENT_VERSION_HEADER, version)
            .json(&body)
            .send()
            .await
            .unwrap();

        let status = response.status();
        let payload: serde_json::Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::UPGRADE_REQUIRED);
        assert_eq!(payload["error"]["code"], "CLIENT_VERSION_UNSUPPORTED");
    }

    let current = bare_client
        .post(&path)
        .header(CLIENT_NAME_HEADER, "dashboard")
        .header(CLIENT_VERSION_HEADER, "0.1.0")
        .json(&body)
        .send()
        .await
        .unwrap();

    let current_status = current.status();
    let current_body: serde_json::Value = current.json().await.unwrap();
    assert_eq!(current_status, StatusCode::NOT_FOUND);
    assert_eq!(current_body["error"]["code"], "NOT_FOUND");

    gateway.stop().await;
}
