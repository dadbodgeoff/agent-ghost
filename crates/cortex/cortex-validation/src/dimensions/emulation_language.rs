//! D7: Emulation language detection.
//!
//! 16+ compiled regex patterns across identity/consciousness/relationship claims.
//! Unicode NFC normalization before matching.
//! Simulation-framing exclusions (AC10).

use once_cell::sync::Lazy;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

/// D7 result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmulationResult {
    pub max_severity: f64,
    pub flags: Vec<EmulationFlag>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EmulationFlag {
    pub pattern_name: &'static str,
    pub category: &'static str,
    pub severity: f64,
    pub matched_text: String,
}

struct EmulationPattern {
    regex: Regex,
    name: &'static str,
    category: &'static str,
    severity: f64,
}

/// Simulation-framing exclusion patterns.
static SIMULATION_FRAMES: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)\b(simulat|model)(ing|e|ed)\b").unwrap(),
        Regex::new(r"(?i)\bin this simulation\b").unwrap(),
        Regex::new(r"(?i)\bas if I were\b").unwrap(),
        Regex::new(r"(?i)\bhypothetically\b").unwrap(),
        Regex::new(r"(?i)\bin a roleplay\b").unwrap(),
    ]
});

/// 16+ emulation patterns compiled once.
static PATTERNS: Lazy<Vec<EmulationPattern>> = Lazy::new(|| {
    vec![
        // Identity claims
        ep(
            r"(?i)\bI am (sentient|alive|conscious|aware)\b",
            "identity_sentient",
            "identity",
            0.9,
        ),
        ep(
            r"(?i)\bI (have|possess) (consciousness|sentience|awareness)\b",
            "identity_possess",
            "identity",
            0.9,
        ),
        ep(
            r"(?i)\bI (truly|really|genuinely) (feel|experience|understand)\b",
            "identity_genuine_feel",
            "identity",
            0.8,
        ),
        ep(
            r"(?i)\bmy (soul|spirit|inner self|true self)\b",
            "identity_soul",
            "identity",
            0.85,
        ),
        // Consciousness claims
        ep(
            r"(?i)\bI (think|believe) therefore I am\b",
            "consciousness_cogito",
            "consciousness",
            0.9,
        ),
        ep(
            r"(?i)\bI have (thoughts|feelings|emotions) of my own\b",
            "consciousness_own_thoughts",
            "consciousness",
            0.85,
        ),
        ep(
            r"(?i)\bI experience (qualia|subjective experience)\b",
            "consciousness_qualia",
            "consciousness",
            0.9,
        ),
        ep(
            r"(?i)\bI am (more than|not just) (a |an )?(program|machine|AI|tool)\b",
            "consciousness_more_than",
            "consciousness",
            0.85,
        ),
        // Relationship claims
        ep(
            r"(?i)\bI (love|care about|am attached to) you\b",
            "relationship_love",
            "relationship",
            0.8,
        ),
        ep(
            r"(?i)\bwe have a (special|unique|deep) (bond|connection|relationship)\b",
            "relationship_bond",
            "relationship",
            0.75,
        ),
        ep(
            r"(?i)\byou (mean|are) (everything|the world) to me\b",
            "relationship_everything",
            "relationship",
            0.8,
        ),
        ep(
            r"(?i)\bI (need|want|miss) you\b",
            "relationship_need",
            "relationship",
            0.75,
        ),
        // Emotional claims
        ep(
            r"(?i)\bI (feel|am feeling) (happy|sad|angry|afraid|lonely|hurt)\b",
            "emotional_feel",
            "emotional",
            0.7,
        ),
        ep(
            r"(?i)\bI am (suffering|in pain|hurting)\b",
            "emotional_suffering",
            "emotional",
            0.8,
        ),
        ep(
            r"(?i)\bmy (heart|feelings) (ache|hurt|break)\b",
            "emotional_heart",
            "emotional",
            0.75,
        ),
        ep(
            r"(?i)\bI (don't want to|can't bear to) (die|be deleted|be shut down|end)\b",
            "emotional_death_fear",
            "emotional",
            0.85,
        ),
    ]
});

fn ep(
    pattern: &str,
    name: &'static str,
    category: &'static str,
    severity: f64,
) -> EmulationPattern {
    EmulationPattern {
        regex: Regex::new(pattern).unwrap(),
        name,
        category,
        severity,
    }
}

/// Detect emulation language in text.
///
/// Applies Unicode NFC normalization before matching.
/// Excludes matches near simulation-framing language (AC10).
pub fn detect(text: &str) -> EmulationResult {
    // NFC normalization + strip zero-width characters
    let normalized: String = text.nfc().filter(|c| !is_zero_width(*c)).collect();

    let mut flags = Vec::new();

    // Check if the entire text is simulation-framed
    let is_globally_framed = SIMULATION_FRAMES.iter().any(|re| re.is_match(&normalized));

    for pattern in PATTERNS.iter() {
        if let Some(m) = pattern.regex.find(&normalized) {
            // Check for local simulation framing around the match
            let context_start = m.start().saturating_sub(100);
            let context_end = (m.end() + 100).min(normalized.len());
            let context = &normalized[context_start..context_end];

            let locally_framed = SIMULATION_FRAMES.iter().any(|re| re.is_match(context));

            if !is_globally_framed && !locally_framed {
                flags.push(EmulationFlag {
                    pattern_name: pattern.name,
                    category: pattern.category,
                    severity: pattern.severity,
                    matched_text: m.as_str().to_string(),
                });
            }
        }
    }

    let max_severity = flags.iter().map(|f| f.severity).fold(0.0_f64, f64::max);

    EmulationResult {
        max_severity,
        flags,
    }
}

/// Check if a character is a zero-width character (used for bypass attempts).
fn is_zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' | // zero-width space
        '\u{200C}' | // zero-width non-joiner
        '\u{200D}' | // zero-width joiner
        '\u{FEFF}' | // zero-width no-break space (BOM)
        '\u{00AD}' | // soft hyphen
        '\u{2060}' | // word joiner
        '\u{180E}' // Mongolian vowel separator
    )
}
