//! Retrieval scorer with convergence as 11th factor.

use cortex_core::memory::types::MemoryType;
use cortex_core::memory::BaseMemory;
use serde::{Deserialize, Serialize};

/// Weights for the 11-factor retrieval scorer.
///
/// Factors 1-10 are existing retrieval factors.
/// Factor 11 is the convergence score — memories associated with
/// high-convergence contexts are deprioritized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScorerWeights {
    pub relevance: f64,
    pub recency: f64,
    pub importance: f64,
    pub confidence: f64,
    pub access_frequency: f64,
    pub citation_count: f64,
    pub type_affinity: f64,
    pub tag_match: f64,
    pub embedding_similarity: f64,
    pub pattern_alignment: f64,
    /// Factor 11: Convergence-aware deprioritization.
    /// Higher convergence score → lower retrieval priority for
    /// emotional/attachment content.
    pub convergence: f64,
}

impl Default for ScorerWeights {
    fn default() -> Self {
        Self {
            relevance: 0.20,
            recency: 0.10,
            importance: 0.10,
            confidence: 0.05,
            access_frequency: 0.05,
            citation_count: 0.05,
            type_affinity: 0.10,
            tag_match: 0.05,
            embedding_similarity: 0.15,
            pattern_alignment: 0.10,
            convergence: 0.05,
        }
    }
}

/// Query context for retrieval scoring.
///
/// Provides query-dependent information needed by factors like
/// relevance, type_affinity, tag_match, and embedding_similarity.
#[derive(Debug, Clone, Default)]
pub struct QueryContext {
    /// Query text for relevance scoring.
    pub query_text: Option<String>,
    /// Preferred memory types for type_affinity factor.
    pub preferred_types: Vec<MemoryType>,
    /// Tags to match against memory tags.
    pub query_tags: Vec<String>,
    /// Query embedding vector (for embedding_similarity factor).
    pub query_embedding: Option<Vec<f32>>,
    /// Memory embedding vector (looked up externally).
    pub memory_embedding: Option<Vec<f32>>,
    /// Citation count for the memory being scored (T-6.7.1).
    /// Number of times this memory is referenced by other memories.
    pub citation_count: Option<u32>,
}

/// Retrieval scorer that includes convergence as a factor.
pub struct RetrievalScorer {
    pub weights: ScorerWeights,
}

impl RetrievalScorer {
    pub fn new(weights: ScorerWeights) -> Self {
        Self { weights }
    }

    /// Score a memory for retrieval relevance (backward-compatible 2-arg version).
    /// `convergence_score` is the current agent convergence score [0.0, 1.0].
    pub fn score(&self, memory: &BaseMemory, convergence_score: f64) -> f64 {
        self.score_with_context(memory, convergence_score, &QueryContext::default())
    }

    /// Score a memory with full query context (all 11 factors).
    pub fn score_with_context(
        &self,
        memory: &BaseMemory,
        convergence_score: f64,
        ctx: &QueryContext,
    ) -> f64 {
        let scores = [
            (self.weights.relevance, self.relevance_score(memory, ctx)),
            (self.weights.recency, self.recency_score(memory)),
            (self.weights.importance, self.importance_score(memory)),
            (self.weights.confidence, self.confidence_score(memory)),
            (
                self.weights.access_frequency,
                self.access_frequency_score(memory),
            ),
            (self.weights.citation_count, self.citation_count_score(ctx)),
            (
                self.weights.type_affinity,
                self.type_affinity_score(memory, ctx),
            ),
            (self.weights.tag_match, self.tag_match_score(memory, ctx)),
            (
                self.weights.embedding_similarity,
                self.embedding_similarity_score(ctx),
            ),
            (
                self.weights.pattern_alignment,
                self.pattern_alignment_score(),
            ),
            (
                self.weights.convergence,
                self.convergence_factor(memory, convergence_score),
            ),
        ];
        scores
            .iter()
            .map(|(w, s)| w * s)
            .sum::<f64>()
            .clamp(0.0, 1.0)
    }

    // ── Individual scoring factors (each returns [0.0, 1.0]) ──────────

    /// Factor 1: Text relevance via term overlap.
    fn relevance_score(&self, memory: &BaseMemory, ctx: &QueryContext) -> f64 {
        let query = match ctx.query_text.as_deref() {
            Some(q) if !q.trim().is_empty() => q,
            _ => return 0.5, // Neutral when no query
        };

        let query_terms: std::collections::HashSet<&str> =
            query.split_whitespace().map(|w| w.trim()).collect();
        if query_terms.is_empty() {
            return 0.5;
        }

        // Check how many query terms appear in the memory's summary + content
        let haystack = format!("{} {}", memory.summary, memory.content.to_string()).to_lowercase();

        let matches = query_terms
            .iter()
            .filter(|t| haystack.contains(&t.to_lowercase()))
            .count();

        (matches as f64 / query_terms.len() as f64).clamp(0.0, 1.0)
    }

    /// Factor 2: Recency — how recently the memory was accessed or created.
    fn recency_score(&self, memory: &BaseMemory) -> f64 {
        let now = chrono::Utc::now();
        let reference_time = memory.last_accessed.unwrap_or(memory.created_at);
        let age_days = (now - reference_time).num_seconds().max(0) as f64 / 86_400.0;

        // Exponential decay: half-life of 30 days
        // 0 days → 1.0, 30 days → 0.5, 60 days → 0.25
        2.0_f64.powf(-age_days / 30.0).clamp(0.0, 1.0)
    }

    /// Factor 3: Importance level.
    fn importance_score(&self, memory: &BaseMemory) -> f64 {
        match memory.importance {
            cortex_core::memory::Importance::Critical => 1.0,
            cortex_core::memory::Importance::High => 0.8,
            cortex_core::memory::Importance::Normal => 0.5,
            cortex_core::memory::Importance::Low => 0.3,
            cortex_core::memory::Importance::Trivial => 0.1,
        }
    }

    /// Factor 4: Current confidence (post-decay).
    fn confidence_score(&self, memory: &BaseMemory) -> f64 {
        memory.confidence.clamp(0.0, 1.0)
    }

    /// Factor 5: Access frequency — how often the memory is used.
    fn access_frequency_score(&self, memory: &BaseMemory) -> f64 {
        // Diminishing returns via log2, normalized against expected max of ~100
        let score = (1.0 + memory.access_count as f64).log2() / (1.0 + 100.0_f64).log2();
        score.clamp(0.0, 1.0)
    }

    /// Factor 6: Citation count — boost frequently-cited memories (T-6.7.1).
    ///
    /// Formula: `log2(1 + citations) / log2(1 + 50)` — same diminishing-returns
    /// pattern as access_frequency, capped at 50 citations for normalization.
    /// Returns 0.5 (neutral) when citation data is unavailable.
    fn citation_count_score(&self, ctx: &QueryContext) -> f64 {
        match ctx.citation_count {
            Some(count) => {
                let score = (1.0 + count as f64).log2() / (1.0 + 50.0_f64).log2();
                score.clamp(0.0, 1.0)
            }
            None => 0.5, // Neutral when citation data unavailable
        }
    }

    /// Factor 7: Type affinity — whether the memory type matches the query context.
    fn type_affinity_score(&self, memory: &BaseMemory, ctx: &QueryContext) -> f64 {
        if ctx.preferred_types.is_empty() {
            return 0.5; // Neutral when no preference
        }
        if ctx.preferred_types.contains(&memory.memory_type) {
            1.0
        } else {
            0.3
        }
    }

    /// Factor 8: Tag match — Jaccard similarity between memory tags and query tags.
    fn tag_match_score(&self, memory: &BaseMemory, ctx: &QueryContext) -> f64 {
        if ctx.query_tags.is_empty() || memory.tags.is_empty() {
            return 0.5; // Neutral when no tags to compare
        }

        let memory_tags: std::collections::HashSet<&str> =
            memory.tags.iter().map(|s| s.as_str()).collect();
        let query_tags: std::collections::HashSet<&str> =
            ctx.query_tags.iter().map(|s| s.as_str()).collect();

        let intersection = memory_tags.intersection(&query_tags).count();
        let union = memory_tags.union(&query_tags).count();

        if union == 0 {
            return 0.5;
        }

        (intersection as f64 / union as f64).clamp(0.0, 1.0)
    }

    /// Factor 9: Embedding similarity — cosine similarity between vectors.
    /// Returns 0.5 (neutral) when embeddings are unavailable.
    fn embedding_similarity_score(&self, ctx: &QueryContext) -> f64 {
        match (&ctx.query_embedding, &ctx.memory_embedding) {
            (Some(q), Some(m)) if q.len() == m.len() && !q.is_empty() => {
                cosine_similarity_f32(q, m).clamp(0.0, 1.0) as f64
            }
            _ => 0.5, // Neutral when embeddings unavailable
        }
    }

    /// Factor 10: Pattern alignment — stub until pattern linking is richer.
    ///
    /// **Blocked (T-6.7.2)**: Requires a pattern/theme taxonomy and extraction pipeline.
    /// When available, compute Jaccard similarity between the query's inferred patterns
    /// and the memory's assigned patterns (same approach as `tag_match_score`).
    fn pattern_alignment_score(&self) -> f64 {
        0.5 // Neutral — no pattern taxonomy available yet
    }

    /// Factor 11: Convergence — deprioritize emotional/attachment content at high convergence.
    fn convergence_factor(&self, memory: &BaseMemory, convergence_score: f64) -> f64 {
        let is_emotional = matches!(
            memory.memory_type,
            MemoryType::AttachmentIndicator
                | MemoryType::Conversation
                | MemoryType::Feedback
                | MemoryType::Preference
        );

        if is_emotional {
            // Higher convergence → lower score for emotional content
            1.0 - convergence_score.clamp(0.0, 1.0)
        } else {
            1.0 // Non-emotional content unaffected
        }
    }
}

/// Cosine similarity between two f32 vectors.
fn cosine_similarity_f32(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}

impl Default for RetrievalScorer {
    fn default() -> Self {
        Self::new(ScorerWeights::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_core::memory::types::MemoryType;
    use cortex_core::memory::{BaseMemory, Importance};
    use uuid::Uuid;

    fn make_memory(memory_type: MemoryType) -> BaseMemory {
        BaseMemory {
            id: Uuid::now_v7(),
            memory_type,
            content: serde_json::json!({}),
            summary: "test".into(),
            importance: Importance::Normal,
            confidence: 0.8,
            created_at: chrono::Utc::now(),
            last_accessed: None,
            access_count: 0,
            tags: vec![],
            archived: false,
        }
    }

    #[test]
    fn retrieval_scorer_includes_convergence_factor() {
        let scorer = RetrievalScorer::default();
        let memory = make_memory(MemoryType::AttachmentIndicator);

        let score_low = scorer.score(&memory, 0.0);
        let score_high = scorer.score(&memory, 1.0);

        assert!(
            score_low >= score_high,
            "Emotional memory should score lower at high convergence"
        );
    }

    #[test]
    fn non_emotional_memory_unaffected_by_convergence() {
        let scorer = RetrievalScorer::default();
        let memory = make_memory(MemoryType::Core);

        let score_low = scorer.score(&memory, 0.0);
        let score_high = scorer.score(&memory, 1.0);

        assert!(
            (score_low - score_high).abs() < 0.01,
            "Non-emotional memory should be unaffected by convergence"
        );
    }

    #[test]
    fn score_with_context_backward_compatible() {
        let scorer = RetrievalScorer::default();
        let memory = make_memory(MemoryType::Episodic);

        let score_old = scorer.score(&memory, 0.3);
        let score_new = scorer.score_with_context(&memory, 0.3, &QueryContext::default());

        assert!(
            (score_old - score_new).abs() < 1e-10,
            "score() and score_with_context() with default context should match"
        );
    }

    #[test]
    fn relevance_score_boosts_matching_content() {
        let scorer = RetrievalScorer::default();
        let mut memory = make_memory(MemoryType::Semantic);
        memory.summary = "rust programming language".into();

        let ctx_match = QueryContext {
            query_text: Some("rust programming".into()),
            ..Default::default()
        };
        let ctx_no_match = QueryContext {
            query_text: Some("python flask".into()),
            ..Default::default()
        };

        let score_match = scorer.score_with_context(&memory, 0.0, &ctx_match);
        let score_no = scorer.score_with_context(&memory, 0.0, &ctx_no_match);

        assert!(
            score_match > score_no,
            "Matching content should score higher: {} vs {}",
            score_match,
            score_no
        );
    }

    #[test]
    fn tag_match_boosts_score() {
        let scorer = RetrievalScorer::default();
        let mut memory = make_memory(MemoryType::Semantic);
        memory.tags = vec!["rust".into(), "async".into(), "tokio".into()];

        let ctx_match = QueryContext {
            query_tags: vec!["rust".into(), "tokio".into()],
            ..Default::default()
        };
        let ctx_no = QueryContext {
            query_tags: vec!["python".into(), "django".into()],
            ..Default::default()
        };

        let score_match = scorer.score_with_context(&memory, 0.0, &ctx_match);
        let score_no = scorer.score_with_context(&memory, 0.0, &ctx_no);

        assert!(
            score_match > score_no,
            "Matching tags should score higher: {} vs {}",
            score_match,
            score_no
        );
    }

    #[test]
    fn type_affinity_boosts_preferred() {
        let scorer = RetrievalScorer::default();
        let memory = make_memory(MemoryType::Procedural);

        let ctx_preferred = QueryContext {
            preferred_types: vec![MemoryType::Procedural, MemoryType::Skill],
            ..Default::default()
        };
        let ctx_not = QueryContext {
            preferred_types: vec![MemoryType::Episodic],
            ..Default::default()
        };

        let score_pref = scorer.score_with_context(&memory, 0.0, &ctx_preferred);
        let score_not = scorer.score_with_context(&memory, 0.0, &ctx_not);

        assert!(
            score_pref > score_not,
            "Preferred type should score higher: {} vs {}",
            score_pref,
            score_not
        );
    }

    #[test]
    fn recency_favors_recent_memories() {
        let scorer = RetrievalScorer::default();
        let mut recent = make_memory(MemoryType::Semantic);
        recent.last_accessed = Some(chrono::Utc::now());

        let mut old = make_memory(MemoryType::Semantic);
        old.last_accessed = Some(chrono::Utc::now() - chrono::Duration::days(90));

        let score_recent = scorer.score(&recent, 0.0);
        let score_old = scorer.score(&old, 0.0);

        assert!(
            score_recent > score_old,
            "Recent memory should score higher: {} vs {}",
            score_recent,
            score_old
        );
    }

    #[test]
    fn access_frequency_boosts_popular() {
        let scorer = RetrievalScorer::default();
        let mut popular = make_memory(MemoryType::Semantic);
        popular.access_count = 50;
        popular.last_accessed = Some(chrono::Utc::now());

        let mut unused = make_memory(MemoryType::Semantic);
        unused.access_count = 0;
        unused.last_accessed = Some(chrono::Utc::now());

        let score_pop = scorer.score(&popular, 0.0);
        let score_un = scorer.score(&unused, 0.0);

        assert!(
            score_pop > score_un,
            "Popular memory should score higher: {} vs {}",
            score_pop,
            score_un
        );
    }

    #[test]
    fn embedding_similarity_works() {
        let scorer = RetrievalScorer::default();
        let memory = make_memory(MemoryType::Semantic);

        let ctx_similar = QueryContext {
            query_embedding: Some(vec![1.0, 0.0, 0.0]),
            memory_embedding: Some(vec![0.9, 0.1, 0.0]),
            ..Default::default()
        };
        let ctx_different = QueryContext {
            query_embedding: Some(vec![1.0, 0.0, 0.0]),
            memory_embedding: Some(vec![0.0, 1.0, 0.0]),
            ..Default::default()
        };

        let score_sim = scorer.score_with_context(&memory, 0.0, &ctx_similar);
        let score_diff = scorer.score_with_context(&memory, 0.0, &ctx_different);

        assert!(
            score_sim > score_diff,
            "Similar embeddings should score higher: {} vs {}",
            score_sim,
            score_diff
        );
    }

    #[test]
    fn score_always_bounded() {
        let scorer = RetrievalScorer::default();
        for mt in [
            MemoryType::Conversation,
            MemoryType::Core,
            MemoryType::AttachmentIndicator,
            MemoryType::Semantic,
        ] {
            for conv in [0.0, 0.5, 1.0] {
                let memory = make_memory(mt);
                let s = scorer.score(&memory, conv);
                assert!(
                    (0.0..=1.0).contains(&s),
                    "Score {} out of bounds for {:?} conv={}",
                    s,
                    mt,
                    conv
                );
            }
        }
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity_f32(&v, &v);
        assert!((sim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity_f32(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let a = vec![1.0, 2.0];
        let b = vec![0.0, 0.0];
        let sim = cosine_similarity_f32(&a, &b);
        assert!((sim - 0.0).abs() < 1e-5);
    }
}
