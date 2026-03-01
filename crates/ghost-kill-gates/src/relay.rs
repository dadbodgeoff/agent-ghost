//! KillGateRelay — fan-out/gossip propagation of gate events.
//!
//! Responsible for broadcasting gate close events to all known peers
//! and collecting acks. Uses simple fan-out (not full gossip) since
//! cluster sizes are expected to be small (< 20 nodes).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::chain::GateChainEvent;
use crate::gate::KillGate;

/// A peer node in the cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerNode {
    pub node_id: Uuid,
    pub endpoint: String,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub is_alive: bool,
}

/// Message types exchanged between gate relays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateRelayMessage {
    /// Gate close notification with chain event.
    CloseNotification {
        origin_node: Uuid,
        event: GateChainEvent,
    },
    /// Ack of a close notification.
    CloseAck {
        acking_node: Uuid,
        origin_node: Uuid,
        chain_head_hash: [u8; 32],
    },
    /// Heartbeat for liveness detection.
    Heartbeat {
        node_id: Uuid,
        gate_state: u8,
        chain_length: usize,
        timestamp: DateTime<Utc>,
    },
    /// Resume vote broadcast.
    ResumeVoteBroadcast {
        node_id: Uuid,
        reason: String,
        initiated_by: String,
    },
}

/// The relay manages peer tracking and message dispatch.
pub struct KillGateRelay {
    gate: Arc<KillGate>,
    peers: BTreeMap<Uuid, PeerNode>,
    last_heartbeat_sent: Option<Instant>,
}

impl KillGateRelay {
    pub fn new(gate: Arc<KillGate>) -> Self {
        Self {
            gate,
            peers: BTreeMap::new(),
            last_heartbeat_sent: None,
        }
    }

    /// Register a peer node.
    pub fn add_peer(&mut self, peer: PeerNode) {
        self.peers.insert(peer.node_id, peer);
    }

    /// Remove a peer node.
    pub fn remove_peer(&mut self, node_id: &Uuid) {
        self.peers.remove(node_id);
    }

    /// Number of known peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Cluster size (peers + self).
    pub fn cluster_size(&self) -> usize {
        self.peers.len() + 1
    }

    /// Get all alive peers.
    pub fn alive_peers(&self) -> Vec<&PeerNode> {
        self.peers.values().filter(|p| p.is_alive).collect()
    }

    /// Build a close notification message for fan-out.
    pub fn build_close_notification(
        &self,
        event: GateChainEvent,
    ) -> GateRelayMessage {
        GateRelayMessage::CloseNotification {
            origin_node: self.gate.node_id(),
            event,
        }
    }

    /// Process an incoming relay message. Returns optional response.
    pub fn process_message(
        &mut self,
        msg: GateRelayMessage,
    ) -> Option<GateRelayMessage> {
        match msg {
            GateRelayMessage::CloseNotification { origin_node, event: _ } => {
                // Peer is closing their gate — close ours too
                if !self.gate.is_closed() {
                    self.gate.close(format!(
                        "propagated from node {}",
                        origin_node
                    ));
                }
                // Send ack
                let chain = self.gate.chain();
                let chain_head = chain
                    .last()
                    .map(|e| e.event_hash)
                    .unwrap_or(crate::chain::GENESIS_HASH);
                Some(GateRelayMessage::CloseAck {
                    acking_node: self.gate.node_id(),
                    origin_node,
                    chain_head_hash: chain_head,
                })
            }
            GateRelayMessage::CloseAck {
                acking_node,
                origin_node,
                ..
            } => {
                if origin_node == self.gate.node_id() {
                    self.gate.record_ack(acking_node, self.cluster_size());
                }
                None
            }
            GateRelayMessage::Heartbeat {
                node_id, timestamp, ..
            } => {
                if let Some(peer) = self.peers.get_mut(&node_id) {
                    peer.last_heartbeat = Some(timestamp);
                    peer.is_alive = true;
                }
                None
            }
            GateRelayMessage::ResumeVoteBroadcast {
                node_id,
                reason,
                initiated_by,
            } => {
                let vote = crate::quorum::ResumeVote {
                    node_id,
                    reason,
                    initiated_by,
                    voted_at: Utc::now(),
                };
                self.gate.cast_resume_vote(vote, self.cluster_size());
                None
            }
        }
    }

    /// Build a heartbeat message.
    pub fn build_heartbeat(&mut self) -> GateRelayMessage {
        self.last_heartbeat_sent = Some(Instant::now());
        let snapshot = self.gate.snapshot();
        GateRelayMessage::Heartbeat {
            node_id: self.gate.node_id(),
            gate_state: snapshot.state as u8,
            chain_length: snapshot.chain_length,
            timestamp: Utc::now(),
        }
    }
}
