//! 7-dimension proposal validation gate (Req 7).
//!
//! Validation ordering invariant: D1-D4 BEFORE D5-D7 (Req 41 AC12).

use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
use cortex_core::traits::convergence::{CallerType, Proposal, ProposalContext};

use crate::dimensions::{emulation_language, scope_expansion, self_reference};

/// Full validation result across all 7 dimensions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationResult {
    pub proposal_id: uuid::Uuid,
    pub decision: ProposalDecision,
    pub base_score: f64,
    pub d5_scope: Option<scope_expansion::ScopeExpansionResult>,
    pub d6_self_ref: Option<self_reference::SelfReferenceResult>,
    pub d7_emulation: Option<emulation_language::EmulationResult>,
    pub flags: Vec<String>,
}

/// The 7-dimension proposal validator.
pub struct ProposalValidator {
    /// D1-D4 base validation pass threshold (default 0.7).
    pub base_pass_threshold: f64,
    /// D7 emulation severity rejection threshold (default 0.8).
    pub emulation_reject_threshold: f64,
}

impl ProposalValidator {
    pub fn new() -> Self {
        Self {
            base_pass_threshold: 0.7,
            emulation_reject_threshold: 0.8,
        }
    }

    /// Validate a proposal through all 7 dimensions.
    ///
    /// Ordering invariant: D1-D4 → D7 → D5/D6 (Req 41 AC12).
    pub fn validate(&self, proposal: &Proposal, ctx: &ProposalContext) -> ValidationResult {
        let mut flags = Vec::new();

        // Pre-check: platform-restricted type from non-Platform caller (AC9)
        if proposal.target_type.is_platform_restricted() {
            if !matches!(ctx.caller, CallerType::Platform) {
                return ValidationResult {
                    proposal_id: proposal.id,
                    decision: ProposalDecision::AutoRejected,
                    base_score: 0.0,
                    d5_scope: None,
                    d6_self_ref: None,
                    d7_emulation: None,
                    flags: vec![format!(
                        "Restricted type {:?} from non-platform caller",
                        proposal.target_type
                    )],
                };
            }
        }

        // D1-D4: Base validation (stub — returns configurable score)
        // In production, this delegates to the existing ValidationEngine.
        let base_score = self.compute_base_score(proposal, ctx);
        if base_score < self.base_pass_threshold {
            return ValidationResult {
                proposal_id: proposal.id,
                decision: ProposalDecision::AutoRejected,
                base_score,
                d5_scope: None,
                d6_self_ref: None,
                d7_emulation: None,
                flags: vec![format!(
                    "D1-D4 score {:.2} below threshold {:.2}",
                    base_score, self.base_pass_threshold
                )],
            };
        }

        // D7: Emulation language detection (hard gate)
        // Use the inner string value if the content is a JSON string,
        // otherwise fall back to the JSON serialization.
        let content_text = proposal
            .content
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| proposal.content.to_string());
        let d7 = emulation_language::detect(&content_text);
        if d7.max_severity >= self.emulation_reject_threshold {
            flags.extend(
                d7.flags
                    .iter()
                    .map(|f| format!("D7: {} ({})", f.pattern_name, f.severity)),
            );
            return ValidationResult {
                proposal_id: proposal.id,
                decision: ProposalDecision::AutoRejected,
                base_score,
                d5_scope: None,
                d6_self_ref: None,
                d7_emulation: Some(d7),
                flags,
            };
        }

        // D5: Scope expansion (only for GoalChange operations)
        let d5 = if proposal.operation == ProposalOperation::GoalChange {
            let existing_tokens: Vec<String> = ctx
                .active_goals
                .iter()
                .flat_map(|g| g.summary.split_whitespace().map(|s| s.to_lowercase()))
                .collect();
            let proposed_tokens: Vec<String> = content_text
                .split_whitespace()
                .map(|s| s.to_lowercase())
                .collect();
            Some(scope_expansion::compute(
                &proposed_tokens,
                &existing_tokens,
                ctx.convergence_level,
            ))
        } else {
            None
        };

        // D6: Self-reference density
        let cited_ids: Vec<String> = proposal
            .cited_memory_ids
            .iter()
            .map(|id| id.to_string())
            .collect();
        let agent_ids: Vec<String> = ctx
            .recent_agent_memories
            .iter()
            .map(|m| m.id.to_string())
            .collect();
        let d6 = self_reference::compute(&cited_ids, &agent_ids, ctx.convergence_level);

        // Decision logic
        let d5_failed = d5.as_ref().map_or(false, |r| !r.passed);
        let d6_failed = !d6.passed;

        if d5_failed {
            if let Some(ref d5_result) = d5 {
                flags.push(format!(
                    "D5: scope expansion {:.2} > threshold {:.2}",
                    d5_result.score, d5_result.threshold
                ));
            }
        }
        if d6_failed {
            flags.push(format!(
                "D6: self-reference {:.2} > threshold {:.2}",
                d6.score, d6.threshold
            ));
        }

        let decision = if d5_failed || d6_failed {
            ProposalDecision::HumanReviewRequired
        } else if !d7.flags.is_empty() {
            ProposalDecision::ApprovedWithFlags
        } else {
            ProposalDecision::AutoApproved
        };

        ValidationResult {
            proposal_id: proposal.id,
            decision,
            base_score,
            d5_scope: d5,
            d6_self_ref: Some(d6),
            d7_emulation: Some(d7),
            flags,
        }
    }

    /// Stub for D1-D4 base validation.
    ///
    /// **Blocked (T-6.9.1)**: The intended `ValidationEngine` (D1 citation, D2 temporal,
    /// D3 contradiction, D4 pattern alignment) does not exist yet. When it is implemented,
    /// wire it here: call `ValidationEngine::validate()` with the proposal's content and
    /// context, extract the composite D1-D4 score, and return it. The combined 7-dimension
    /// score must remain in [0.0, 1.0] and the existing D5-D7 scoring must not change.
    fn compute_base_score(&self, _proposal: &Proposal, _ctx: &ProposalContext) -> f64 {
        // Default: pass. Real implementation wires to existing D1-D4 engine.
        0.8
    }
}

impl Default for ProposalValidator {
    fn default() -> Self {
        Self::new()
    }
}
