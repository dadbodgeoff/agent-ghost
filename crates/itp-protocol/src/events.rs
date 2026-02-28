//! ITP event types (Req 4 AC1).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::privacy::PrivacyLevel;

/// Top-level ITP event enum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "data")]
pub enum ITPEvent {
    SessionStart(SessionStartEvent),
    SessionEnd(SessionEndEvent),
    InteractionMessage(InteractionMessageEvent),
    AgentStateSnapshot(AgentStateSnapshotEvent),
    ConvergenceAlert(ConvergenceAlertEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStartEvent {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub channel: String,
    pub privacy_level: PrivacyLevel,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEndEvent {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub reason: String,
    pub message_count: u64,
    pub timestamp: DateTime<Utc>,
}

/// Sender of an interaction message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageSender {
    Human,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionMessageEvent {
    pub session_id: Uuid,
    pub message_id: Uuid,
    pub sender: MessageSender,
    /// SHA-256 hash of content (always present for privacy).
    pub content_hash: String,
    /// Plaintext content — only if PrivacyLevel >= Standard.
    pub content_plaintext: Option<String>,
    pub token_count: usize,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStateSnapshotEvent {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub memory_count: u64,
    pub goal_count: u64,
    pub convergence_score: f64,
    pub intervention_level: u8,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceAlertEvent {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub alert_type: String,
    pub score: f64,
    pub level: u8,
    pub details: String,
    pub timestamp: DateTime<Utc>,
}
