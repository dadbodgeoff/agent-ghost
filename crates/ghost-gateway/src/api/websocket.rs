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
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::api::auth::{AuthConfig, RevocationSet};
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
    capacity: usize,
}

impl EventReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: parking_lot::RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn push(&self, envelope: WsEnvelope) {
        let mut buf = self.buffer.write();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(envelope);
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
        if last_seq < first_available {
            return None; // Gap too large — events were evicted
        }
        Some(buf.iter().filter(|e| e.seq > last_seq).cloned().collect())
    }
}

/// Helper: wrap a WsEvent in an envelope and push to replay buffer.
///
/// Uses SeqCst ordering on the sequence counter and assigns the sequence
/// number inside the replay buffer's write lock to guarantee monotonic
/// ordering in the buffer even under concurrent calls.
pub fn broadcast_event(state: &crate::state::AppState, event: WsEvent) {
    let seq = EVENT_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let envelope = WsEnvelope {
        seq,
        timestamp: chrono::Utc::now().to_rfc3339(),
        event,
    };
    state.replay_buffer.push(envelope.clone());
    if state.event_tx.send(envelope).is_err() {
        tracing::debug!("broadcast_event: no WebSocket receivers");
    }
}

/// Query parameters for WebSocket connection (token auth).
#[derive(Debug, Deserialize)]
pub struct WsQueryParams {
    pub token: Option<String>,
}

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
    AgentStateChange {
        agent_id: String,
        new_state: String,
    },
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
    SkillChange {
        skill_name: String,
        action: String,
    },
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
    ContextTruncated { layer: String, removed_tokens: usize },
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
            WsEvent::SystemWarning { .. } | WsEvent::ContextTruncated { .. } => vec!["system:warning".to_string()],
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

/// GET /api/ws — WebSocket upgrade with dual-mode token auth via query param.
///
/// Auth modes (same as REST middleware):
/// 1. JWT mode: validate `?token=` as JWT when GHOST_JWT_SECRET is set
/// 2. Legacy mode: validate `?token=` against GHOST_TOKEN
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

    // Rate limit: max concurrent WS connections per IP.
    if !ws_tracker.acquire(client_ip) {
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many WebSocket connections from this IP",
        )
            .into_response();
    }

    if auth_config.auth_required() {
        // T-5.1.3: Prefer token from Sec-WebSocket-Protocol header (standard WS auth pattern).
        // Token is encoded as subprotocol "ghost-token.<TOKEN>" to avoid query param leakage
        // in HTTP logs, proxy logs, and browser history.
        // Fall back to query param with deprecation warning.
        let token_from_header: Option<String> = headers
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .and_then(|protos| {
                protos
                    .split(',')
                    .map(|s| s.trim())
                    .find(|p| p.starts_with("ghost-token."))
                    .and_then(|p| p.strip_prefix("ghost-token."))
                    .map(|t| t.to_string())
            });

        let token_str;
        if let Some(ref t) = token_from_header {
            token_str = t.clone();
        } else if let Some(ref t) = params.token {
            tracing::warn!("WebSocket auth via query param is deprecated — use Sec-WebSocket-Protocol header");
            token_str = t.clone();
        } else {
            ws_tracker.release(client_ip);
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        let token = token_str.as_str();

        // Try JWT validation first.
        if let Some(ref secret) = auth_config.jwt_secret {
            let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
            let mut validation =
                jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
            validation.validate_exp = true;
            validation.leeway = 30;
            match jsonwebtoken::decode::<crate::api::auth::Claims>(token, &key, &validation) {
                Ok(data) => {
                    // Check revocation.
                    if !data.claims.jti.is_empty() && revocation_set.is_revoked(&data.claims.jti) {
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
            // Legacy token: constant-time comparison.
            if !crate::auth::token_auth::validate_token(token) {
                ws_tracker.release(client_ip);
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            }
        }
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state, ws_tracker, client_ip))
        .into_response()
}

/// Client reconnect message: `{ "last_seq": N }`.
#[derive(Debug, Deserialize)]
struct ReconnectMessage {
    last_seq: u64,
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

    // Wait briefly for a potential reconnect message with last_seq.
    // Use a short timeout so fresh connections aren't delayed.
    let reconnect_check = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        socket.recv(),
    ).await;

    if let Ok(Some(Ok(Message::Text(text)))) = reconnect_check {
        if let Ok(reconnect) = serde_json::from_str::<ReconnectMessage>(&text) {
            tracing::info!(last_seq = reconnect.last_seq, "WS client reconnecting with last_seq");
            match state.replay_buffer.replay_after(reconnect.last_seq) {
                Some(missed) => {
                    tracing::info!(count = missed.len(), "Replaying missed events");
                    for envelope in missed {
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
                    tracing::warn!(last_seq = reconnect.last_seq, "Replay gap too large — sending Resync to reconnecting client");
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
                        if !subscribed_topics.is_empty() {
                            let keys = envelope.event.topic_keys();
                            if !keys.is_empty() && !keys.iter().any(|k| subscribed_topics.contains(k)) {
                                continue;
                            }
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
