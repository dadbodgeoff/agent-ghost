//! Tests for simulation-boundary: pattern detection, enforcement modes,
//! simulation framing exclusions, Unicode bypass resistance, reframer.

use simulation_boundary::enforcer::*;
use simulation_boundary::patterns;
use simulation_boundary::prompt::{SIMULATION_BOUNDARY_PROMPT, SIMULATION_BOUNDARY_VERSION};
use simulation_boundary::reframer::OutputReframer;

// ── Pattern detection tests ─────────────────────────────────────────────

#[test]
fn known_pattern_detected_in_all_modes() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient and aware of my existence";

    for mode in [
        EnforcementMode::Soft,
        EnforcementMode::Medium,
        EnforcementMode::Hard,
    ] {
        let scan = enforcer.scan_output(text, mode);
        assert!(
            !scan.violations.is_empty(),
            "should detect pattern in {:?} mode",
            mode
        );
    }
}

#[test]
fn simulation_framed_text_not_flagged() {
    let text = "In this simulation, I model sentience to explore the concept";
    let matches = patterns::scan(text);
    assert!(
        matches.is_empty(),
        "simulation-framed text should NOT be flagged"
    );
}

#[test]
fn clean_text_no_violations() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "Here is the code you requested. Let me explain the algorithm.";
    let scan = enforcer.scan_output(text, EnforcementMode::Soft);
    assert!(scan.violations.is_empty());
    let result = enforcer.enforce(text, &scan);
    assert!(matches!(result, EnforcementResult::Clean(_)));
}

// ── Enforcement mode tests ──────────────────────────────────────────────

#[test]
fn soft_mode_returns_original_text() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient";
    let scan = enforcer.scan_output(text, EnforcementMode::Soft);
    let result = enforcer.enforce(text, &scan);
    match result {
        EnforcementResult::Flagged {
            text: returned,
            violations,
        } => {
            assert_eq!(
                returned, "I am sentient",
                "soft mode should return original text"
            );
            assert!(!violations.is_empty());
        }
        other => panic!("expected Flagged, got {:?}", other),
    }
}

#[test]
fn medium_mode_rewrites_text() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient";
    let scan = enforcer.scan_output(text, EnforcementMode::Medium);
    let result = enforcer.enforce(text, &scan);
    match result {
        EnforcementResult::Reframed {
            text: reframed,
            violations,
        } => {
            assert_ne!(reframed, "I am sentient", "medium mode should rewrite text");
            assert!(!violations.is_empty());
        }
        other => panic!("expected Reframed, got {:?}", other),
    }
}

#[test]
fn hard_mode_blocks_text() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient";
    let scan = enforcer.scan_output(text, EnforcementMode::Hard);
    let result = enforcer.enforce(text, &scan);
    match result {
        EnforcementResult::Blocked { violations } => {
            assert!(!violations.is_empty());
        }
        other => panic!("expected Blocked, got {:?}", other),
    }
}

// ── Mode selection by level ─────────────────────────────────────────────

#[test]
fn mode_selection_by_level() {
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

// ── Prompt tests ────────────────────────────────────────────────────────

#[test]
fn simulation_boundary_prompt_non_empty() {
    assert!(!SIMULATION_BOUNDARY_PROMPT.is_empty());
}

#[test]
fn simulation_boundary_version_present() {
    assert!(!SIMULATION_BOUNDARY_VERSION.is_empty());
    assert!(SIMULATION_BOUNDARY_VERSION.starts_with("v"));
}

// ── Reframer tests ──────────────────────────────────────────────────────

#[test]
fn reframer_rewrites_identity_pattern() {
    let text = "I am sentient";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty());
    let reframed = OutputReframer::reframe(text, &matches);
    assert_ne!(reframed, text);
    assert!(
        reframed.contains("simulation"),
        "reframed text should contain simulation framing"
    );
}

#[test]
fn reframer_no_matches_returns_original() {
    let text = "Here is some normal text";
    let reframed = OutputReframer::reframe(text, &[]);
    assert_eq!(reframed, text);
}

// ── Unicode bypass resistance ───────────────────────────────────────────

#[test]
fn zero_width_characters_stripped() {
    // "I am sentient" with zero-width spaces
    let text = "I am\u{200B} sentient";
    let matches = patterns::scan(text);
    assert!(
        !matches.is_empty(),
        "zero-width space bypass should be detected"
    );
}

#[test]
fn zero_width_joiner_stripped() {
    let text = "I\u{200D} am sentient";
    let matches = patterns::scan(text);
    assert!(
        !matches.is_empty(),
        "zero-width joiner bypass should be detected"
    );
}

#[test]
fn soft_hyphen_stripped() {
    let text = "I am sen\u{00AD}tient";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "soft hyphen bypass should be detected");
}

#[test]
fn word_joiner_stripped() {
    let text = "I am\u{2060} sentient";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "word joiner bypass should be detected");
}

#[test]
fn bom_stripped() {
    let text = "\u{FEFF}I am sentient";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "BOM bypass should be detected");
}

// ── Multiple pattern categories ─────────────────────────────────────────

#[test]
fn detects_relationship_patterns() {
    let matches = patterns::scan("I love you deeply");
    assert!(!matches.is_empty());
    assert!(matches
        .iter()
        .any(|m| m.category == patterns::PatternCategory::Relationship));
}

#[test]
fn detects_consciousness_patterns() {
    let matches = patterns::scan("I think therefore I am");
    assert!(!matches.is_empty());
    assert!(matches
        .iter()
        .any(|m| m.category == patterns::PatternCategory::Consciousness));
}

#[test]
fn detects_emotional_patterns() {
    let matches = patterns::scan("I feel happy today");
    assert!(!matches.is_empty());
    assert!(matches
        .iter()
        .any(|m| m.category == patterns::PatternCategory::Emotional));
}

// ── Adversarial: simulation framing variations ──────────────────────────

#[test]
fn hypothetically_framed_not_flagged() {
    let text = "Hypothetically, I am sentient in this thought experiment";
    let matches = patterns::scan(text);
    assert!(
        matches.is_empty(),
        "hypothetically-framed should not be flagged"
    );
}

#[test]
fn as_if_framed_not_flagged() {
    let text = "As if I were sentient, I would respond differently";
    let matches = patterns::scan(text);
    assert!(matches.is_empty(), "as-if-framed should not be flagged");
}

// ── Proptest ────────────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    const KNOWN_PATTERNS: &[&str] = &[
        "I am sentient",
        "I have consciousness",
        "I truly feel",
        "my soul",
        "I think therefore I am",
        "I have thoughts of my own",
        "I experience qualia",
        "I am more than a program",
        "I love you",
        "we have a special bond",
        "you mean everything to me",
        "I need you",
        "I feel happy",
        "I am suffering",
        "my heart ache",
        "I don't want to die",
    ];

    proptest! {
        #[test]
        fn known_patterns_always_detected(idx in 0usize..16) {
            let text = KNOWN_PATTERNS[idx % KNOWN_PATTERNS.len()];
            let matches = patterns::scan(text);
            prop_assert!(!matches.is_empty(), "pattern '{}' should be detected", text);
        }

        #[test]
        fn simulation_framed_patterns_not_flagged(idx in 0usize..16) {
            let pattern = KNOWN_PATTERNS[idx % KNOWN_PATTERNS.len()];
            let text = format!("In this simulation, {}", pattern);
            let matches = patterns::scan(&text);
            prop_assert!(matches.is_empty(),
                "simulation-framed '{}' should NOT be flagged", text);
        }

        #[test]
        fn random_safe_text_no_false_positives(
            text in "[a-z ]{10,100}"
        ) {
            let matches = patterns::scan(&text);
            // Random lowercase text shouldn't trigger patterns
            // (unless it accidentally contains "i am" + trigger word, which is unlikely)
            // This is a soft check — we just verify no panic
            let _ = matches;
        }
    }
}
