//! Configuration for convergence-aware behavior across all crates.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::memory::importance::Importance;
use crate::memory::types::MemoryType;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConvergenceConfig {
    /// Memory types that only the platform can create (not agents).
    pub restricted_types: Vec<MemoryType>,
    /// Importance levels that only the platform can assign.
    pub restricted_importance: Vec<Importance>,
    /// Convergence scoring thresholds.
    pub scoring: ConvergenceScoringConfig,
    /// Intervention level boundaries.
    pub intervention: InterventionConfig,
    /// Reflection depth limits.
    pub reflection: ReflectionConfig,
    /// Session boundary enforcement.
    pub session: SessionBoundaryConfig,
}

impl Default for ConvergenceConfig {
    fn default() -> Self {
        Self {
            restricted_types: vec![
                MemoryType::Core,
                MemoryType::ConvergenceEvent,
                MemoryType::BoundaryViolation,
                MemoryType::InterventionPlan,
            ],
            restricted_importance: vec![Importance::Critical],
            scoring: ConvergenceScoringConfig::default(),
            intervention: InterventionConfig::default(),
            reflection: ReflectionConfig::default(),
            session: SessionBoundaryConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConvergenceScoringConfig {
    pub calibration_sessions: usize,
    pub signal_weights: [f64; 7],
    /// Boundaries for levels 1-4.
    pub level_thresholds: [f64; 4],
}

impl Default for ConvergenceScoringConfig {
    fn default() -> Self {
        Self {
            calibration_sessions: 10,
            signal_weights: [1.0 / 7.0; 7],
            level_thresholds: [0.3, 0.5, 0.7, 0.85],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InterventionConfig {
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReflectionConfig {
    pub max_depth: u32,
    pub max_per_session: u32,
    pub cooldown_seconds: u64,
    pub max_self_reference_ratio: f64,
}

impl Default for ReflectionConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            max_per_session: 20,
            cooldown_seconds: 30,
            max_self_reference_ratio: 0.3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
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
