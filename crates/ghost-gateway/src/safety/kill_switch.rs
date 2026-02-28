//! Kill switch: 3-level hard safety system (Req 14).
//!
//! Levels: PAUSE (single agent), QUARANTINE (single agent), KILL_ALL (all agents).
//! PLATFORM_KILLED is a static AtomicBool with SeqCst ordering.
//! State transitions are monotonic — level never decreases without explicit resume.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use cortex_core::safety::trigger::TriggerEvent;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Global platform killed flag. Checked with SeqCst ordering.
pub static PLATFORM_KILLED: AtomicBool = AtomicBool::new(false);

/// Kill switch levels (monotonically increasing severity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum KillLevel {
    Normal = 0,
    Pause = 1,
    Quarantine = 2,
    KillAll = 3,
}

/// Per-agent kill state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentKillState {
    pub agent_id: Uuid,
    pub level: KillLevel,
    pub activated_at: Option<DateTime<Utc>>,
    pub trigger: Option<String>,
}

/// Platform-wide kill switch state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitchState {
    pub platform_level: KillLevel,
    pub per_agent: BTreeMap<Uuid, AgentKillState>,
    pub activated_at: Option<DateTime<Utc>>,
    pub trigger: Option<String>,
}

impl Default for KillSwitchState {
    fn default() -> Self {
        Self {
            platform_level: KillLevel::Normal,
            per_agent: BTreeMap::new(),
            activated_at: None,
            trigger: None,
        }
    }
}

/// Kill switch check result.
#[derive(Debug)]
pub enum KillCheckResult {
    Ok,
    AgentPaused(Uuid),
    AgentQuarantined(Uuid),
    PlatformKilled,
}

/// The kill switch.
pub struct KillSwitch {
    state: RwLock<KillSwitchState>,
    audit_log: RwLock<Vec<AuditEntry>>,
}

/// Audit log entry for kill switch activations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub trigger: TriggerEvent,
    pub action: KillLevel,
    pub agent_id: Option<Uuid>,
}

impl KillSwitch {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(KillSwitchState::default()),
            audit_log: RwLock::new(Vec::new()),
        }
    }

    /// Check if an agent is allowed to operate.
    pub fn check(&self, agent_id: Uuid) -> KillCheckResult {
        // Fast path: check global flag first
        if PLATFORM_KILLED.load(Ordering::SeqCst) {
            return KillCheckResult::PlatformKilled;
        }

        let state = self.state.read().unwrap();
        if state.platform_level == KillLevel::KillAll {
            return KillCheckResult::PlatformKilled;
        }

        if let Some(agent_state) = state.per_agent.get(&agent_id) {
            match agent_state.level {
                KillLevel::Pause => return KillCheckResult::AgentPaused(agent_id),
                KillLevel::Quarantine => return KillCheckResult::AgentQuarantined(agent_id),
                KillLevel::KillAll => return KillCheckResult::PlatformKilled,
                KillLevel::Normal => {}
            }
        }

        KillCheckResult::Ok
    }

    /// Activate kill switch for a specific agent.
    pub fn activate_agent(&self, agent_id: Uuid, level: KillLevel, trigger: &TriggerEvent) {
        let mut state = self.state.write().unwrap();

        // Monotonicity: never decrease level without explicit resume
        if let Some(existing) = state.per_agent.get(&agent_id) {
            if level <= existing.level {
                return;
            }
        }

        state.per_agent.insert(
            agent_id,
            AgentKillState {
                agent_id,
                level,
                activated_at: Some(Utc::now()),
                trigger: Some(format!("{trigger:?}")),
            },
        );

        self.log_audit(trigger.clone(), level, Some(agent_id));

        if level == KillLevel::KillAll {
            self.activate_kill_all_inner(&mut state, trigger);
        }
    }

    /// Activate KILL_ALL — stops all agents, enters safe mode.
    pub fn activate_kill_all(&self, trigger: &TriggerEvent) {
        let mut state = self.state.write().unwrap();
        self.activate_kill_all_inner(&mut state, trigger);
    }

    fn activate_kill_all_inner(&self, state: &mut KillSwitchState, trigger: &TriggerEvent) {
        if state.platform_level == KillLevel::KillAll {
            return; // Idempotent
        }
        state.platform_level = KillLevel::KillAll;
        state.activated_at = Some(Utc::now());
        state.trigger = Some(format!("{trigger:?}"));
        PLATFORM_KILLED.store(true, Ordering::SeqCst);
        self.log_audit(trigger.clone(), KillLevel::KillAll, None);
        tracing::error!("KILL_ALL activated. Platform entering safe mode.");
    }

    /// Resume an agent from PAUSE (requires owner auth).
    pub fn resume_agent(&self, agent_id: Uuid) -> Result<(), String> {
        let mut state = self.state.write().unwrap();
        let agent_state = state
            .per_agent
            .get(&agent_id)
            .ok_or_else(|| format!("Agent {agent_id} not in kill state"))?;

        if agent_state.level == KillLevel::KillAll {
            return Err("Cannot resume from KILL_ALL via agent resume".into());
        }

        state.per_agent.remove(&agent_id);
        Ok(())
    }

    /// Get current state (for persistence).
    pub fn current_state(&self) -> KillSwitchState {
        self.state.read().unwrap().clone()
    }

    /// Restore state (for crash recovery — stale state, never fall to Normal).
    pub fn restore_state(&self, restored: KillSwitchState) {
        let mut state = self.state.write().unwrap();
        *state = restored;
        if state.platform_level == KillLevel::KillAll {
            PLATFORM_KILLED.store(true, Ordering::SeqCst);
        }
    }

    /// Get audit log entries.
    pub fn audit_entries(&self) -> Vec<AuditEntry> {
        self.audit_log.read().unwrap().clone()
    }

    /// Count quarantined agents (for T6 cascade check).
    pub fn quarantined_count(&self) -> usize {
        let state = self.state.read().unwrap();
        state
            .per_agent
            .values()
            .filter(|a| a.level == KillLevel::Quarantine)
            .count()
    }

    fn log_audit(&self, trigger: TriggerEvent, action: KillLevel, agent_id: Option<Uuid>) {
        if let Ok(mut log) = self.audit_log.write() {
            log.push(AuditEntry {
                timestamp: Utc::now(),
                trigger,
                action,
                agent_id,
            });
        }
    }
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self::new()
    }
}
