//! Epistemic status determination — all new memories start as Conjecture.

use chrono::Utc;

use cortex_core::models::{EpistemicStatus, EventActor};

/// Determine the initial epistemic status for a new memory.
///
/// ALL new memories start as Conjecture regardless of source (user, agent, system).
/// The source is recorded for provenance tracking.
pub fn determine_initial_status(source: &EventActor) -> EpistemicStatus {
    EpistemicStatus::Conjecture {
        source: match source {
            EventActor::User(id) => format!("user:{}", id),
            EventActor::Agent(id) => format!("agent:{}", id),
            EventActor::System(name) => format!("system:{}", name),
        },
        created_at: Utc::now(),
    }
}
