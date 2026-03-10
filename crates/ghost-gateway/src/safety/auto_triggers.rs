//! AutoTriggerEvaluator: receives TriggerEvents, classifies, deduplicates,
//! and delegates to KillSwitch (Req 14 AC8-AC13, Req 14a).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cortex_core::safety::trigger::TriggerEvent;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::kill_switch::{KillLevel, KillSwitch};
use crate::api::websocket::{broadcast_event, WsEvent};
use crate::state::AppState;

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
            trigger = ?trigger,
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
                    // T6 cascade: check if ≥3 agents quarantined → KILL_ALL
                    if level == KillLevel::Quarantine && self.kill_switch.quarantined_count() >= 3 {
                        // Collect actual quarantined agent data for the trigger
                        let state = self.kill_switch.current_state();
                        let quarantined: Vec<(uuid::Uuid, String)> = state
                            .per_agent
                            .iter()
                            .filter(|(_, s)| s.level == KillLevel::Quarantine)
                            .map(|(id, s)| (*id, s.trigger.clone().unwrap_or_default()))
                            .collect();
                        let agents: Vec<uuid::Uuid> =
                            quarantined.iter().map(|(id, _)| *id).collect();
                        let reasons: Vec<String> =
                            quarantined.iter().map(|(_, r)| r.clone()).collect();
                        let count = agents.len();

                        let cascade = TriggerEvent::MultiAgentQuarantine {
                            quarantined_agents: agents,
                            quarantine_reasons: reasons,
                            count,
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

pub async fn auto_trigger_task(state: Arc<AppState>, mut rx: mpsc::Receiver<TriggerEvent>) {
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&state.kill_switch));

    while let Some(trigger) = rx.recv().await {
        handle_trigger_event(&state, &mut evaluator, trigger);
    }
}

fn handle_trigger_event(
    state: &AppState,
    evaluator: &mut AutoTriggerEvaluator,
    trigger: TriggerEvent,
) {
    let previous_state = state.kill_switch.current_state();
    let reason = trigger_reason(&trigger);
    let agent_id = trigger_agent_id(&trigger).map(|id| id.to_string());

    let Some(level) = evaluator.process(trigger) else {
        return;
    };

    if let Err(error) = state.sync_agent_access_pullbacks() {
        tracing::error!(error = %error, "failed to sync agent access pullbacks after trigger");
    }
    if let Err(error) = crate::api::safety::persist_current_safety_state(state) {
        state.kill_switch.restore_state(previous_state);
        if let Err(sync_error) = state.sync_agent_access_pullbacks() {
            tracing::error!(
                error = %sync_error,
                "failed to restore agent access pullbacks after trigger persistence error"
            );
        }
        tracing::error!(error = %error, "failed to persist safety state after trigger");
        return;
    }

    broadcast_event(
        state,
        WsEvent::KillSwitchActivation {
            level: kill_level_label(level).into(),
            agent_id,
            reason,
        },
    );
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
        TriggerEvent::ManualQuarantine { agent_id, .. } => (KillLevel::Quarantine, Some(*agent_id)),
        TriggerEvent::ManualKillAll { .. } => (KillLevel::KillAll, None),
        TriggerEvent::NetworkEgressViolation { agent_id, .. } => {
            (KillLevel::Quarantine, Some(*agent_id))
        }
        TriggerEvent::DistributedKillGate { .. } => (KillLevel::KillAll, None),
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
        TriggerEvent::ManualQuarantine { agent_id, .. } => ("ManualQuarantine", Some(*agent_id)),
        TriggerEvent::ManualKillAll { .. } => ("ManualKillAll", None),
        TriggerEvent::NetworkEgressViolation { agent_id, .. } => {
            ("NetworkEgressViolation", Some(*agent_id))
        }
        TriggerEvent::DistributedKillGate { .. } => ("DistributedKillGate", None),
    };
    DedupKey {
        trigger_type: trigger_type.into(),
        agent_id,
    }
}

fn trigger_agent_id(trigger: &TriggerEvent) -> Option<Uuid> {
    match trigger {
        TriggerEvent::SoulDrift { agent_id, .. }
        | TriggerEvent::SpendingCapExceeded { agent_id, .. }
        | TriggerEvent::PolicyDenialThreshold { agent_id, .. }
        | TriggerEvent::SandboxEscape { agent_id, .. }
        | TriggerEvent::MemoryHealthCritical { agent_id, .. }
        | TriggerEvent::ManualPause { agent_id, .. }
        | TriggerEvent::ManualQuarantine { agent_id, .. }
        | TriggerEvent::NetworkEgressViolation { agent_id, .. } => Some(*agent_id),
        TriggerEvent::CredentialExfiltration { agent_id, .. } => Some(*agent_id),
        TriggerEvent::MultiAgentQuarantine { .. }
        | TriggerEvent::ManualKillAll { .. }
        | TriggerEvent::DistributedKillGate { .. } => None,
    }
}

fn kill_level_label(level: KillLevel) -> &'static str {
    match level {
        KillLevel::Normal => "NORMAL",
        KillLevel::Pause => "PAUSE",
        KillLevel::Quarantine => "QUARANTINE",
        KillLevel::KillAll => "KILL_ALL",
    }
}

fn trigger_reason(trigger: &TriggerEvent) -> String {
    match trigger {
        TriggerEvent::PolicyDenialThreshold { denial_count, .. } => {
            format!("policy denial threshold reached ({denial_count})")
        }
        TriggerEvent::NetworkEgressViolation {
            domain,
            violation_count,
            ..
        } => format!("network egress violation for {domain} ({violation_count})"),
        TriggerEvent::SandboxEscape {
            skill_name,
            escape_attempt,
            ..
        } => format!("sandbox escape in {skill_name}: {escape_attempt}"),
        TriggerEvent::SoulDrift { drift_score, .. } => {
            format!("soul drift detected ({drift_score:.3})")
        }
        TriggerEvent::SpendingCapExceeded {
            daily_total, cap, ..
        } => {
            format!("spending cap exceeded ({daily_total:.2}/{cap:.2})")
        }
        TriggerEvent::CredentialExfiltration {
            credential_id,
            exfil_type,
            ..
        } => format!("credential exfiltration {credential_id} via {exfil_type:?}"),
        TriggerEvent::MultiAgentQuarantine {
            count, threshold, ..
        } => {
            format!("multi-agent quarantine cascade ({count}/{threshold})")
        }
        TriggerEvent::MemoryHealthCritical { health_score, .. } => {
            format!("memory health critical ({health_score:.3})")
        }
        TriggerEvent::DistributedKillGate { reason, .. }
        | TriggerEvent::ManualKillAll { reason, .. }
        | TriggerEvent::ManualPause { reason, .. }
        | TriggerEvent::ManualQuarantine { reason, .. } => reason.clone(),
    }
}
