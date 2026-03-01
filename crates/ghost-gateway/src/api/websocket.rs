//! WebSocket upgrade handler for real-time events (Req 25 AC3).
//!
//! Pushes convergence score updates, intervention level changes,
//! kill switch activations, and proposal decisions to connected clients.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// Query parameters for WebSocket connection (token auth).
#[derive(Debug, Deserialize)]
pub struct WsQueryParams {
    pub token: Option<String>,
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
    /// Heartbeat to keep connection alive.
    Ping,
}

/// GET /api/ws — WebSocket upgrade with token auth via query param.
pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
    Query(params): Query<WsQueryParams>,
) -> impl IntoResponse {
    // Validate token from query param.
    if let Some(token) = &params.token {
        if !crate::auth::token_auth::validate_token(token) {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    // Send initial ping.
    let ping = match serde_json::to_string(&WsEvent::Ping) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize WebSocket ping");
            return;
        }
    };
    if socket.send(Message::Text(ping)).await.is_err() {
        return;
    }

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
                        tracing::debug!(msg = %text, "WebSocket message received");
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

/// Push an event to a WebSocket client.
pub async fn push_event(socket: &mut WebSocket, event: &WsEvent) -> Result<(), String> {
    let json = serde_json::to_string(event).map_err(|e| e.to_string())?;
    socket
        .send(Message::Text(json))
        .await
        .map_err(|e| e.to_string())
}
