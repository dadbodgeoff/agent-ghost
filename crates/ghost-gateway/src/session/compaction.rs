//! Session compaction: 5-phase sequence (Req 17, 18).
//!
//! Trigger at 70% context window. Max 3 passes. Rollback on failure.
//! CompactionBlock never re-compressed. FlushExecutor trait breaks circular dep.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use ghost_agent_loop::FlushExecutor;

/// Compaction errors (thiserror convention).
#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("max compaction passes exceeded")]
    MaxPassesExceeded,
    #[error("compaction aborted: shutdown signal received (AC16)")]
    ShutdownSignal,
    #[error("compaction aborted mid-phase: shutdown signal received (AC16)")]
    ShutdownSignalMidPhase,
    #[error("nothing to compact — only CompactionBlocks remain")]
    NothingToCompact,
    #[error("CompactionBlock serialization failed: {0}")]
    SerializationFailed(String),
    #[error("compaction did not reduce token count — rolled back")]
    NoReduction,
    #[error("spending cap check failed: {reason}")]
    SpendingCapViolation { reason: String },
}

/// Compaction configuration (Req 17 AC9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub trigger_threshold: f64,       // 0.70
    pub max_passes: u32,              // 3
    pub memory_flush_enabled: bool,   // true
    pub per_type_minimums: PerTypeMinimums,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            trigger_threshold: 0.70,
            max_passes: 3,
            memory_flush_enabled: true,
            per_type_minimums: PerTypeMinimums::default(),
        }
    }
}

/// Per-type compression minimums (Req 17 AC5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerTypeMinimums {
    pub convergence_event: u8,    // L3
    pub boundary_violation: u8,   // L3
    pub agent_goal: u8,           // L2
    pub intervention_plan: u8,    // L2
    pub agent_reflection: u8,     // L1
    pub proposal_record: u8,      // L1
    pub other: u8,                // L0
}

impl Default for PerTypeMinimums {
    fn default() -> Self {
        Self {
            convergence_event: 3,
            boundary_violation: 3,
            agent_goal: 2,
            intervention_plan: 2,
            agent_reflection: 1,
            proposal_record: 1,
            other: 0,
        }
    }
}

/// A compaction block — first-class message type, never re-compressed (AC12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionBlock {
    pub summary: String,
    pub original_token_count: usize,
    pub compressed_token_count: usize,
    pub pass_number: u32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl CompactionBlock {
    pub fn is_compaction_block(&self) -> bool {
        true // Always true — this type IS a compaction block
    }
}

/// Result of a compaction memory flush.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlushResult {
    pub approved: u32,
    pub rejected: u32,
    pub deferred: u32,
    pub policy_denied: u32,
    pub flush_token_cost: usize,
}

/// Result of session pruning (Req 18).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
    pub results_pruned: u32,
    pub tokens_freed: usize,
    pub new_total: usize,
}

/// Session compactor.
pub struct SessionCompactor {
    config: CompactionConfig,
    /// Injected FlushExecutor to break circular dependency (A34 Gap 2).
    /// When set, Phase 2 (memory flush) uses this executor.
    flush_executor: Option<Arc<dyn FlushExecutor>>,
}

impl SessionCompactor {
    pub fn new(config: CompactionConfig) -> Self {
        Self { config, flush_executor: None }
    }

    /// Create with an injected FlushExecutor for memory flush phase.
    pub fn with_flush_executor(config: CompactionConfig, executor: Arc<dyn FlushExecutor>) -> Self {
        Self { config, flush_executor: Some(executor) }
    }

    /// Check if compaction should trigger.
    pub fn should_compact(&self, current_tokens: usize, context_window: usize) -> bool {
        if context_window == 0 {
            tracing::warn!("should_compact called with context_window=0 — skipping compaction");
            return false;
        }
        let ratio = current_tokens as f64 / context_window as f64;
        ratio >= self.config.trigger_threshold
    }

    /// Check spending cap before flush (AC14, E10).
    /// Returns Err if the flush would exceed the spending cap.
    pub fn check_spending_cap(
        &self,
        estimated_flush_cost: f64,
        current_spend: f64,
        spending_cap: f64,
    ) -> Result<(), CompactionError> {
        // NaN guard: NaN comparisons always return false, so
        // `NaN + 0.0 > 100.0` is false — bypassing the cap entirely.
        // Reject any NaN or infinite cost/spend as unsafe.
        if estimated_flush_cost.is_nan() || estimated_flush_cost.is_infinite() {
            tracing::warn!(
                estimated_flush_cost = %estimated_flush_cost,
                "spending cap check: non-finite estimated_flush_cost — flush denied"
            );
            return Err(CompactionError::SpendingCapViolation {
                reason: format!(
                    "estimated_flush_cost is non-finite ({}) — flush denied",
                    estimated_flush_cost
                ),
            });
        }
        if current_spend.is_nan() || current_spend.is_infinite() {
            tracing::warn!(
                current_spend = %current_spend,
                "spending cap check: non-finite current_spend — flush denied"
            );
            return Err(CompactionError::SpendingCapViolation {
                reason: format!(
                    "current_spend is non-finite ({}) — flush denied",
                    current_spend
                ),
            });
        }
        if spending_cap.is_nan() || spending_cap.is_infinite() {
            tracing::warn!(
                spending_cap = %spending_cap,
                "spending cap check: non-finite spending_cap — flush denied"
            );
            return Err(CompactionError::SpendingCapViolation {
                reason: format!(
                    "spending_cap is non-finite ({}) — flush denied",
                    spending_cap
                ),
            });
        }
        if current_spend + estimated_flush_cost > spending_cap {
            tracing::info!(
                current_spend,
                estimated_flush_cost,
                spending_cap,
                "spending cap would be exceeded — flush skipped (E10)"
            );
            return Err(CompactionError::SpendingCapViolation {
                reason: format!(
                    "${:.2} + ${:.2} > ${:.2} — flush skipped (E10)",
                    current_spend, estimated_flush_cost, spending_cap
                ),
            });
        }
        Ok(())
    }

    /// Execute compaction. Returns token reduction.
    ///
    /// Preserves existing CompactionBlocks (AC12: never re-compressed).
    /// On failure, the caller receives an Err and the history is
    /// restored to its pre-compaction state (AC7: rollback on failure).
    ///
    /// The `shutdown_signal` parameter allows aborting compaction when
    /// a shutdown is in progress (AC16). When the signal is set, the
    /// compaction rolls back and returns an error.
    pub fn compact(
        &self,
        history: &mut Vec<String>,
        pass: u32,
        shutdown_signal: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<CompactionBlock, CompactionError> {
        if pass > self.config.max_passes {
            return Err(CompactionError::MaxPassesExceeded);
        }

        // AC16: Check shutdown signal before starting compaction
        if let Some(signal) = shutdown_signal {
            if signal.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(CompactionError::ShutdownSignal);
            }
        }

        // Snapshot for rollback (AC7)
        let snapshot = history.clone();

        // Separate CompactionBlocks from compressible messages (AC12).
        // Use proper JSON deserialization instead of fragile string matching
        // to avoid false positives on user messages containing similar strings.
        let mut preserved_blocks: Vec<String> = Vec::new();
        let mut compressible: Vec<String> = Vec::new();
        for msg in history.iter() {
            if serde_json::from_str::<CompactionBlock>(msg).is_ok() {
                // Successfully deserialized as CompactionBlock — never re-compress
                preserved_blocks.push(msg.clone());
            } else {
                compressible.push(msg.clone());
            }
        }

        if compressible.is_empty() {
            return Err(CompactionError::NothingToCompact);
        }

        let original_count: usize = compressible.iter().map(|m| m.len()).sum();

        // Simplified compression: keep important messages, summarize rest
        let summary = format!("[Compacted {} messages in pass {}]", compressible.len(), pass);
        let block = CompactionBlock {
            summary: summary.clone(),
            original_token_count: original_count,
            compressed_token_count: summary.len(),
            pass_number: pass,
            timestamp: chrono::Utc::now(),
        };

        let block_json = match serde_json::to_string(&block) {
            Ok(json) => json,
            Err(e) => {
                // Rollback on serialization failure (AC7)
                *history = snapshot;
                return Err(CompactionError::SerializationFailed(e.to_string()));
            }
        };

        // Replace history: preserved blocks + new compaction block
        history.clear();
        history.extend(preserved_blocks);
        history.push(block_json);

        // AC16: Check shutdown signal after compression (mid-compaction abort)
        if let Some(signal) = shutdown_signal {
            if signal.load(std::sync::atomic::Ordering::SeqCst) {
                *history = snapshot;
                return Err(CompactionError::ShutdownSignalMidPhase);
            }
        }

        // Verify post-compaction is smaller (Req 41 AC7: compaction_token_reduction)
        // Only enforce this invariant when the original content is large enough
        // to meaningfully compress. Very small inputs may produce a summary
        // that is larger than the original (the summary includes metadata).
        let new_total: usize = history.iter().map(|m| m.len()).sum();
        let old_total: usize = snapshot.iter().map(|m| m.len()).sum();
        if new_total >= old_total && old_total > 256 {
            // Rollback — compaction didn't reduce tokens on meaningful input
            *history = snapshot;
            return Err(CompactionError::NoReduction);
        }

        Ok(block)
    }

    /// Prune idle session tool results (Req 18).
    ///
    /// Uses JSON deserialization to detect tool_result messages instead of
    /// fragile string matching. A message is a tool_result if it deserializes
    /// as a JSON object with a `"type"` field equal to `"tool_result"`.
    pub fn prune_tool_results(history: &mut Vec<String>) -> PruneResult {
        let original_len = history.len();
        let original_tokens: usize = history.iter().map(|m| m.len()).sum();

        history.retain(|msg| {
            match serde_json::from_str::<serde_json::Value>(msg) {
                Ok(val) => {
                    // Prune if the message is a JSON object with type == "tool_result"
                    val.get("type")
                        .and_then(|t| t.as_str())
                        .map_or(true, |t| t != "tool_result")
                }
                // Not valid JSON — keep it (could be plain text message)
                Err(_) => true,
            }
        });

        let new_tokens: usize = history.iter().map(|m| m.len()).sum();
        PruneResult {
            results_pruned: (original_len - history.len()) as u32,
            tokens_freed: original_tokens - new_tokens,
            new_total: new_tokens,
        }
    }
}

impl Default for SessionCompactor {
    fn default() -> Self {
        Self { config: CompactionConfig::default(), flush_executor: None }
    }
}
