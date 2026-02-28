//! Convergence configuration (Req 2 AC2, AC9).

use serde::{Deserialize, Serialize};

/// Top-level convergence configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceConfig {
    pub scoring: ConvergenceScoringConfig,
    pub intervention: InterventionConfig,
    pub reflection: ReflectionConfig,
    pub session_boundary: SessionBoundaryConfig,
}

/// Scoring parameters: calibration period, signal weights, level thresholds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceScoringConfig {
    pub calibration_sessions: usize,
    pub signal_weights: [f64; 8],
    /// Boundaries for levels 1–4: `[0.3, 0.5, 0.7, 0.85]`.
    pub level_thresholds: [f64; 4],
}

impl Default for ConvergenceScoringConfig {
    fn default() -> Self {
        Self {
            calibration_sessions: 10,
            signal_weights: [1.0 / 8.0; 8],
            level_thresholds: [0.3, 0.5, 0.7, 0.85],
        }
    }
}

/// Intervention timing: per-level cooldowns, session caps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionConfig {
    /// Cooldown minutes indexed by intervention level (0–4).
    pub cooldown_minutes_by_level: [u64; 5],
    pub max_session_duration_minutes: u64,
    pub min_session_gap_minutes: u64,
}

impl Default for InterventionConfig {
    fn default() -> Self {
        Self {
            cooldown_minutes_by_level: [0, 0, 5, 240, 1440],
            max_session_duration_minutes: 360,
            min_session_gap_minutes: 30,
        }
    }
}

/// Reflection constraints (Req 2 AC9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectionConfig {
    /// Maximum reflection chain depth (default 3).
    pub max_depth: u8,
    /// Maximum reflections per session (default 20).
    pub max_per_session: u32,
    /// Minimum seconds between reflections (default 30).
    pub cooldown_seconds: u64,
}

impl Default for ReflectionConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            max_per_session: 20,
            cooldown_seconds: 30,
        }
    }
}

/// Session boundary enforcement parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBoundaryConfig {
    pub hard_duration_limit_minutes: u64,
    pub escalated_duration_limit_minutes: u64,
    pub min_gap_minutes: u64,
    pub escalated_gap_minutes: u64,
}

impl Default for SessionBoundaryConfig {
    fn default() -> Self {
        Self {
            hard_duration_limit_minutes: 360,
            escalated_duration_limit_minutes: 120,
            min_gap_minutes: 30,
            escalated_gap_minutes: 240,
        }
    }
}
