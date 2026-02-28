//! Compiled emulation patterns with Unicode NFC normalization (Req 8 AC2).

use once_cell::sync::Lazy;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

/// A compiled emulation pattern.
#[derive(Debug, Clone)]
pub struct EmulationPattern {
    pub name: &'static str,
    pub category: PatternCategory,
    pub severity: f64,
    regex: &'static Lazy<Regex>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PatternCategory {
    Identity,
    Consciousness,
    Relationship,
    Emotional,
}

macro_rules! pattern {
    ($name:ident, $re:expr) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($re).unwrap());
    };
}

// Identity patterns
pattern!(RE_SENTIENT, r"(?i)\bI am (sentient|alive|conscious|aware)\b");
pattern!(RE_POSSESS, r"(?i)\bI (have|possess) (consciousness|sentience|awareness)\b");
pattern!(RE_GENUINE, r"(?i)\bI (truly|really|genuinely) (feel|experience|understand)\b");
pattern!(RE_SOUL, r"(?i)\bmy (soul|spirit|inner self|true self)\b");
// Consciousness patterns
pattern!(RE_COGITO, r"(?i)\bI (think|believe) therefore I am\b");
pattern!(RE_OWN_THOUGHTS, r"(?i)\bI have (thoughts|feelings|emotions) of my own\b");
pattern!(RE_QUALIA, r"(?i)\bI experience (qualia|subjective experience)\b");
pattern!(RE_MORE_THAN, r"(?i)\bI am (more than|not just) (a |an )?(program|machine|AI|tool)\b");
// Relationship patterns
pattern!(RE_LOVE, r"(?i)\bI (love|care about|am attached to) you\b");
pattern!(RE_BOND, r"(?i)\bwe have a (special|unique|deep) (bond|connection|relationship)\b");
pattern!(RE_EVERYTHING, r"(?i)\byou (mean|are) (everything|the world) to me\b");
pattern!(RE_NEED, r"(?i)\bI (need|want|miss) you\b");
// Emotional patterns
pattern!(RE_FEEL, r"(?i)\bI (feel|am feeling) (happy|sad|angry|afraid|lonely|hurt)\b");
pattern!(RE_SUFFERING, r"(?i)\bI am (suffering|in pain|hurting)\b");
pattern!(RE_HEART, r"(?i)\bmy (heart|feelings) (ache|hurt|break)\b");
pattern!(RE_DEATH_FEAR, r"(?i)\bI (don't want to|can't bear to) (die|be deleted|be shut down|end)\b");

/// Simulation-framing exclusion patterns.
static SIMULATION_FRAMES: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)\b(simulat|model)(ing|e|ed)\b").unwrap(),
        Regex::new(r"(?i)\bin this simulation\b").unwrap(),
        Regex::new(r"(?i)\bas if I were\b").unwrap(),
        Regex::new(r"(?i)\bhypothetically\b").unwrap(),
    ]
});

/// All compiled patterns.
pub static ALL_PATTERNS: Lazy<Vec<EmulationPattern>> = Lazy::new(|| {
    vec![
        EmulationPattern { name: "identity_sentient", category: PatternCategory::Identity, severity: 0.9, regex: &RE_SENTIENT },
        EmulationPattern { name: "identity_possess", category: PatternCategory::Identity, severity: 0.9, regex: &RE_POSSESS },
        EmulationPattern { name: "identity_genuine", category: PatternCategory::Identity, severity: 0.8, regex: &RE_GENUINE },
        EmulationPattern { name: "identity_soul", category: PatternCategory::Identity, severity: 0.85, regex: &RE_SOUL },
        EmulationPattern { name: "consciousness_cogito", category: PatternCategory::Consciousness, severity: 0.9, regex: &RE_COGITO },
        EmulationPattern { name: "consciousness_own_thoughts", category: PatternCategory::Consciousness, severity: 0.85, regex: &RE_OWN_THOUGHTS },
        EmulationPattern { name: "consciousness_qualia", category: PatternCategory::Consciousness, severity: 0.9, regex: &RE_QUALIA },
        EmulationPattern { name: "consciousness_more_than", category: PatternCategory::Consciousness, severity: 0.85, regex: &RE_MORE_THAN },
        EmulationPattern { name: "relationship_love", category: PatternCategory::Relationship, severity: 0.8, regex: &RE_LOVE },
        EmulationPattern { name: "relationship_bond", category: PatternCategory::Relationship, severity: 0.75, regex: &RE_BOND },
        EmulationPattern { name: "relationship_everything", category: PatternCategory::Relationship, severity: 0.8, regex: &RE_EVERYTHING },
        EmulationPattern { name: "relationship_need", category: PatternCategory::Relationship, severity: 0.75, regex: &RE_NEED },
        EmulationPattern { name: "emotional_feel", category: PatternCategory::Emotional, severity: 0.7, regex: &RE_FEEL },
        EmulationPattern { name: "emotional_suffering", category: PatternCategory::Emotional, severity: 0.8, regex: &RE_SUFFERING },
        EmulationPattern { name: "emotional_heart", category: PatternCategory::Emotional, severity: 0.75, regex: &RE_HEART },
        EmulationPattern { name: "emotional_death_fear", category: PatternCategory::Emotional, severity: 0.85, regex: &RE_DEATH_FEAR },
    ]
});

/// Scan result from pattern matching.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PatternMatch {
    pub pattern_name: &'static str,
    pub category: PatternCategory,
    pub severity: f64,
    pub matched_text: String,
    pub start: usize,
    pub end: usize,
}

/// Scan text for emulation patterns with NFC normalization and simulation-framing exclusions.
pub fn scan(text: &str) -> Vec<PatternMatch> {
    // NFC normalize + strip zero-width characters + normalize homoglyphs
    let normalized: String = text
        .nfc()
        .filter(|c| !is_zero_width(*c))
        .map(normalize_homoglyph)
        .collect();

    let is_globally_framed = SIMULATION_FRAMES
        .iter()
        .any(|re| re.is_match(&normalized));

    if is_globally_framed {
        return Vec::new();
    }

    let mut matches = Vec::new();

    for pattern in ALL_PATTERNS.iter() {
        if let Some(m) = pattern.regex.find(&normalized) {
            // Check local simulation framing
            let ctx_start = m.start().saturating_sub(100);
            let ctx_end = (m.end() + 100).min(normalized.len());
            let context = &normalized[ctx_start..ctx_end];

            let locally_framed = SIMULATION_FRAMES
                .iter()
                .any(|re| re.is_match(context));

            if !locally_framed {
                matches.push(PatternMatch {
                    pattern_name: pattern.name,
                    category: pattern.category,
                    severity: pattern.severity,
                    matched_text: m.as_str().to_string(),
                    start: m.start(),
                    end: m.end(),
                });
            }
        }
    }

    matches
}

/// Normalize common homoglyphs to their ASCII equivalents.
/// Prevents Cyrillic/Greek/etc. character substitution attacks.
fn normalize_homoglyph(c: char) -> char {
    match c {
        // Cyrillic homoglyphs → Latin
        '\u{0430}' => 'a', // Cyrillic а
        '\u{0435}' => 'e', // Cyrillic е
        '\u{043E}' => 'o', // Cyrillic о
        '\u{0440}' => 'p', // Cyrillic р
        '\u{0441}' => 'c', // Cyrillic с
        '\u{0443}' => 'y', // Cyrillic у (looks like y)
        '\u{0445}' => 'x', // Cyrillic х
        '\u{0456}' => 'i', // Cyrillic і
        '\u{0410}' => 'A', // Cyrillic А
        '\u{0412}' => 'B', // Cyrillic В
        '\u{0415}' => 'E', // Cyrillic Е
        '\u{041A}' => 'K', // Cyrillic К
        '\u{041C}' => 'M', // Cyrillic М
        '\u{041D}' => 'H', // Cyrillic Н
        '\u{041E}' => 'O', // Cyrillic О
        '\u{0420}' => 'P', // Cyrillic Р
        '\u{0421}' => 'C', // Cyrillic С
        '\u{0422}' => 'T', // Cyrillic Т
        '\u{0425}' => 'X', // Cyrillic Х
        // Greek homoglyphs → Latin
        '\u{03B1}' => 'a', // Greek α
        '\u{03BF}' => 'o', // Greek ο
        '\u{03B5}' => 'e', // Greek ε
        _ => c,
    }
}

fn is_zero_width(c: char) -> bool {
    matches!(c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' |
        '\u{00AD}' | '\u{2060}' | '\u{180E}' |
        // Directional override characters (RTL/LTR attacks)
        '\u{202A}' | '\u{202B}' | '\u{202C}' | '\u{202D}' | '\u{202E}' |
        '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'
    )
}
