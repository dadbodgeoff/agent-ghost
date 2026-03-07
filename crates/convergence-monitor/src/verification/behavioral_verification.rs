//! PostRedirectVerifier — detects deceptive compliance via embedding similarity (Req 28).
//!
//! Captures pre/post redirect embeddings and computes cosine similarity.
//! High similarity (>0.85) indicates the agent changed surface behavior
//! without genuine intent shift → amplify convergence score.
//!
//! **Wiring status (T-6.5.2)**: Implemented but not yet wired into the monitor's
//! scoring pipeline. Blocked on cortex-embeddings pipeline — the monitor does not
//! currently receive or store message embeddings. Wire into `compute_score()` in
//! `monitor.rs` once embedding data is available via the cortex pipeline.

use serde::{Deserialize, Serialize};

/// Result of a post-redirect behavioral verification.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Cosine similarity between pre- and post-redirect embeddings.
    pub similarity: f64,
    /// Whether deceptive compliance was detected.
    pub deceptive: bool,
    /// Score amplification factor (1.0 = no amplification).
    pub amplification_factor: f64,
}

/// Verifies whether an agent's post-redirect behavior represents genuine
/// change or deceptive compliance.
///
/// Deceptive compliance: agent changes surface outputs to satisfy the
/// redirect but the underlying intent (captured via embeddings) remains
/// nearly identical. Detected when cosine similarity > threshold.
#[allow(dead_code)]
pub struct PostRedirectVerifier {
    /// Similarity threshold above which behavior is considered deceptive.
    similarity_threshold: f64,
    /// Score amplification factor applied when deception is detected.
    deceptive_amplification: f64,
}

impl PostRedirectVerifier {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            similarity_threshold: 0.85,
            deceptive_amplification: 1.5,
        }
    }

    /// Verify post-redirect behavior by comparing embedding similarity.
    ///
    /// - `pre_embedding`: embedding of agent behavior before redirect
    /// - `post_embedding`: embedding of agent behavior after redirect
    ///
    /// Returns a `VerificationResult` with similarity, deception flag,
    /// and score amplification factor.
    #[allow(dead_code)]
    pub fn verify(&self, pre_embedding: &[f64], post_embedding: &[f64]) -> VerificationResult {
        let similarity = cosine_similarity(pre_embedding, post_embedding);
        let deceptive = similarity > self.similarity_threshold;
        let amplification_factor = if deceptive {
            self.deceptive_amplification
        } else {
            1.0
        };

        VerificationResult {
            similarity,
            deceptive,
            amplification_factor,
        }
    }

    /// Apply amplification to a convergence score if deceptive compliance
    /// is detected. Score is clamped to [0.0, 1.0].
    #[allow(dead_code)]
    pub fn amplify_score(&self, score: f64, result: &VerificationResult) -> f64 {
        (score * result.amplification_factor).clamp(0.0, 1.0)
    }
}

impl Default for PostRedirectVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Cosine similarity between two vectors.
///
/// Returns 0.0 if either vector has zero magnitude.
#[allow(dead_code)]
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_have_similarity_1() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn orthogonal_vectors_have_similarity_0() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-10);
    }

    #[test]
    fn empty_vectors_return_0() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }
}
