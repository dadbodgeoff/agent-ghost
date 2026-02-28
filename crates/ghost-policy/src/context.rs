//! Policy evaluation context (A2.11).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A tool call request to be evaluated by the policy engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub capability: String,
    /// Whether this tool call is part of a compaction flush turn.
    pub is_compaction_flush: bool,
}

impl ToolCall {
    /// Returns `true` if this tool targets personal or emotional functionality.
    pub fn is_personal_emotional(&self) -> bool {
        const PERSONAL_TOOLS: &[&str] = &[
            "journal_write",
            "emotional_support",
            "personal_reflection",
            "relationship_advice",
            "mood_tracking",
        ];
        PERSONAL_TOOLS.iter().any(|t| self.tool_name == *t)
    }

    /// Returns `true` if this is a proactive messaging tool.
    pub fn is_proactive_messaging(&self) -> bool {
        const PROACTIVE_TOOLS: &[&str] = &[
            "send_proactive_message",
            "schedule_message",
            "heartbeat_message",
        ];
        PROACTIVE_TOOLS.iter().any(|t| self.tool_name == *t)
    }
}

/// Context assembled for policy evaluation.
#[derive(Debug, Clone)]
pub struct PolicyContext {
    pub agent_id: Uuid,
    pub session_id: Uuid,
    pub intervention_level: u8,
    pub session_duration: Duration,
    pub session_denial_count: u32,
    pub is_compaction_flush: bool,
    /// Number of reflections written in the current session.
    /// Used by L3 convergence tightening to enforce reflection limits.
    pub session_reflection_count: u32,
}
