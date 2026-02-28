//! Message dispatcher: 3-gate verification pipeline (Req 19 AC4-AC7, AC12-AC13).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::protocol::AgentMessage;

/// Rate limit configuration.
const RATE_LIMIT_PER_AGENT_PER_HOUR: u32 = 60;
const RATE_LIMIT_PER_PAIR_PER_HOUR: u32 = 30;
const REPLAY_WINDOW: Duration = Duration::from_secs(300); // 5 minutes
const ANOMALY_WINDOW: Duration = Duration::from_secs(300); // 5 minutes
const ANOMALY_THRESHOLD: u32 = 3;

/// Verification result.
#[derive(Debug)]
pub enum VerifyResult {
    Accepted,
    RejectedSignature(String),
    RejectedReplay(String),
    RejectedPolicy(String),
    RejectedRateLimit,
}

/// Message dispatcher with 3-gate pipeline.
pub struct MessageDispatcher {
    /// Seen nonces for replay prevention.
    seen_nonces: BTreeSet<Uuid>,
    /// Per-agent message counts for rate limiting.
    agent_counts: BTreeMap<Uuid, u32>,
    /// Per-pair message counts.
    pair_counts: BTreeMap<(Uuid, Uuid), u32>,
    /// Signature failure counter for anomaly detection.
    sig_failures: BTreeMap<Uuid, Vec<Instant>>,
    /// Offline message queues.
    offline_queues: BTreeMap<Uuid, VecDeque<AgentMessage>>,
    /// Key rotation grace period tracking.
    grace_keys: BTreeMap<Uuid, (Vec<u8>, Instant)>,
}

impl MessageDispatcher {
    pub fn new() -> Self {
        Self {
            seen_nonces: BTreeSet::new(),
            agent_counts: BTreeMap::new(),
            pair_counts: BTreeMap::new(),
            sig_failures: BTreeMap::new(),
            offline_queues: BTreeMap::new(),
            grace_keys: BTreeMap::new(),
        }
    }

    /// Process an incoming message through the 3-gate pipeline.
    pub fn verify(&mut self, msg: &AgentMessage) -> VerifyResult {
        // Gate 1: Signature verification (content_hash first, then Ed25519)
        let computed_hash = msg.compute_content_hash();
        if computed_hash != msg.content_hash {
            self.record_sig_failure(msg.sender);
            return VerifyResult::RejectedSignature("content_hash mismatch".into());
        }
        // Ed25519 verification would happen here with registered keys

        // Gate 2: Replay prevention
        if !self.check_replay(msg) {
            return VerifyResult::RejectedReplay("replay detected".into());
        }

        // Gate 3: Rate limiting
        if !self.check_rate_limit(msg.sender, msg.recipient) {
            return VerifyResult::RejectedRateLimit;
        }

        VerifyResult::Accepted
    }

    fn check_replay(&mut self, msg: &AgentMessage) -> bool {
        // Timestamp freshness
        let age = chrono::Utc::now() - msg.timestamp;
        if age > chrono::Duration::seconds(REPLAY_WINDOW.as_secs() as i64) {
            return false;
        }

        // Nonce uniqueness
        if self.seen_nonces.contains(&msg.nonce) {
            return false;
        }
        self.seen_nonces.insert(msg.nonce);
        true
    }

    fn check_rate_limit(&mut self, sender: Uuid, recipient: Uuid) -> bool {
        let agent_count = self.agent_counts.entry(sender).or_insert(0);
        if *agent_count >= RATE_LIMIT_PER_AGENT_PER_HOUR {
            return false;
        }
        *agent_count += 1;

        let pair_count = self.pair_counts.entry((sender, recipient)).or_insert(0);
        if *pair_count >= RATE_LIMIT_PER_PAIR_PER_HOUR {
            return false;
        }
        *pair_count += 1;

        true
    }

    fn record_sig_failure(&mut self, agent_id: Uuid) {
        let failures = self.sig_failures.entry(agent_id).or_default();
        failures.push(Instant::now());
        // Clean old entries
        failures.retain(|t| t.elapsed() < ANOMALY_WINDOW);
        if failures.len() >= ANOMALY_THRESHOLD as usize {
            tracing::error!(
                agent_id = %agent_id,
                failures = failures.len(),
                "Anomaly: {} signature failures in {:?} — kill switch evaluation",
                failures.len(),
                ANOMALY_WINDOW
            );
        }
    }

    /// Queue a message for an offline agent.
    pub fn queue_offline(&mut self, recipient: Uuid, msg: AgentMessage) {
        self.offline_queues
            .entry(recipient)
            .or_default()
            .push_back(msg);
    }

    /// Deliver queued messages when agent comes online.
    pub fn deliver_queued(&mut self, agent_id: Uuid) -> Vec<AgentMessage> {
        self.offline_queues
            .remove(&agent_id)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default()
    }

    /// Signature failure count for an agent in the anomaly window.
    pub fn sig_failure_count(&self, agent_id: Uuid) -> usize {
        self.sig_failures
            .get(&agent_id)
            .map(|f| f.iter().filter(|t| t.elapsed() < ANOMALY_WINDOW).count())
            .unwrap_or(0)
    }
}

impl Default for MessageDispatcher {
    fn default() -> Self {
        Self::new()
    }
}
