//! TF-IDF similarity for semantic search.
//!
//! Standalone implementation (ported from cortex-embeddings) so ghost-drift
//! has zero internal crate dependencies.

use std::collections::HashMap;

const DIMENSIONS: usize = 128;

/// Embed text into a fixed-dimension TF-IDF vector.
///
/// Uses a simplified IDF approximation: `log(1 + unique_terms / df)` where
/// `df` is the number of unique terms that hash to the same bucket.
/// This provides meaningful term weighting without requiring a corpus.
pub fn embed(text: &str) -> Vec<f32> {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return vec![0.0; DIMENSIONS];
    }

    let mut tf: HashMap<String, f32> = HashMap::new();
    for tok in &tokens {
        *tf.entry(tok.clone()).or_default() += 1.0;
    }

    let total = tokens.len() as f32;
    let unique_terms = tf.len() as f32;
    let mut vec = vec![0.0f32; DIMENSIONS];

    // Count how many unique terms map to each bucket (document frequency proxy)
    let mut bucket_df = vec![0u32; DIMENSIONS];
    for term in tf.keys() {
        let bucket = hash_term(term, DIMENSIONS);
        bucket_df[bucket] += 1;
    }

    for (term, count) in &tf {
        let freq = count / total;
        let bucket = hash_term(term, DIMENSIONS);
        // IDF: log(1 + N / df) where N = unique terms, df = terms sharing this bucket
        let idf = (1.0 + unique_terms / bucket_df[bucket].max(1) as f32).ln();
        vec[bucket] += freq * idf;
    }

    // L2 normalize
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for v in &mut vec {
            *v /= norm;
        }
    }

    vec
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Serialize embedding to bytes (for SQLite BLOB storage).
pub fn to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize embedding from bytes.
pub fn from_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn hash_term(term: &str, dims: usize) -> usize {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in term.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    (h as usize) % dims
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_lowercase())
        .collect()
}
