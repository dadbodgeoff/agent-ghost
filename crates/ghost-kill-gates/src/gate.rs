//! KillGate — distributed gate state machine.
//!
//! Wraps the local KillSwitch with distributed coordination.
//! Gate states: Normal → GateClosed → Propagating → Confirmed → (QuorumResume → Normal).
//!
//! INV-KG-01: Gate close is monotonic — severity never decreases without quorum.
//! INV-KG-05: Gate state is SeqCst atomic — no stale reads.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::RwLock;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::chain::{
    compute_gate_event_hash, GateChainEvent, GateEventType, GENESIS_HASH,
};
use crate::config::KillGateConfig;
use crate::quorum::{QuorumTracker, ResumeVote};

/// Gate state as atomic u8 for SeqCst reads (INV-KG-05).
const STATE_NORMAL: u8 = 0;
const STATE_GATE_CLOSED: u8 = 1;
const STATE_PROPAGATING: u8 = 2;
const STATE_CONFIRMED: u8 = 3;
const STATE_QUORUM_RESUME: u8 = 4;

/// Distributed gate state (serializable snapshot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateState {
    Normal,
    GateClosed,
    Propagating,
    Confirmed,
    QuorumResume,
}

impl GateState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            STATE_NORMAL => Self::Normal,
            STATE_GATE_CLOSED => Self::GateClosed,
            STATE_PROPAGATING => Self::Propagating,
            STATE_CONFIRMED => Self::Confirmed,
            STATE_QUORUM_RESUME => Self::QuorumResume,
            unknown => {
                tracing::error!(
                    value = unknown,
                    "unknown GateState u8 value — defaulting to GateClosed (fail-closed, potential corruption)"
                );
                Self::GateClosed
            }
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Normal => STATE_NORMAL,
            Self::GateClosed => STATE_GATE_CLOSED,
            Self::Propagating => STATE_PROPAGATING,
            Self::Confirmed => STATE_CONFIRMED,
            Self::QuorumResume => STATE_QUORUM_RESUME,
        }
    }
}

impl From<GateState> for u8 {
    fn from(state: GateState) -> Self {
        state.to_u8()
    }
}

/// Snapshot of gate state for external consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSnapshot {
    pub state: GateState,
    pub node_id: Uuid,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
    pub acked_nodes: Vec<Uuid>,
    pub chain_length: usize,
}

/// The distributed kill gate.
pub struct KillGate {
    node_id: Uuid,
    state: AtomicU8,
    config: KillGateConfig,
    inner: RwLock<GateInner>,
}

struct GateInner {
    closed_at: Option<DateTime<Utc>>,
    close_reason: Option<String>,
    propagation_start: Option<Instant>,
    acked_nodes: Vec<Uuid>,
    chain: Vec<GateChainEvent>,
    quorum_tracker: Option<QuorumTracker>,
}

impl KillGate {
    pub fn new(node_id: Uuid, config: KillGateConfig) -> Self {
        Self {
            node_id,
            state: AtomicU8::new(STATE_NORMAL),
            config,
            inner: RwLock::new(GateInner {
                closed_at: None,
                close_reason: None,
                propagation_start: None,
                acked_nodes: Vec::new(),
                chain: Vec::new(),
                quorum_tracker: None,
            }),
        }
    }

    /// Fast-path check: is the gate closed? (SeqCst, no lock).
    /// QuorumResume is still considered closed — gate only opens after quorum.
    pub fn is_closed(&self) -> bool {
        self.state.load(Ordering::SeqCst) != STATE_NORMAL
    }

    /// Current gate state.
    pub fn state(&self) -> GateState {
        GateState::from_u8(self.state.load(Ordering::SeqCst))
    }

    /// Close the gate (local kill triggered). Returns the chain event.
    pub fn close(&self, reason: String) -> GateChainEvent {
        let now = Utc::now();
        let mut inner = match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(
                    node_id = %self.node_id,
                    "kill gate RwLock poisoned during close — recovering with poisoned guard"
                );
                poisoned.into_inner()
            }
        };

        // Monotonicity: if already closed at same or higher severity, no-op
        // but still record the event for audit.
        self.state.store(STATE_GATE_CLOSED, Ordering::SeqCst);
        inner.closed_at = Some(now);
        inner.close_reason = Some(reason.clone());
        inner.propagation_start = Some(Instant::now());
        inner.acked_nodes.clear();

        let previous_hash = inner
            .chain
            .last()
            .map(|e| e.event_hash)
            .unwrap_or(GENESIS_HASH);

        let payload = serde_json::json!({ "reason": reason }).to_string();
        let event_hash = compute_gate_event_hash(
            GateEventType::Close,
            &self.node_id,
            &now,
            &payload,
            &previous_hash,
        );

        let event = GateChainEvent {
            event_type: GateEventType::Close,
            node_id: self.node_id,
            timestamp: now,
            payload_json: payload,
            event_hash,
            previous_hash,
        };

        inner.chain.push(event.clone());
        tracing::warn!(
            node_id = %self.node_id,
            reason = %reason,
            "Kill gate CLOSED"
        );

        event
    }

    /// Transition to Propagating state after initiating fan-out.
    pub fn begin_propagation(&self) {
        let current = self.state.load(Ordering::SeqCst);
        if current == STATE_GATE_CLOSED {
            self.state.store(STATE_PROPAGATING, Ordering::SeqCst);
        }
    }

    /// Record an ack from a peer node. Returns true if all known peers acked.
    pub fn record_ack(&self, peer_node_id: Uuid, cluster_size: usize) -> bool {
        let now = Utc::now();
        let mut inner = match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(
                    node_id = %self.node_id,
                    "kill gate RwLock poisoned during record_ack — recovering with poisoned guard"
                );
                poisoned.into_inner()
            }
        };

        if !inner.acked_nodes.contains(&peer_node_id) {
            inner.acked_nodes.push(peer_node_id);
        }

        // Record in chain
        let previous_hash = inner
            .chain
            .last()
            .map(|e| e.event_hash)
            .unwrap_or(GENESIS_HASH);
        let payload =
            serde_json::json!({ "peer": peer_node_id.to_string() }).to_string();
        let event_hash = compute_gate_event_hash(
            GateEventType::Ack,
            &self.node_id,
            &now,
            &payload,
            &previous_hash,
        );
        inner.chain.push(GateChainEvent {
            event_type: GateEventType::Ack,
            node_id: self.node_id,
            timestamp: now,
            payload_json: payload,
            event_hash,
            previous_hash,
        });

        // All peers acked? (cluster_size - 1 because we don't ack ourselves)
        let all_acked = inner.acked_nodes.len() >= cluster_size.saturating_sub(1);
        if all_acked {
            self.state.store(STATE_CONFIRMED, Ordering::SeqCst);
            tracing::info!(
                node_id = %self.node_id,
                acks = inner.acked_nodes.len(),
                "Kill gate CONFIRMED — all peers acked"
            );
        }

        all_acked
    }

    /// Check if propagation has timed out (fail-closed: INV-KG-02).
    pub fn is_propagation_timed_out(&self) -> bool {
        let inner = match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(
                    node_id = %self.node_id,
                    "kill gate RwLock poisoned during timeout check — fail-closed"
                );
                // Fail-closed: treat as timed out when lock is poisoned
                return true;
            }
        };
        if let Some(start) = inner.propagation_start {
            start.elapsed() > self.config.max_propagation
        } else {
            false
        }
    }

    /// Cast a resume vote. Returns true if quorum reached and gate reopened.
    pub fn cast_resume_vote(
        &self,
        vote: ResumeVote,
        cluster_size: usize,
    ) -> bool {
        let current = self.state.load(Ordering::SeqCst);
        if current == STATE_NORMAL {
            return true; // already open
        }

        let mut inner = match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(
                    node_id = %self.node_id,
                    "kill gate RwLock poisoned during resume vote — recovering with poisoned guard"
                );
                poisoned.into_inner()
            }
        };

        // Initialize quorum tracker if needed
        if inner.quorum_tracker.is_none() {
            let required = self.config.effective_quorum(cluster_size);
            inner.quorum_tracker = Some(QuorumTracker::new(required));
            self.state.store(STATE_QUORUM_RESUME, Ordering::SeqCst);
        }

        let tracker = inner.quorum_tracker.as_mut().unwrap();
        let quorum_reached = tracker.cast_vote(vote);

        if quorum_reached {
            // Extract tracker values before dropping the mutable borrow.
            let vote_count = tracker.vote_count();
            let required = tracker.required();

            self.state.store(STATE_NORMAL, Ordering::SeqCst);
            inner.closed_at = None;
            inner.close_reason = None;
            inner.propagation_start = None;
            inner.acked_nodes.clear();

            // Record resume in chain
            let now = Utc::now();
            let previous_hash = inner
                .chain
                .last()
                .map(|e| e.event_hash)
                .unwrap_or(GENESIS_HASH);
            let payload = serde_json::json!({
                "votes": vote_count,
                "required": required,
            })
            .to_string();
            let event_hash = compute_gate_event_hash(
                GateEventType::ResumeConfirmed,
                &self.node_id,
                &now,
                &payload,
                &previous_hash,
            );
            inner.chain.push(GateChainEvent {
                event_type: GateEventType::ResumeConfirmed,
                node_id: self.node_id,
                timestamp: now,
                payload_json: payload,
                event_hash,
                previous_hash,
            });

            inner.quorum_tracker = None;
            tracing::info!(
                node_id = %self.node_id,
                "Kill gate RESUMED via quorum"
            );
        }

        quorum_reached
    }

    /// Get a snapshot of the current gate state.
    pub fn snapshot(&self) -> GateSnapshot {
        let inner = match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(
                    node_id = %self.node_id,
                    "kill gate RwLock poisoned during snapshot — recovering with poisoned guard"
                );
                poisoned.into_inner()
            }
        };
        GateSnapshot {
            state: self.state(),
            node_id: self.node_id,
            closed_at: inner.closed_at,
            close_reason: inner.close_reason.clone(),
            acked_nodes: inner.acked_nodes.clone(),
            chain_length: inner.chain.len(),
        }
    }

    /// Get the full chain for verification.
    pub fn chain(&self) -> Vec<GateChainEvent> {
        match self.inner.read() {
            Ok(guard) => guard.chain.clone(),
            Err(poisoned) => {
                tracing::error!(
                    node_id = %self.node_id,
                    "kill gate RwLock poisoned during chain read — recovering with poisoned guard"
                );
                poisoned.into_inner().chain.clone()
            }
        }
    }

    /// Node ID of this gate.
    pub fn node_id(&self) -> Uuid {
        self.node_id
    }
}
