//! Bridge between the local KillSwitch and the distributed KillGate.
//!
//! When a kill is activated locally, the bridge propagates it through
//! the KillGate relay. When a remote kill arrives, the bridge activates
//! the local KillSwitch.

use std::sync::Arc;

use ghost_kill_gates::config::KillGateConfig;
use ghost_kill_gates::gate::KillGate;
use ghost_kill_gates::relay::{KillGateRelay, PeerNode};
use uuid::Uuid;

use super::kill_switch::KillSwitch;

/// Bridges local KillSwitch with distributed KillGate.
pub struct KillGateBridge {
    pub kill_switch: Arc<KillSwitch>,
    pub gate: Arc<KillGate>,
    pub relay: KillGateRelay,
}

impl KillGateBridge {
    /// Create a new bridge with the given node ID and config.
    pub fn new(
        node_id: Uuid,
        kill_switch: Arc<KillSwitch>,
        config: KillGateConfig,
    ) -> Self {
        let gate = Arc::new(KillGate::new(node_id, config));
        let relay = KillGateRelay::new(Arc::clone(&gate));
        Self {
            kill_switch,
            gate,
            relay,
        }
    }

    /// Register a peer node for distributed coordination.
    pub fn add_peer(&mut self, peer: PeerNode) {
        self.relay.add_peer(peer);
    }

    /// Close the gate and initiate propagation.
    /// Called after local KillSwitch activation.
    pub fn close_and_propagate(&mut self, reason: String) {
        let event = self.gate.close(reason);
        self.gate.begin_propagation();
        let _msg = self.relay.build_close_notification(event);
        // In production, _msg would be sent to all peers via the transport layer.
        // The relay.process_message() handles incoming acks.
    }

    /// Check if the distributed gate is closed.
    /// Used by GATE 3 in the agent loop.
    pub fn is_gate_closed(&self) -> bool {
        self.gate.is_closed()
    }

    /// Cluster size (peers + self).
    pub fn cluster_size(&self) -> usize {
        self.relay.cluster_size()
    }
}
