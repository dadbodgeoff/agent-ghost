//! Message dispatcher: 3-gate verification pipeline (Req 19 AC4-AC7, AC12-AC13).

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use cortex_core::safety::trigger::TriggerEvent;
use uuid::Uuid;

use super::protocol::AgentMessage;

/// Rate limit configuration.
const RATE_LIMIT_PER_AGENT_PER_HOUR: u32 = 60;
const RATE_LIMIT_PER_PAIR_PER_HOUR: u32 = 30;
const REPLAY_WINDOW: Duration = Duration::from_secs(300); // 5 minutes
const ANOMALY_WINDOW: Duration = Duration::from_secs(300); // 5 minutes
const ANOMALY_THRESHOLD: u32 = 3;
/// Hourly reset interval for rate limit counters.
const RATE_LIMIT_RESET_INTERVAL: Duration = Duration::from_secs(3600);

/// Verification result.
#[derive(Debug, Clone)]
pub enum VerifyResult {
    Accepted,
    RejectedSignature(String),
    RejectedReplay(String),
    RejectedPolicy(String),
    RejectedRateLimit,
    /// Anomaly detected — kill switch evaluation required (AC6).
    AnomalyDetected {
        agent_id: Uuid,
        failure_count: usize,
        trigger: TriggerEvent,
    },
}

/// Message dispatcher with 3-gate pipeline.
pub struct MessageDispatcher {
    /// Seen nonces for replay prevention.
    seen_nonces: BTreeMap<Uuid, Instant>,
    /// Per-agent message counts for rate limiting.
    agent_counts: BTreeMap<Uuid, u32>,
    /// Per-pair message counts.
    pair_counts: BTreeMap<(Uuid, Uuid), u32>,
    /// Signature failure counter for anomaly detection.
    sig_failures: BTreeMap<Uuid, Vec<Instant>>,
    /// Offline message queues.
    offline_queues: BTreeMap<Uuid, VecDeque<AgentMessage>>,
    /// Key rotation grace period tracking.
    _grace_keys: BTreeMap<Uuid, (Vec<u8>, Instant)>,
    /// Last rate limit counter reset time.
    last_rate_reset: Instant,
    /// Per-sender last seen UUIDv7 nonce for monotonicity check (AC4).
    last_nonce: BTreeMap<Uuid, (Uuid, Instant)>,
    /// Registered sender verifying keys.
    verifying_keys: BTreeMap<Uuid, ghost_signing::VerifyingKey>,
}

impl MessageDispatcher {
    pub fn new() -> Self {
        Self {
            seen_nonces: BTreeMap::new(),
            agent_counts: BTreeMap::new(),
            pair_counts: BTreeMap::new(),
            sig_failures: BTreeMap::new(),
            offline_queues: BTreeMap::new(),
            _grace_keys: BTreeMap::new(),
            last_rate_reset: Instant::now(),
            last_nonce: BTreeMap::new(),
            verifying_keys: BTreeMap::new(),
        }
    }

    pub fn register_verifying_key(&mut self, agent_id: Uuid, key: ghost_signing::VerifyingKey) {
        self.verifying_keys.insert(agent_id, key);
    }

    /// Process an incoming message through the 3-gate pipeline.
    ///
    /// Returns `AnomalyDetected` when 3+ signature failures occur within
    /// 5 minutes for the same agent (AC6). The caller MUST forward the
    /// contained `TriggerEvent` to the kill switch evaluator.
    pub fn verify(&mut self, msg: &AgentMessage) -> VerifyResult {
        // Periodic rate limit counter reset (hourly)
        self.maybe_reset_rate_limits();
        self.prune_replay_state();

        if msg.encrypted {
            return VerifyResult::RejectedPolicy(
                "encrypted inter-agent messages are disabled until authenticated encryption is implemented"
                    .into(),
            );
        }

        // Gate 1: Signature verification (content_hash first, then Ed25519)
        let computed_hash = msg.compute_content_hash();
        if computed_hash != msg.content_hash {
            if let Some(result) = self.record_sig_failure(msg.sender) {
                return result;
            }
            return VerifyResult::RejectedSignature("content_hash mismatch".into());
        }
        if msg.signature.is_empty() {
            if let Some(result) = self.record_sig_failure(msg.sender) {
                return result;
            }
            return VerifyResult::RejectedSignature("missing signature".into());
        }
        let signature = match ghost_signing::Signature::from_bytes(&msg.signature) {
            Some(signature) => signature,
            None => {
                if let Some(result) = self.record_sig_failure(msg.sender) {
                    return result;
                }
                return VerifyResult::RejectedSignature("invalid signature length".into());
            }
        };
        let Some(key) = self.verifying_keys.get(&msg.sender) else {
            if let Some(result) = self.record_sig_failure(msg.sender) {
                return result;
            }
            return VerifyResult::RejectedSignature("unknown sender verifying key".into());
        };
        if !ghost_signing::verify(&msg.canonical_bytes(), &signature, key) {
            if let Some(result) = self.record_sig_failure(msg.sender) {
                return result;
            }
            return VerifyResult::RejectedSignature("signature verification failed".into());
        }

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
        // Timestamp freshness — reject messages older than REPLAY_WINDOW
        let age = chrono::Utc::now() - msg.timestamp;
        if age > chrono::Duration::seconds(REPLAY_WINDOW.as_secs() as i64) {
            return false;
        }

        // Reject future-dated messages: a negative age means the
        // timestamp is in the future. Allow a small clock-skew
        // tolerance (30s) but reject anything beyond that.
        if age < chrono::Duration::seconds(-30) {
            tracing::warn!(
                sender = %msg.sender,
                timestamp = %msg.timestamp,
                "rejected future-dated message (clock skew > 30s)"
            );
            return false;
        }

        // Nonce uniqueness
        if self.seen_nonces.contains_key(&msg.nonce) {
            return false;
        }

        // UUIDv7 monotonicity check (AC4): nonce must be strictly
        // greater than the last seen nonce from this sender. UUIDv7
        // encodes a timestamp in the high bits, so lexicographic
        // comparison enforces temporal monotonicity.
        if let Some((last, _seen_at)) = self.last_nonce.get(&msg.sender) {
            if msg.nonce <= *last {
                tracing::warn!(
                    sender = %msg.sender,
                    nonce = %msg.nonce,
                    last_nonce = %last,
                    "UUIDv7 monotonicity violation — replay rejected"
                );
                return false;
            }
        }
        let now = Instant::now();
        self.last_nonce.insert(msg.sender, (msg.nonce, now));

        self.seen_nonces.insert(msg.nonce, now);
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

    /// Reset rate limit counters hourly. Without this, agents are
    /// permanently rate-limited after reaching the per-hour threshold.
    fn maybe_reset_rate_limits(&mut self) {
        if self.last_rate_reset.elapsed() >= RATE_LIMIT_RESET_INTERVAL {
            self.agent_counts.clear();
            self.pair_counts.clear();
            self.last_rate_reset = Instant::now();
        }
    }

    fn prune_replay_state(&mut self) {
        self.seen_nonces
            .retain(|_, seen_at| seen_at.elapsed() < REPLAY_WINDOW);
        self.last_nonce
            .retain(|_, (_, seen_at)| seen_at.elapsed() < REPLAY_WINDOW);
    }

    /// Record a signature failure and check anomaly threshold (AC6).
    ///
    /// Returns `Some(AnomalyDetected)` when the threshold is reached,
    /// signaling the caller to forward the trigger to the kill switch.
    /// Uses T7 (MemoryHealthCritical) with signature_anomaly sub-score
    /// to classify as QUARANTINE per the trigger classification table.
    fn record_sig_failure(&mut self, agent_id: Uuid) -> Option<VerifyResult> {
        let failures = self.sig_failures.entry(agent_id).or_default();
        failures.push(Instant::now());
        // Clean old entries outside the anomaly window
        failures.retain(|t| t.elapsed() < ANOMALY_WINDOW);

        if failures.len() >= ANOMALY_THRESHOLD as usize {
            tracing::error!(
                agent_id = %agent_id,
                failures = failures.len(),
                "Anomaly: {} signature failures in {:?} — triggering kill switch evaluation (AC6)",
                failures.len(),
                ANOMALY_WINDOW
            );
            // T7 MemoryHealthCritical with signature_anomaly sub-score.
            // health_score=0.0 (critical) ensures the trigger fires.
            // The sub_scores map clearly identifies this as a signature
            // verification anomaly, not a general memory health issue.
            let trigger = TriggerEvent::MemoryHealthCritical {
                agent_id,
                health_score: 0.0,
                threshold: 1.0,
                sub_scores: {
                    let mut m = std::collections::BTreeMap::new();
                    m.insert(
                        "signature_verification_failures".to_string(),
                        failures.len() as f64,
                    );
                    m.insert(
                        "anomaly_window_secs".to_string(),
                        ANOMALY_WINDOW.as_secs() as f64,
                    );
                    m
                },
                detected_at: chrono::Utc::now(),
            };
            return Some(VerifyResult::AnomalyDetected {
                agent_id,
                failure_count: failures.len(),
                trigger,
            });
        }
        None
    }

    /// Queue a message for an offline agent.
    pub fn queue_offline(&mut self, recipient: Uuid, msg: AgentMessage) {
        let queue = self.offline_queues.entry(recipient).or_default();
        // Bound the offline queue — messages expire after replay window
        while queue.len() >= 100 {
            queue.pop_front();
        }
        queue.push_back(msg);
    }

    /// Deliver queued messages when agent comes online.
    /// Filters out expired messages (older than replay window).
    pub fn deliver_queued(&mut self, agent_id: Uuid) -> Vec<AgentMessage> {
        self.offline_queues
            .remove(&agent_id)
            .map(|q| {
                let now = chrono::Utc::now();
                q.into_iter()
                    .filter(|msg| {
                        let age = now - msg.timestamp;
                        // Reject expired messages (older than REPLAY_WINDOW)
                        if age > chrono::Duration::seconds(REPLAY_WINDOW.as_secs() as i64) {
                            return false;
                        }
                        // Reject future-dated messages (same guard as check_replay):
                        // a negative age means the timestamp is in the future.
                        // Allow 30s clock-skew tolerance.
                        if age < chrono::Duration::seconds(-30) {
                            tracing::warn!(
                                sender = %msg.sender,
                                timestamp = %msg.timestamp,
                                "deliver_queued: rejected future-dated message (clock skew > 30s)"
                            );
                            return false;
                        }
                        true
                    })
                    .collect()
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn signed_message(
        dispatcher: &mut MessageDispatcher,
        sender_key: &ghost_signing::SigningKey,
        sender: Uuid,
        recipient: Uuid,
    ) -> AgentMessage {
        dispatcher.register_verifying_key(sender, sender_key.verifying_key());
        let mut msg = AgentMessage::new(
            sender,
            recipient,
            "Notification".into(),
            serde_json::json!({ "message": "hello" }),
        );
        msg.timestamp = Utc::now();
        msg.sign(sender_key);
        msg
    }

    #[test]
    fn missing_signature_is_rejected() {
        let mut dispatcher = MessageDispatcher::new();
        let (signing_key, verifying_key) = ghost_signing::generate_keypair();
        let sender = Uuid::now_v7();
        dispatcher.register_verifying_key(sender, verifying_key);
        let mut msg = AgentMessage::new(
            sender,
            Uuid::now_v7(),
            "Notification".into(),
            serde_json::json!({ "message": "unsigned" }),
        );
        msg.content_hash = msg.compute_content_hash();
        assert!(matches!(
            dispatcher.verify(&msg),
            VerifyResult::RejectedSignature(_)
        ));
        drop(signing_key);
    }

    #[test]
    fn signed_message_is_accepted() {
        let mut dispatcher = MessageDispatcher::new();
        let (signing_key, _) = ghost_signing::generate_keypair();
        let msg = signed_message(
            &mut dispatcher,
            &signing_key,
            Uuid::now_v7(),
            Uuid::now_v7(),
        );
        assert!(matches!(dispatcher.verify(&msg), VerifyResult::Accepted));
    }

    #[test]
    fn forged_signature_is_rejected() {
        let mut dispatcher = MessageDispatcher::new();
        let (trusted_key, _) = ghost_signing::generate_keypair();
        let (forged_key, _) = ghost_signing::generate_keypair();
        let sender = Uuid::now_v7();
        let mut msg = signed_message(&mut dispatcher, &trusted_key, sender, Uuid::now_v7());
        msg.sign(&forged_key);
        assert!(matches!(
            dispatcher.verify(&msg),
            VerifyResult::RejectedSignature(_)
        ));
    }

    #[test]
    fn encrypted_messages_fail_closed() {
        let mut dispatcher = MessageDispatcher::new();
        let (signing_key, _) = ghost_signing::generate_keypair();
        let mut msg = signed_message(
            &mut dispatcher,
            &signing_key,
            Uuid::now_v7(),
            Uuid::now_v7(),
        );
        msg.encrypted = true;
        assert!(matches!(
            dispatcher.verify(&msg),
            VerifyResult::RejectedPolicy(_)
        ));
    }

    #[test]
    fn replay_window_survives_rate_limit_reset_boundary() {
        let mut dispatcher = MessageDispatcher::new();
        let (signing_key, _) = ghost_signing::generate_keypair();
        let sender = Uuid::now_v7();
        let original = signed_message(&mut dispatcher, &signing_key, sender, Uuid::now_v7());
        assert!(matches!(
            dispatcher.verify(&original),
            VerifyResult::Accepted
        ));

        dispatcher.last_rate_reset = Instant::now() - RATE_LIMIT_RESET_INTERVAL;
        let fresh = signed_message(&mut dispatcher, &signing_key, sender, Uuid::now_v7());
        assert!(matches!(dispatcher.verify(&fresh), VerifyResult::Accepted));
        assert!(matches!(
            dispatcher.verify(&original),
            VerifyResult::RejectedReplay(_)
        ));
    }
}
