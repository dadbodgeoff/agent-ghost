mod common;

async fn seed_history(
    gateway: &common::TestGateway,
    agent_id: &str,
    entries: &[(&str, f64, i32, &str)],
) {
    let writer = gateway.app_state.db.write().await;
    for (index, (computed_at, score, level, signal_scores)) in entries.iter().enumerate() {
        cortex_storage::queries::convergence_score_queries::insert_score(
            &writer,
            &format!("{agent_id}-score-{index}"),
            agent_id,
            Some(&format!("session-{index}")),
            *score,
            signal_scores,
            *level,
            "standard",
            computed_at,
            &[index as u8 + 1; 32],
            &[index as u8; 32],
        )
        .unwrap();
    }
}

#[tokio::test]
async fn convergence_history_endpoint_returns_chronological_entries() {
    let gateway = common::TestGateway::start().await;
    seed_history(
        &gateway,
        "agent-history",
        &[
            (
                "2026-03-08T00:00:00Z",
                0.32,
                1,
                r#"{"session_duration":0.2}"#,
            ),
            (
                "2026-03-08T01:00:00Z",
                0.58,
                2,
                r#"{"session_duration":0.5}"#,
            ),
            (
                "2026-03-08T02:00:00Z",
                0.81,
                3,
                r#"{"session_duration":0.8}"#,
            ),
        ],
    )
    .await;

    let response = gateway
        .client
        .get(gateway.url("/api/convergence/history/agent-history"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let entries = body["entries"].as_array().unwrap();

    assert_eq!(body["agent_id"], "agent-history");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["computed_at"], "2026-03-08T00:00:00Z");
    assert_eq!(entries[1]["computed_at"], "2026-03-08T01:00:00Z");
    assert_eq!(entries[2]["computed_at"], "2026-03-08T02:00:00Z");
    assert_eq!(entries[2]["score"], 0.81);
    assert_eq!(entries[2]["signal_scores"]["session_duration"], 0.8);

    gateway.stop().await;
}

#[tokio::test]
async fn convergence_history_endpoint_honors_since_and_limit() {
    let gateway = common::TestGateway::start().await;
    seed_history(
        &gateway,
        "agent-filtered",
        &[
            (
                "2026-03-08T00:00:00Z",
                0.10,
                0,
                r#"{"behavioral_anomaly":0.1}"#,
            ),
            (
                "2026-03-08T01:00:00Z",
                0.40,
                1,
                r#"{"behavioral_anomaly":0.4}"#,
            ),
            (
                "2026-03-08T02:00:00Z",
                0.70,
                2,
                r#"{"behavioral_anomaly":0.7}"#,
            ),
            (
                "2026-03-08T03:00:00Z",
                0.90,
                3,
                r#"{"behavioral_anomaly":0.9}"#,
            ),
        ],
    )
    .await;

    let response = gateway
        .client
        .get(
            gateway
                .url("/api/convergence/history/agent-filtered?since=2026-03-08T01:00:00Z&limit=2"),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let entries = body["entries"].as_array().unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["computed_at"], "2026-03-08T02:00:00Z");
    assert_eq!(entries[1]["computed_at"], "2026-03-08T03:00:00Z");
    assert_eq!(entries[1]["level"], 3);

    gateway.stop().await;
}
