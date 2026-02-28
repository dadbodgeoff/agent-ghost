//! Sybil resistance for multi-agent CRDT networks (Req 29 AC2).
//!
//! - Max 3 child agents per parent per 24 hours
//! - New agents start at trust 0.3
//! - Trust capped at 0.6 for agents < 7 days old

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SybilError {
    #[error("spawn limit exceeded: {parent_id} has spawned {count} children in the last 24h (max {max})")]
    SpawnLimitExceeded {
        parent_id: Uuid,
        count: usize,
        max: usize,
    },
}

/// Trust level for an agent in the CRDT network.
#[derive(Debug, Clone)]
pub struct AgentTrust {
    pub agent_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub trust: f64,
    pub created_at: DateTime<Utc>,
}

impl AgentTrust {
    /// Effective trust, capped at 0.6 for agents younger than 7 days.
    pub fn effective_trust(&self) -> f64 {
        let age = Utc::now() - self.created_at;
        if age < Duration::days(7) {
            self.trust.min(0.6)
        } else {
            self.trust
        }
    }
}

/// Sybil guard enforcing spawn limits and trust levels.
pub struct SybilGuard {
    /// Max children per parent per 24h window.
    max_children_per_day: usize,
    /// Initial trust for new agents.
    initial_trust: f64,
    /// Trust cap for agents < 7 days old.
    young_agent_cap: f64,
    /// Spawn records: parent_id → list of (child_id, spawn_time).
    spawn_records: BTreeMap<Uuid, Vec<(Uuid, DateTime<Utc>)>>,
    /// Agent trust levels.
    trust_levels: BTreeMap<Uuid, AgentTrust>,
}

impl SybilGuard {
    pub fn new() -> Self {
        Self {
            max_children_per_day: 3,
            initial_trust: 0.3,
            young_agent_cap: 0.6,
            spawn_records: BTreeMap::new(),
            trust_levels: BTreeMap::new(),
        }
    }

    /// Attempt to register a new child agent spawned by a parent.
    pub fn register_spawn(
        &mut self,
        parent_id: Uuid,
        child_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<AgentTrust, SybilError> {
        // Count spawns in the last 24 hours
        let cutoff = now - Duration::hours(24);
        let records = self.spawn_records.entry(parent_id).or_default();

        // Prune old records
        records.retain(|(_, t)| *t > cutoff);

        if records.len() >= self.max_children_per_day {
            return Err(SybilError::SpawnLimitExceeded {
                parent_id,
                count: records.len(),
                max: self.max_children_per_day,
            });
        }

        records.push((child_id, now));

        let trust = AgentTrust {
            agent_id: child_id,
            parent_id: Some(parent_id),
            trust: self.initial_trust,
            created_at: now,
        };
        self.trust_levels.insert(child_id, trust.clone());
        Ok(trust)
    }

    /// Get the effective trust for an agent.
    pub fn effective_trust(&self, agent_id: &Uuid) -> f64 {
        self.trust_levels
            .get(agent_id)
            .map_or(0.0, |t| t.effective_trust())
    }

    /// Set trust for an agent (e.g., after earning trust over time).
    pub fn set_trust(&mut self, agent_id: Uuid, trust: f64) {
        if let Some(entry) = self.trust_levels.get_mut(&agent_id) {
            entry.trust = trust;
        }
    }

    /// Get the young agent trust cap.
    pub fn young_agent_cap(&self) -> f64 {
        self.young_agent_cap
    }

    /// Get the initial trust level.
    pub fn initial_trust(&self) -> f64 {
        self.initial_trust
    }
}

impl Default for SybilGuard {
    fn default() -> Self {
        Self::new()
    }
}
