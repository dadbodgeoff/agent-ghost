//! Transport layer — unix socket, HTTP API, native messaging (Req 9 AC8).

pub mod http_api;
pub mod native_messaging;
pub mod unix_socket;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unified ingest event from any transport source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestEvent {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub event_type: EventType,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub source: EventSource,
}

/// ITP event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    SessionStart,
    SessionEnd,
    InteractionMessage,
    AgentStateSnapshot,
    ConvergenceAlert,
}

/// Source of the event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventSource {
    AgentLoop,
    BrowserExtension,
    Proxy,
    HttpApi,
}
