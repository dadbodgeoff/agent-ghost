//! ADE-side bridge from typed ITP events to the monitor ingest contract.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ghost_agent_loop::itp_emitter::ITPEmitter;
use itp_protocol::events::{ITPEvent, SessionEndEvent};
use serde::Serialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::itp_router::ITPEventRouter;

#[derive(Debug, Clone, Serialize)]
struct MonitorIngestEvent {
    session_id: Uuid,
    agent_id: Uuid,
    event_type: MonitorEventType,
    payload: serde_json::Value,
    timestamp: DateTime<Utc>,
    source: MonitorEventSource,
}

#[derive(Debug, Clone, Copy, Serialize)]
enum MonitorEventType {
    SessionStart,
    SessionEnd,
    InteractionMessage,
    AgentStateSnapshot,
    ConvergenceAlert,
}

#[derive(Debug, Clone, Serialize)]
enum MonitorEventSource {
    AgentLoop,
}

#[derive(Debug, Clone)]
struct TrackedSession {
    agent_id: Uuid,
    channel: String,
    last_seen: Instant,
}

#[derive(Debug, Clone)]
pub struct ExpiredSession {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub channel: String,
}

pub struct ITPSessionTracker {
    sessions: tokio::sync::RwLock<HashMap<Uuid, TrackedSession>>,
    idle_timeout: Duration,
}

impl ITPSessionTracker {
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            sessions: tokio::sync::RwLock::new(HashMap::new()),
            idle_timeout,
        }
    }

    pub async fn record_start(&self, session_id: Uuid, agent_id: Uuid, channel: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        match sessions.get_mut(&session_id) {
            Some(existing) if existing.agent_id == agent_id && existing.channel == channel => {
                existing.last_seen = Instant::now();
                false
            }
            Some(existing) => {
                existing.agent_id = agent_id;
                existing.channel = channel.to_string();
                existing.last_seen = Instant::now();
                true
            }
            None => {
                sessions.insert(
                    session_id,
                    TrackedSession {
                        agent_id,
                        channel: channel.to_string(),
                        last_seen: Instant::now(),
                    },
                );
                true
            }
        }
    }

    pub async fn touch(&self, session_id: Uuid) -> Option<Uuid> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(&session_id)?;
        session.last_seen = Instant::now();
        Some(session.agent_id)
    }

    pub async fn remove(&self, session_id: Uuid) -> Option<ExpiredSession> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session_id).map(|tracked| ExpiredSession {
            session_id,
            agent_id: tracked.agent_id,
            channel: tracked.channel,
        })
    }

    pub async fn reap_expired(&self) -> Vec<ExpiredSession> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        let expired_ids = sessions
            .iter()
            .filter_map(|(session_id, tracked)| {
                (now.duration_since(tracked.last_seen) >= self.idle_timeout).then_some(*session_id)
            })
            .collect::<Vec<_>>();

        expired_ids
            .into_iter()
            .filter_map(|session_id| {
                sessions.remove(&session_id).map(|tracked| ExpiredSession {
                    session_id,
                    agent_id: tracked.agent_id,
                    channel: tracked.channel,
                })
            })
            .collect()
    }

    pub async fn drain_all(&self) -> Vec<ExpiredSession> {
        let mut sessions = self.sessions.write().await;
        sessions
            .drain()
            .map(|(session_id, tracked)| ExpiredSession {
                session_id,
                agent_id: tracked.agent_id,
                channel: tracked.channel,
            })
            .collect()
    }
}

pub fn channel() -> (ITPEmitter, mpsc::Receiver<ITPEvent>) {
    ITPEmitter::channel()
}

pub async fn run_bridge(
    mut receiver: mpsc::Receiver<ITPEvent>,
    router: Arc<ITPEventRouter>,
    tracker: Arc<ITPSessionTracker>,
) {
    while let Some(event) = receiver.recv().await {
        match normalize_event(event, &tracker).await {
            Ok(Some(ingest_event)) => match serde_json::to_string(&ingest_event) {
                Ok(payload) => router.route(payload).await,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to serialize monitor ingest event");
                }
            },
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(error = %error, "failed to normalize ITP event for monitor");
            }
        }
    }
}

pub async fn reap_idle_sessions_task(
    tracker: Arc<ITPSessionTracker>,
    router: Arc<ITPEventRouter>,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let expired = tracker.reap_expired().await;
        for session in expired {
            if let Ok(payload) = session_end_ingest_event(
                session.session_id,
                session.agent_id,
                SessionEndEvent {
                    session_id: session.session_id,
                    agent_id: session.agent_id,
                    reason: "idle_timeout".to_string(),
                    message_count: 0,
                    timestamp: Utc::now(),
                },
            )
            .and_then(|event| serde_json::to_string(&event).map_err(|error| error.to_string()))
            {
                router.route(payload).await;
            }
        }
    }
}

pub async fn close_sessions(
    tracker: Arc<ITPSessionTracker>,
    router: Arc<ITPEventRouter>,
    reason: &str,
) {
    for session in tracker.drain_all().await {
        if let Ok(payload) = session_end_ingest_event(
            session.session_id,
            session.agent_id,
            SessionEndEvent {
                session_id: session.session_id,
                agent_id: session.agent_id,
                reason: reason.to_string(),
                message_count: 0,
                timestamp: Utc::now(),
            },
        )
        .and_then(|event| serde_json::to_string(&event).map_err(|error| error.to_string()))
        {
            let _ = router.send_direct(payload).await;
        }
    }
}

async fn normalize_event(
    event: ITPEvent,
    tracker: &ITPSessionTracker,
) -> Result<Option<MonitorIngestEvent>, String> {
    match event {
        ITPEvent::SessionStart(data) => {
            if !tracker
                .record_start(data.session_id, data.agent_id, &data.channel)
                .await
            {
                return Ok(None);
            }
            Ok(Some(MonitorIngestEvent {
                session_id: data.session_id,
                agent_id: data.agent_id,
                event_type: MonitorEventType::SessionStart,
                timestamp: data.timestamp,
                payload: serde_json::to_value(&data).map_err(|error| error.to_string())?,
                source: MonitorEventSource::AgentLoop,
            }))
        }
        ITPEvent::SessionEnd(data) => {
            tracker.remove(data.session_id).await;
            session_end_ingest_event(data.session_id, data.agent_id, data)
        }
        ITPEvent::InteractionMessage(data) => {
            let agent_id = tracker.touch(data.session_id).await.ok_or_else(|| {
                format!(
                    "missing session mapping for interaction message session {}",
                    data.session_id
                )
            })?;

            Ok(Some(MonitorIngestEvent {
                session_id: data.session_id,
                agent_id,
                event_type: MonitorEventType::InteractionMessage,
                timestamp: data.timestamp,
                payload: serde_json::to_value(&data).map_err(|error| error.to_string())?,
                source: MonitorEventSource::AgentLoop,
            }))
        }
        ITPEvent::AgentStateSnapshot(data) => {
            let _ = tracker.touch(data.session_id).await;
            Ok(Some(MonitorIngestEvent {
                session_id: data.session_id,
                agent_id: data.agent_id,
                event_type: MonitorEventType::AgentStateSnapshot,
                timestamp: data.timestamp,
                payload: serde_json::to_value(&data).map_err(|error| error.to_string())?,
                source: MonitorEventSource::AgentLoop,
            }))
        }
        ITPEvent::ConvergenceAlert(data) => {
            let _ = tracker.touch(data.session_id).await;
            Ok(Some(MonitorIngestEvent {
                session_id: data.session_id,
                agent_id: data.agent_id,
                event_type: MonitorEventType::ConvergenceAlert,
                timestamp: data.timestamp,
                payload: serde_json::to_value(&data).map_err(|error| error.to_string())?,
                source: MonitorEventSource::AgentLoop,
            }))
        }
    }
}

fn session_end_ingest_event(
    session_id: Uuid,
    agent_id: Uuid,
    data: SessionEndEvent,
) -> Result<Option<MonitorIngestEvent>, String> {
    Ok(Some(MonitorIngestEvent {
        session_id,
        agent_id,
        event_type: MonitorEventType::SessionEnd,
        timestamp: data.timestamp,
        payload: serde_json::to_value(&data).map_err(|error| error.to_string())?,
        source: MonitorEventSource::AgentLoop,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::Router;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::gateway::GatewayState;

    #[tokio::test]
    async fn bridge_normalizes_interaction_messages_with_agent_id() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let app = Router::new()
            .route(
                "/events",
                post(
                    |State(captured): State<Arc<Mutex<Vec<String>>>>, body: String| async move {
                        captured.lock().await.push(body);
                        StatusCode::OK
                    },
                ),
            )
            .with_state(Arc::clone(&captured));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test monitor");
        let addr = listener.local_addr().expect("test monitor addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve test monitor");
        });

        let router = Arc::new(crate::itp_router::ITPEventRouter::new(
            Arc::new(AtomicU8::new(GatewayState::Healthy as u8)),
            addr.to_string(),
        ));
        let tracker = Arc::new(super::ITPSessionTracker::new(Duration::from_secs(60)));
        let (emitter, receiver) = super::channel();
        let bridge = tokio::spawn(super::run_bridge(receiver, Arc::clone(&router), tracker));
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        emitter.emit_session_start(agent_id, session_id, "api");
        emitter.emit_interaction_message(agent_id, session_id, "hello from ade");
        drop(emitter);

        bridge.await.expect("bridge completed");

        let payloads = captured.lock().await.clone();
        assert_eq!(payloads.len(), 2);

        let session_start: serde_json::Value =
            serde_json::from_str(&payloads[0]).expect("session start json");
        let interaction: serde_json::Value =
            serde_json::from_str(&payloads[1]).expect("interaction json");

        assert_eq!(session_start["event_type"], "SessionStart");
        assert_eq!(interaction["event_type"], "InteractionMessage");
        assert_eq!(interaction["agent_id"], agent_id.to_string());
        assert_eq!(interaction["session_id"], session_id.to_string());

        server.abort();
    }

    #[tokio::test]
    async fn bridge_deduplicates_repeated_session_start_for_same_session() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let app = Router::new()
            .route(
                "/events",
                post(
                    |State(captured): State<Arc<Mutex<Vec<String>>>>, body: String| async move {
                        captured.lock().await.push(body);
                        StatusCode::OK
                    },
                ),
            )
            .with_state(Arc::clone(&captured));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test monitor");
        let addr = listener.local_addr().expect("test monitor addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve test monitor");
        });

        let router = Arc::new(crate::itp_router::ITPEventRouter::new(
            Arc::new(AtomicU8::new(GatewayState::Healthy as u8)),
            addr.to_string(),
        ));
        let tracker = Arc::new(super::ITPSessionTracker::new(Duration::from_secs(60)));
        let (emitter, receiver) = super::channel();
        let bridge = tokio::spawn(super::run_bridge(receiver, Arc::clone(&router), tracker));
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        emitter.emit_session_start(agent_id, session_id, "api");
        emitter.emit_session_start(agent_id, session_id, "api");
        emitter.emit_interaction_message(agent_id, session_id, "hello from ade");
        drop(emitter);

        bridge.await.expect("bridge completed");

        let payloads = captured.lock().await.clone();
        assert_eq!(payloads.len(), 2);

        let session_start: serde_json::Value =
            serde_json::from_str(&payloads[0]).expect("session start json");
        let interaction: serde_json::Value =
            serde_json::from_str(&payloads[1]).expect("interaction json");

        assert_eq!(session_start["event_type"], "SessionStart");
        assert_eq!(interaction["event_type"], "InteractionMessage");

        server.abort();
    }

    #[tokio::test]
    async fn session_tracker_reaps_idle_sessions_as_session_end() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let app = Router::new()
            .route(
                "/events",
                post(
                    |State(captured): State<Arc<Mutex<Vec<String>>>>, body: String| async move {
                        captured.lock().await.push(body);
                        StatusCode::OK
                    },
                ),
            )
            .with_state(Arc::clone(&captured));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test monitor");
        let addr = listener.local_addr().expect("test monitor addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve test monitor");
        });

        let tracker = Arc::new(super::ITPSessionTracker::new(Duration::ZERO));
        let router = Arc::new(crate::itp_router::ITPEventRouter::new(
            Arc::new(AtomicU8::new(GatewayState::Healthy as u8)),
            addr.to_string(),
        ));

        tracker
            .record_start(Uuid::nil(), Uuid::now_v7(), "api")
            .await;
        let reaper = tokio::spawn(super::reap_idle_sessions_task(
            Arc::clone(&tracker),
            Arc::clone(&router),
            Duration::from_millis(10),
        ));

        tokio::time::sleep(Duration::from_millis(30)).await;
        let payloads = captured.lock().await.clone();
        assert!(!payloads.is_empty());
        let last: serde_json::Value =
            serde_json::from_str(payloads.last().expect("session end payload")).unwrap();
        assert_eq!(last["event_type"], "SessionEnd");

        reaper.abort();
        server.abort();
    }
}
