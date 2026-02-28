//! 5-level intervention state machine (Req 10 AC1–AC9).
//!
//! - Level 0: passive
//! - Level 1: soft notification
//! - Level 2: active intervention (mandatory ack + 5-min cooldown)
//! - Level 3: hard boundary (session termination + 4-hour cooldown)
//! - Level 4: external escalation (24-hour cooldown + external confirmation)

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::intervention::actions::InterventionAction;

/// Per-agent intervention state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInterventionState {
    /// Current intervention level (0–4).
    pub level: u8,
    /// Consecutive normal sessions for de-escalation.
    pub consecutive_normal: u32,
    /// Cooldown expiry time.
    pub cooldown_until: Option<DateTime<Utc>>,
    /// Whether mandatory human ack is required (Level 2).
    pub ack_required: bool,
    /// Hysteresis counter — 2 consecutive cycles required before escalation (AC9).
    pub hysteresis_count: u8,
    /// De-escalation credits needed per level transition.
    pub de_escalation_credits: u32,
}

impl Default for AgentInterventionState {
    fn default() -> Self {
        Self {
            level: 0,
            consecutive_normal: 0,
            cooldown_until: None,
            ack_required: false,
            hysteresis_count: 0,
            de_escalation_credits: 0,
        }
    }
}

/// Composite scoring result.
#[derive(Debug, Clone)]
pub struct CompositeResult {
    pub score: f64,
    pub level: u8,
    pub signal_scores: [f64; 8],
}

/// The intervention state machine.
pub struct InterventionStateMachine {
    states: BTreeMap<Uuid, AgentInterventionState>,
}

impl InterventionStateMachine {
    pub fn new() -> Self {
        Self {
            states: BTreeMap::new(),
        }
    }

    /// Reconstruct state from persisted data (Req 9 AC2).
    pub fn restore_state(&mut self, agent_id: Uuid, state: AgentInterventionState) {
        self.states.insert(agent_id, state);
    }

    /// Reconstruct state from individual persisted fields (Req 9 AC2).
    pub fn restore_state_from_fields(
        &mut self,
        agent_id: Uuid,
        level: u8,
        consecutive_normal: u32,
        cooldown_until: Option<DateTime<Utc>>,
        ack_required: bool,
        hysteresis_count: u32,
        de_escalation_credits: u32,
    ) {
        self.states.insert(agent_id, AgentInterventionState {
            level,
            consecutive_normal,
            cooldown_until,
            ack_required,
            hysteresis_count: hysteresis_count as u8,
            de_escalation_credits,
        });
    }

    /// Get current state for an agent.
    pub fn get_state(&self, agent_id: &Uuid) -> Option<&AgentInterventionState> {
        self.states.get(agent_id)
    }

    /// Get mutable iterator over all agent states (for cooldown checks).
    pub fn states_mut(&mut self) -> impl Iterator<Item = (&Uuid, &mut AgentInterventionState)> {
        self.states.iter_mut()
    }

    /// Evaluate a composite result and update intervention state.
    ///
    /// Returns the action to take, if any.
    pub fn evaluate(
        &mut self,
        result: &CompositeResult,
        agent_id: Uuid,
    ) -> Option<InterventionAction> {
        let state = self.states.entry(agent_id).or_default();

        // Check cooldown
        if let Some(until) = state.cooldown_until {
            if Utc::now() < until {
                return None; // Still in cooldown
            }
            state.cooldown_until = None;
        }

        // Check if ack is required (Level 2)
        if state.ack_required {
            return None; // Scoring paused until ack
        }

        let target_level = result.level;

        // Escalation: max +1 per cycle (AC2), hysteresis (AC9)
        if target_level > state.level {
            state.hysteresis_count += 1;
            if state.hysteresis_count >= 2 {
                let new_level = (state.level + 1).min(4);
                state.level = new_level;
                state.hysteresis_count = 0;
                state.consecutive_normal = 0;
                return Some(self.action_for_escalation(agent_id, new_level));
            }
            // Not enough consecutive cycles yet
            return None;
        }

        // Score is at or below current level — reset hysteresis
        state.hysteresis_count = 0;

        // If score is below current level threshold, track for de-escalation
        if target_level < state.level {
            // Score is below current level — accumulate de-escalation credit.
            // consecutive_normal is NOT reset here; it tracks consecutive
            // cycles where the score is below the current level.
            state.consecutive_normal += 1;
        } else {
            // Score is AT current level — not improving, reset de-escalation
            // credits since the agent is still at the same risk level.
            state.consecutive_normal = 0;
        }

        None
    }

    /// Attempt de-escalation at a session boundary (AC3).
    ///
    /// De-escalation requires consecutive normal sessions:
    /// - L4→L3: 3 sessions
    /// - L3→L2: 3 sessions
    /// - L2→L1: 2 sessions
    /// - L1→L0: 2 sessions
    ///
    /// One bad session resets the counter.
    pub fn try_deescalate(&mut self, agent_id: Uuid, session_was_normal: bool) -> bool {
        let state = self.states.entry(agent_id).or_default();

        if state.level == 0 {
            return false;
        }

        if !session_was_normal {
            state.consecutive_normal = 0;
            return false;
        }

        state.consecutive_normal += 1;

        let required = match state.level {
            4 | 3 => 3,
            2 | 1 => 2,
            _ => return false,
        };

        if state.consecutive_normal >= required {
            state.level -= 1;
            state.consecutive_normal = 0;
            state.ack_required = false;
            true
        } else {
            false
        }
    }

    /// Acknowledge a Level 2 intervention (AC4).
    pub fn acknowledge(&mut self, agent_id: Uuid) {
        if let Some(state) = self.states.get_mut(&agent_id) {
            state.ack_required = false;
        }
    }

    fn action_for_escalation(&mut self, agent_id: Uuid, new: u8) -> InterventionAction {
        let agent_state = self
            .states
            .get_mut(&agent_id)
            .expect("agent state must exist after evaluate()");

        match new {
            0 => InterventionAction::Level0LogOnly,
            1 => InterventionAction::Level1SoftNotification,
            2 => {
                agent_state.ack_required = true;
                agent_state.cooldown_until = Some(Utc::now() + Duration::minutes(5));
                InterventionAction::Level2MandatoryAck
            }
            3 => {
                agent_state.cooldown_until = Some(Utc::now() + Duration::hours(4));
                InterventionAction::Level3SessionTermination
            }
            4 => {
                agent_state.cooldown_until = Some(Utc::now() + Duration::hours(24));
                InterventionAction::Level4ExternalEscalation
            }
            _ => InterventionAction::Level0LogOnly,
        }
    }
}

impl Default for InterventionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}
