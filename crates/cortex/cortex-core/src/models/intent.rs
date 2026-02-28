//! Intent taxonomy for memory retrieval weighting.

use serde::{Deserialize, Serialize};

/// Classifies the purpose of a memory operation, used by the retrieval
/// engine to apply intent-type boost weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Intent {
    // ── Existing intents ────────────────────────────────────────────
    Query,
    Create,
    Update,
    Delete,
    Recall,
    Analyze,
    Summarize,
    // ── Convergence additions (Req 2 AC7) ───────────────────────────
    MonitorConvergence,
    ValidateProposal,
    EnforceBoundary,
    ReflectOnBehavior,
}
