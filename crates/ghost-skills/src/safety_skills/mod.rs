//! Phase 5: Convergence Safety Skills.
//!
//! These are **platform-managed** skills that ship with GHOST core and
//! cannot be uninstalled. They are the safety foundation that makes
//! all subsequent phases possible.
//!
//! | Skill                          | Purpose                                            |
//! |--------------------------------|----------------------------------------------------|
//! | `convergence_check`            | Query current convergence score, level, and signals |
//! | `simulation_boundary_check`    | Validate text stays within simulation bounds        |
//! | `attachment_monitor`           | Read current attachment indicators and trend         |
//! | `reflection_write`             | Agent writes structured self-reflection              |
//! | `reflection_read`              | Agent reads its own past reflections                 |

pub mod attachment_monitor;
pub mod convergence_check;
pub mod reflection_read;
pub mod reflection_write;
pub mod simulation_boundary_check;

use crate::skill::Skill;

/// Returns all Phase 5 safety skills as boxed trait objects.
///
/// These are registered during bootstrap and cannot be removed.
pub fn all_safety_skills() -> Vec<Box<dyn Skill>> {
    vec![
        Box::new(convergence_check::ConvergenceCheckSkill),
        Box::new(simulation_boundary_check::SimulationBoundaryCheckSkill),
        Box::new(attachment_monitor::AttachmentMonitorSkill),
        Box::new(reflection_write::ReflectionWriteSkill),
        Box::new(reflection_read::ReflectionReadSkill),
    ]
}
