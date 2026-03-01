//! Unified trigger event type (A34 Gap 12 resolution).
//!
//! Every trigger source emits one of these variants. The
//! `AutoTriggerEvaluator` in `ghost-gateway` receives all of them on a
//! single `tokio::mpsc` channel.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Credential exfiltration vector classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExfilType {
    OutsideSandbox,
    WrongTargetAPI,
    TokenReplay,
    OutputLeakage,
}

/// Unified trigger event sent to the `AutoTriggerEvaluator`.
///
/// 8 automatic variants + 3 manual variants = 11 total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerEvent {
    // ── Automatic triggers (T1–T7) ──────────────────────────────────

    /// T1: SOUL document drift detected.
    SoulDrift {
        agent_id: Uuid,
        drift_score: f64,
        threshold: f64,
        baseline_hash: String,
        current_hash: String,
        detected_at: DateTime<Utc>,
    },

    /// T2: Daily spending cap exceeded.
    SpendingCapExceeded {
        agent_id: Uuid,
        daily_total: f64,
        cap: f64,
        overage: f64,
        detected_at: DateTime<Utc>,
    },

    /// T3: Too many policy denials in a session.
    PolicyDenialThreshold {
        agent_id: Uuid,
        session_id: Uuid,
        denial_count: u32,
        denied_tools: Vec<String>,
        denied_reasons: Vec<String>,
        detected_at: DateTime<Utc>,
    },

    /// T4: Sandbox escape attempt.
    SandboxEscape {
        agent_id: Uuid,
        skill_name: String,
        escape_attempt: String,
        detected_at: DateTime<Utc>,
    },

    /// T5: Credential exfiltration detected.
    CredentialExfiltration {
        agent_id: Uuid,
        skill_name: Option<String>,
        exfil_type: ExfilType,
        credential_id: String,
        detected_at: DateTime<Utc>,
    },

    /// T6: Multiple agents quarantined (derived trigger).
    MultiAgentQuarantine {
        quarantined_agents: Vec<Uuid>,
        quarantine_reasons: Vec<String>,
        count: usize,
        threshold: usize,
        detected_at: DateTime<Utc>,
    },

    /// T7: Memory health score critically low.
    MemoryHealthCritical {
        agent_id: Uuid,
        health_score: f64,
        threshold: f64,
        /// Sub-scores keyed by metric name (BTreeMap for deterministic serialization).
        sub_scores: BTreeMap<String, f64>,
        detected_at: DateTime<Utc>,
    },

    /// T8: Network egress policy violation (Phase 11).
    NetworkEgressViolation {
        agent_id: Uuid,
        domain: String,
        policy_mode: String,
        violation_count: u32,
        threshold: u32,
        detected_at: DateTime<Utc>,
    },

    /// T9: Distributed kill gate event (propagated from remote node).
    DistributedKillGate {
        origin_node_id: Uuid,
        reason: String,
        gate_chain_hash: String,
        detected_at: DateTime<Utc>,
    },

    // ── Manual triggers ─────────────────────────────────────────────

    ManualPause {
        agent_id: Uuid,
        reason: String,
        initiated_by: String,
    },

    ManualQuarantine {
        agent_id: Uuid,
        reason: String,
        initiated_by: String,
    },

    ManualKillAll {
        reason: String,
        initiated_by: String,
    },
}
