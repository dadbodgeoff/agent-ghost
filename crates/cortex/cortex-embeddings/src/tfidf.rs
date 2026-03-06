//! TF-IDF sparse-to-dense embedding provider.
//!
//! Generates fixed-dimension vectors from term frequency–inverse document frequency
//! scores. No external dependencies — works in air-gapped environments.

use std::collections::HashMap;

/// TF-IDF embedding provider.
///
/// Produces deterministic dense vectors by hashing terms into fixed-dimension
/// buckets and weighting by term frequency. Not as semantically rich as neural
/// embeddings, but always available with zero dependencies.
pub struct TfIdfProvider {
    dimensions: usize,
}

impl TfIdfProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }

    /// Embed text into a fixed-dimension vector.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.tfidf_vector(text)
    }

    /// Embed a batch of texts.
    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.tfidf_vector(t)).collect()
    }

    /// Number of output dimensions.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Hash a term into a bucket index using FNV-1a.
    fn hash_term(term: &str, dims: usize) -> usize {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in term.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        (h as usize) % dims
    }

    /// Tokenize text into lowercase alphanumeric terms.
    fn tokenize(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| s.len() >= 2)
            .map(|s| s.to_lowercase())
            .collect()
    }

    /// Build a TF-IDF vector for the given text.
    fn tfidf_vector(&self, text: &str) -> Vec<f32> {
        let tokens = Self::tokenize(text);
        if tokens.is_empty() {
            return vec![0.0; self.dimensions];
        }

        // Count term frequencies.
        let mut tf: HashMap<String, f32> = HashMap::new();
        for tok in &tokens {
            *tf.entry(tok.clone()).or_default() += 1.0;
        }

        let total = tokens.len() as f32;
        let mut vec = vec![0.0f32; self.dimensions];

        for (term, count) in &tf {
            let freq = count / total;
            // IDF approximation: penalize very short terms (likely stopwords).
            let idf = 1.0 + (term.len() as f32).ln();
            let bucket = Self::hash_term(term, self.dimensions);
            vec[bucket] += freq * idf;
        }

        // L2 normalize.
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > f32::EPSILON {
            for v in &mut vec {
                *v /= norm;
            }
        }

        vec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_returns_zero_vector() {
        let p = TfIdfProvider::new(128);
        let v = p.embed("");
        assert_eq!(v.len(), 128);
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn produces_correct_dimensions() {
        let p = TfIdfProvider::new(384);
        let v = p.embed("hello world test embedding");
        assert_eq!(v.len(), 384);
    }

    #[test]
    fn output_is_normalized() {
        let p = TfIdfProvider::new(256);
        let v = p.embed("rust programming language systems");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "expected unit norm, got {norm}");
    }

    #[test]
    fn deterministic() {
        let p = TfIdfProvider::new(256);
        let a = p.embed("deterministic test");
        let b = p.embed("deterministic test");
        assert_eq!(a, b);
    }

    #[test]
    fn batch_matches_individual() {
        let p = TfIdfProvider::new(128);
        let texts = vec!["hello world".to_string(), "foo bar baz".to_string()];
        let batch = p.embed_batch(&texts);
        for (i, text) in texts.iter().enumerate() {
            let single = p.embed(text);
            assert_eq!(batch[i], single);
        }
    }

    #[test]
    fn similar_texts_have_higher_cosine() {
        let p = TfIdfProvider::new(256);
        let a = p.embed("rust programming language");
        let b = p.embed("rust programming systems");
        let c = p.embed("cooking recipes pasta");

        let cos_ab: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        let cos_ac: f32 = a.iter().zip(&c).map(|(x, y)| x * y).sum();
        assert!(
            cos_ab > cos_ac,
            "similar texts should have higher cosine similarity"
        );
    }
}
