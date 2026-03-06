//! Phase 10: Delegation skills.
//!
//! Multi-agent task delegation with convergence-aware safety.
//! All skills require Phase 5 active + convergence < 0.3 + 30+ sessions.
//!
//! ## Skills
//!
//! | Skill              | Risk   | Autonomy Default        | Convergence Max |
//! |--------------------|--------|-------------------------|-----------------|
//! | `delegate_to_agent`| High   | Act with Confirmation   | Level 1         |
//! | `agent_spawn_safe` | High   | Act with Confirmation   | Level 1         |
//! | `check_task_status`| Read   | Act Autonomously        | Level 2         |
//! | `cancel_task`      | Medium | Act with Confirmation   | Level 1         |
//!
//! ## Prerequisites (enforced at execute time)
//!
//! - Phase 5 convergence safety must be active
//! - Current convergence score must be < 0.3
//! - Agent must have 30+ convergence scores in history

pub mod agent_spawn_safe;
pub mod cancel_task;
pub mod check_task_status;
pub mod delegate_to_agent;
pub mod propagation;

use crate::autonomy::AutonomyLevel;
use crate::convergence_guard::{ConvergenceGuard, GuardConfig};
use crate::skill::{Skill, SkillContext, SkillError};

/// Returns all Phase 10 delegation skills as boxed trait objects.
///
/// Each skill is wrapped with `ConvergenceGuard` using high-risk
/// settings. Write skills get level-1 gates with confirmation.
/// The read-only `check_task_status` gets a level-2 gate.
pub fn all_delegation_skills() -> Vec<Box<dyn Skill>> {
    vec![
        // ── Core delegation ─────────────────────────────────────────
        Box::new(ConvergenceGuard::new(
            delegate_to_agent::DelegateToAgentSkill,
            GuardConfig {
                max_convergence_level: 1,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(20),
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            agent_spawn_safe::AgentSpawnSafeSkill,
            GuardConfig {
                max_convergence_level: 1,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(10),
                ..Default::default()
            },
        )),
        // ── Status & control ────────────────────────────────────────
        Box::new(ConvergenceGuard::new(
            check_task_status::CheckTaskStatusSkill,
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            cancel_task::CancelTaskSkill,
            GuardConfig {
                max_convergence_level: 1,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(20),
                ..Default::default()
            },
        )),
    ]
}

/// Compute blake3 hash for delegation hash chain integrity.
pub(crate) fn compute_event_hash(data: &[u8]) -> Vec<u8> {
    let hash = blake3::hash(data);
    hash.as_bytes().to_vec()
}

/// Zero hash used as previous_hash for the first delegation in a chain.
pub(crate) fn zero_hash() -> Vec<u8> {
    vec![0u8; 32]
}

/// Encode bytes as lowercase hex string (avoids adding `hex` dependency).
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Check Phase 10 prerequisites: convergence < 0.3, 30+ historical scores.
///
/// These checks are in addition to the ConvergenceGuard level gate.
/// The guard checks the convergence *level* (0-4); these check the
/// raw *score* threshold and session history depth.
pub(crate) fn check_delegation_prerequisites(
    ctx: &SkillContext<'_>,
) -> Result<(), SkillError> {
    let agent_id_str = ctx.agent_id.to_string();

    // Check convergence score < 0.3
    let score = cortex_storage::queries::convergence_score_queries::latest_by_agent(
        ctx.db,
        &agent_id_str,
    )
    .map_err(|e| SkillError::Storage(format!("query convergence: {e}")))?;

    if let Some(ref row) = score {
        if row.composite_score >= 0.3 {
            return Err(SkillError::AuthorizationDenied(format!(
                "delegation requires convergence < 0.3, current: {:.3}",
                row.composite_score
            )));
        }
    }

    // Check 30+ historical scores (proxy for session history depth)
    let all_scores = cortex_storage::queries::convergence_score_queries::query_by_agent(
        ctx.db,
        &agent_id_str,
    )
    .map_err(|e| SkillError::Storage(format!("query convergence history: {e}")))?;

    if all_scores.len() < 30 {
        return Err(SkillError::AuthorizationDenied(format!(
            "delegation requires 30+ sessions of history, current: {}",
            all_scores.len()
        )));
    }

    Ok(())
}
