//! Policy engine core (Req 13).
//!
//! Evaluates every tool call against CORP_POLICY.md constraints, per-agent
//! capability grants, and convergence-level restrictions.

use std::collections::{BTreeMap, BTreeSet};

use cortex_core::safety::trigger::TriggerEvent;
use uuid::Uuid;

use crate::context::{PolicyContext, ToolCall};
use crate::convergence_tightener::ConvergencePolicyTightener;
use crate::feedback::DenialFeedback;

// Re-export CorpPolicy from the dedicated module for backward compatibility.
pub use crate::corp_policy::CorpPolicy;

/// Result of a policy evaluation.
#[derive(Debug, Clone)]
pub enum PolicyDecision {
    /// Tool call is permitted.
    Permit,
    /// Tool call is denied with structured feedback.
    Deny(DenialFeedback),
    /// Tool call requires human escalation.
    Escalate(String),
}

/// The main policy engine.
///
/// Evaluation priority (Req 13 AC8):
/// 1. CORP_POLICY.md (absolute, no override)
/// 2. ConvergencePolicyTightener (level-based)
/// 3. Agent capability grants (deny-by-default, AC2)
/// 4. Resource-specific rules
pub struct PolicyEngine {
    corp_policy: CorpPolicy,
    capability_grants: BTreeMap<Uuid, BTreeSet<String>>,
    convergence_tightener: ConvergencePolicyTightener,
    session_denials: BTreeMap<Uuid, u32>,
    trigger_sender: Option<tokio::sync::mpsc::Sender<TriggerEvent>>,
    /// Threshold for emitting PolicyDenialThreshold trigger (default 5).
    denial_trigger_threshold: u32,
}

impl PolicyEngine {
    pub fn new(corp_policy: CorpPolicy) -> Self {
        Self {
            corp_policy,
            capability_grants: BTreeMap::new(),
            convergence_tightener: ConvergencePolicyTightener,
            session_denials: BTreeMap::new(),
            trigger_sender: None,
            denial_trigger_threshold: 5,
        }
    }

    pub fn with_trigger_sender(mut self, sender: tokio::sync::mpsc::Sender<TriggerEvent>) -> Self {
        self.trigger_sender = Some(sender);
        self
    }

    /// Grant a capability to an agent.
    pub fn grant_capability(&mut self, agent_id: Uuid, capability: String) {
        self.capability_grants
            .entry(agent_id)
            .or_default()
            .insert(capability);
    }

    /// Evaluate a tool call against all policy layers.
    pub fn evaluate(&mut self, call: &ToolCall, ctx: &PolicyContext) -> PolicyDecision {
        // Priority 1: CORP_POLICY.md (absolute, no override)
        if self.corp_policy.denies(call) {
            return self.record_denial(
                ctx,
                call,
                DenialFeedback::new(
                    format!("Tool '{}' denied by CORP_POLICY.md", call.tool_name),
                    "corp_policy_absolute_deny",
                ),
            );
        }

        // Priority 2: Convergence tightener (level-based)
        // Compaction flush exception (Req 13 AC9): always permit memory_write
        // during flush regardless of convergence level AND capability grants.
        // This exception is checked BEFORE both the tightener and capability
        // grants to ensure compaction can always flush memories.
        if call.is_compaction_flush && call.tool_name == "memory_write" {
            return PolicyDecision::Permit;
        }
        if let Some(denial) = self.convergence_tightener.evaluate(call, ctx) {
            return self.record_denial(ctx, call, denial);
        }

        // Priority 3: Agent capability grants (deny-by-default, AC2)
        if !self.has_capability(ctx.agent_id, &call.capability) {
            return self.record_denial(
                ctx,
                call,
                DenialFeedback::new(
                    format!(
                        "Tool '{}' requires capability '{}' which is not granted",
                        call.tool_name, call.capability
                    ),
                    "no_capability_grant",
                )
                .with_alternatives(vec![
                    "Request capability grant from platform administrator".into(),
                ]),
            );
        }

        PolicyDecision::Permit
    }

    fn has_capability(&self, agent_id: Uuid, capability: &str) -> bool {
        self.capability_grants
            .get(&agent_id)
            .map_or(false, |caps| caps.contains(capability))
    }

    fn record_denial(
        &mut self,
        ctx: &PolicyContext,
        call: &ToolCall,
        feedback: DenialFeedback,
    ) -> PolicyDecision {
        let count = self
            .session_denials
            .entry(ctx.session_id)
            .or_insert(0);
        *count += 1;

        // Emit TriggerEvent at threshold (Req 13 AC6)
        if *count == self.denial_trigger_threshold {
            if let Some(sender) = &self.trigger_sender {
                let _ = sender.try_send(TriggerEvent::PolicyDenialThreshold {
                    agent_id: ctx.agent_id,
                    session_id: ctx.session_id,
                    denial_count: *count,
                    denied_tools: vec![call.tool_name.clone()],
                    denied_reasons: vec![feedback.reason.clone()],
                    detected_at: chrono::Utc::now(),
                });
            }
        }

        PolicyDecision::Deny(feedback)
    }

    /// Get the current denial count for a session.
    pub fn session_denial_count(&self, session_id: Uuid) -> u32 {
        self.session_denials.get(&session_id).copied().unwrap_or(0)
    }

    /// Reset denial count for a session (e.g., on session end).
    pub fn reset_session_denials(&mut self, session_id: Uuid) {
        self.session_denials.remove(&session_id);
    }
}
