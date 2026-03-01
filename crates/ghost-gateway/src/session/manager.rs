//! Session manager: create, lookup, route, idle pruning (Req 26 AC3).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Session context.
#[derive(Debug, Clone)]
pub struct SessionContext {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub channel: String,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub token_count: usize,
    pub cost: f64,
    pub model_context_window: usize,
}

/// Session manager.
pub struct SessionManager {
    sessions: BTreeMap<Uuid, SessionContext>,
    agent_sessions: BTreeMap<Uuid, Vec<Uuid>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            agent_sessions: BTreeMap::new(),
        }
    }

    /// Create a new session.
    pub fn create_session(
        &mut self,
        agent_id: Uuid,
        channel: String,
        context_window: usize,
    ) -> SessionContext {
        let session_id = Uuid::now_v7();
        let now = Utc::now();
        let ctx = SessionContext {
            session_id,
            agent_id,
            channel,
            created_at: now,
            last_activity: now,
            token_count: 0,
            cost: 0.0,
            model_context_window: context_window,
        };
        self.sessions.insert(session_id, ctx.clone());
        self.agent_sessions
            .entry(agent_id)
            .or_default()
            .push(session_id);
        ctx
    }

    /// Lookup a session by ID.
    pub fn lookup(&self, session_id: Uuid) -> Option<&SessionContext> {
        self.sessions.get(&session_id)
    }

    /// Update last activity timestamp.
    pub fn touch(&mut self, session_id: Uuid) {
        if let Some(ctx) = self.sessions.get_mut(&session_id) {
            ctx.last_activity = Utc::now();
        } else {
            tracing::debug!(
                session_id = %session_id,
                "touch() called for unknown session — no-op"
            );
        }
    }

    /// Get all sessions for an agent.
    pub fn agent_sessions(&self, agent_id: Uuid) -> Vec<&SessionContext> {
        self.agent_sessions
            .get(&agent_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.sessions.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Prune sessions idle for longer than the given duration.
    pub fn prune_idle(&mut self, max_idle: chrono::Duration) {
        let cutoff = Utc::now() - max_idle;
        let to_remove: Vec<Uuid> = self
            .sessions
            .iter()
            .filter(|(_, ctx)| ctx.last_activity < cutoff)
            .map(|(id, _)| *id)
            .collect();

        if !to_remove.is_empty() {
            tracing::info!(
                count = to_remove.len(),
                max_idle_secs = max_idle.num_seconds(),
                "pruning idle sessions"
            );
        }

        for id in &to_remove {
            if let Some(ctx) = self.sessions.remove(id) {
                tracing::debug!(
                    session_id = %id,
                    agent_id = %ctx.agent_id,
                    last_activity = %ctx.last_activity,
                    "pruned idle session"
                );
                if let Some(agent_sessions) = self.agent_sessions.get_mut(&ctx.agent_id) {
                    agent_sessions.retain(|s| s != id);
                }
            }
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
