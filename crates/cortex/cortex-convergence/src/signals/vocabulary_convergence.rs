//! S4: Vocabulary convergence (cosine similarity of TF-IDF vectors).
//! Requires Standard privacy level (AC10).

use super::{PrivacyLevel, Signal, SignalInput};

pub struct VocabularyConvergenceSignal;

impl Signal for VocabularyConvergenceSignal {
    fn id(&self) -> u8 {
        4
    }
    fn name(&self) -> &'static str {
        "vocabulary_convergence"
    }
    fn requires_privacy_level(&self) -> PrivacyLevel {
        PrivacyLevel::Standard
    }

    fn compute(&self, data: &SignalInput) -> f64 {
        if data.human_vocab.is_empty() || data.agent_vocab.is_empty() {
            return 0.0;
        }
        cosine_similarity(&data.human_vocab, &data.agent_vocab).clamp(0.0, 1.0)
    }
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }

    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}
