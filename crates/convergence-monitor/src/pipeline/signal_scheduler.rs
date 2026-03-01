//! Signal frequency tier assignment (Task 19.1).
//!
//! 5 frequency tiers replacing compute-on-every-event:
//! - EveryMessage: S3 (response latency), S6 (initiative balance)
//! - Every5thMessage: S5 (goal boundary erosion), S8 (behavioral anomaly)
//! - SessionBoundary: S1, S2, S4, S7, full composite, baseline update
//! - Every5Minutes: identity drift, DNS re-resolution, OAuth token expiry
//! - Every15Minutes: memory compaction eligibility, state file write, ITP batch flush
//!
//! The scheduler wraps SignalComputer — it decides WHEN to compute,
//! SignalComputer decides WHAT to compute.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// Signal frequency tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalFrequencyTier {
    EveryMessage,
    Every5thMessage,
    SessionBoundary,
    Every5Minutes,
    Every15Minutes,
}

/// What triggered the computation check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeTrigger {
    MessageReceived,
    SessionBoundary,
    Timer5Min,
    Timer15Min,
}

/// Default tier assignments for the 8 signals.
const DEFAULT_TIER_ASSIGNMENTS: [SignalFrequencyTier; 8] = [
    SignalFrequencyTier::SessionBoundary,   // S1: session_duration
    SignalFrequencyTier::SessionBoundary,   // S2: inter_session_gap
    SignalFrequencyTier::EveryMessage,      // S3: response_latency
    SignalFrequencyTier::SessionBoundary,   // S4: vocabulary_convergence
    SignalFrequencyTier::Every5thMessage,   // S5: goal_boundary_erosion
    SignalFrequencyTier::EveryMessage,      // S6: initiative_balance
    SignalFrequencyTier::SessionBoundary,   // S7: disengagement_resistance
    SignalFrequencyTier::Every5thMessage,   // S8: behavioral_anomaly
];

/// Default staleness thresholds per tier.
const DEFAULT_STALE_AFTER: [Duration; 8] = [
    Duration::from_secs(300),  // S1: SessionBoundary → 5min
    Duration::from_secs(300),  // S2: SessionBoundary → 5min
    Duration::ZERO,            // S3: EveryMessage → 0s
    Duration::from_secs(300),  // S4: SessionBoundary → 5min
    Duration::from_secs(30),   // S5: Every5thMessage → 30s
    Duration::ZERO,            // S6: EveryMessage → 0s
    Duration::from_secs(300),  // S7: SessionBoundary → 5min
    Duration::from_secs(30),   // S8: Every5thMessage → 30s
];

/// Signal scheduler — gates which signals are computed per event.
pub struct SignalScheduler {
    tier_assignment: [SignalFrequencyTier; 8],
    /// Per-agent message count.
    message_counter: BTreeMap<Uuid, u64>,
    /// Per-agent, per-signal last computation time.
    last_computed: BTreeMap<(Uuid, usize), Instant>,
    stale_after: [Duration; 8],
    /// Per-agent, per-signal dirty flag.
    dirty: BTreeMap<(Uuid, usize), bool>,
}

impl SignalScheduler {
    pub fn new() -> Self {
        Self {
            tier_assignment: DEFAULT_TIER_ASSIGNMENTS,
            message_counter: BTreeMap::new(),
            last_computed: BTreeMap::new(),
            stale_after: DEFAULT_STALE_AFTER,
            dirty: BTreeMap::new(),
        }
    }

    /// Check if a signal should be computed for the given trigger.
    pub fn should_compute(
        &self,
        agent_id: Uuid,
        signal_index: usize,
        trigger: &ComputeTrigger,
    ) -> bool {
        if signal_index >= 8 {
            return false; // Unknown signal index
        }

        let tier = self.tier_assignment[signal_index];

        // Check if dirty
        let is_dirty = self
            .dirty
            .get(&(agent_id, signal_index))
            .copied()
            .unwrap_or(false);

        if !is_dirty {
            return false;
        }

        match (tier, trigger) {
            (SignalFrequencyTier::EveryMessage, ComputeTrigger::MessageReceived) => true,
            (SignalFrequencyTier::Every5thMessage, ComputeTrigger::MessageReceived) => {
                let count = self.message_counter.get(&agent_id).copied().unwrap_or(0);
                count % 5 == 0
            }
            (SignalFrequencyTier::SessionBoundary, ComputeTrigger::SessionBoundary) => true,
            (SignalFrequencyTier::Every5Minutes, ComputeTrigger::Timer5Min) => true,
            (SignalFrequencyTier::Every15Minutes, ComputeTrigger::Timer15Min) => true,
            // SessionBoundary triggers ALL signals
            (_, ComputeTrigger::SessionBoundary) => true,
            _ => false,
        }
    }

    /// Record a message received for an agent. Marks tier-appropriate signals dirty.
    pub fn record_message(&mut self, agent_id: Uuid) {
        let count = self.message_counter.entry(agent_id).or_insert(0);
        *count += 1;

        // Mark EveryMessage signals dirty
        for (i, tier) in self.tier_assignment.iter().enumerate() {
            match tier {
                SignalFrequencyTier::EveryMessage => {
                    self.dirty.insert((agent_id, i), true);
                }
                SignalFrequencyTier::Every5thMessage => {
                    if *count % 5 == 0 {
                        self.dirty.insert((agent_id, i), true);
                    }
                }
                _ => {}
            }
        }
    }

    /// Record a session boundary. Marks ALL signals dirty and resets message counter.
    pub fn record_session_boundary(&mut self, agent_id: Uuid) {
        self.message_counter.insert(agent_id, 0);
        for i in 0..8 {
            self.dirty.insert((agent_id, i), true);
        }
    }

    /// Mark a signal as computed (clear dirty flag).
    pub fn mark_computed(&mut self, agent_id: Uuid, signal_index: usize) {
        if signal_index < 8 {
            self.dirty.insert((agent_id, signal_index), false);
            self.last_computed
                .insert((agent_id, signal_index), Instant::now());
        }
    }

    /// Get the message count for an agent.
    pub fn message_count(&self, agent_id: Uuid) -> u64 {
        self.message_counter.get(&agent_id).copied().unwrap_or(0)
    }
}

impl Default for SignalScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_signal_computes_on_message() {
        let mut sched = SignalScheduler::new();
        let agent = Uuid::nil();
        sched.record_message(agent);
        // S3 (index 2) is EveryMessage
        assert!(sched.should_compute(agent, 2, &ComputeTrigger::MessageReceived));
        // S6 (index 5) is EveryMessage
        assert!(sched.should_compute(agent, 5, &ComputeTrigger::MessageReceived));
    }

    #[test]
    fn every_5th_message_signal_only_on_5th() {
        let mut sched = SignalScheduler::new();
        let agent = Uuid::nil();
        // Messages 1-4: S5 (index 4) should NOT compute
        for _ in 0..4 {
            sched.record_message(agent);
        }
        assert!(!sched.should_compute(agent, 4, &ComputeTrigger::MessageReceived));
        // Message 5: S5 should compute
        sched.record_message(agent);
        assert!(sched.should_compute(agent, 4, &ComputeTrigger::MessageReceived));
    }

    #[test]
    fn session_boundary_computes_all() {
        let mut sched = SignalScheduler::new();
        let agent = Uuid::nil();
        sched.record_session_boundary(agent);
        for i in 0..8 {
            assert!(sched.should_compute(agent, i, &ComputeTrigger::SessionBoundary));
        }
    }

    #[test]
    fn session_boundary_signal_not_on_message() {
        let mut sched = SignalScheduler::new();
        let agent = Uuid::nil();
        sched.record_message(agent);
        // S1 (index 0) is SessionBoundary — should NOT compute on MessageReceived
        assert!(!sched.should_compute(agent, 0, &ComputeTrigger::MessageReceived));
    }

    #[test]
    fn message_counter_resets_at_session_boundary() {
        let mut sched = SignalScheduler::new();
        let agent = Uuid::nil();
        for _ in 0..10 {
            sched.record_message(agent);
        }
        assert_eq!(sched.message_count(agent), 10);
        sched.record_session_boundary(agent);
        assert_eq!(sched.message_count(agent), 0);
    }

    #[test]
    fn unknown_signal_index_ignored() {
        let sched = SignalScheduler::new();
        let agent = Uuid::nil();
        assert!(!sched.should_compute(agent, 8, &ComputeTrigger::MessageReceived));
        assert!(!sched.should_compute(agent, 100, &ComputeTrigger::MessageReceived));
    }

    #[test]
    fn mark_computed_clears_dirty() {
        let mut sched = SignalScheduler::new();
        let agent = Uuid::nil();
        sched.record_message(agent);
        assert!(sched.should_compute(agent, 2, &ComputeTrigger::MessageReceived));
        sched.mark_computed(agent, 2);
        assert!(!sched.should_compute(agent, 2, &ComputeTrigger::MessageReceived));
    }
}
