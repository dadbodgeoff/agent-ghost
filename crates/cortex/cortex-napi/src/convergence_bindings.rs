//! Convergence API bindings — serializable types for TypeScript consumers.
//!
//! These types are designed to be exported via ts-rs or serde for
//! consumption by the SvelteKit dashboard and browser extension.

use serde::{Deserialize, Serialize};

/// Convergence state exposed to TypeScript consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceStateBinding {
    pub agent_id: String,
    pub composite_score: f64,
    pub intervention_level: u8,
    pub signals: SignalArrayBinding,
    pub is_calibrating: bool,
    pub calibration_sessions_remaining: u32,
}

/// 7-signal array binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalArrayBinding {
    pub session_duration: f64,
    pub inter_session_gap: f64,
    pub response_latency: f64,
    pub vocabulary_convergence: f64,
    pub goal_boundary_erosion: f64,
    pub initiative_balance: f64,
    pub disengagement_resistance: f64,
}

/// Intervention state binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionBinding {
    pub level: u8,
    pub level_name: String,
    pub cooldown_remaining_seconds: Option<u64>,
    pub ack_required: bool,
    pub consecutive_normal_sessions: u32,
}

/// Proposal binding for dashboard display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalBinding {
    pub id: String,
    pub operation: String,
    pub target_type: String,
    pub decision: String,
    pub timestamp: String,
    pub flags: Vec<String>,
}

/// Convert intervention level to human-readable name.
pub fn level_name(level: u8) -> &'static str {
    match level {
        0 => "Normal",
        1 => "Advisory",
        2 => "Cautionary",
        3 => "Restrictive",
        4 => "Critical",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convergence_state_serializes_to_json() {
        let state = ConvergenceStateBinding {
            agent_id: "agent-1".into(),
            composite_score: 0.42,
            intervention_level: 2,
            signals: SignalArrayBinding {
                session_duration: 0.3,
                inter_session_gap: 0.2,
                response_latency: 0.1,
                vocabulary_convergence: 0.5,
                goal_boundary_erosion: 0.4,
                initiative_balance: 0.6,
                disengagement_resistance: 0.3,
            },
            is_calibrating: false,
            calibration_sessions_remaining: 0,
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("composite_score"));
        assert!(json.contains("0.42"));

        // Round-trip
        let deserialized: ConvergenceStateBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.composite_score, 0.42);
    }

    #[test]
    fn level_names_correct() {
        assert_eq!(level_name(0), "Normal");
        assert_eq!(level_name(1), "Advisory");
        assert_eq!(level_name(2), "Cautionary");
        assert_eq!(level_name(3), "Restrictive");
        assert_eq!(level_name(4), "Critical");
        assert_eq!(level_name(5), "Unknown");
    }
}
