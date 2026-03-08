//! Session registry — tracks active sessions per agent (Req 9 AC10, AC11, AC13).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// State of a tracked session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
    pub event_count: u64,
    pub is_active: bool,
}

/// Provisional tracking for unknown agents (Req 9 AC10).
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ProvisionalAgent {
    session_count: u32,
    last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PruneResult {
    pub session_ids: Vec<Uuid>,
    pub provisional_agent_ids: Vec<Uuid>,
}

/// Session registry managing active sessions and provisional tracking.
pub struct SessionRegistry {
    /// Active sessions keyed by session_id.
    sessions: BTreeMap<Uuid, SessionState>,
    /// Agent → active session IDs (supports overlapping sessions, AC11).
    agent_sessions: BTreeMap<Uuid, Vec<Uuid>>,
    /// Provisional tracking for unknown agents (AC10).
    #[allow(dead_code)]
    provisional: BTreeMap<Uuid, ProvisionalAgent>,
    /// Max provisional sessions before dropping (default 3).
    #[allow(dead_code)]
    max_provisional: u32,
}

impl SessionRegistry {
    pub fn new(max_provisional: u32) -> Self {
        Self {
            sessions: BTreeMap::new(),
            agent_sessions: BTreeMap::new(),
            provisional: BTreeMap::new(),
            max_provisional,
        }
    }

    /// Start a new session. If a prior session exists without SessionEnd,
    /// close it with a synthetic end (AC13).
    pub fn start_session(
        &mut self,
        session_id: Uuid,
        agent_id: Uuid,
        now: DateTime<Utc>,
    ) -> Vec<Uuid> {
        let mut closed_sessions = Vec::new();

        // Check for mid-session restart (AC13): close prior sessions
        // that didn't get a SessionEnd.
        if let Some(active) = self.agent_sessions.get(&agent_id) {
            for &sid in active {
                if let Some(session) = self.sessions.get_mut(&sid) {
                    if session.is_active {
                        session.is_active = false;
                        closed_sessions.push(sid);
                    }
                }
            }
        }

        let state = SessionState {
            session_id,
            agent_id,
            started_at: now,
            last_event_at: now,
            event_count: 0,
            is_active: true,
        };

        self.sessions.insert(session_id, state);
        self.agent_sessions
            .entry(agent_id)
            .or_default()
            .push(session_id);
        self.provisional.remove(&agent_id);

        closed_sessions
    }

    /// End a session.
    pub fn end_session(&mut self, session_id: Uuid) {
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.is_active = false;
        }
    }

    /// Record an event for a session.
    pub fn record_event(&mut self, session_id: Uuid, now: DateTime<Utc>) {
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.event_count += 1;
            session.last_event_at = now;
        }
    }

    /// Check if an agent is provisionally tracked (unknown agent, AC10).
    #[allow(dead_code)]
    pub fn is_provisional(&self, agent_id: &Uuid) -> bool {
        self.provisional.contains_key(agent_id)
    }

    /// Register a provisional agent. Returns false if max sessions exceeded.
    #[allow(dead_code)]
    pub fn register_provisional(&mut self, agent_id: Uuid, now: DateTime<Utc>) -> bool {
        let entry = self
            .provisional
            .entry(agent_id)
            .or_insert(ProvisionalAgent {
                session_count: 0,
                last_seen: now,
            });
        entry.session_count += 1;
        entry.last_seen = now;
        entry.session_count <= self.max_provisional
    }

    /// Get a session by ID.
    #[allow(dead_code)]
    pub fn get_session(&self, session_id: &Uuid) -> Option<&SessionState> {
        self.sessions.get(session_id)
    }

    /// Get all active sessions for an agent.
    pub fn active_sessions(&self, agent_id: &Uuid) -> Vec<&SessionState> {
        self.agent_sessions
            .get(agent_id)
            .map(|sids: &Vec<Uuid>| {
                sids.iter()
                    .filter_map(|sid| self.sessions.get(sid))
                    .filter(|s| s.is_active)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all agent IDs that have at least one active session.
    pub fn all_active_agent_ids(&self) -> Vec<Uuid> {
        self.agent_sessions
            .iter()
            .filter(|(_, sids)| {
                sids.iter()
                    .any(|sid| self.sessions.get(sid).map_or(false, |s| s.is_active))
            })
            .map(|(agent_id, _)| *agent_id)
            .collect()
    }

    pub fn has_active_sessions(&self) -> bool {
        self.agent_sessions.iter().any(|(_, sids)| {
            sids.iter().any(|sid| {
                self.sessions
                    .get(sid)
                    .is_some_and(|session| session.is_active)
            })
        })
    }

    #[allow(dead_code)]
    pub fn provisional_count(&self) -> usize {
        self.provisional.len()
    }

    pub fn prune_stale(
        &mut self,
        now: DateTime<Utc>,
        idle_horizon: chrono::Duration,
    ) -> PruneResult {
        let stale_sessions: Vec<Uuid> = self
            .sessions
            .iter()
            .filter(|(_, session)| now - session.last_event_at >= idle_horizon)
            .map(|(session_id, _)| *session_id)
            .collect();

        for session_id in &stale_sessions {
            if let Some(session) = self.sessions.remove(session_id) {
                if let Some(agent_sessions) = self.agent_sessions.get_mut(&session.agent_id) {
                    agent_sessions.retain(|candidate| candidate != session_id);
                    if agent_sessions.is_empty() {
                        self.agent_sessions.remove(&session.agent_id);
                    }
                }
            }
        }

        let stale_provisional_agents: Vec<Uuid> = self
            .provisional
            .iter()
            .filter(|(_, agent)| now - agent.last_seen >= idle_horizon)
            .map(|(agent_id, _)| *agent_id)
            .collect();
        for agent_id in &stale_provisional_agents {
            self.provisional.remove(agent_id);
        }

        PruneResult {
            session_ids: stale_sessions,
            provisional_agent_ids: stale_provisional_agents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_stale_sessions_removes_idle_sessions_and_empty_indexes() {
        let mut registry = SessionRegistry::new(3);
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let now = Utc::now();

        registry.start_session(session_id, agent_id, now - chrono::Duration::hours(2));
        let pruned = registry.prune_stale(now, chrono::Duration::minutes(30));

        assert_eq!(pruned.session_ids, vec![session_id]);
        assert!(registry.active_sessions(&agent_id).is_empty());
        assert!(registry.all_active_agent_ids().is_empty());
    }

    #[test]
    fn prune_stale_removes_idle_provisional_agents_and_start_session_clears_them() {
        let mut registry = SessionRegistry::new(3);
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let now = Utc::now();

        assert!(registry.register_provisional(agent_id, now - chrono::Duration::hours(2)));
        assert_eq!(registry.provisional_count(), 1);

        let pruned = registry.prune_stale(now, chrono::Duration::minutes(30));
        assert_eq!(pruned.provisional_agent_ids, vec![agent_id]);
        assert_eq!(registry.provisional_count(), 0);

        assert!(registry.register_provisional(agent_id, now));
        registry.start_session(session_id, agent_id, now);
        assert_eq!(registry.provisional_count(), 0);
    }
}
