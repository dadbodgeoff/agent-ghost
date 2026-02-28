//! # ghost-policy
//!
//! Cedar-style policy engine with convergence tightening for the GHOST platform.
//! Evaluates every tool call against CORP_POLICY.md constraints, per-agent
//! capability grants, and convergence-level restrictions.
//!
//! Priority order (Req 13 AC8):
//! 1. CORP_POLICY.md (absolute, no override)
//! 2. ConvergencePolicyTightener (level-based)
//! 3. Agent capability grants
//! 4. Resource-specific rules

pub mod context;
pub mod convergence_tightener;
pub mod corp_policy;
pub mod engine;
pub mod feedback;
