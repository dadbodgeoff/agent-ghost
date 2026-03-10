//! E2E integration test for the health endpoint.
//!
//! Boots a real gateway on a random port with a temp database
//! and verifies the /api/health and /api/ready endpoints.

mod common;

#[tokio::test]
async fn health_endpoint_returns_200() {
    let gw = common::TestGateway::start().await;

    let resp = gw.client.get(gw.url("/api/health")).send().await.unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "alive");
    assert_eq!(body["state"], "Healthy");
    assert_eq!(body["speculative_context"]["enabled"], true);
    assert_eq!(body["speculative_context"]["available"], true);
    assert_eq!(body["speculative_context"]["window_hours"], 24);
    assert_eq!(body["speculative_context"]["attempts_created"], 0);
    assert_eq!(body["speculative_context"]["promotions_created"], 0);
    assert_eq!(body["speculative_context"]["pending_job_depth"], 0);
    assert_eq!(body["speculative_context"]["dead_letter_jobs"], 0);
    assert_eq!(body["speculative_context"]["ttl_backlog"], 0);
    assert_eq!(
        body["speculative_context"]["pending_jobs_by_type"],
        serde_json::json!({})
    );
    assert_eq!(
        body["speculative_context"]["status_counts"],
        serde_json::json!({})
    );

    gw.stop().await;
}

#[tokio::test]
async fn ready_endpoint_returns_200_when_healthy() {
    let gw = common::TestGateway::start().await;

    let resp = gw.client.get(gw.url("/api/ready")).send().await.unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ready");
    assert_eq!(body["state"], "Healthy");

    gw.stop().await;
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let gw = common::TestGateway::start().await;

    let resp = gw
        .client
        .get(gw.url("/api/nonexistent"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);

    gw.stop().await;
}
