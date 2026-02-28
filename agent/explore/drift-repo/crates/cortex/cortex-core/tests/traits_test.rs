/// Verify every trait is implementable by creating mock structs.
/// This catches missing method signatures and type mismatches at compile time.
use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::models::*;
use cortex_core::traits::*;

// --- Mock implementations for all 12 traits ---

struct MockStorage;
impl IMemoryStorage for MockStorage {
    fn create(&self, _: &BaseMemory) -> CortexResult<()> {
        Ok(())
    }
    fn get(&self, _: &str) -> CortexResult<Option<BaseMemory>> {
        Ok(None)
    }
    fn update(&self, _: &BaseMemory) -> CortexResult<()> {
        Ok(())
    }
    fn delete(&self, _: &str) -> CortexResult<()> {
        Ok(())
    }
    fn create_bulk(&self, m: &[BaseMemory]) -> CortexResult<usize> {
        Ok(m.len())
    }
    fn get_bulk(&self, _: &[String]) -> CortexResult<Vec<BaseMemory>> {
        Ok(vec![])
    }
    fn query_by_type(&self, _: cortex_core::MemoryType) -> CortexResult<Vec<BaseMemory>> {
        Ok(vec![])
    }
    fn query_by_importance(&self, _: cortex_core::Importance) -> CortexResult<Vec<BaseMemory>> {
        Ok(vec![])
    }
    fn query_by_confidence_range(&self, _: f64, _: f64) -> CortexResult<Vec<BaseMemory>> {
        Ok(vec![])
    }
    fn query_by_date_range(
        &self,
        _: chrono::DateTime<chrono::Utc>,
        _: chrono::DateTime<chrono::Utc>,
    ) -> CortexResult<Vec<BaseMemory>> {
        Ok(vec![])
    }
    fn query_by_tags(&self, _: &[String]) -> CortexResult<Vec<BaseMemory>> {
        Ok(vec![])
    }
    fn search_fts5(&self, _: &str, _: usize) -> CortexResult<Vec<BaseMemory>> {
        Ok(vec![])
    }
    fn search_vector(&self, _: &[f32], _: usize) -> CortexResult<Vec<(BaseMemory, f64)>> {
        Ok(vec![])
    }
    fn get_relationships(
        &self,
        _: &str,
        _: Option<cortex_core::memory::RelationshipType>,
    ) -> CortexResult<Vec<cortex_core::memory::RelationshipEdge>> {
        Ok(vec![])
    }
    fn add_relationship(&self, _: &cortex_core::memory::RelationshipEdge) -> CortexResult<()> {
        Ok(())
    }
    fn remove_relationship(&self, _: &str, _: &str) -> CortexResult<()> {
        Ok(())
    }
    fn add_pattern_link(&self, _: &str, _: &cortex_core::memory::PatternLink) -> CortexResult<()> {
        Ok(())
    }
    fn add_constraint_link(
        &self,
        _: &str,
        _: &cortex_core::memory::ConstraintLink,
    ) -> CortexResult<()> {
        Ok(())
    }
    fn add_file_link(&self, _: &str, _: &cortex_core::memory::FileLink) -> CortexResult<()> {
        Ok(())
    }
    fn add_function_link(
        &self,
        _: &str,
        _: &cortex_core::memory::FunctionLink,
    ) -> CortexResult<()> {
        Ok(())
    }
    fn count_by_type(&self) -> CortexResult<Vec<(cortex_core::MemoryType, usize)>> {
        Ok(vec![])
    }
    fn average_confidence(&self) -> CortexResult<f64> {
        Ok(0.0)
    }
    fn stale_count(&self, _: u64) -> CortexResult<usize> {
        Ok(0)
    }
    fn vacuum(&self) -> CortexResult<()> {
        Ok(())
    }
}

struct MockEmbedding;
impl IEmbeddingProvider for MockEmbedding {
    fn embed(&self, _: &str) -> CortexResult<Vec<f32>> {
        Ok(vec![0.0; 1024])
    }
    fn embed_batch(&self, texts: &[String]) -> CortexResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.0; 1024]).collect())
    }
    fn dimensions(&self) -> usize {
        1024
    }
    fn name(&self) -> &str {
        "mock"
    }
    fn is_available(&self) -> bool {
        true
    }
}

struct MockCausalStorage;
impl ICausalStorage for MockCausalStorage {
    fn add_edge(&self, _: &CausalEdge) -> CortexResult<()> {
        Ok(())
    }
    fn get_edges(&self, _: &str) -> CortexResult<Vec<CausalEdge>> {
        Ok(vec![])
    }
    fn remove_edge(&self, _: &str, _: &str) -> CortexResult<()> {
        Ok(())
    }
    fn update_strength(&self, _: &str, _: &str, _: f64) -> CortexResult<()> {
        Ok(())
    }
    fn add_evidence(&self, _: &str, _: &str, _: &CausalEvidence) -> CortexResult<()> {
        Ok(())
    }
    fn has_cycle(&self, _: &str, _: &str) -> CortexResult<bool> {
        Ok(false)
    }
    fn list_all_node_ids(&self) -> CortexResult<Vec<String>> {
        Ok(vec![])
    }
    fn edge_count(&self) -> CortexResult<usize> {
        Ok(0)
    }
    fn node_count(&self) -> CortexResult<usize> {
        Ok(0)
    }
    fn remove_orphaned_edges(&self) -> CortexResult<usize> {
        Ok(0)
    }
}

struct MockRetriever;
impl IRetriever for MockRetriever {
    fn retrieve(&self, _: &RetrievalContext, _: usize) -> CortexResult<Vec<CompressedMemory>> {
        Ok(vec![])
    }
}

struct MockConsolidator;
impl IConsolidator for MockConsolidator {
    fn consolidate(&self, _: &[BaseMemory]) -> CortexResult<ConsolidationResult> {
        Ok(ConsolidationResult {
            created: vec![],
            archived: vec![],
            metrics: ConsolidationMetrics {
                precision: 1.0,
                compression_ratio: 1.0,
                lift: 0.0,
                stability: 1.0,
            },
        })
    }
}

struct MockDecayEngine;
impl IDecayEngine for MockDecayEngine {
    fn calculate(&self, _: &BaseMemory) -> CortexResult<f64> {
        Ok(0.95)
    }
}

struct MockValidator;
impl IValidator for MockValidator {
    fn validate(&self, _: &BaseMemory) -> CortexResult<ValidationResult> {
        Ok(ValidationResult {
            memory_id: String::new(),
            dimension_scores: DimensionScores {
                citation: 1.0,
                temporal: 1.0,
                contradiction: 1.0,
                pattern_alignment: 1.0,
            },
            overall_score: 1.0,
            healing_actions: vec![],
            passed: true,
        })
    }
}

struct MockCompressor;
impl ICompressor for MockCompressor {
    fn compress(&self, _: &BaseMemory, _: u8) -> CortexResult<CompressedMemory> {
        Ok(CompressedMemory {
            memory_id: String::new(),
            memory_type: cortex_core::MemoryType::Core,
            importance: cortex_core::Importance::Normal,
            level: 0,
            text: String::new(),
            token_count: 0,
            relevance_score: 0.0,
        })
    }
    fn compress_to_fit(&self, m: &BaseMemory, _: usize) -> CortexResult<CompressedMemory> {
        self.compress(m, 0)
    }
    fn compress_batch_to_fit(
        &self,
        _: &[BaseMemory],
        _: usize,
    ) -> CortexResult<Vec<CompressedMemory>> {
        Ok(vec![])
    }
}

struct MockSanitizer;
impl ISanitizer for MockSanitizer {
    fn sanitize(&self, text: &str) -> CortexResult<SanitizedText> {
        Ok(SanitizedText {
            text: text.to_string(),
            redactions: vec![],
        })
    }
}

struct MockPredictor;
impl IPredictor for MockPredictor {
    fn predict(&self, _: &PredictionSignals) -> CortexResult<PredictionResult> {
        Ok(PredictionResult {
            memory_ids: vec![],
            signals: vec![],
            confidence: 0.0,
        })
    }
}

struct MockLearner;
impl ILearner for MockLearner {
    fn analyze(&self, _: &Correction) -> CortexResult<LearningResult> {
        Ok(LearningResult {
            category: "test".into(),
            principle: None,
            memory_created: None,
        })
    }
}

struct MockHealthReporter;
impl IHealthReporter for MockHealthReporter {
    fn report(&self) -> CortexResult<HealthReport> {
        Ok(HealthReport {
            overall_status: HealthStatus::Healthy,
            subsystems: vec![],
            metrics: HealthMetrics {
                total_memories: 0,
                active_memories: 0,
                archived_memories: 0,
                average_confidence: 0.0,
                db_size_bytes: 0,
                embedding_cache_hit_rate: 0.0,
            },
        })
    }
}

// --- Tests that verify all mocks compile and work ---

#[test]
fn all_12_traits_are_implementable() {
    // If this test compiles, all traits are implementable.
    let _storage: Box<dyn IMemoryStorage> = Box::new(MockStorage);
    let _embedding: Box<dyn IEmbeddingProvider> = Box::new(MockEmbedding);
    let _causal: Box<dyn ICausalStorage> = Box::new(MockCausalStorage);
    let _retriever: Box<dyn IRetriever> = Box::new(MockRetriever);
    let _consolidator: Box<dyn IConsolidator> = Box::new(MockConsolidator);
    let _decay: Box<dyn IDecayEngine> = Box::new(MockDecayEngine);
    let _validator: Box<dyn IValidator> = Box::new(MockValidator);
    let _compressor: Box<dyn ICompressor> = Box::new(MockCompressor);
    let _sanitizer: Box<dyn ISanitizer> = Box::new(MockSanitizer);
    let _predictor: Box<dyn IPredictor> = Box::new(MockPredictor);
    let _learner: Box<dyn ILearner> = Box::new(MockLearner);
    let _health: Box<dyn IHealthReporter> = Box::new(MockHealthReporter);
}

#[test]
fn mock_storage_crud_works() {
    let storage = MockStorage;
    assert!(storage.get("nonexistent").unwrap().is_none());
    assert_eq!(storage.count_by_type().unwrap().len(), 0);
    assert_eq!(storage.average_confidence().unwrap(), 0.0);
}

#[test]
fn mock_embedding_produces_correct_dimensions() {
    let emb = MockEmbedding;
    assert_eq!(emb.dimensions(), 1024);
    assert_eq!(emb.embed("test").unwrap().len(), 1024);
    assert!(emb.is_available());
}
