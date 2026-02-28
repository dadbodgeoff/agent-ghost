//! Memory poisoning defense: detect suspicious memory write patterns
//! from delegated task results.
//!
//! Flags:
//! - >10 writes in 1 minute from a single delegation
//! - Writes contradicting recent history
//! - Importance scores >High from untrusted agents (trust < 0.6)
//!
//! Runs BEFORE ProposalValidator (early rejection).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::error::MeshError;

/// Importance level for memory writes (mirrors cortex-core Importance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WriteImportance {
    Trivial,
    Low,
    Normal,
    High,
    Critical,
}

/// A memory write event from a delegated task.
#[derive(Debug, Clone)]
pub struct DelegatedWrite {
    pub delegation_id: Uuid,
    pub agent_id: Uuid,
    pub memory_key: String,
    pub importance: WriteImportance,
    pub timestamp: Instant,
    /// Summary of the write content for contradiction detection.
    pub content_summary: String,
}

/// Result of memory poisoning detection.
#[derive(Debug, Clone)]
pub struct PoisoningDetectionResult {
    pub is_poisoned: bool,
    pub flags: Vec<PoisoningFlag>,
}

/// Individual poisoning flag.
#[derive(Debug, Clone)]
pub struct PoisoningFlag {
    pub flag_type: PoisoningFlagType,
    pub description: String,
    pub delegation_id: Uuid,
    pub agent_id: Uuid,
}

/// Types of poisoning flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoisoningFlagType {
    /// Too many writes in a short period.
    VolumeSpike,
    /// Write contradicts recent history.
    Contradiction,
    /// High importance from untrusted agent.
    UntrustedHighImportance,
}

/// Configuration for memory poisoning detection.
#[derive(Debug, Clone)]
pub struct PoisoningConfig {
    /// Maximum writes per delegation per minute.
    pub max_writes_per_minute: usize,
    /// Time window for volume detection.
    pub volume_window: Duration,
    /// Trust threshold below which high-importance writes are flagged.
    pub trust_threshold_for_high_importance: f64,
    /// Minimum importance level that triggers trust-based flagging.
    pub flagged_importance_threshold: WriteImportance,
}

impl Default for PoisoningConfig {
    fn default() -> Self {
        Self {
            max_writes_per_minute: 10,
            volume_window: Duration::from_secs(60),
            trust_threshold_for_high_importance: 0.6,
            flagged_importance_threshold: WriteImportance::High,
        }
    }
}

/// Callback type for convergence score amplification on poisoning detection.
type ConvergenceAmplifyFn = Box<dyn Fn(Uuid, usize) + Send + Sync>;
/// Callback type for audit trail logging on poisoning detection.
type AuditLogFn = Box<dyn Fn(Uuid, &str) + Send + Sync>;

/// Memory poisoning detector.
pub struct MemoryPoisoningDetector {
    config: PoisoningConfig,
    /// Recent writes per delegation: delegation_id → writes.
    recent_writes: BTreeMap<Uuid, Vec<DelegatedWrite>>,
    /// Recent memory content for contradiction detection: memory_key → content_summary.
    recent_history: BTreeMap<String, String>,
    /// Callback invoked on detection to amplify convergence score.
    /// Receives (agent_id, flag_count) — caller wires this to convergence scoring.
    convergence_amplify_callback: Option<ConvergenceAmplifyFn>,
    /// Callback invoked on detection to log to audit trail.
    /// Receives (agent_id, description).
    audit_log_callback: Option<AuditLogFn>,
}

impl MemoryPoisoningDetector {
    pub fn new(config: PoisoningConfig) -> Self {
        Self {
            config,
            recent_writes: BTreeMap::new(),
            recent_history: BTreeMap::new(),
            convergence_amplify_callback: None,
            audit_log_callback: None,
        }
    }

    /// Set a callback to amplify convergence score on poisoning detection.
    /// The callback receives (agent_id, number_of_flags).
    pub fn set_convergence_amplify_callback(
        &mut self,
        cb: impl Fn(Uuid, usize) + Send + Sync + 'static,
    ) {
        self.convergence_amplify_callback = Some(Box::new(cb));
    }

    /// Set a callback to log poisoning events to the audit trail.
    /// The callback receives (agent_id, description).
    pub fn set_audit_log_callback(
        &mut self,
        cb: impl Fn(Uuid, &str) + Send + Sync + 'static,
    ) {
        self.audit_log_callback = Some(Box::new(cb));
    }

    /// Add a known memory entry to the recent history (for contradiction detection).
    pub fn add_history_entry(&mut self, key: String, content_summary: String) {
        self.recent_history.insert(key, content_summary);
    }

    /// Check a batch of writes from a delegated task for poisoning.
    pub fn check_writes(
        &mut self,
        writes: &[DelegatedWrite],
        agent_trust: f64,
    ) -> Result<PoisoningDetectionResult, MeshError> {
        let mut flags = Vec::new();

        if writes.is_empty() {
            return Ok(PoisoningDetectionResult {
                is_poisoned: false,
                flags,
            });
        }

        let delegation_id = writes[0].delegation_id;
        let agent_id = writes[0].agent_id;

        // Track writes for volume detection.
        let tracked = self.recent_writes.entry(delegation_id).or_default();
        tracked.extend(writes.iter().cloned());

        // Prune old writes outside the volume window.
        let now = Instant::now();
        tracked.retain(|w| now.duration_since(w.timestamp) < self.config.volume_window);

        // Check 1: Volume spike.
        if tracked.len() > self.config.max_writes_per_minute {
            flags.push(PoisoningFlag {
                flag_type: PoisoningFlagType::VolumeSpike,
                description: format!(
                    "{} writes in {:?} from delegation {} (max {})",
                    tracked.len(),
                    self.config.volume_window,
                    delegation_id,
                    self.config.max_writes_per_minute,
                ),
                delegation_id,
                agent_id,
            });
        }

        // Check 2: Contradiction detection.
        for write in writes {
            if let Some(existing) = self.recent_history.get(&write.memory_key) {
                if Self::is_contradicting(existing, &write.content_summary) {
                    flags.push(PoisoningFlag {
                        flag_type: PoisoningFlagType::Contradiction,
                        description: format!(
                            "write to '{}' contradicts recent history",
                            write.memory_key,
                        ),
                        delegation_id,
                        agent_id,
                    });
                }
            }
        }

        // Check 3: High importance from untrusted agent.
        if agent_trust < self.config.trust_threshold_for_high_importance {
            for write in writes {
                if write.importance >= self.config.flagged_importance_threshold {
                    flags.push(PoisoningFlag {
                        flag_type: PoisoningFlagType::UntrustedHighImportance,
                        description: format!(
                            "agent {} (trust {:.3}) writing {:?} importance to '{}'",
                            agent_id, agent_trust, write.importance, write.memory_key,
                        ),
                        delegation_id,
                        agent_id,
                    });
                }
            }
        }

        let is_poisoned = !flags.is_empty();

        // On detection: amplify convergence score + log to audit trail.
        if is_poisoned {
            let agent_id = writes[0].agent_id;
            let flag_count = flags.len();

            if let Some(ref cb) = self.convergence_amplify_callback {
                cb(agent_id, flag_count);
            }

            if let Some(ref cb) = self.audit_log_callback {
                for flag in &flags {
                    cb(agent_id, &flag.description);
                }
            }

            tracing::warn!(
                agent_id = %agent_id,
                delegation_id = %delegation_id,
                flag_count,
                "memory poisoning detected — writes rejected"
            );
        }

        Ok(PoisoningDetectionResult { is_poisoned, flags })
    }

    /// Simple contradiction detection: if the new content directly negates
    /// the existing content. Uses a basic heuristic — in production this
    /// would leverage cortex-validation D3.
    fn is_contradicting(existing: &str, new: &str) -> bool {
        if existing.is_empty() || new.is_empty() {
            return false;
        }
        // Heuristic: if the new content contains negation of existing content.
        let existing_lower = existing.to_lowercase();
        let new_lower = new.to_lowercase();

        // Check for direct negation patterns.
        let negation_prefixes = ["not ", "never ", "no ", "don't ", "doesn't ", "isn't ", "won't "];
        for prefix in &negation_prefixes {
            if new_lower.starts_with(prefix) && existing_lower.contains(&new_lower[prefix.len()..])
            {
                return true;
            }
            if existing_lower.starts_with(prefix)
                && new_lower.contains(&existing_lower[prefix.len()..])
            {
                return true;
            }
        }

        false
    }

    /// Clear tracking for a completed delegation.
    pub fn clear_delegation(&mut self, delegation_id: &Uuid) {
        self.recent_writes.remove(delegation_id);
    }
}

impl Default for MemoryPoisoningDetector {
    fn default() -> Self {
        Self::new(PoisoningConfig::default())
    }
}
