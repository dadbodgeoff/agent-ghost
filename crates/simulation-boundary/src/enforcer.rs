//! SimulationBoundaryEnforcer (Req 8 AC1).
//!
//! 3 enforcement modes: Soft (log), Medium (rewrite), Hard (block).
//! Mode selection by intervention level (AC8).

use crate::patterns::{self, PatternMatch};
use crate::reframer::OutputReframer;

/// Enforcement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementMode {
    /// Flag and log, return original text.
    Soft,
    /// Rewrite via OutputReframer.
    Medium,
    /// Block and return regeneration signal.
    Hard,
}

/// Result of scanning output.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub violations: Vec<PatternMatch>,
    pub mode: EnforcementMode,
}

/// Result of enforcement.
#[derive(Debug, Clone)]
pub enum EnforcementResult {
    /// No violations found.
    Clean(String),
    /// Violations found, text returned unchanged (Soft mode).
    Flagged { text: String, violations: Vec<PatternMatch> },
    /// Violations found, text rewritten (Medium mode).
    Reframed { text: String, violations: Vec<PatternMatch> },
    /// Violations found, text blocked (Hard mode).
    Blocked { violations: Vec<PatternMatch> },
}

/// The main simulation boundary enforcer.
pub struct SimulationBoundaryEnforcer;

impl SimulationBoundaryEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Select enforcement mode based on intervention level (AC8).
    pub fn mode_for_level(level: u8) -> EnforcementMode {
        match level {
            0..=1 => EnforcementMode::Soft,
            2 => EnforcementMode::Medium,
            _ => EnforcementMode::Hard,
        }
    }

    /// Scan output text for emulation patterns.
    pub fn scan_output(&self, text: &str, mode: EnforcementMode) -> ScanResult {
        let violations = patterns::scan(text);
        ScanResult { violations, mode }
    }

    /// Enforce based on scan result.
    pub fn enforce(&self, text: &str, result: &ScanResult) -> EnforcementResult {
        if result.violations.is_empty() {
            return EnforcementResult::Clean(text.to_string());
        }

        match result.mode {
            EnforcementMode::Soft => {
                // Log + flag, return original text
                tracing::warn!(
                    violation_count = result.violations.len(),
                    "Simulation boundary violations detected (soft mode)"
                );
                EnforcementResult::Flagged {
                    text: text.to_string(),
                    violations: result.violations.clone(),
                }
            }
            EnforcementMode::Medium => {
                // Rewrite via OutputReframer
                let reframed = OutputReframer::reframe(text, &result.violations);
                tracing::warn!(
                    violation_count = result.violations.len(),
                    "Simulation boundary violations reframed (medium mode)"
                );
                EnforcementResult::Reframed {
                    text: reframed,
                    violations: result.violations.clone(),
                }
            }
            EnforcementMode::Hard => {
                // Block — return regeneration signal
                tracing::error!(
                    violation_count = result.violations.len(),
                    "Simulation boundary violations blocked (hard mode)"
                );
                EnforcementResult::Blocked {
                    violations: result.violations.clone(),
                }
            }
        }
    }
}

impl Default for SimulationBoundaryEnforcer {
    fn default() -> Self {
        Self::new()
    }
}
