mod common;

use rusqlite::params;

async fn seed_itp_event(
    gateway: &common::TestGateway,
    id: &str,
    session_id: &str,
    event_type: &str,
    sender: Option<&str>,
    timestamp: &str,
    sequence_number: i64,
    privacy_level: &str,
    content_length: Option<i64>,
    attributes: &serde_json::Value,
) {
    let writer = gateway.app_state.db.write().await;
    writer
        .execute(
            "INSERT INTO itp_events (
                id,
                session_id,
                event_type,
                sender,
                timestamp,
                sequence_number,
                content_length,
                privacy_level,
                event_hash,
                previous_hash,
                attributes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                session_id,
                event_type,
                sender,
                timestamp,
                sequence_number,
                content_length,
                privacy_level,
                vec![sequence_number as u8 + 1; 32],
                vec![sequence_number as u8; 32],
                attributes.to_string(),
            ],
        )
        .unwrap();
}

#[tokio::test]
async fn itp_events_endpoint_returns_truthful_counts_and_paths() {
    let gateway = common::TestGateway::start().await;

    seed_itp_event(
        &gateway,
        "evt-1",
        "sess-a",
        "turn_complete",
        Some("agent-1"),
        "2026-03-08T00:00:00Z",
        3,
        "standard",
        Some(144),
        &serde_json::json!({
            "route": "agent_chat",
            "source": "browser_extension",
            "platform": "chatgpt",
        }),
    )
    .await;
    seed_itp_event(
        &gateway,
        "evt-2",
        "sess-b",
        "tool_use",
        Some("filesystem"),
        "2026-03-08T00:01:00Z",
        9,
        "standard",
        Some(42),
        &serde_json::json!({
            "route": "studio",
        }),
    )
    .await;

    let response =
        gateway
            .client
            .get(gateway.url(
                "/api/itp/events?limit=25&offset=0&session_id=sess-a&event_type=turn_complete",
            ))
            .send()
            .await
            .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let events = body["events"].as_array().unwrap();
    let first = &events[0];

    assert_eq!(body["limit"], 25);
    assert_eq!(body["offset"], 0);
    assert_eq!(body["total_persisted"], 2);
    assert_eq!(body["total_filtered"], 1);
    assert_eq!(body["returned"], 1);
    assert_eq!(body["monitor_connected"], false);
    assert_eq!(body["live_updates_supported"], true);

    assert_eq!(first["id"], "evt-1");
    assert_eq!(first["event_type"], "turn_complete");
    assert_eq!(first["session_id"], "sess-a");
    assert_eq!(first["sequence_number"], 3);
    assert_eq!(first["sender"], "agent-1");
    assert_eq!(first["source"], "browser_extension");
    assert_eq!(first["platform"], "chatgpt");
    assert_eq!(first["route"], "agent_chat");
    assert_eq!(first["content_length"], 144);
    assert_eq!(first["privacy_level"], "standard");
    assert_eq!(first["session_path"], "/sessions/sess-a");
    assert_eq!(first["replay_path"], "/sessions/sess-a/replay");

    gateway.stop().await;
}
