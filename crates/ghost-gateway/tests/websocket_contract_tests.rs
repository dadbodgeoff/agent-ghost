#![allow(clippy::await_holding_lock)]

mod common;

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use ghost_gateway::api::websocket::{broadcast_event, WsEnvelope, WsEvent};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::common::TestGateway;

fn auth_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn auth_env_guard() -> std::sync::MutexGuard<'static, ()> {
    auth_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(ref value) = self.previous {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

async fn read_envelope(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> WsEnvelope {
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("timed out waiting for websocket message")
        .expect("websocket stream ended")
        .expect("failed to read websocket message");

    let Message::Text(text) = message else {
        panic!("expected text websocket message, got {message:?}");
    };

    serde_json::from_str::<WsEnvelope>(&text).expect("failed to parse websocket envelope")
}

async fn read_non_ping_envelope(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> WsEnvelope {
    for _ in 0..3 {
        let envelope = read_envelope(socket).await;
        if !matches!(envelope.event, WsEvent::Ping) {
            return envelope;
        }
    }
    panic!("timed out waiting for non-ping websocket envelope");
}

#[tokio::test]
async fn websocket_allows_legacy_auth_when_ticket_only_mode_is_disabled() {
    let _guard = auth_env_guard();
    let _env = EnvVarGuard::set("GHOST_TOKEN", "ws-test-token");

    let gateway = TestGateway::start().await;
    let client = reqwest::Client::new();
    let ws_url = format!("ws://127.0.0.1:{}/api/ws", gateway.port);

    let err = connect_async(&ws_url)
        .await
        .expect_err("unauthenticated websocket connection should fail");
    match err {
        tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), 401);
        }
        other => panic!("expected HTTP auth failure, got {other:?}"),
    }

    let ticket_response: serde_json::Value = client
        .post(gateway.url("/api/ws/tickets"))
        .bearer_auth("ws-test-token")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ticket = ticket_response["ticket"].as_str().unwrap();

    let mut ticket_request = ws_url.clone().into_client_request().unwrap();
    ticket_request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        format!("ghost-ticket.{ticket}").parse().unwrap(),
    );
    let (mut ticket_socket, _) = connect_async(ticket_request)
        .await
        .expect("ticket websocket connection should succeed");

    let envelope = read_envelope(&mut ticket_socket).await;
    assert!(matches!(envelope.event, WsEvent::Ping));
    let _ = ticket_socket.close(None).await;

    let mut reused_ticket_request = ws_url.clone().into_client_request().unwrap();
    reused_ticket_request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        format!("ghost-ticket.{ticket}").parse().unwrap(),
    );
    let reused = connect_async(reused_ticket_request)
        .await
        .expect_err("reused websocket ticket should fail");
    match reused {
        tungstenite::Error::Http(response) => assert_eq!(response.status(), 401),
        other => panic!("expected HTTP auth failure, got {other:?}"),
    }

    let mut request = format!("{ws_url}?token=ws-test-token")
        .into_client_request()
        .unwrap();
    request.headers_mut().remove("Sec-WebSocket-Protocol");
    let (mut socket, _) = connect_async(request)
        .await
        .expect("query-token websocket connection should succeed");

    let envelope = read_envelope(&mut socket).await;
    assert!(matches!(envelope.event, WsEvent::Ping));

    let _ = socket.close(None).await;
    gateway.stop().await;
}

#[tokio::test]
async fn websocket_rejects_legacy_auth_when_ticket_only_mode_is_enabled() {
    let _guard = auth_env_guard();
    let _env = EnvVarGuard::set("GHOST_TOKEN", "ws-test-token");

    let gateway = TestGateway::start_with_ws_ticket_auth_only(true).await;
    let client = reqwest::Client::new();
    let ws_url = format!("ws://127.0.0.1:{}/api/ws", gateway.port);

    let ticket_response: serde_json::Value = client
        .post(gateway.url("/api/ws/tickets"))
        .bearer_auth("ws-test-token")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ticket = ticket_response["ticket"].as_str().unwrap();

    let mut ticket_request = ws_url.clone().into_client_request().unwrap();
    ticket_request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        format!("ghost-ticket.{ticket}").parse().unwrap(),
    );
    let (mut ticket_socket, _) = connect_async(ticket_request)
        .await
        .expect("ticket websocket connection should succeed");
    let envelope = read_envelope(&mut ticket_socket).await;
    assert!(matches!(envelope.event, WsEvent::Ping));
    let _ = ticket_socket.close(None).await;

    let mut protocol_request = ws_url.clone().into_client_request().unwrap();
    protocol_request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "ghost-token.ws-test-token".parse().unwrap(),
    );
    let protocol_err = connect_async(protocol_request)
        .await
        .expect_err("legacy subprotocol auth should be rejected");
    match protocol_err {
        tungstenite::Error::Http(response) => assert_eq!(response.status(), 401),
        other => panic!("expected HTTP auth failure, got {other:?}"),
    }

    let query_err = connect_async(format!("{ws_url}?token=ws-test-token"))
        .await
        .expect_err("legacy query auth should be rejected");
    match query_err {
        tungstenite::Error::Http(response) => assert_eq!(response.status(), 401),
        other => panic!("expected HTTP auth failure, got {other:?}"),
    }

    gateway.stop().await;
}

#[tokio::test]
async fn websocket_replays_events_after_last_seq() {
    let _guard = auth_env_guard();
    let gateway = TestGateway::start().await;
    let ws_url = format!("ws://127.0.0.1:{}/api/ws", gateway.port);

    let (mut first_socket, _) = connect_async(&ws_url).await.unwrap();
    let ping = read_envelope(&mut first_socket).await;
    assert!(matches!(ping.event, WsEvent::Ping));

    broadcast_event(
        &gateway.app_state,
        WsEvent::ScoreUpdate {
            agent_id: "agent-1".into(),
            score: 0.5,
            level: 2,
            signals: vec![0.1, 0.2],
        },
    );
    let first_event = read_non_ping_envelope(&mut first_socket).await;

    broadcast_event(
        &gateway.app_state,
        WsEvent::AgentStateChange {
            agent_id: "agent-1".into(),
            new_state: "Running".into(),
        },
    );
    let second_event = read_non_ping_envelope(&mut first_socket).await;

    let _ = first_socket.close(None).await;

    let (mut second_socket, _) = connect_async(&ws_url).await.unwrap();
    second_socket
        .send(Message::Text(
            serde_json::json!({ "last_seq": first_event.seq }).to_string(),
        ))
        .await
        .unwrap();

    let reconnect_ping = read_envelope(&mut second_socket).await;
    assert!(matches!(reconnect_ping.event, WsEvent::Ping));
    let envelope = read_non_ping_envelope(&mut second_socket).await;
    assert_eq!(envelope.seq, second_event.seq);
    match envelope.event {
        WsEvent::AgentStateChange {
            ref agent_id,
            ref new_state,
        } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(new_state, "Running");
        }
        other => panic!("expected replayed AgentStateChange event, got {other:?}"),
    }

    let _ = second_socket.close(None).await;
    gateway.stop().await;
}

#[tokio::test]
async fn websocket_reconnect_replays_only_subscribed_topics() {
    let _guard = auth_env_guard();
    let gateway = TestGateway::start().await;
    let ws_url = format!("ws://127.0.0.1:{}/api/ws", gateway.port);

    let (mut first_socket, _) = connect_async(&ws_url).await.unwrap();
    let ping = read_envelope(&mut first_socket).await;
    assert!(matches!(ping.event, WsEvent::Ping));

    broadcast_event(
        &gateway.app_state,
        WsEvent::ScoreUpdate {
            agent_id: "alpha".into(),
            score: 0.5,
            level: 2,
            signals: vec![0.1, 0.2],
        },
    );
    let first_event = read_non_ping_envelope(&mut first_socket).await;

    broadcast_event(
        &gateway.app_state,
        WsEvent::AgentStateChange {
            agent_id: "beta".into(),
            new_state: "Running".into(),
        },
    );
    let _ = read_non_ping_envelope(&mut first_socket).await;
    let _ = first_socket.close(None).await;

    let (mut second_socket, _) = connect_async(&ws_url).await.unwrap();
    second_socket
        .send(Message::Text(
            serde_json::json!({
                "last_seq": first_event.seq,
                "topics": ["agent:alpha"],
            })
            .to_string(),
        ))
        .await
        .unwrap();

    let reconnect_ping = read_envelope(&mut second_socket).await;
    assert!(matches!(reconnect_ping.event, WsEvent::Ping));

    let replay = tokio::time::timeout(Duration::from_millis(250), second_socket.next()).await;
    assert!(
        replay.is_err(),
        "off-topic replay should be filtered when reconnect topics are provided"
    );

    broadcast_event(
        &gateway.app_state,
        WsEvent::ScoreUpdate {
            agent_id: "alpha".into(),
            score: 0.9,
            level: 3,
            signals: vec![0.4, 0.5],
        },
    );
    let live_event = read_non_ping_envelope(&mut second_socket).await;
    match live_event.event {
        WsEvent::ScoreUpdate { ref agent_id, .. } => assert_eq!(agent_id, "alpha"),
        other => panic!("expected alpha ScoreUpdate event, got {other:?}"),
    }

    let _ = second_socket.close(None).await;
    gateway.stop().await;
}

#[tokio::test]
async fn websocket_sends_resync_when_replay_gap_is_too_large() {
    let _guard = auth_env_guard();
    let gateway = TestGateway::start_with_replay_capacity(1).await;
    let ws_url = format!("ws://127.0.0.1:{}/api/ws", gateway.port);

    let (mut first_socket, _) = connect_async(&ws_url).await.unwrap();
    let ping = read_envelope(&mut first_socket).await;
    assert!(matches!(ping.event, WsEvent::Ping));

    broadcast_event(
        &gateway.app_state,
        WsEvent::ScoreUpdate {
            agent_id: "agent-1".into(),
            score: 0.1,
            level: 1,
            signals: vec![0.1],
        },
    );
    let first_event = read_non_ping_envelope(&mut first_socket).await;

    broadcast_event(
        &gateway.app_state,
        WsEvent::AgentStateChange {
            agent_id: "agent-1".into(),
            new_state: "Paused".into(),
        },
    );
    let _second_event = read_non_ping_envelope(&mut first_socket).await;

    let _ = first_socket.close(None).await;

    let (mut second_socket, _) = connect_async(&ws_url).await.unwrap();
    second_socket
        .send(Message::Text(
            serde_json::json!({ "last_seq": first_event.seq }).to_string(),
        ))
        .await
        .unwrap();

    let reconnect_ping = read_envelope(&mut second_socket).await;
    assert!(matches!(reconnect_ping.event, WsEvent::Ping));
    let envelope = read_non_ping_envelope(&mut second_socket).await;
    match envelope.event {
        WsEvent::Resync { missed_events } => {
            assert_eq!(missed_events, 0);
        }
        other => panic!("expected Resync event for replay gap, got {other:?}"),
    }

    let _ = second_socket.close(None).await;
    gateway.stop().await;
}

#[tokio::test]
async fn concurrent_broadcast_maintains_monotonic_order() {
    let _guard = auth_env_guard();
    let gateway = TestGateway::start_with_replay_capacity(2048).await;
    let mut event_rx = gateway.app_state.event_tx.subscribe();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(11));
    let mut handles = Vec::new();

    for producer in 0..10 {
        let state = std::sync::Arc::clone(&gateway.app_state);
        let barrier = std::sync::Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            for offset in 0..100 {
                broadcast_event(
                    &state,
                    WsEvent::SystemWarning {
                        message: format!("producer-{producer}-{offset}"),
                    },
                );
                tokio::task::yield_now().await;
            }
        }));
    }

    barrier.wait().await;

    let mut seqs = Vec::with_capacity(1000);
    while seqs.len() < 1000 {
        let envelope = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
            .await
            .expect("timed out waiting for broadcast event")
            .expect("broadcast channel closed");
        seqs.push(envelope.seq);
    }

    for handle in handles {
        handle.await.expect("producer task should complete");
    }

    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "broadcast sequence must be strictly increasing: {seqs:?}"
    );

    gateway.stop().await;
}
