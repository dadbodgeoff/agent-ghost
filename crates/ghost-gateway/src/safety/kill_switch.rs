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
#[derive(Debug, Clone, PartialEq, Eq)]
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

        let state = match self.state.read() {
            Ok(s) => s,
            Err(_poisoned) => {
                // RwLock poisoned — treat as platform killed for safety.
                // A poisoned lock means a thread panicked while holding it,
                // which is a critical failure. Err on the side of caution.
                tracing::error!("kill switch RwLock poisoned — treating as PlatformKilled");
                PLATFORM_KILLED.store(true, Ordering::SeqCst);
                return KillCheckResult::PlatformKilled;
            }
        };
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
        let mut state = match self.state.write() {
            Ok(s) => s,
            Err(_) => {
                tracing::error!("kill switch RwLock poisoned during activate_agent");
                PLATFORM_KILLED.store(true, Ordering::SeqCst);
                return;
            }
        };

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
        let mut state = match self.state.write() {
            Ok(s) => s,
            Err(_) => {
                tracing::error!("kill switch RwLock poisoned during activate_kill_all");
                PLATFORM_KILLED.store(true, Ordering::SeqCst);
                return;
            }
        };
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
    /// Resuming from QUARANTINE requires forensic review + second confirmation (Req 14b AC4).
    /// Resume an agent from paused/quarantined state.
    ///
    /// `expected_level` prevents TOCTOU races: the caller passes the level
    /// they checked authorization for, and if the actual level inside the
    /// write lock differs (e.g. escalated from Pause to Quarantine by another
    /// thread), the resume is rejected.
    pub fn resume_agent(
        &self,
        agent_id: Uuid,
        expected_level: Option<KillLevel>,
    ) -> Result<(), String> {
        let mut state = match self.state.write() {
            Ok(s) => s,
            Err(_) => return Err("kill switch RwLock poisoned".into()),
        };
        let agent_state = state
            .per_agent
            .get(&agent_id)
            .ok_or_else(|| format!("Agent {agent_id} not in kill state"))?;

        // TOCTOU guard: if the caller expected a specific level, verify it
        // hasn't changed since they checked (e.g. escalated concurrently).
        if let Some(expected) = expected_level {
            if agent_state.level != expected {
                return Err(format!(
                    "Agent kill level changed: expected {:?} but is now {:?}. \
                     This is a time-of-check/time-of-use (TOCTOU) race — the kill level was \
                     escalated between when you checked it and when you attempted to resume. \
                     Please re-check the agent's current status with `ghost safety status` \
                     and retry the resume operation with the correct expected level.",
                    expected, agent_state.level
                ));
            }
        }

        match agent_state.level {
            KillLevel::KillAll => {
                Err("Cannot resume from KILL_ALL via agent resume — use platform resume with confirmation token".into())
            }
            KillLevel::Quarantine => {
                // Req 14b AC4: Quarantine resume requires forensic review + second confirmation.
                // The caller (API handler) must verify these preconditions before calling resume.
                // We log the heightened monitoring requirement (24h).
                tracing::warn!(
                    agent_id = %agent_id,
                    "Agent resumed from QUARANTINE — heightened monitoring for 24h"
                );
                self.log_audit(
                    TriggerEvent::ManualPause { agent_id, reason: "quarantine_resume".into(), initiated_by: "api".into() },
                    KillLevel::Normal,
                    Some(agent_id),
                );
                state.per_agent.remove(&agent_id);
                Ok(())
            }
            KillLevel::Pause => {
                self.log_audit(
                    TriggerEvent::ManualPause { agent_id, reason: "pause_resume".into(), initiated_by: "api".into() },
                    KillLevel::Normal,
                    Some(agent_id),
                );
                state.per_agent.remove(&agent_id);
                Ok(())
            }
            KillLevel::Normal => {
                Err(format!("Agent {agent_id} is not paused or quarantined"))
            }
        }
    }

    /// Get current state (for persistence).
    pub fn current_state(&self) -> KillSwitchState {
        match self.state.read() {
            Ok(s) => s.clone(),
            Err(poisoned) => {
                tracing::error!("kill switch RwLock poisoned during current_state");
                poisoned.into_inner().clone()
            }
        }
    }

    /// Restore state (for crash recovery — stale state, never fall to Normal).
    pub fn restore_state(&self, restored: KillSwitchState) {
        let mut state = match self.state.write() {
            Ok(s) => s,
            Err(poisoned) => {
                tracing::error!("kill switch RwLock poisoned during restore_state");
                poisoned.into_inner()
            }
        };
        *state = restored;
        if state.platform_level == KillLevel::KillAll {
            PLATFORM_KILLED.store(true, Ordering::SeqCst);
        }
    }

    /// Get audit log entries.
    pub fn audit_entries(&self) -> Vec<AuditEntry> {
        match self.audit_log.read() {
            Ok(log) => log.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Count quarantined agents (for T6 cascade check).
    pub fn quarantined_count(&self) -> usize {
        let state = match self.state.read() {
            Ok(state) => state,
            Err(poisoned) => {
                tracing::error!("kill switch RwLock poisoned in quarantined_count — recovering");
                poisoned.into_inner()
            }
        };
        state
            .per_agent
            .values()
            .filter(|a| a.level == KillLevel::Quarantine)
            .count()
    }

    fn log_audit(&self, trigger: TriggerEvent, action: KillLevel, agent_id: Option<Uuid>) {
        let mut log = match self.audit_log.write() {
            Ok(log) => log,
            Err(poisoned) => {
                tracing::error!(
                    "kill switch audit_log RwLock poisoned — recovering to preserve audit trail"
                );
                poisoned.into_inner()
            }
        };
        log.push(AuditEntry {
            timestamp: Utc::now(),
            trigger,
            action,
            agent_id,
        });
    }
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self::new()
    }
}
