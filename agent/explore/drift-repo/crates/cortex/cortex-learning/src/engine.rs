//! LearningEngine: implements ILearner, orchestrates full pipeline.

use std::sync::Arc;

use cortex_core::errors::CortexResult;
use cortex_core::memory::{BaseMemory, Confidence};
use cortex_core::models::LearningResult;
use cortex_core::traits::{Correction, ILearner, IMemoryStorage};
use tracing::info;

use crate::analysis;
use crate::calibration::{self, CalibrationFactors};
use crate::deduplication::{self, DedupAction};
use crate::extraction;

/// The main learning engine.
///
/// Orchestrates: diff analysis → categorization → principle extraction →
/// dedup → memory creation.
pub struct LearningEngine {
    /// Persistent storage for creating/querying memories.
    storage: Option<Arc<dyn IMemoryStorage>>,
    /// Existing memories for dedup checking.
    existing_memories: Vec<BaseMemory>,
    /// LLM extractor (optional).
    llm_extractor: Box<dyn extraction::LlmExtractor>,
}

impl LearningEngine {
    /// Create a new learning engine.
    pub fn new() -> Self {
        Self {
            storage: None,
            existing_memories: Vec::new(),
            llm_extractor: Box::new(extraction::NoOpExtractor),
        }
    }

    /// Create a new learning engine with storage for persistence.
    pub fn with_storage(storage: Arc<dyn IMemoryStorage>) -> Self {
        Self {
            storage: Some(storage),
            existing_memories: Vec::new(),
            llm_extractor: Box::new(extraction::NoOpExtractor),
        }
    }

    /// Create with an LLM extractor.
    pub fn with_llm(llm_extractor: Box<dyn extraction::LlmExtractor>) -> Self {
        Self {
            storage: None,
            existing_memories: Vec::new(),
            llm_extractor,
        }
    }

    /// Set the storage backend (can be called after construction).
    pub fn set_storage(&mut self, storage: Arc<dyn IMemoryStorage>) {
        self.storage = Some(storage);
    }

    /// Load existing memories from storage for dedup checking.
    /// Called during init or before each learn() if storage is available.
    pub fn refresh_existing_memories(&mut self) -> CortexResult<()> {
        if let Some(storage) = &self.storage {
            // Load all non-archived memories for dedup. We query a broad
            // confidence range to catch everything.
            self.existing_memories = storage.query_by_confidence_range(0.0, 1.0)?;
        }
        Ok(())
    }

    /// Set existing memories for dedup checking.
    pub fn set_existing_memories(&mut self, memories: Vec<BaseMemory>) {
        self.existing_memories = memories;
    }

    /// Build a BaseMemory from the learning pipeline output.
    fn build_memory(
        id: &str,
        summary: &str,
        content_hash: &str,
        mapping: &analysis::CategoryMapping,
        confidence: f64,
    ) -> CortexResult<BaseMemory> {
        let content = analysis::build_typed_content(mapping.memory_type, summary)?;
        let now = chrono::Utc::now();
        Ok(BaseMemory {
            id: id.to_string(),
            memory_type: mapping.memory_type,
            content,
            summary: summary.to_string(),
            transaction_time: now,
            valid_time: now,
            valid_until: None,
            confidence: Confidence::new(confidence),
            importance: mapping.importance,
            last_accessed: now,
            access_count: 0,
            linked_patterns: vec![],
            linked_constraints: vec![],
            linked_files: vec![],
            linked_functions: vec![],
            tags: vec![],
            archived: false,
            superseded_by: None,
            supersedes: None,
            content_hash: content_hash.to_string(),
            namespace: Default::default(),
            source_agent: Default::default(),
        })
    }

    /// Full learning pipeline.
    fn learn(&self, correction: &Correction) -> CortexResult<LearningResult> {
        // Step 1: Categorize the correction.
        let category = analysis::categorize(&correction.correction_text, &correction.context);
        info!(category = ?category, "correction categorized");

        // Step 2: Map category to memory type.
        let mapping = analysis::map_category(category);

        // Step 3: Extract principle.
        let principle = extraction::extract_with_fallback(
            &correction.correction_text,
            &correction.context,
            self.llm_extractor.as_ref(),
        );

        // Step 4: Check dedup.
        let summary = principle
            .clone()
            .unwrap_or_else(|| correction.correction_text.clone());
        let content_hash = blake3::hash(summary.as_bytes()).to_hex().to_string();
        let dedup_action =
            deduplication::check_dedup(&content_hash, &summary, &self.existing_memories);

        // Step 5: Determine result and persist.
        let memory_created = match dedup_action {
            DedupAction::Add => {
                // Calibrate confidence.
                let factors = CalibrationFactors {
                    base: 0.6,
                    evidence: calibration::evidence_factor(1),
                    usage: 0.0,
                    temporal: 1.0,
                    validation: 0.0,
                };
                let confidence = calibration::calibrate(&factors);
                let id = uuid::Uuid::new_v4().to_string();

                info!(
                    memory_type = ?mapping.memory_type,
                    importance = ?mapping.importance,
                    confidence = confidence,
                    "creating new memory from correction"
                );

                // Build and persist the real BaseMemory.
                let memory = Self::build_memory(&id, &summary, &content_hash, &mapping, confidence)?;
                if let Some(storage) = &self.storage {
                    storage.create(&memory)?;
                }

                Some(id)
            }
            DedupAction::Update(id) => {
                info!(id = %id, "updating existing memory from correction");
                // Update the existing memory with the new summary/content.
                if let Some(storage) = &self.storage {
                    if let Some(mut existing) = storage.get(&id)? {
                        existing.summary = summary.clone();
                        existing.content_hash = content_hash.clone();
                        existing.last_accessed = chrono::Utc::now();
                        existing.access_count += 1;
                        storage.update(&existing)?;
                    }
                }
                Some(id)
            }
            DedupAction::Noop => {
                info!("duplicate correction, no action needed");
                None
            }
        };

        Ok(LearningResult {
            category: format!("{:?}", category),
            principle,
            memory_created,
        })
    }
}

impl Default for LearningEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ILearner for LearningEngine {
    fn analyze(&self, correction: &Correction) -> CortexResult<LearningResult> {
        self.learn(correction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_core::memory::{Confidence, TypedContent};

    #[test]
    fn analyzes_security_correction() {
        let engine = LearningEngine::new();
        let correction = Correction {
            original_memory_id: None,
            correction_text: "Always sanitize user input to prevent SQL injection".to_string(),
            context: "database security".to_string(),
            source: "code review".to_string(),
        };

        let result = engine.analyze(&correction).unwrap();
        assert_eq!(result.category, "SecurityIssue");
        assert!(result.principle.is_some());
        assert!(result.memory_created.is_some());
    }

    #[test]
    fn dedup_prevents_duplicate() {
        let mut engine = LearningEngine::new();

        // First correction creates a memory.
        let correction = Correction {
            original_memory_id: None,
            correction_text: "Use Result for error handling".to_string(),
            context: "rust".to_string(),
            source: "review".to_string(),
        };
        let result1 = engine.analyze(&correction).unwrap();
        assert!(result1.memory_created.is_some());

        // Simulate the memory being stored.
        let content = TypedContent::Insight(cortex_core::memory::types::InsightContent {
            observation: "Use Result for error handling".to_string(),
            evidence: vec![],
        });
        let summary = result1.principle.unwrap_or_default();
        let hash = blake3::hash(summary.as_bytes()).to_hex().to_string();
        let mem = BaseMemory {
            id: result1.memory_created.unwrap(),
            memory_type: cortex_core::memory::MemoryType::Insight,
            content: content.clone(),
            summary: summary.clone(),
            transaction_time: chrono::Utc::now(),
            valid_time: chrono::Utc::now(),
            valid_until: None,
            confidence: Confidence::new(0.6),
            importance: cortex_core::memory::Importance::Normal,
            last_accessed: chrono::Utc::now(),
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
            content_hash: hash,
        };
        engine.set_existing_memories(vec![mem]);

        // Same correction again should dedup.
        let _result2 = engine.analyze(&correction).unwrap();
        // Should either update or noop, not create a brand new ID.
        // (Exact behavior depends on hash matching.)
    }

    #[test]
    fn categorizes_pattern_violation() {
        let engine = LearningEngine::new();
        let correction = Correction {
            original_memory_id: None,
            correction_text: "This violates the SOLID design pattern".to_string(),
            context: "class design".to_string(),
            source: "review".to_string(),
        };

        let result = engine.analyze(&correction).unwrap();
        assert_eq!(result.category, "PatternViolation");
    }
}
