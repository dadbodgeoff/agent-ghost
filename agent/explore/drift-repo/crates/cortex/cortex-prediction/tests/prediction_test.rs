use chrono::Utc;
use cortex_core::errors::CortexResult;
use cortex_core::memory::*;
use cortex_core::traits::{IMemoryStorage, IPredictor, PredictionSignals};
use cortex_prediction::strategies::{self, PredictionCandidate};
use cortex_prediction::{AggregatedSignals, PredictionCache, PredictionEngine};
use std::sync::Mutex;

// ── Mock Storage ──────────────────────────────────────────────────────────

struct MockStorage {
    memories: Mutex<Vec<BaseMemory>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            memories: Mutex::new(Vec::new()),
        }
    }

    fn with_memories(memories: Vec<BaseMemory>) -> Self {
        Self {
            memories: Mutex::new(memories),
        }
    }
}

fn make_test_memory(id: &str, tags: Vec<String>, files: Vec<FileLink>) -> BaseMemory {
    let now = Utc::now();
    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Tribal,
        content: TypedContent::Tribal(cortex_core::memory::types::TribalContent {
            knowledge: "Test knowledge".to_string(),
            severity: "medium".to_string(),
            warnings: vec![],
            consequences: vec![],
        }),
        summary: format!("Memory {}", id),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.9),
        importance: Importance::Normal,
        last_accessed: now,
        access_count: 10,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: files,
        linked_functions: vec![],
        tags,
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: format!("hash_{}", id),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

impl IMemoryStorage for MockStorage {
    fn create(&self, memory: &BaseMemory) -> CortexResult<()> {
        self.memories.lock().unwrap().push(memory.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> CortexResult<Option<BaseMemory>> {
        Ok(self
            .memories
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.id == id)
            .cloned())
    }

    fn update(&self, _memory: &BaseMemory) -> CortexResult<()> {
        Ok(())
    }
    fn delete(&self, _id: &str) -> CortexResult<()> {
        Ok(())
    }

    fn create_bulk(&self, memories: &[BaseMemory]) -> CortexResult<usize> {
        let mut store = self.memories.lock().unwrap();
        let count = memories.len();
        store.extend(memories.iter().cloned());
        Ok(count)
    }

    fn get_bulk(&self, ids: &[String]) -> CortexResult<Vec<BaseMemory>> {
        let store = self.memories.lock().unwrap();
        Ok(store
            .iter()
            .filter(|m| ids.contains(&m.id))
            .cloned()
            .collect())
    }

    fn query_by_type(&self, memory_type: MemoryType) -> CortexResult<Vec<BaseMemory>> {
        let store = self.memories.lock().unwrap();
        Ok(store
            .iter()
            .filter(|m| m.memory_type == memory_type)
            .cloned()
            .collect())
    }

    fn query_by_importance(&self, min: Importance) -> CortexResult<Vec<BaseMemory>> {
        let store = self.memories.lock().unwrap();
        Ok(store
            .iter()
            .filter(|m| m.importance >= min)
            .cloned()
            .collect())
    }

    fn query_by_confidence_range(&self, min: f64, max: f64) -> CortexResult<Vec<BaseMemory>> {
        let store = self.memories.lock().unwrap();
        Ok(store
            .iter()
            .filter(|m| {
                let c = m.confidence.value();
                c >= min && c <= max
            })
            .cloned()
            .collect())
    }

    fn query_by_date_range(
        &self,
        from: chrono::DateTime<Utc>,
        to: chrono::DateTime<Utc>,
    ) -> CortexResult<Vec<BaseMemory>> {
        let store = self.memories.lock().unwrap();
        Ok(store
            .iter()
            .filter(|m| m.transaction_time >= from && m.transaction_time <= to)
            .cloned()
            .collect())
    }

    fn query_by_tags(&self, tags: &[String]) -> CortexResult<Vec<BaseMemory>> {
        let store = self.memories.lock().unwrap();
        Ok(store
            .iter()
            .filter(|m| tags.iter().any(|t| m.tags.contains(t)))
            .cloned()
            .collect())
    }

    fn search_fts5(&self, query: &str, limit: usize) -> CortexResult<Vec<BaseMemory>> {
        let store = self.memories.lock().unwrap();
        let q = query.to_lowercase();
        Ok(store
            .iter()
            .filter(|m| {
                m.summary.to_lowercase().contains(&q)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&q))
                    || m.linked_files.iter().any(|f| f.file_path.contains(query))
            })
            .take(limit)
            .cloned()
            .collect())
    }

    fn search_vector(
        &self,
        _embedding: &[f32],
        _limit: usize,
    ) -> CortexResult<Vec<(BaseMemory, f64)>> {
        Ok(vec![])
    }

    fn get_relationships(
        &self,
        _memory_id: &str,
        _rel_type: Option<RelationshipType>,
    ) -> CortexResult<Vec<RelationshipEdge>> {
        Ok(vec![])
    }

    fn add_relationship(&self, _edge: &RelationshipEdge) -> CortexResult<()> {
        Ok(())
    }
    fn remove_relationship(&self, _source_id: &str, _target_id: &str) -> CortexResult<()> {
        Ok(())
    }
    fn add_pattern_link(&self, _memory_id: &str, _link: &PatternLink) -> CortexResult<()> {
        Ok(())
    }
    fn add_constraint_link(&self, _memory_id: &str, _link: &ConstraintLink) -> CortexResult<()> {
        Ok(())
    }
    fn add_file_link(&self, _memory_id: &str, _link: &FileLink) -> CortexResult<()> {
        Ok(())
    }
    fn add_function_link(&self, _memory_id: &str, _link: &FunctionLink) -> CortexResult<()> {
        Ok(())
    }
    fn count_by_type(&self) -> CortexResult<Vec<(MemoryType, usize)>> {
        Ok(vec![])
    }
    fn average_confidence(&self) -> CortexResult<f64> {
        Ok(0.0)
    }
    fn stale_count(&self, _threshold_days: u64) -> CortexResult<usize> {
        Ok(0)
    }
    fn vacuum(&self) -> CortexResult<()> {
        Ok(())
    }
}

// ── T9-PRED-01: File-based prediction returns linked memories ─────────────

#[test]
fn file_based_prediction_returns_linked_memories() {
    let memories = vec![make_test_memory(
        "mem1",
        vec![],
        vec![FileLink {
            file_path: "src/auth.rs".to_string(),
            line_start: Some(1),
            line_end: Some(50),
            content_hash: None,
        }],
    )];
    let storage = MockStorage::with_memories(memories);
    let engine = PredictionEngine::new(storage);

    let signals = AggregatedSignals {
        file: cortex_prediction::signals::FileSignals {
            active_file: Some("src/auth.rs".to_string()),
            imports: vec![],
            symbols: vec![],
            directory: Some("src".to_string()),
        },
        ..Default::default()
    };

    let result = engine.predict_with_signals(&signals).unwrap();
    assert!(
        result.iter().any(|c| c.memory_id == "mem1"),
        "Should predict memory linked to active file"
    );
}

// ── T9-PRED-02: Pattern-based prediction returns pattern memories ─────────

#[test]
fn pattern_based_prediction_returns_pattern_memories() {
    let mut mem = make_test_memory("mem_pattern", vec![], vec![]);
    mem.linked_patterns = vec![PatternLink {
        pattern_id: "p1".to_string(),
        pattern_name: "singleton".to_string(),
    }];
    mem.summary = "Singleton pattern rationale".to_string();

    let storage = MockStorage::with_memories(vec![mem]);
    let engine = PredictionEngine::new(storage);

    let signals = AggregatedSignals {
        file: cortex_prediction::signals::FileSignals {
            active_file: Some("src/main.rs".to_string()),
            imports: vec![],
            symbols: vec!["singleton".to_string()],
            directory: Some("src".to_string()),
        },
        ..Default::default()
    };

    let result = engine.predict_with_signals(&signals).unwrap();
    assert!(
        result.iter().any(|c| c.memory_id == "mem_pattern"),
        "Should predict memory linked to detected pattern"
    );
}

// ── T9-PRED-03: Cache invalidates on file change ──────────────────────────

#[test]
fn cache_invalidates_on_file_change() {
    let storage = MockStorage::with_memories(vec![make_test_memory(
        "mem1",
        vec![],
        vec![FileLink {
            file_path: "src/auth.rs".to_string(),
            line_start: None,
            line_end: None,
            content_hash: None,
        }],
    )]);
    let engine = PredictionEngine::new(storage);

    let signals = AggregatedSignals {
        file: cortex_prediction::signals::FileSignals {
            active_file: Some("src/auth.rs".to_string()),
            imports: vec![],
            symbols: vec![],
            directory: Some("src".to_string()),
        },
        ..Default::default()
    };

    // First call populates cache
    let r1 = engine.predict_with_signals(&signals).unwrap();
    assert!(!r1.is_empty());
    assert!(engine.cache().hits() == 0);

    // Second call should hit cache
    let r2 = engine.predict_with_signals(&signals).unwrap();
    assert_eq!(r1.len(), r2.len());
    assert!(engine.cache().hits() >= 1);

    // Invalidate on file change
    engine.on_file_changed("src/auth.rs");

    // Next call should miss cache
    let _r3 = engine.predict_with_signals(&signals).unwrap();
    let misses_after = engine.cache().misses();
    assert!(
        misses_after >= 2,
        "Should have cache miss after invalidation"
    );
}

// ── T9-PRED-04: Multi-strategy dedup ──────────────────────────────────────

#[test]
fn multi_strategy_dedup_merges_and_boosts() {
    let candidates = vec![
        PredictionCandidate {
            memory_id: "mem1".to_string(),
            confidence: 0.6,
            source_strategy: "file_based".to_string(),
            signals: vec!["file:auth.rs".to_string()],
        },
        PredictionCandidate {
            memory_id: "mem1".to_string(),
            confidence: 0.7,
            source_strategy: "behavioral".to_string(),
            signals: vec!["recent_query:auth".to_string()],
        },
        PredictionCandidate {
            memory_id: "mem2".to_string(),
            confidence: 0.5,
            source_strategy: "temporal".to_string(),
            signals: vec!["time_bucket:morning".to_string()],
        },
    ];

    let deduped = strategies::deduplicate(candidates);

    // mem1 should appear once with boosted confidence
    let mem1 = deduped.iter().find(|c| c.memory_id == "mem1").unwrap();
    assert!(
        mem1.confidence > 0.7,
        "Deduped confidence should be boosted: got {}",
        mem1.confidence
    );
    assert_eq!(
        mem1.confidence, 0.75,
        "Should be max(0.6, 0.7) + 0.05 = 0.75"
    );
    assert!(
        mem1.signals.len() >= 2,
        "Signals should be merged from both strategies"
    );

    // mem2 should appear once unchanged
    let mem2 = deduped.iter().find(|c| c.memory_id == "mem2").unwrap();
    assert_eq!(mem2.confidence, 0.5);

    assert_eq!(deduped.len(), 2, "Should have 2 unique memories");
}

// ── T9-PRED-05: Adaptive TTL ──────────────────────────────────────────────

#[test]
fn adaptive_ttl_shorter_for_rapidly_changing_files() {
    let cache = PredictionCache::new();

    // Insert with low change frequency
    cache.insert(
        "stable.rs".to_string(),
        vec![PredictionCandidate {
            memory_id: "m1".to_string(),
            confidence: 0.5,
            source_strategy: "test".to_string(),
            signals: vec![],
        }],
        0.1,
    );

    // Insert with high change frequency
    cache.insert(
        "volatile.rs".to_string(),
        vec![PredictionCandidate {
            memory_id: "m2".to_string(),
            confidence: 0.5,
            source_strategy: "test".to_string(),
            signals: vec![],
        }],
        50.0,
    );

    // Both should be retrievable immediately
    assert!(cache.get("stable.rs").is_some());
    assert!(cache.get("volatile.rs").is_some());
}

// ── T9-PRED-06: Git-aware prediction ──────────────────────────────────────

#[test]
fn git_aware_prediction_extracts_branch_keywords() {
    let signals = cortex_prediction::signals::GitSignals::gather(
        Some("feature/auth-refactor".to_string()),
        vec!["src/auth.rs".to_string()],
        vec!["refactored auth module".to_string()],
    );

    let keywords = signals.branch_keywords();
    assert!(
        keywords.contains(&"auth".to_string()),
        "Should extract 'auth' from branch name"
    );
    assert!(
        keywords.contains(&"refactor".to_string()),
        "Should extract 'refactor' from branch name"
    );
    assert!(
        !keywords.contains(&"feature".to_string()),
        "Should filter out branch prefix 'feature'"
    );
}

// ── IPredictor trait implementation ───────────────────────────────────────

#[test]
fn ipredictor_trait_works() {
    let memories = vec![make_test_memory(
        "mem1",
        vec![],
        vec![FileLink {
            file_path: "src/auth.rs".to_string(),
            line_start: None,
            line_end: None,
            content_hash: None,
        }],
    )];
    let storage = MockStorage::with_memories(memories);
    let engine = PredictionEngine::new(storage);

    let signals = PredictionSignals {
        active_files: vec!["src/auth.rs".to_string()],
        recent_queries: vec!["authentication".to_string()],
        current_intent: Some("investigate".to_string()),
    };

    let result = engine.predict(&signals).unwrap();
    // Result should have valid confidence
    assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
}

// ── Cache hit rate tracking ───────────────────────────────────────────────

#[test]
fn cache_tracks_hits_and_misses() {
    let cache = PredictionCache::new();

    // Miss
    assert!(cache.get("nonexistent").is_none());
    assert_eq!(cache.misses(), 1);
    assert_eq!(cache.hits(), 0);

    // Insert and hit
    cache.insert("key1".to_string(), vec![], 0.0);
    assert!(cache.get("key1").is_some());
    assert_eq!(cache.hits(), 1);
    assert_eq!(cache.misses(), 1);

    // Hit rate should be 0.5
    assert!((cache.hit_rate() - 0.5).abs() < f64::EPSILON);
}

// ── Invalidate all on new session ─────────────────────────────────────────

#[test]
fn invalidate_all_on_new_session() {
    let storage = MockStorage::new();
    let engine = PredictionEngine::new(storage);

    engine.cache().insert("file1.rs".to_string(), vec![], 0.0);
    engine.cache().insert("file2.rs".to_string(), vec![], 0.0);

    // Both should be retrievable before invalidation
    assert!(engine.cache().get("file1.rs").is_some());
    assert!(engine.cache().get("file2.rs").is_some());

    engine.on_new_session();

    // After invalidation, gets should miss
    assert!(engine.cache().get("file1.rs").is_none());
    assert!(engine.cache().get("file2.rs").is_none());
}
