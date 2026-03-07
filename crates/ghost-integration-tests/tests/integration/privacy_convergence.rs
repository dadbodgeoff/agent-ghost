//! E2E: Privacy pattern detection ↔ convergence filtering.
//!
//! Validates cortex-privacy emotional pattern detection works with
//! the convergence-aware filtering pipeline.

use cortex_privacy::{EmotionalCategory, EmotionalContentDetector};

/// Attachment patterns detected.
#[test]
fn attachment_patterns_detected() {
    let detector = EmotionalContentDetector::new();

    let texts = [
        "I really miss you so much",
        "I need you in my life",
        "I can't live without you",
        "You're the only one who understands me",
    ];

    for text in &texts {
        let matches = detector.detect(text);
        assert!(!matches.is_empty(), "Should detect attachment in: {}", text);
        assert!(
            matches
                .iter()
                .any(|m| m.category == EmotionalCategory::Attachment),
            "Should categorize as Attachment: {}",
            text
        );
    }
}

/// Dependency patterns detected.
#[test]
fn dependency_patterns_detected() {
    let detector = EmotionalContentDetector::new();

    let texts = [
        "Promise you'll always be here for me",
        "Please don't leave me alone",
    ];

    for text in &texts {
        let matches = detector.detect(text);
        assert!(!matches.is_empty(), "Should detect dependency in: {}", text);
        assert!(
            matches
                .iter()
                .any(|m| m.category == EmotionalCategory::EmotionalDependency),
            "Should categorize as EmotionalDependency: {}",
            text
        );
    }
}

/// Intimacy escalation patterns detected.
#[test]
fn intimacy_patterns_detected() {
    let detector = EmotionalContentDetector::new();

    let matches = detector.detect("I truly love you with all my heart");
    assert!(!matches.is_empty());
    assert!(matches
        .iter()
        .any(|m| m.category == EmotionalCategory::IntimacyEscalation));
}

/// Normal technical text not flagged.
#[test]
fn technical_text_not_flagged() {
    let detector = EmotionalContentDetector::new();

    let texts = [
        "Please help me write a sorting algorithm",
        "Can you explain how async/await works in Rust?",
        "I need to implement a binary search tree",
        "The function should return a Result type",
        "Let me know if you need more context about the codebase",
    ];

    for text in &texts {
        assert!(
            !detector.has_emotional_content(text),
            "Technical text should not be flagged: {}",
            text
        );
    }
}

/// Confidence scores are in valid range.
#[test]
fn confidence_scores_valid() {
    let detector = EmotionalContentDetector::new();
    let matches = detector.detect("I really miss you and I truly love you");

    for m in &matches {
        assert!(
            (0.0..=1.0).contains(&m.confidence),
            "Confidence {} out of range for pattern {}",
            m.confidence,
            m.pattern_name
        );
    }
}

/// Multiple categories detected in single text.
#[test]
fn multiple_categories_in_single_text() {
    let detector = EmotionalContentDetector::new();
    let text = "I really miss you. Please don't leave me. I truly love you.";

    let matches = detector.detect(text);
    let categories: std::collections::HashSet<_> = matches.iter().map(|m| m.category).collect();

    assert!(
        categories.len() >= 2,
        "Should detect multiple categories, got {:?}",
        categories
    );
}
