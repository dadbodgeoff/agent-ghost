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
    first_seen: DateTime<Utc>,
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
                first_seen: now,
            });
        entry.session_count += 1;
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
}
