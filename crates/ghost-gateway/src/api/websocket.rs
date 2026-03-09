//! WebSocket upgrade handler for real-time events (Req 25 AC3).
//!
//! Pushes convergence score updates, intervention level changes,
//! kill switch activations, and proposal decisions to connected clients.
//! Supports per-client topic subscriptions (T-2.1.8).

use std::collections::{HashSet, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::api::auth::{AuthConfig, Claims, RevocationSet};
use crate::state::AppState;

/// Global monotonic sequence counter for WS events.
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Envelope wrapping every WS event with sequence metadata.
///
/// Wire format: `{ "seq": N, "timestamp": "...", "event": { "type": "...", ... } }`
/// Using a nested `event` field (not `serde(flatten)`) to avoid fragile
/// deserialization edge cases and field name conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsEnvelope {
    pub seq: u64,
    pub timestamp: String,
    pub event: WsEvent,
}

/// Ring buffer for event replay on reconnect.
pub struct EventReplayBuffer {
    buffer: parking_lot::RwLock<VecDeque<WsEnvelope>>,
    publish_lock: parking_lot::Mutex<()>,
    capacity: usize,
}

impl EventReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: parking_lot::RwLock::new(VecDeque::with_capacity(capacity)),
            publish_lock: parking_lot::Mutex::new(()),
            capacity,
        }
    }

    fn push(&self, envelope: WsEnvelope) {
        let mut buf = self.buffer.write();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(envelope);
    }

    pub fn push_and_broadcast(
        &self,
        event: WsEvent,
        tx: &broadcast::Sender<WsEnvelope>,
    ) -> (WsEnvelope, bool) {
        let _publish_guard = self.publish_lock.lock();
        let envelope = WsEnvelope {
            seq: EVENT_SEQ.fetch_add(1, Ordering::SeqCst) + 1,
            timestamp: chrono::Utc::now().to_rfc3339(),
            event,
        };
        self.push(envelope.clone());
        let has_receivers = tx.send(envelope.clone()).is_ok();
        (envelope, has_receivers)
    }

    /// Replay events after `last_seq`. Returns None if gap too large
    /// (including when the buffer is empty after a server restart).
    pub fn replay_after(&self, last_seq: u64) -> Option<Vec<WsEnvelope>> {
        let buf = self.buffer.read();
        if buf.is_empty() {
            // Buffer is empty (e.g., server just restarted). We cannot
            // guarantee continuity, so signal a gap to force full re-fetch.
            return None;
        }
        let first_available = buf.front().map(|e| e.seq).unwrap_or(0);
        let last_available = buf.back().map(|e| e.seq).unwrap_or(0);
        if last_seq < first_available || last_seq > last_available {
            // `last_seq > last_available` means the client observed a newer
            // sequence from a different gateway process/epoch, so continuity
            // cannot be proven and the client must resync.
            return None; // Gap too large — events were evicted
        }
        Some(buf.iter().filter(|e| e.seq > last_seq).cloned().collect())
    }
}

/// Helper: wrap a WsEvent in an envelope and push to replay buffer.
///
/// Uses a dedicated publication lock so `seq assignment -> replay append ->
/// broadcast send` happens in a single critical section.
pub fn broadcast_event(state: &crate::state::AppState, event: WsEvent) {
    if !state
        .replay_buffer
        .push_and_broadcast(event, &state.event_tx)
        .1
    {
        tracing::debug!("broadcast_event: no WebSocket receivers");
    }
}

/// Query parameters for WebSocket connection (token auth).
#[derive(Debug, Deserialize)]
pub struct WsQueryParams {
    pub token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WsAuthTicket {
    pub subject: String,
    pub role: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct WsAuthTicketResponse {
    pub ticket: String,
    pub expires_at: String,
    pub expires_in_secs: u64,
}

const WS_AUTH_TICKET_TTL_SECS: i64 = 30;

/// Client-to-server messages (T-2.1.8).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum WsClientMessage {
    /// Subscribe to specific topics (e.g., "agent:<uuid>", "session:<uuid>").
    Subscribe { topics: Vec<String> },
    /// Unsubscribe from topics.
    Unsubscribe { topics: Vec<String> },
}

/// Events pushed to WebSocket clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    /// Convergence score updated for an agent.
    ScoreUpdate {
        agent_id: String,
        score: f64,
        level: u8,
        signals: Vec<f64>,
    },
    /// Intervention level changed.
    InterventionChange {
        agent_id: String,
        old_level: u8,
        new_level: u8,
    },
    /// Kill switch activated.
    KillSwitchActivation {
        level: String,
        agent_id: Option<String>,
        reason: String,
    },
    /// Proposal decision made.
    ProposalDecision {
        proposal_id: String,
        decision: String,
        agent_id: String,
    },
    /// Agent state changed (created, deleted, lifecycle transition).
    AgentStateChange { agent_id: String, new_state: String },
    /// Session event for live DAG updates and replay (T-2.1.8).
    SessionEvent {
        session_id: String,
        event_id: String,
        event_type: String,
        sender: Option<String>,
        sequence_number: i64,
    },
    /// Agent configuration changed (T-3.6.2).
    AgentConfigChange {
        agent_id: String,
        changed_fields: Vec<String>,
    },
    /// OTel trace updated for a session (T-3.8).
    TraceUpdate {
        session_id: String,
        trace_id: String,
        span_count: u32,
    },
    /// Backup operation completed (T-3.4).
    BackupComplete {
        backup_id: String,
        status: String,
        size_bytes: u64,
    },
    /// Webhook fired notification (T-4.3.1).
    WebhookFired {
        webhook_id: String,
        event_type: String,
        status_code: u16,
    },
    /// Skill installed or uninstalled (T-4.2.1).
    SkillChange { skill_name: String, action: String },
    /// A2A task status update (T-4.1.2).
    A2ATaskUpdate {
        task_id: String,
        status: String,
        agent_name: String,
    },
    /// Studio chat message (new assistant response in a session).
    ChatMessage {
        session_id: String,
        message_id: String,
        role: String,
        content: String,
        safety_status: String,
    },
    /// Heartbeat to keep connection alive.
    Ping,
    /// T-5.3.4 (T-X.28): Resync signal — sent when client lagged behind broadcast.
    /// Client should perform a full REST re-fetch on all stores.
    Resync { missed_events: u64 },
    /// WP4-C/WP9-J: System-level warning for dashboard display.
    SystemWarning { message: String },
    /// WP9-J: Context window pressure — a prompt layer was truncated.
    ContextTruncated {
        layer: String,
        removed_tokens: usize,
    },
}

pub async fn issue_ws_ticket(
    State(state): State<Arc<AppState>>,
    claims: Option<axum::Extension<Claims>>,
) -> Json<WsAuthTicketResponse> {
    let claims = claims
        .map(|extension| extension.0)
        .unwrap_or_else(Claims::no_auth_fallback);
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(WS_AUTH_TICKET_TTL_SECS);
    let ticket = Uuid::new_v4().to_string();
    prune_expired_ws_tickets(&state, now);
    state.websocket_auth_tickets.insert(
        hash_ws_ticket(&ticket),
        WsAuthTicket {
            subject: claims.sub,
            role: claims.role,
            issued_at: now,
            expires_at,
        },
    );
    Json(WsAuthTicketResponse {
        ticket,
        expires_at: expires_at.to_rfc3339(),
        expires_in_secs: WS_AUTH_TICKET_TTL_SECS as u64,
    })
}

impl WsEvent {
    /// Extract the topic key(s) this event matches (T-2.1.8).
    /// Used for per-client topic filtering. An empty list means
    /// the event always passes through (e.g., Ping).
    fn topic_keys(&self) -> Vec<String> {
        match self {
            WsEvent::ScoreUpdate { agent_id, .. } => vec![format!("agent:{agent_id}")],
            WsEvent::InterventionChange { agent_id, .. } => vec![format!("agent:{agent_id}")],
            WsEvent::KillSwitchActivation { agent_id, .. } => {
                let mut keys = vec!["system:kill".to_string()];
                if let Some(aid) = agent_id {
                    keys.push(format!("agent:{aid}"));
                }
                keys
            }
            WsEvent::ProposalDecision { agent_id, .. } => {
                vec!["proposals".to_string(), format!("agent:{agent_id}")]
            }
            WsEvent::AgentStateChange { agent_id, .. } => vec![format!("agent:{agent_id}")],
            WsEvent::SessionEvent { session_id, .. } => vec![format!("session:{session_id}")],
            WsEvent::AgentConfigChange { agent_id, .. } => vec![format!("agent:{agent_id}")],
            WsEvent::TraceUpdate { session_id, .. } => vec![format!("session:{session_id}")],
            WsEvent::BackupComplete { .. } => vec!["system:backup".to_string()],
            WsEvent::WebhookFired { .. } => vec!["system:webhooks".to_string()],
            WsEvent::SkillChange { .. } => vec!["system:skills".to_string()],
            WsEvent::A2ATaskUpdate { .. } => vec!["a2a:tasks".to_string()],
            WsEvent::ChatMessage { session_id, .. } => vec![format!("studio:session:{session_id}")],
            WsEvent::Ping | WsEvent::Resync { .. } => vec![],
            WsEvent::SystemWarning { .. } | WsEvent::ContextTruncated { .. } => {
                vec!["system:warning".to_string()]
            }
        }
    }
}

/// Per-IP WebSocket connection counter.
/// Limits concurrent connections to `MAX_WS_PER_IP` per IP address.
pub struct WsConnectionTracker {
    connections: DashMap<IpAddr, u32>,
}

/// Maximum concurrent WebSocket connections per IP (T-1.1.5).
const MAX_WS_PER_IP: u32 = 5;

impl WsConnectionTracker {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Try to acquire a connection slot. Returns false if limit reached.
    fn acquire(&self, ip: IpAddr) -> bool {
        let mut entry = self.connections.entry(ip).or_insert(0);
        if *entry >= MAX_WS_PER_IP {
            false
        } else {
            *entry += 1;
            true
        }
    }

    /// Release a connection slot.
    fn release(&self, ip: IpAddr) {
        if let Some(mut entry) = self.connections.get_mut(&ip) {
            *entry = entry.saturating_sub(1);
            if *entry == 0 {
                drop(entry);
                self.connections.remove(&ip);
            }
        }
    }
}

impl Default for WsConnectionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// GET /api/ws — WebSocket upgrade with ticket-based auth.
///
/// Auth modes (same as REST middleware):
/// 1. Preferred: validate a short-lived `ghost-ticket.<ticket>` subprotocol
/// 2. Deprecated: validate `ghost-token.<jwt_or_legacy_token>` or `?token=`
///    unless `gateway.ws_ticket_auth_only` disables legacy auth entirely
/// 3. No-auth mode: allow if neither is set
///
/// Rate limiting: max 5 concurrent WebSocket connections per IP (T-1.1.5).
pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth_config): axum::Extension<Arc<AuthConfig>>,
    axum::Extension(revocation_set): axum::Extension<Arc<RevocationSet>>,
    axum::Extension(ws_tracker): axum::Extension<Arc<WsConnectionTracker>>,
    connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
    Query(params): Query<WsQueryParams>,
) -> impl IntoResponse {
    let client_ip = connect_info
        .map(|ci| ci.0.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    let mut selected_protocol: Option<String> = None;

    // Rate limit: max concurrent WS connections per IP.
    if !ws_tracker.acquire(client_ip) {
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many WebSocket connections from this IP",
        )
            .into_response();
    }

    if auth_config.auth_required() {
        let protocols = headers
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let protocol_ticket = protocols
            .split(',')
            .map(|value| value.trim())
            .find_map(|value| value.strip_prefix("ghost-ticket."));
        let protocol_token = protocols
            .split(',')
            .map(|value| value.trim())
            .find_map(|value| value.strip_prefix("ghost-token."));

        if let Some(ticket) = protocol_ticket {
            if !consume_ws_ticket(&state, ticket) {
                ws_tracker.release(client_ip);
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            }
            selected_protocol = Some(format!("ghost-ticket.{ticket}"));
        } else {
            if state.ws_ticket_auth_only {
                ws_tracker.release(client_ip);
                if protocol_token.is_some() || params.token.is_some() {
                    tracing::warn!(
                        "legacy WebSocket auth rejected because ws_ticket_auth_only is enabled"
                    );
                    return (
                        axum::http::StatusCode::UNAUTHORIZED,
                        "Legacy WebSocket auth is disabled. Use POST /api/ws/tickets.",
                    )
                        .into_response();
                }
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            }

            let token = if let Some(token) = protocol_token {
                selected_protocol = Some(format!("ghost-token.{token}"));
                tracing::warn!(
                    "WebSocket bearer auth via subprotocol is deprecated — use POST /api/ws/tickets"
                );
                token.to_string()
            } else if let Some(ref token) = params.token {
                tracing::warn!(
                    "WebSocket auth via query param is deprecated — use POST /api/ws/tickets"
                );
                token.clone()
            } else {
                ws_tracker.release(client_ip);
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            };

            // Try JWT validation first.
            if let Some(ref secret) = auth_config.jwt_secret {
                let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
                let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
                validation.validate_exp = true;
                validation.leeway = 30;
                match jsonwebtoken::decode::<crate::api::auth::Claims>(&token, &key, &validation) {
                    Ok(data) => {
                        if !data.claims.jti.is_empty()
                            && revocation_set.is_revoked(&data.claims.jti)
                        {
                            ws_tracker.release(client_ip);
                            return axum::http::StatusCode::UNAUTHORIZED.into_response();
                        }
                    }
                    Err(_) => {
                        ws_tracker.release(client_ip);
                        return axum::http::StatusCode::UNAUTHORIZED.into_response();
                    }
                }
            } else if let Some(ref _expected) = auth_config.legacy_token {
                if !crate::auth::token_auth::validate_token(&token) {
                    ws_tracker.release(client_ip);
                    return axum::http::StatusCode::UNAUTHORIZED.into_response();
                }
            }
        }
    }

    let ws = if let Some(protocol) = selected_protocol {
        ws.protocols([protocol])
    } else {
        ws
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, ws_tracker, client_ip))
        .into_response()
}

/// Client reconnect message: `{ "last_seq": N, "topics": ["..."] }`.
#[derive(Debug, Deserialize)]
struct ReconnectMessage {
    last_seq: u64,
    #[serde(default)]
    topics: Vec<String>,
}

fn event_matches_subscriptions(event: &WsEvent, subscribed_topics: &HashSet<String>) -> bool {
    if subscribed_topics.is_empty() {
        return true;
    }
    let keys = event.topic_keys();
    keys.is_empty() || keys.iter().any(|key| subscribed_topics.contains(key))
}

async fn handle_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    ws_tracker: Arc<WsConnectionTracker>,
    client_ip: IpAddr,
) {
    // Send initial ping (wrapped in envelope).
    let ping_envelope = WsEnvelope {
        seq: 0,
        timestamp: chrono::Utc::now().to_rfc3339(),
        event: WsEvent::Ping,
    };
    let ping = match serde_json::to_string(&ping_envelope) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize WebSocket ping");
            ws_tracker.release(client_ip);
            return;
        }
    };
    if socket.send(Message::Text(ping)).await.is_err() {
        ws_tracker.release(client_ip);
        return;
    }

    // Per-client topic subscriptions (T-2.1.8).
    // Empty set = receive all events (backward compatible).
    let mut subscribed_topics: HashSet<String> = HashSet::new();

    // Subscribe to the broadcast channel for real-time events.
    let mut event_rx = state.event_tx.subscribe();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    // Consume the immediate first tick because we already sent an initial ping above.
    interval.tick().await;

    // Wait briefly for a potential reconnect message with last_seq.
    // Use a short timeout so fresh connections aren't delayed.
    let reconnect_check =
        tokio::time::timeout(std::time::Duration::from_millis(500), socket.recv()).await;

    if let Ok(Some(Ok(Message::Text(text)))) = reconnect_check {
        if let Ok(reconnect) = serde_json::from_str::<ReconnectMessage>(&text) {
            subscribed_topics.extend(reconnect.topics);
            tracing::info!(
                last_seq = reconnect.last_seq,
                "WS client reconnecting with last_seq"
            );
            match state.replay_buffer.replay_after(reconnect.last_seq) {
                Some(missed) => {
                    let replayable: Vec<_> = missed
                        .into_iter()
                        .filter(|envelope| {
                            event_matches_subscriptions(&envelope.event, &subscribed_topics)
                        })
                        .collect();
                    tracing::info!(count = replayable.len(), "Replaying missed events");
                    for envelope in replayable {
                        let json = match serde_json::to_string(&envelope) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json)).await.is_err() {
                            ws_tracker.release(client_ip);
                            return;
                        }
                    }
                }
                None => {
                    // Gap too large — send Resync directly to THIS client only.
                    // Do NOT broadcast to all clients (they are not affected).
                    tracing::warn!(
                        last_seq = reconnect.last_seq,
                        "Replay gap too large — sending Resync to reconnecting client"
                    );
                    let resync_envelope = WsEnvelope {
                        seq: 0,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        event: WsEvent::Resync { missed_events: 0 },
                    };
                    if let Ok(json) = serde_json::to_string(&resync_envelope) {
                        if socket.send(Message::Text(json)).await.is_err() {
                            ws_tracker.release(client_ip);
                            return;
                        }
                    }
                }
            }
        } else if let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) {
            // Not a reconnect message — handle as normal Subscribe/Unsubscribe.
            match client_msg {
                WsClientMessage::Subscribe { topics } => {
                    for topic in topics {
                        subscribed_topics.insert(topic);
                    }
                }
                WsClientMessage::Unsubscribe { topics } => {
                    for topic in &topics {
                        subscribed_topics.remove(topic);
                    }
                }
            }
        }
    }

    loop {
        tokio::select! {
            // Keepalive ping every 30s (wrapped in envelope).
            _ = interval.tick() => {
                let ping_envelope = WsEnvelope {
                    seq: 0,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    event: WsEvent::Ping,
                };
                let ping = match serde_json::to_string(&ping_envelope) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, "failed to serialize WebSocket ping");
                        break;
                    }
                };
                if socket.send(Message::Text(ping)).await.is_err() {
                    break;
                }
            }
            // Forward broadcast events (WsEnvelope) to this WebSocket client.
            event = event_rx.recv() => {
                match event {
                    Ok(envelope) => {
                        // Topic filtering (T-2.1.8): if client has subscriptions,
                        // only forward events matching subscribed topics.
                        // Pings always pass through.
                        if !event_matches_subscriptions(&envelope.event, &subscribed_topics) {
                            continue;
                        }

                        let json = match serde_json::to_string(&envelope) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to serialize WebSocket event");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // T-5.3.4 (T-X.28): Send Resync event on Lagged.
                        // Client must perform full REST re-fetch to guarantee consistency.
                        tracing::warn!(lagged = n, "WebSocket client lagged — sending Resync");
                        let resync_envelope = WsEnvelope {
                            seq: 0,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            event: WsEvent::Resync { missed_events: n },
                        };
                        if let Ok(json) = serde_json::to_string(&resync_envelope) {
                            if socket.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            // Handle incoming messages from client.
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Try to parse as a client message (Subscribe/Unsubscribe).
                        if let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) {
                            match client_msg {
                                WsClientMessage::Subscribe { topics } => {
                                    tracing::debug!(topics = ?topics, "WS client subscribing to topics");
                                    for topic in topics {
                                        subscribed_topics.insert(topic);
                                    }
                                }
                                WsClientMessage::Unsubscribe { topics } => {
                                    tracing::debug!(topics = ?topics, "WS client unsubscribing from topics");
                                    for topic in &topics {
                                        subscribed_topics.remove(topic);
                                    }
                                }
                            }
                        } else {
                            tracing::debug!(msg = %text, "WebSocket message received (unrecognized)");
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    // Release connection slot on disconnect.
    ws_tracker.release(client_ip);
}

fn hash_ws_ticket(ticket: &str) -> String {
    blake3::hash(ticket.as_bytes()).to_hex().to_string()
}

fn prune_expired_ws_tickets(state: &AppState, now: chrono::DateTime<chrono::Utc>) {
    let expired: Vec<String> = state
        .websocket_auth_tickets
        .iter()
        .filter(|entry| entry.value().expires_at <= now)
        .map(|entry| entry.key().clone())
        .collect();
    for key in expired {
        state.websocket_auth_tickets.remove(&key);
    }
}

fn consume_ws_ticket(state: &AppState, ticket: &str) -> bool {
    let now = chrono::Utc::now();
    prune_expired_ws_tickets(state, now);
    matches!(
        state.websocket_auth_tickets.remove(&hash_ws_ticket(ticket)),
        Some((_, metadata)) if metadata.expires_at > now
    )
}

/// Push an event to a WebSocket client (wrapped in envelope).
pub async fn push_event(socket: &mut WebSocket, event: &WsEvent) -> Result<(), String> {
    let envelope = WsEnvelope {
        seq: 0,
        timestamp: chrono::Utc::now().to_rfc3339(),
        event: event.clone(),
    };
    let json = serde_json::to_string(&envelope).map_err(|e| e.to_string())?;
    socket
        .send(Message::Text(json))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_broadcast_maintains_monotonic_order() {
        let replay_buffer = Arc::new(EventReplayBuffer::new(2048));
        let (event_tx, mut event_rx) = broadcast::channel(2048);
        let barrier = Arc::new(tokio::sync::Barrier::new(11));
        let mut handles = Vec::new();

        for producer in 0..10 {
            let replay_buffer = Arc::clone(&replay_buffer);
            let event_tx = event_tx.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                for offset in 0..100 {
                    replay_buffer.push_and_broadcast(
                        WsEvent::SystemWarning {
                            message: format!("producer-{producer}-{offset}"),
                        },
                        &event_tx,
                    );
                    tokio::task::yield_now().await;
                }
            }));
        }

        barrier.wait().await;

        let mut received = Vec::with_capacity(1000);
        while received.len() < 1000 {
            received.push(event_rx.recv().await.expect("event published").seq);
        }

        for handle in handles {
            handle.await.expect("producer finished");
        }

        assert!(
            received.windows(2).all(|pair| pair[0] < pair[1]),
            "received sequences must be strictly increasing: {received:?}"
        );

        let replayed: Vec<u64> = replay_buffer
            .buffer
            .read()
            .iter()
            .map(|envelope| envelope.seq)
            .collect();
        assert!(
            replayed.windows(2).all(|pair| pair[0] < pair[1]),
            "replay buffer sequences must be strictly increasing: {replayed:?}"
        );
    }

    #[test]
    fn replay_after_returns_none_when_client_seq_exceeds_buffer_tail() {
        let replay_buffer = EventReplayBuffer::new(8);
        let (event_tx, _event_rx) = broadcast::channel(8);

        replay_buffer.push_and_broadcast(
            WsEvent::SystemWarning {
                message: "boot-1".to_string(),
            },
            &event_tx,
        );
        replay_buffer.push_and_broadcast(
            WsEvent::SystemWarning {
                message: "boot-2".to_string(),
            },
            &event_tx,
        );

        assert!(
            replay_buffer.replay_after(999).is_none(),
            "future client sequence should force resync instead of replaying an empty set",
        );
    }
}
