//! SessionManager — concurrent per-session access via DashMap.

use dashmap::DashMap;
use std::sync::Arc;

use crate::context::SessionContext;

/// Thread-safe session manager using `DashMap` for concurrent access.
pub struct SessionManager {
    sessions: Arc<DashMap<String, SessionContext>>,
}

impl SessionManager {
    /// Create a new SessionManager.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }

    /// Create a new session and return its ID.
    pub fn create_session(&self, session_id: String) -> String {
        let ctx = SessionContext::new(session_id.clone());
        self.sessions.insert(session_id.clone(), ctx);
        session_id
    }

    /// Get a session context by ID (cloned snapshot).
    pub fn get_session(&self, session_id: &str) -> Option<SessionContext> {
        self.sessions.get(session_id).map(|r| r.clone())
    }

    /// Update a session context.
    pub fn update_session(&self, ctx: SessionContext) {
        self.sessions.insert(ctx.session_id.clone(), ctx);
    }

    /// Remove a session.
    pub fn remove_session(&self, session_id: &str) -> Option<SessionContext> {
        self.sessions.remove(session_id).map(|(_, v)| v)
    }

    /// Mark a memory as sent in a session. Returns false if session not found.
    pub fn mark_memory_sent(&self, session_id: &str, memory_id: &str, tokens: usize) -> bool {
        if let Some(mut entry) = self.sessions.get_mut(session_id) {
            entry.mark_memory_sent(memory_id, tokens);
            true
        } else {
            false
        }
    }

    /// Check if a memory has been sent in a session.
    pub fn is_memory_sent(&self, session_id: &str, memory_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|s| s.is_memory_sent(memory_id))
            .unwrap_or(false)
    }

    /// Record a query in a session.
    pub fn record_query(&self, session_id: &str) -> bool {
        if let Some(mut entry) = self.sessions.get_mut(session_id) {
            entry.record_query();
            true
        } else {
            false
        }
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get all session IDs.
    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.iter().map(|r| r.key().clone()).collect()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
