//! Epistemic status model — tracks the verification state of memories.
//!
//! Epistemic status is orthogonal to confidence. A memory can have high
//! confidence (0.9) but be a Conjecture (no one verified it). A memory can
//! have moderate confidence (0.6) but be Verified (multiple people confirmed it).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The epistemic status of a memory — its verification lifecycle state.
///
/// Valid promotion paths (strictly enforced):
/// - Conjecture → Provisional → Verified (only forward, no skipping)
/// - Verified → Stale (only degradation path, via evidence decay)
/// - Conjecture → Verified: REJECTED
/// - Verified → Provisional: REJECTED
/// - Provisional → Stale: REJECTED
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EpistemicStatus {
    /// Initial state — no verification has occurred.
    Conjecture {
        /// Source that created this memory (e.g. "user:alice", "agent:coder").
        source: String,
        /// When the memory was created.
        created_at: DateTime<Utc>,
    },
    /// Has some supporting evidence but not yet fully verified.
    Provisional {
        /// Number of supporting evidence items.
        evidence_count: u32,
        /// When last validated.
        last_validated: DateTime<Utc>,
    },
    /// Confirmed by one or more verifiers with evidence references.
    Verified {
        /// Who verified this memory.
        verified_by: Vec<String>,
        /// When verification occurred.
        verified_at: DateTime<Utc>,
        /// References to supporting evidence.
        evidence_refs: Vec<String>,
    },
    /// Was previously Verified but evidence has become stale.
    Stale {
        /// When it was last verified before going stale.
        was_verified_at: DateTime<Utc>,
        /// When staleness was detected.
        staleness_detected_at: DateTime<Utc>,
        /// Why it became stale.
        reason: String,
    },
}

impl EpistemicStatus {
    /// Returns the variant name as a string (for error messages).
    pub fn variant_name(&self) -> &'static str {
        match self {
            EpistemicStatus::Conjecture { .. } => "conjecture",
            EpistemicStatus::Provisional { .. } => "provisional",
            EpistemicStatus::Verified { .. } => "verified",
            EpistemicStatus::Stale { .. } => "stale",
        }
    }
}

/// Strategy for aggregating confidence from multiple evidence sources.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    /// Weighted average (mean) — existing default approach.
    #[default]
    WeightedAverage,
    /// Gödel t-norm (min operator) — conservative, from TS11 (FPF paper).
    /// A single weak evidence (0.3) drags aggregate to 0.3 regardless of
    /// how many strong sources exist.
    GodelTNorm,
}
