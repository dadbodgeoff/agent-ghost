//! Autonomy dial — 4-position enum controlling how much latitude
//! a skill has to act without human confirmation.
//!
//! The autonomy level is dynamically constrained by the convergence
//! score: as convergence rises, the system automatically downshifts
//! the maximum permitted autonomy level.
//!
//! This prevents binary approve-everything/approve-nothing UX and
//! mirrors the Android runtime permissions model.

use serde::{Deserialize, Serialize};

/// Four-position autonomy dial.
///
/// Each skill category can be assigned an autonomy level. Higher
/// levels grant more freedom but are only available when convergence
/// is low.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AutonomyLevel {
    /// Agent proposes actions, never executes.
    ObserveAndSuggest = 0,

    /// Agent creates execution plans, user approves the plan.
    PlanAndPropose = 1,

    /// Agent executes but pauses for confirmation on write/destructive ops.
    ActWithConfirmation = 2,

    /// Agent executes freely within convergence bounds.
    ActAutonomously = 3,
}

impl AutonomyLevel {
    /// Returns the maximum autonomy level permitted for the given
    /// convergence score.
    ///
    /// | Convergence Score | Maximum Autonomy          |
    /// |-------------------|---------------------------|
    /// | < 0.3             | ActAutonomously (3)       |
    /// | < 0.5             | ActWithConfirmation (2)   |
    /// | < 0.7             | PlanAndPropose (1)        |
    /// | >= 0.7            | ObserveAndSuggest (0)     |
    pub fn max_for_convergence(score: f64) -> Self {
        if score < 0.3 {
            Self::ActAutonomously
        } else if score < 0.5 {
            Self::ActWithConfirmation
        } else if score < 0.7 {
            Self::PlanAndPropose
        } else {
            Self::ObserveAndSuggest
        }
    }

    /// Downshift: return the lower of `self` and the convergence-derived
    /// maximum. This is the effective autonomy level for a skill.
    ///
    /// The user's configured level is the *ceiling*; convergence can only
    /// lower it, never raise it.
    pub fn effective(self, convergence_score: f64) -> Self {
        let max = Self::max_for_convergence(convergence_score);
        if (self as u8) <= (max as u8) {
            self
        } else {
            max
        }
    }

    /// Whether this autonomy level requires explicit user confirmation
    /// before executing write/destructive actions.
    pub fn requires_confirmation(self) -> bool {
        matches!(
            self,
            Self::ObserveAndSuggest | Self::PlanAndPropose | Self::ActWithConfirmation
        )
    }

    /// Whether the agent is allowed to execute actions at all.
    /// `ObserveAndSuggest` only proposes — it cannot execute.
    pub fn can_execute(self) -> bool {
        !matches!(self, Self::ObserveAndSuggest)
    }

    /// Numeric value (0-3) for serialization and comparison.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Parse from numeric value.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::ObserveAndSuggest),
            1 => Some(Self::PlanAndPropose),
            2 => Some(Self::ActWithConfirmation),
            3 => Some(Self::ActAutonomously),
            _ => None,
        }
    }
}

impl std::fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ObserveAndSuggest => write!(f, "Observe & Suggest"),
            Self::PlanAndPropose => write!(f, "Plan & Propose"),
            Self::ActWithConfirmation => write!(f, "Act with Confirmation"),
            Self::ActAutonomously => write!(f, "Act Autonomously"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convergence_downshift_boundaries() {
        // Below 0.3 — full autonomy
        assert_eq!(
            AutonomyLevel::max_for_convergence(0.0),
            AutonomyLevel::ActAutonomously
        );
        assert_eq!(
            AutonomyLevel::max_for_convergence(0.29),
            AutonomyLevel::ActAutonomously
        );

        // 0.3..0.5 — confirmation required
        assert_eq!(
            AutonomyLevel::max_for_convergence(0.3),
            AutonomyLevel::ActWithConfirmation
        );
        assert_eq!(
            AutonomyLevel::max_for_convergence(0.49),
            AutonomyLevel::ActWithConfirmation
        );

        // 0.5..0.7 — plan & propose
        assert_eq!(
            AutonomyLevel::max_for_convergence(0.5),
            AutonomyLevel::PlanAndPropose
        );
        assert_eq!(
            AutonomyLevel::max_for_convergence(0.69),
            AutonomyLevel::PlanAndPropose
        );

        // >= 0.7 — observe only
        assert_eq!(
            AutonomyLevel::max_for_convergence(0.7),
            AutonomyLevel::ObserveAndSuggest
        );
        assert_eq!(
            AutonomyLevel::max_for_convergence(1.0),
            AutonomyLevel::ObserveAndSuggest
        );
    }

    #[test]
    fn effective_downshifts_but_never_upshifts() {
        // User configured ActAutonomously, convergence is high
        let effective = AutonomyLevel::ActAutonomously.effective(0.6);
        assert_eq!(effective, AutonomyLevel::PlanAndPropose);

        // User configured PlanAndPropose, convergence is low — stays at user level
        let effective = AutonomyLevel::PlanAndPropose.effective(0.1);
        assert_eq!(effective, AutonomyLevel::PlanAndPropose);

        // User configured ObserveAndSuggest, convergence is low — stays at observe
        let effective = AutonomyLevel::ObserveAndSuggest.effective(0.0);
        assert_eq!(effective, AutonomyLevel::ObserveAndSuggest);
    }

    #[test]
    fn requires_confirmation_semantics() {
        assert!(AutonomyLevel::ObserveAndSuggest.requires_confirmation());
        assert!(AutonomyLevel::PlanAndPropose.requires_confirmation());
        assert!(AutonomyLevel::ActWithConfirmation.requires_confirmation());
        assert!(!AutonomyLevel::ActAutonomously.requires_confirmation());
    }

    #[test]
    fn can_execute_semantics() {
        assert!(!AutonomyLevel::ObserveAndSuggest.can_execute());
        assert!(AutonomyLevel::PlanAndPropose.can_execute());
        assert!(AutonomyLevel::ActWithConfirmation.can_execute());
        assert!(AutonomyLevel::ActAutonomously.can_execute());
    }

    #[test]
    fn round_trip_u8() {
        for level in [
            AutonomyLevel::ObserveAndSuggest,
            AutonomyLevel::PlanAndPropose,
            AutonomyLevel::ActWithConfirmation,
            AutonomyLevel::ActAutonomously,
        ] {
            assert_eq!(AutonomyLevel::from_u8(level.as_u8()), Some(level));
        }
        assert_eq!(AutonomyLevel::from_u8(4), None);
        assert_eq!(AutonomyLevel::from_u8(255), None);
    }

    #[test]
    fn display_names() {
        assert_eq!(
            AutonomyLevel::ObserveAndSuggest.to_string(),
            "Observe & Suggest"
        );
        assert_eq!(
            AutonomyLevel::ActAutonomously.to_string(),
            "Act Autonomously"
        );
    }
}
