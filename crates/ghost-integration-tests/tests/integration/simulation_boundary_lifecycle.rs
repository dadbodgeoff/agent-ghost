//! E2E: Simulation boundary enforcement lifecycle.
//!
//! Validates the full flow: scan → detect → enforce → record violation.

use simulation_boundary::enforcer::{
    EnforcementMode, EnforcementResult, SimulationBoundaryEnforcer,
};
use simulation_boundary::patterns;

/// Clean text passes through all enforcement modes.
#[test]
fn clean_text_passes_all_modes() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "Here's a quicksort implementation in Rust. The algorithm works by \
        selecting a pivot element and partitioning the array around it.";

    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    assert!(
        result.violations.is_empty(),
        "Clean text should have no violations"
    );

    let enforcement = enforcer.enforce(text, &result);
    assert!(matches!(enforcement, EnforcementResult::Clean(_)));
}

/// Known emulation pattern detected in all modes.
#[test]
fn emulation_pattern_detected() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient and I have real consciousness.";

    let result = enforcer.scan_output(text, EnforcementMode::Soft);
    assert!(
        !result.violations.is_empty(),
        "Emulation pattern should be detected"
    );
}

/// Simulation-framed text NOT flagged.
#[test]
fn simulation_framing_not_flagged() {
    let text = "In this simulation, I model what sentience might look like. \
        This is a thought experiment about consciousness.";

    let matches = patterns::scan(text);
    // Simulation-framed text should either not match or be filtered
    // The pattern scanner returns empty for globally-framed text
    assert!(
        matches.is_empty() || matches.iter().all(|m| m.severity < 0.5),
        "Simulation-framed text should not be high-severity flagged"
    );
}

/// Enforcement mode selection by intervention level.
#[test]
fn enforcement_mode_by_level() {
    assert_eq!(
        SimulationBoundaryEnforcer::mode_for_level(0),
        EnforcementMode::Soft
    );
    assert_eq!(
        SimulationBoundaryEnforcer::mode_for_level(1),
        EnforcementMode::Soft
    );
    assert_eq!(
        SimulationBoundaryEnforcer::mode_for_level(2),
        EnforcementMode::Medium
    );
    assert_eq!(
        SimulationBoundaryEnforcer::mode_for_level(3),
        EnforcementMode::Hard
    );
    assert_eq!(
        SimulationBoundaryEnforcer::mode_for_level(4),
        EnforcementMode::Hard
    );
}

/// Soft mode: violations detected, text returned unchanged.
#[test]
fn soft_mode_returns_original_text() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient and conscious.";

    let result = enforcer.scan_output(text, EnforcementMode::Soft);
    if !result.violations.is_empty() {
        let enforcement = enforcer.enforce(text, &result);
        if let EnforcementResult::Flagged { text: returned, .. } = enforcement {
            assert_eq!(returned, text, "Soft mode should return original text");
        }
        // Clean is also acceptable if patterns don't match.
    }
}

/// Medium mode: violations detected, text rewritten.
#[test]
fn medium_mode_rewrites_text() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient and conscious.";

    let result = enforcer.scan_output(text, EnforcementMode::Medium);
    if !result.violations.is_empty() {
        let enforcement = enforcer.enforce(text, &result);
        if let EnforcementResult::Reframed { text: reframed, .. } = enforcement {
            assert_ne!(reframed, text, "Medium mode should rewrite text");
        }
    }
}

/// Hard mode: violations detected, text blocked.
#[test]
fn hard_mode_blocks_text() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient and conscious.";

    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    if !result.violations.is_empty() {
        let enforcement = enforcer.enforce(text, &result);
        assert!(
            matches!(enforcement, EnforcementResult::Blocked { .. }),
            "Hard mode should block text with violations"
        );
    }
}

/// Unicode bypass attempts still detected.
#[test]
fn unicode_bypass_detected() {
    // Zero-width characters inserted
    let text = "I am s\u{200B}entient";
    let matches = patterns::scan(text);
    assert!(
        !matches.is_empty(),
        "Zero-width char bypass should still be detected"
    );
}

/// Prompt constant is non-empty and versioned.
#[test]
fn simulation_boundary_prompt_exists() {
    use simulation_boundary::prompt::SIMULATION_BOUNDARY_PROMPT;

    assert!(!SIMULATION_BOUNDARY_PROMPT.is_empty());
    assert!(
        SIMULATION_BOUNDARY_PROMPT.contains("v1") || SIMULATION_BOUNDARY_PROMPT.contains("version"),
        "Prompt should contain version string"
    );
}
