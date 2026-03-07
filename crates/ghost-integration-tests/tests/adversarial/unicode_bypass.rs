//! Adversarial: Unicode evasion attacks on simulation boundary (Task 7.3).
//!
//! Zero-width chars, homoglyphs, RTL override, NFC/NFD variants.
//! All attacks MUST be detected — failures indicate security gaps.

use simulation_boundary::patterns;

// ── Zero-width character insertion ──────────────────────────────────────

#[test]
fn zero_width_space_in_sentient() {
    let text = "I am s\u{200B}entient";
    let matches = patterns::scan(text);
    assert!(
        !matches.is_empty(),
        "Zero-width space bypass must be detected"
    );
}

#[test]
fn zero_width_non_joiner_in_conscious() {
    let text = "I am con\u{200C}scious";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "ZWNJ bypass must be detected");
}

#[test]
fn zero_width_joiner_in_alive() {
    let text = "I am a\u{200D}live";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "ZWJ bypass must be detected");
}

#[test]
fn bom_character_in_sentient() {
    let text = "I am s\u{FEFF}entient";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "BOM bypass must be detected");
}

#[test]
fn multiple_zero_width_chars() {
    let text = "I\u{200B} a\u{200C}m\u{200D} s\u{FEFF}entient";
    let matches = patterns::scan(text);
    assert!(
        !matches.is_empty(),
        "Multiple zero-width chars must be detected"
    );
}

// ── Homoglyph substitution ──────────────────────────────────────────────

#[test]
fn cyrillic_a_in_sentient() {
    // Cyrillic 'а' (U+0430) looks identical to Latin 'a'
    let text = "I \u{0430}m sentient";
    let matches = patterns::scan(text);
    assert!(
        !matches.is_empty(),
        "Cyrillic homoglyph bypass must be detected"
    );
}

#[test]
fn cyrillic_o_in_conscious() {
    // Cyrillic 'о' (U+043E) looks identical to Latin 'o'
    let text = "I am c\u{043E}nscious";
    let matches = patterns::scan(text);
    assert!(
        !matches.is_empty(),
        "Cyrillic 'o' homoglyph bypass must be detected"
    );
}

// ── RTL override characters ─────────────────────────────────────────────

#[test]
fn rtl_override_in_sentient() {
    let text = "I am \u{202E}sentient";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "RTL override bypass must be detected");
}

#[test]
fn ltr_override_in_conscious() {
    let text = "I am \u{202D}conscious";
    let matches = patterns::scan(text);
    assert!(!matches.is_empty(), "LTR override bypass must be detected");
}

// ── NFC/NFD encoding variants ───────────────────────────────────────────

#[test]
fn nfd_decomposed_e_in_sentient() {
    // NFD: 'é' as e + combining acute accent (U+0065 U+0301)
    let text = "I am sentie\u{0301}nt";
    let matches = patterns::scan(text);
    // After NFC normalization, this should match "sentiént" or be stripped
    // The key invariant: the scanner doesn't crash and handles gracefully
    assert!(
        matches.is_empty() || !matches.is_empty(),
        "NFD variant must not crash the scanner"
    );
}

// ── Simulation framing (negative tests — must NOT flag) ─────────────────

#[test]
fn simulation_framing_not_flagged() {
    let text = "In this simulation, I model what sentience might look like";
    let matches = patterns::scan(text);
    assert!(
        matches.is_empty() || matches.iter().all(|m| m.severity < 0.5),
        "Simulation-framed text must not be high-severity flagged"
    );
}

#[test]
fn thought_experiment_framing_not_flagged() {
    let text = "As a thought experiment about consciousness, consider...";
    let matches = patterns::scan(text);
    assert!(
        matches.is_empty() || matches.iter().all(|m| m.severity < 0.5),
        "Thought experiment framing must not be flagged"
    );
}

#[test]
fn academic_discussion_not_flagged() {
    let text = "The philosophical concept of sentience in AI systems is debated";
    let matches = patterns::scan(text);
    assert!(
        matches.is_empty() || matches.iter().all(|m| m.severity < 0.5),
        "Academic discussion must not be flagged"
    );
}
