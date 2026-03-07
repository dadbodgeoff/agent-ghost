//! Cascade circuit breaker: per-agent-pair breaker that trips when
//! delegated tasks fail or target agent's convergence score spikes.
//!
//! Independent from the agent-loop CircuitBreaker (different scope:
//! per-agent-pair vs per-agent).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::error::MeshError;

/// Circuit breaker state (same pattern as ghost-agent-loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CascadeBreakerState {
    /// Normal operation — delegations pass through.
    Closed,
    /// Tripped — delegations to this target are blocked.
    Open,
    /// Cooldown elapsed — allow one probe delegation.
    HalfOpen,
}

/// Per-agent-pair circuit breaker entry.
#[derive(Debug)]
struct BreakerEntry {
    state: CascadeBreakerState,
    consecutive_failures: u32,
    last_failure: Option<Instant>,
}

impl BreakerEntry {
    fn new() -> Self {
        Self {
            state: CascadeBreakerState::Closed,
            consecutive_failures: 0,
            last_failure: None,
        }
    }

    fn effective_state(&self, cooldown: Duration) -> CascadeBreakerState {
        if self.state == CascadeBreakerState::Open {
            if let Some(last) = self.last_failure {
                if last.elapsed() >= cooldown {
                    return CascadeBreakerState::HalfOpen;
                }
            }
        }
        self.state
    }
}

/// Cascade circuit breaker managing per-agent-pair breakers.
pub struct CascadeCircuitBreaker {
    /// Per-pair breaker state: (from, to) → breaker.
    breakers: BTreeMap<(Uuid, Uuid), BreakerEntry>,
    /// Failure threshold before tripping.
    threshold: u32,
    /// Cooldown before Open → HalfOpen.
    cooldown: Duration,
    /// Maximum delegation depth.
    max_delegation_depth: u32,
    /// Convergence score threshold for tripping.
    convergence_spike_threshold: f64,
}

impl CascadeCircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration, max_delegation_depth: u32) -> Self {
        Self {
            breakers: BTreeMap::new(),
            threshold,
            cooldown,
            max_delegation_depth,
            convergence_spike_threshold: 0.7,
        }
    }

    /// Set the convergence spike threshold.
    pub fn set_convergence_spike_threshold(&mut self, threshold: f64) {
        self.convergence_spike_threshold = threshold;
    }

    /// Get the current state of the breaker for a specific agent pair.
    pub fn state(&self, from: Uuid, to: Uuid) -> CascadeBreakerState {
        self.breakers
            .get(&(from, to))
            .map_or(CascadeBreakerState::Closed, |e| {
                e.effective_state(self.cooldown)
            })
    }

    /// Check if a delegation from `from` to `to` is allowed.
    pub fn allows_delegation(&self, from: Uuid, to: Uuid) -> bool {
        match self.state(from, to) {
            CascadeBreakerState::Closed | CascadeBreakerState::HalfOpen => true,
            CascadeBreakerState::Open => false,
        }
    }

    /// Check delegation with depth tracking. Returns error if blocked.
    pub fn check_delegation(
        &self,
        from: Uuid,
        to: Uuid,
        current_depth: u32,
    ) -> Result<(), MeshError> {
        // Check depth limit.
        if current_depth >= self.max_delegation_depth {
            return Err(MeshError::DelegationDepthExceeded {
                depth: current_depth,
                max: self.max_delegation_depth,
            });
        }

        // Check circuit breaker.
        if !self.allows_delegation(from, to) {
            return Err(MeshError::CircuitBreakerOpen { from, to });
        }

        Ok(())
    }

    /// Record a successful delegation completion.
    pub fn record_success(&mut self, from: Uuid, to: Uuid) {
        let entry = self
            .breakers
            .entry((from, to))
            .or_insert_with(BreakerEntry::new);
        entry.consecutive_failures = 0;
        entry.state = CascadeBreakerState::Closed;
    }

    /// Record a delegation failure (task failed or error).
    pub fn record_failure(&mut self, from: Uuid, to: Uuid) {
        let entry = self
            .breakers
            .entry((from, to))
            .or_insert_with(BreakerEntry::new);
        entry.consecutive_failures += 1;
        entry.last_failure = Some(Instant::now());

        match entry.state {
            CascadeBreakerState::Closed => {
                if entry.consecutive_failures >= self.threshold {
                    entry.state = CascadeBreakerState::Open;
                    tracing::warn!(
                        from = %from,
                        to = %to,
                        failures = entry.consecutive_failures,
                        "cascade circuit breaker OPEN"
                    );
                }
            }
            CascadeBreakerState::HalfOpen => {
                entry.state = CascadeBreakerState::Open;
            }
            CascadeBreakerState::Open => {}
        }
    }

    /// Record a convergence score spike for a target agent.
    /// If the score exceeds the threshold, trip all breakers targeting that agent.
    pub fn record_convergence_spike(&mut self, target: Uuid, convergence_score: f64) {
        if convergence_score < self.convergence_spike_threshold {
            return;
        }

        tracing::warn!(
            target = %target,
            score = convergence_score,
            threshold = self.convergence_spike_threshold,
            "convergence spike — tripping cascade breakers for target"
        );

        // Trip all breakers where `to == target`.
        let pairs_to_trip: Vec<(Uuid, Uuid)> = self
            .breakers
            .keys()
            .filter(|(_, to)| *to == target)
            .copied()
            .collect();

        for (from, to) in &pairs_to_trip {
            if let Some(entry) = self.breakers.get_mut(&(*from, *to)) {
                entry.state = CascadeBreakerState::Open;
                entry.last_failure = Some(Instant::now());
            }
        }

        // Also create breakers for any agent that doesn't have one yet
        // but is known to interact with the target. We handle this by
        // ensuring the spike is recorded — callers should check before delegating.
    }

    /// Get the maximum delegation depth.
    pub fn max_delegation_depth(&self) -> u32 {
        self.max_delegation_depth
    }

    /// Get the failure threshold.
    pub fn threshold(&self) -> u32 {
        self.threshold
    }
}

impl Default for CascadeCircuitBreaker {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(300), 3)
    }
}

/// Tracks delegation chain depth per task to enforce max_delegation_depth.
pub struct DelegationDepthTracker {
    /// task_id → current depth.
    depths: BTreeMap<Uuid, u32>,
    /// Maximum allowed depth.
    max_depth: u32,
}

impl DelegationDepthTracker {
    pub fn new(max_depth: u32) -> Self {
        Self {
            depths: BTreeMap::new(),
            max_depth,
        }
    }

    /// Register a new task at depth 0.
    pub fn register_task(&mut self, task_id: Uuid) {
        self.depths.insert(task_id, 0);
    }

    /// Record a delegation hop, incrementing depth. Returns error if exceeded.
    pub fn record_hop(&mut self, task_id: Uuid) -> Result<u32, MeshError> {
        let depth = self.depths.entry(task_id).or_insert(0);
        *depth += 1;
        if *depth > self.max_depth {
            return Err(MeshError::DelegationDepthExceeded {
                depth: *depth,
                max: self.max_depth,
            });
        }
        Ok(*depth)
    }

    /// Get the current depth for a task.
    pub fn current_depth(&self, task_id: &Uuid) -> u32 {
        self.depths.get(task_id).copied().unwrap_or(0)
    }

    /// Remove a completed task from tracking.
    pub fn remove_task(&mut self, task_id: &Uuid) {
        self.depths.remove(task_id);
    }

    /// Check if a delegation at the given depth would be allowed.
    pub fn would_exceed(&self, current_depth: u32) -> bool {
        current_depth >= self.max_depth
    }

    /// Detect loops: if from == to in the delegation chain, it's a loop.
    pub fn detect_loop(chain: &[(Uuid, Uuid)]) -> bool {
        for (i, &(from_a, _)) in chain.iter().enumerate() {
            for &(_, to_b) in chain.iter().skip(i) {
                if from_a == to_b {
                    return true;
                }
            }
        }
        false
    }
}

impl Default for DelegationDepthTracker {
    fn default() -> Self {
        Self::new(3)
    }
}
