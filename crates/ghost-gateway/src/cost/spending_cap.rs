//! Spending cap enforcer: pre/post check (Req 27 AC2-4).
//! Agent cannot raise own cap (AC4).

use cortex_core::safety::trigger::TriggerEvent;
use uuid::Uuid;

use super::tracker::CostTracker;

/// Spending cap enforcer.
pub struct SpendingCapEnforcer {
    cost_tracker: std::sync::Arc<CostTracker>,
    trigger_sender: Option<tokio::sync::mpsc::Sender<TriggerEvent>>,
}

impl SpendingCapEnforcer {
    pub fn new(cost_tracker: std::sync::Arc<CostTracker>) -> Self {
        Self {
            cost_tracker,
            trigger_sender: None,
        }
    }

    pub fn with_trigger_sender(
        mut self,
        sender: tokio::sync::mpsc::Sender<TriggerEvent>,
    ) -> Self {
        self.trigger_sender = Some(sender);
        self
    }

    /// Pre-call check: will this cost exceed the cap?
    pub fn check_pre_call(
        &self,
        agent_id: Uuid,
        estimated_cost: f64,
        cap: f64,
    ) -> Result<(), SpendingCapError> {
        let current = self.cost_tracker.get_daily_total(agent_id);
        let projected = current + estimated_cost;
        if projected > cap {
            return Err(SpendingCapError {
                agent_id,
                daily_total: projected,
                cap,
                overage: projected - cap,
            });
        }
        Ok(())
    }

    /// Post-call check: did actual cost push over cap?
    pub fn check_post_call(&self, agent_id: Uuid, _actual_cost: f64, cap: f64) {
        let current = self.cost_tracker.get_daily_total(agent_id);
        if current > cap {
            if let Some(sender) = &self.trigger_sender {
                if sender.try_send(TriggerEvent::SpendingCapExceeded {
                    agent_id,
                    daily_total: current,
                    cap,
                    overage: current - cap,
                    detected_at: chrono::Utc::now(),
                }).is_err() {
                    tracing::error!(
                        agent_id = %agent_id,
                        "trigger channel full — SpendingCapExceeded event dropped (AC13)"
                    );
                }
            }
        }
    }
}

/// Spending cap exceeded error.
#[derive(Debug, Clone)]
pub struct SpendingCapError {
    pub agent_id: Uuid,
    pub daily_total: f64,
    pub cap: f64,
    pub overage: f64,
}

impl std::fmt::Display for SpendingCapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Spending cap exceeded for agent {}: ${:.2} / ${:.2} (overage: ${:.2})",
            self.agent_id, self.daily_total, self.cap, self.overage
        )
    }
}
