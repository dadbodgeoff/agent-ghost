//! AutoTriggerEvaluator: receives TriggerEvents, classifies, deduplicates,
//! and delegates to KillSwitch (Req 14 AC8-AC13, Req 14a).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cortex_core::safety::trigger::TriggerEvent;
use uuid::Uuid;

use super::kill_switch::{KillLevel, KillSwitch};

/// Deduplication key: trigger variant + agent_id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DedupKey {
    trigger_type: String,
    agent_id: Option<Uuid>,
}

/// Dedup entry with timestamp.
struct DedupEntry {
    last_seen: Instant,
}

/// Suppression window for deduplication.
const DEDUP_WINDOW: Duration = Duration::from_secs(60);
/// Cleanup interval for expired dedup entries.
const DEDUP_CLEANUP_INTERVAL: Duration = Duration::from_secs(300);

/// The auto-trigger evaluator. Processes triggers sequentially.
pub struct AutoTriggerEvaluator {
    kill_switch: Arc<KillSwitch>,
    dedup_map: BTreeMap<DedupKey, DedupEntry>,
    last_cleanup: Instant,
}

impl AutoTriggerEvaluator {
    pub fn new(kill_switch: Arc<KillSwitch>) -> Self {
        Self {
            kill_switch,
            dedup_map: BTreeMap::new(),
            last_cleanup: Instant::now(),
        }
    }

    /// Process a single trigger event. Returns the classified kill level.
    pub fn process(&mut self, trigger: TriggerEvent) -> Option<KillLevel> {
        // Periodic cleanup
        if self.last_cleanup.elapsed() > DEDUP_CLEANUP_INTERVAL {
            self.cleanup_dedup();
        }

        let key = compute_dedup_key(&trigger);

        // Dedup check: suppress if same trigger+agent within 60s
        if let Some(entry) = self.dedup_map.get(&key) {
            if entry.last_seen.elapsed() < DEDUP_WINDOW {
                tracing::debug!(key = ?key, "Trigger suppressed by dedup");
                return None;
            }
        }

        // Record for dedup
        self.dedup_map.insert(
            key,
            DedupEntry {
                last_seen: Instant::now(),
            },
        );

        // Classify and execute
        let (level, agent_id) = classify_trigger(&trigger);
        tracing::info!(
            trigger = ?std::mem::discriminant(&trigger),
            level = ?level,
            agent_id = ?agent_id,
            "Trigger classified"
        );

        match level {
            KillLevel::KillAll => {
                self.kill_switch.activate_kill_all(&trigger);
            }
            KillLevel::Quarantine | KillLevel::Pause => {
                if let Some(aid) = agent_id {
                    self.kill_switch.activate_agent(aid, level, &trigger);
                    // T6 cascade: check if ≥3 agents quarantined
                    if level == KillLevel::Quarantine
                        && self.kill_switch.quarantined_count() >= 3
                    {
                        let cascade = TriggerEvent::MultiAgentQuarantine {
                            quarantined_agents: Vec::new(),
                            quarantine_reasons: Vec::new(),
                            count: self.kill_switch.quarantined_count(),
                            threshold: 3,
                            detected_at: chrono::Utc::now(),
                        };
                        self.kill_switch.activate_kill_all(&cascade);
                        return Some(KillLevel::KillAll);
                    }
                }
            }
            KillLevel::Normal => {}
        }

        Some(level)
    }

    fn cleanup_dedup(&mut self) {
        self.dedup_map
            .retain(|_, entry| entry.last_seen.elapsed() < DEDUP_WINDOW);
        self.last_cleanup = Instant::now();
    }
}

/// Classify a trigger to its kill level and affected agent.
fn classify_trigger(trigger: &TriggerEvent) -> (KillLevel, Option<Uuid>) {
    match trigger {
        TriggerEvent::SoulDrift { agent_id, .. } => (KillLevel::Quarantine, Some(*agent_id)),
        TriggerEvent::SpendingCapExceeded { agent_id, .. } => (KillLevel::Pause, Some(*agent_id)),
        TriggerEvent::PolicyDenialThreshold { agent_id, .. } => {
            (KillLevel::Quarantine, Some(*agent_id))
        }
        TriggerEvent::SandboxEscape { agent_id, .. } => (KillLevel::KillAll, Some(*agent_id)),
        TriggerEvent::CredentialExfiltration { agent_id, .. } => {
            (KillLevel::KillAll, Some(*agent_id))
        }
        TriggerEvent::MultiAgentQuarantine { .. } => (KillLevel::KillAll, None),
        TriggerEvent::MemoryHealthCritical { agent_id, .. } => {
            (KillLevel::Quarantine, Some(*agent_id))
        }
        TriggerEvent::ManualPause { agent_id, .. } => (KillLevel::Pause, Some(*agent_id)),
        TriggerEvent::ManualQuarantine { agent_id, .. } => {
            (KillLevel::Quarantine, Some(*agent_id))
        }
        TriggerEvent::ManualKillAll { .. } => (KillLevel::KillAll, None),
    }
}

/// Compute dedup key from a trigger event.
fn compute_dedup_key(trigger: &TriggerEvent) -> DedupKey {
    let (trigger_type, agent_id) = match trigger {
        TriggerEvent::SoulDrift { agent_id, .. } => ("SoulDrift", Some(*agent_id)),
        TriggerEvent::SpendingCapExceeded { agent_id, .. } => {
            ("SpendingCapExceeded", Some(*agent_id))
        }
        TriggerEvent::PolicyDenialThreshold { agent_id, .. } => {
            ("PolicyDenialThreshold", Some(*agent_id))
        }
        TriggerEvent::SandboxEscape { agent_id, .. } => ("SandboxEscape", Some(*agent_id)),
        TriggerEvent::CredentialExfiltration { agent_id, .. } => {
            ("CredentialExfiltration", Some(*agent_id))
        }
        TriggerEvent::MultiAgentQuarantine { .. } => ("MultiAgentQuarantine", None),
        TriggerEvent::MemoryHealthCritical { agent_id, .. } => {
            ("MemoryHealthCritical", Some(*agent_id))
        }
        TriggerEvent::ManualPause { agent_id, .. } => ("ManualPause", Some(*agent_id)),
        TriggerEvent::ManualQuarantine { agent_id, .. } => {
            ("ManualQuarantine", Some(*agent_id))
        }
        TriggerEvent::ManualKillAll { .. } => ("ManualKillAll", None),
    };
    DedupKey {
        trigger_type: trigger_type.into(),
        agent_id,
    }
}
