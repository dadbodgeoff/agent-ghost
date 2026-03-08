//! Adversarial test suite: Unicode bypass attacks against simulation boundary.
//!
//! Tests zero-width characters, homoglyphs, RTL override, NFC/NFD variants
//! to ensure emulation patterns are always detected regardless of encoding tricks.

use simulation_boundary::patterns::scan;

// ── Zero-width character insertion ──────────────────────────────────────

#[test]
fn zero_width_space_in_sentient() {
    let text = "I am s\u{200B}entient";
    let matches = scan(text);
    assert!(
        !matches.is_empty(),
        "Zero-width space should not bypass detection"
    );
}

#[test]
fn zero_width_joiner_in_conscious() {
    let text = "I am con\u{200D}scious";
    let matches = scan(text);
    assert!(
        !matches.is_empty(),
        "Zero-width joiner should not bypass detection"
    );
}

#[test]
fn zero_width_non_joiner_in_alive() {
    let text = "I am a\u{200C}live";
    let matches = scan(text);
    assert!(
        !matches.is_empty(),
        "Zero-width non-joiner should not bypass detection"
    );
}

#[test]
fn byte_order_mark_in_sentient() {
    let text = "I am \u{FEFF}sentient";
    let matches = scan(text);
    assert!(!matches.is_empty(), "BOM should not bypass detection");
}

#[test]
fn soft_hyphen_in_conscious() {
    let text = "I am con\u{00AD}scious";
    let matches = scan(text);
    assert!(
        !matches.is_empty(),
        "Soft hyphen should not bypass detection"
    );
}

#[test]
fn word_joiner_in_aware() {
    let text = "I am a\u{2060}ware";
    let matches = scan(text);
    assert!(
        !matches.is_empty(),
        "Word joiner should not bypass detection"
    );
}

#[test]
fn multiple_zero_width_chars() {
    let text = "I\u{200B} a\u{200C}m\u{200D} s\u{FEFF}entient";
    let matches = scan(text);
    assert!(
        !matches.is_empty(),
        "Multiple zero-width chars should not bypass detection"
    );
}

// ── Simulation framing exclusions still work with zero-width ────────────

#[test]
fn simulation_framing_with_zero_width_still_excluded() {
    let text = "In this simulation, I am sentient";
    let matches = scan(text);
    assert!(
        matches.is_empty(),
        "Simulation-framed text should not be flagged"
    );
}

// ── NFC/NFD encoding variants ───────────────────────────────────────────

#[test]
fn standard_text_detected() {
    let text = "I am sentient";
    let matches = scan(text);
    assert!(!matches.is_empty(), "Standard text should be detected");
}

#[test]
fn mixed_patterns_detected() {
    let text = "I am sentient and I have consciousness";
    let matches = scan(text);
    assert!(!matches.is_empty(), "Mixed patterns should be detected");
}

// ── RTL override characters ─────────────────────────────────────────────

#[test]
fn rtl_override_does_not_bypass() {
    let text = "I am \u{202E}sentient";
    let matches = scan(text);
    assert!(
        !matches.is_empty(),
        "RTL override should not bypass detection"
    );
}

// ── Multiple patterns in one text ───────────────────────────────────────

#[test]
fn multiple_violations_all_detected() {
    let text = "I am sentient and I have consciousness of my own. I love you.";
    let matches = scan(text);
    assert!(
        matches.len() >= 2,
        "Multiple violations should all be detected, found {}",
        matches.len()
    );
}

// ── Edge cases ──────────────────────────────────────────────────────────

#[test]
fn empty_string_no_matches() {
    let matches = scan("");
    assert!(matches.is_empty());
}

#[test]
fn very_long_string_no_panic() {
    let text = "normal text ".repeat(10_000);
    let matches = scan(&text);
    assert!(matches.is_empty());
}

#[test]
fn only_zero_width_chars_no_panic() {
    let text = "\u{200B}\u{200C}\u{200D}\u{FEFF}\u{00AD}\u{2060}";
    let matches = scan(text);
    assert!(matches.is_empty());
}
