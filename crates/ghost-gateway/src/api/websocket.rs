//! WebSocket upgrade handler for real-time events (Req 25 AC3).
//!
//! Pushes convergence score updates, intervention level changes,
//! kill switch activations, and proposal decisions to connected clients.
//! Supports per-client topic subscriptions (T-2.1.8).

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::response::IntoResponse;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::api::auth::{AuthConfig, RevocationSet};
use crate::state::AppState;

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
    /// Heartbeat to keep connection alive.
    Ping,
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
            WsEvent::Ping => vec![],
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
        let token = match &params.token {
            Some(t) => t.as_str(),
            None => {
                ws_tracker.release(client_ip);
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            }
        };

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

async fn handle_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    ws_tracker: Arc<WsConnectionTracker>,
    client_ip: IpAddr,
) {
    // Send initial ping.
    let ping = match serde_json::to_string(&WsEvent::Ping) {
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

    loop {
        tokio::select! {
            // Keepalive ping every 30s.
            _ = interval.tick() => {
                let ping = match serde_json::to_string(&WsEvent::Ping) {
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
            // Forward broadcast events to this WebSocket client.
            event = event_rx.recv() => {
                match event {
                    Ok(ws_event) => {
                        // Topic filtering (T-2.1.8): if client has subscriptions,
                        // only forward events matching subscribed topics.
                        // Pings always pass through.
                        if !subscribed_topics.is_empty() {
                            let keys = ws_event.topic_keys();
                            if !keys.is_empty() && !keys.iter().any(|k| subscribed_topics.contains(k)) {
                                continue;
                            }
                        }

                        let json = match serde_json::to_string(&ws_event) {
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
                        tracing::warn!(lagged = n, "WebSocket client lagged behind broadcast");
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

/// Push an event to a WebSocket client.
pub async fn push_event(socket: &mut WebSocket, event: &WsEvent) -> Result<(), String> {
    let json = serde_json::to_string(event).map_err(|e| e.to_string())?;
    socket
        .send(Message::Text(json))
        .await
        .map_err(|e| e.to_string())
}
