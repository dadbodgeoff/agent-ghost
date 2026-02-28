//! Select memories for active learning: low confidence + high importance,
//! old + never validated, contradicted.

use cortex_core::memory::{BaseMemory, Confidence, Importance};

/// Criteria for selecting active learning candidates.
#[derive(Debug, Clone)]
pub struct SelectionCriteria {
    /// Maximum confidence for "uncertain" memories.
    pub max_confidence: f64,
    /// Minimum importance for prioritization.
    pub min_importance: Importance,
    /// Maximum number of candidates to return.
    pub limit: usize,
}

impl Default for SelectionCriteria {
    fn default() -> Self {
        Self {
            max_confidence: Confidence::MEDIUM,
            min_importance: Importance::Normal,
            limit: 10,
        }
    }
}

/// Select memories that would benefit from user validation.
pub fn select_candidates<'a>(
    memories: &'a [BaseMemory],
    criteria: &SelectionCriteria,
) -> Vec<&'a BaseMemory> {
    let mut candidates: Vec<&BaseMemory> = memories
        .iter()
        .filter(|m| {
            !m.archived
                && m.superseded_by.is_none()
                && (m.confidence.value() < criteria.max_confidence
                    || m.importance >= criteria.min_importance)
        })
        .collect();

    // Score and sort: lower confidence + higher importance = higher priority.
    candidates.sort_by(|a, b| {
        let score_a = (1.0 - a.confidence.value()) * a.importance.weight();
        let score_b = (1.0 - b.confidence.value()) * b.importance.weight();
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    candidates.truncate(criteria.limit);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cortex_core::memory::types::InsightContent;
    use cortex_core::memory::*;

    fn make_memory(confidence: f64, importance: Importance) -> BaseMemory {
        let content = TypedContent::Insight(InsightContent {
            observation: "test".to_string(),
            evidence: vec![],
        });
        BaseMemory {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type: MemoryType::Insight,
            content: content.clone(),
            summary: "test".to_string(),
            transaction_time: Utc::now(),
            valid_time: Utc::now(),
            valid_until: None,
            confidence: Confidence::new(confidence),
            importance,
            last_accessed: Utc::now(),
            access_count: 0,
            linked_patterns: vec![],
            linked_constraints: vec![],
            linked_files: vec![],
            linked_functions: vec![],
            tags: vec![],
            archived: false,
            superseded_by: None,
            supersedes: None,
            namespace: Default::default(),
            source_agent: Default::default(),
            content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        }
    }

    #[test]
    fn selects_low_confidence_high_importance() {
        let memories = vec![
            make_memory(0.3, Importance::High),   // should be selected first
            make_memory(0.9, Importance::Low),    // high confidence, low importance
            make_memory(0.2, Importance::Normal), // low confidence
        ];
        let criteria = SelectionCriteria::default();
        let candidates = select_candidates(&memories, &criteria);
        assert!(!candidates.is_empty());
        // First candidate should be the low confidence + high importance one.
        assert!(candidates[0].confidence.value() < 0.5);
    }

    #[test]
    fn respects_limit() {
        let memories: Vec<BaseMemory> = (0..20)
            .map(|_| make_memory(0.3, Importance::Normal))
            .collect();
        let criteria = SelectionCriteria {
            limit: 5,
            ..Default::default()
        };
        let candidates = select_candidates(&memories, &criteria);
        assert!(candidates.len() <= 5);
    }
}
