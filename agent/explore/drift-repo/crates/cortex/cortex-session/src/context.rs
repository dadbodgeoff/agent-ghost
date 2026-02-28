//! SessionContext — loaded sets and token tracking per conversation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use cortex_core::models::agent::AgentId;

/// Per-session state tracking loaded memories, patterns, files, constraints,
/// and token usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    /// Unique session identifier.
    pub session_id: String,
    /// When this session was created.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    pub last_activity: DateTime<Utc>,
    /// Memory IDs already sent in this session.
    pub loaded_memories: HashSet<String>,
    /// Pattern IDs already sent.
    pub loaded_patterns: HashSet<String>,
    /// File paths already sent.
    pub loaded_files: HashSet<String>,
    /// Constraint IDs already sent.
    pub loaded_constraints: HashSet<String>,
    /// Total tokens sent in this session.
    pub tokens_sent: usize,
    /// Total queries made in this session.
    pub queries_made: u64,
    /// Agent ID for multi-agent sessions. Defaults to `AgentId::default_agent()`.
    #[serde(default)]
    pub agent_id: AgentId,
}

impl SessionContext {
    /// Create a new session context.
    pub fn new(session_id: String) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            created_at: now,
            last_activity: now,
            loaded_memories: HashSet::new(),
            loaded_patterns: HashSet::new(),
            loaded_files: HashSet::new(),
            loaded_constraints: HashSet::new(),
            tokens_sent: 0,
            queries_made: 0,
            agent_id: AgentId::default_agent(),
        }
    }

    /// Create a new session context for a specific agent.
    pub fn new_with_agent(session_id: String, agent_id: AgentId) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            created_at: now,
            last_activity: now,
            loaded_memories: HashSet::new(),
            loaded_patterns: HashSet::new(),
            loaded_files: HashSet::new(),
            loaded_constraints: HashSet::new(),
            tokens_sent: 0,
            queries_made: 0,
            agent_id,
        }
    }

    /// Mark a memory as sent and add its token count.
    pub fn mark_memory_sent(&mut self, memory_id: &str, tokens: usize) {
        self.loaded_memories.insert(memory_id.to_string());
        self.tokens_sent += tokens;
        self.last_activity = Utc::now();
    }

    /// Check if a memory has already been sent.
    pub fn is_memory_sent(&self, memory_id: &str) -> bool {
        self.loaded_memories.contains(memory_id)
    }

    /// Record a query.
    pub fn record_query(&mut self) {
        self.queries_made += 1;
        self.last_activity = Utc::now();
    }

    /// Duration since last activity.
    pub fn idle_duration(&self) -> chrono::Duration {
        Utc::now() - self.last_activity
    }

    /// Duration since session creation.
    pub fn session_duration(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }
}
