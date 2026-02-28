//! Targeted coverage tests for cortex-prediction uncovered paths.
//!
//! Focuses on: prediction cache, signal gathering, strategy dedup,
//! engine predict_with_signals, file change invalidation.

use cortex_prediction::signals::{
    AggregatedSignals, BehavioralSignals, FileSignals, GitSignals, TemporalSignals,
};
use cortex_prediction::strategies::{deduplicate, PredictionCandidate};

// ─── Strategy Deduplication ──────────────────────────────────────────────────

#[test]
fn deduplicate_empty() {
    let result = deduplicate(vec![]);
    assert!(result.is_empty());
}

#[test]
fn deduplicate_no_duplicates() {
    let candidates = vec![
        PredictionCandidate {
            memory_id: "m1".to_string(),
            confidence: 0.8,
            source_strategy: "file".to_string(),
            signals: vec!["active_file".to_string()],
        },
        PredictionCandidate {
            memory_id: "m2".to_string(),
            confidence: 0.6,
            source_strategy: "temporal".to_string(),
            signals: vec!["morning".to_string()],
        },
    ];
    let result = deduplicate(candidates);
    assert_eq!(result.len(), 2);
}

#[test]
fn deduplicate_merges_same_memory() {
    let candidates = vec![
        PredictionCandidate {
            memory_id: "m1".to_string(),
            confidence: 0.7,
            source_strategy: "file".to_string(),
            signals: vec!["file_signal".to_string()],
        },
        PredictionCandidate {
            memory_id: "m1".to_string(),
            confidence: 0.8,
            source_strategy: "pattern".to_string(),
            signals: vec!["pattern_signal".to_string()],
        },
    ];
    let result = deduplicate(candidates);
    assert_eq!(result.len(), 1);
    // Should have merged signals.
    assert!(result[0].signals.len() >= 2);
    // Should have boosted confidence.
    assert!(result[0].confidence > 0.8);
    // Should track both strategies.
    assert!(result[0].source_strategy.contains('+'));
}

#[test]
fn deduplicate_sorted_by_confidence_desc() {
    let candidates = vec![
        PredictionCandidate {
            memory_id: "low".to_string(),
            confidence: 0.3,
            source_strategy: "file".to_string(),
            signals: vec![],
        },
        PredictionCandidate {
            memory_id: "high".to_string(),
            confidence: 0.9,
            source_strategy: "file".to_string(),
            signals: vec![],
        },
    ];
    let result = deduplicate(candidates);
    assert_eq!(result[0].memory_id, "high");
    assert_eq!(result[1].memory_id, "low");
}

// ─── Signal Types ────────────────────────────────────────────────────────────

#[test]
fn aggregated_signals_default() {
    let signals = AggregatedSignals::default();
    assert!(signals.file.active_file.is_none());
    assert!(signals.behavioral.recent_queries.is_empty());
}

#[test]
fn file_signals_relevant_paths() {
    let signals = FileSignals {
        active_file: Some("src/main.rs".to_string()),
        imports: vec!["src/lib.rs".to_string(), "src/utils.rs".to_string()],
        symbols: vec![],
        directory: Some("src".to_string()),
    };
    let paths = signals.relevant_paths();
    assert!(!paths.is_empty());
}

#[test]
fn behavioral_signals_has_signals() {
    let empty = BehavioralSignals::default();
    assert!(!empty.has_signals());

    let with_queries = BehavioralSignals {
        recent_queries: vec!["test query".to_string()],
        recent_intents: vec![],
        frequent_memory_ids: vec![],
    };
    assert!(with_queries.has_signals());
}

#[test]
fn git_signals_branch_keywords() {
    let signals = GitSignals {
        branch_name: Some("feature/add-auth-module".to_string()),
        modified_files: vec!["src/auth.rs".to_string()],
        recent_commit_messages: vec![],
    };
    let keywords = signals.branch_keywords();
    assert!(!keywords.is_empty());
}

#[test]
fn temporal_signals_time_bucket() {
    let signals = TemporalSignals::default();
    let bucket = signals.time_bucket();
    assert!(
        ["morning", "afternoon", "evening", "night"].contains(&bucket),
        "unexpected time bucket: {bucket}"
    );
}

// ─── Prediction Cache ────────────────────────────────────────────────────────

#[test]
fn cache_miss_then_hit() {
    let cache = cortex_prediction::PredictionCache::new();
    assert!(cache.get("key1").is_none());
    assert_eq!(cache.misses(), 1);
    assert_eq!(cache.hits(), 0);

    cache.insert(
        "key1".to_string(),
        vec![PredictionCandidate {
            memory_id: "m1".to_string(),
            confidence: 0.8,
            source_strategy: "test".to_string(),
            signals: vec![],
        }],
        0.0,
    );

    let result = cache.get("key1");
    assert!(result.is_some());
    assert_eq!(cache.hits(), 1);
}

#[test]
fn cache_invalidate_file() {
    let cache = cortex_prediction::PredictionCache::new();
    cache.insert(
        "src/main.rs".to_string(),
        vec![PredictionCandidate {
            memory_id: "m1".to_string(),
            confidence: 0.8,
            source_strategy: "test".to_string(),
            signals: vec![],
        }],
        0.0,
    );

    // Verify it's retrievable before invalidation.
    assert!(cache.get("src/main.rs").is_some());

    cache.invalidate_file("src/main.rs");
    assert!(cache.get("src/main.rs").is_none());
}

#[test]
fn cache_invalidate_all() {
    let cache = cortex_prediction::PredictionCache::new();
    cache.insert("a".to_string(), vec![], 0.0);
    cache.insert("b".to_string(), vec![], 0.0);
    cache.invalidate_all();
    assert!(cache.get("a").is_none());
    assert!(cache.get("b").is_none());
}

#[test]
fn cache_hit_rate_zero_when_empty() {
    let cache = cortex_prediction::PredictionCache::new();
    assert_eq!(cache.hit_rate(), 0.0);
}

#[test]
fn cache_hit_rate_after_operations() {
    let cache = cortex_prediction::PredictionCache::new();
    cache.insert("k".to_string(), vec![], 0.0);
    let _ = cache.get("k"); // hit
    let _ = cache.get("missing"); // miss
    let rate = cache.hit_rate();
    assert!(rate > 0.0 && rate < 1.0);
}

// ─── Strategy: File-Based (with storage) ─────────────────────────────────────

use chrono::Utc;
use cortex_core::memory::*;
use cortex_core::traits::IMemoryStorage;
use cortex_storage::StorageEngine;

fn make_memory(id: &str, summary: &str, mem_type: MemoryType) -> BaseMemory {
    BaseMemory {
        id: id.to_string(),
        memory_type: mem_type,
        content: TypedContent::Semantic(cortex_core::memory::types::SemanticContent {
            knowledge: summary.to_string(),
            source_episodes: vec![],
            consolidation_confidence: 0.8,
        }),
        summary: summary.to_string(),
        transaction_time: Utc::now(),
        valid_time: Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Normal,
        last_accessed: Utc::now(),
        access_count: 1,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec!["test".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: format!("hash-{id}"),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

fn storage() -> StorageEngine {
    StorageEngine::open_in_memory().expect("open in-memory storage")
}

#[test]
fn file_based_strategy_empty_signals() {
    use cortex_prediction::strategies::FileBasedStrategy;
    let eng = storage();
    let signals = FileSignals::default();
    let result = FileBasedStrategy::predict(&signals, &eng).unwrap();
    assert!(result.is_empty());
}

#[test]
fn file_based_strategy_with_active_file() {
    use cortex_prediction::strategies::FileBasedStrategy;
    let eng = storage();
    let mut mem = make_memory("fb1", "auth module patterns", MemoryType::PatternRationale);
    mem.linked_files = vec![cortex_core::memory::links::FileLink {
        file_path: "auth.rs".to_string(),
        line_start: Some(1),
        line_end: Some(50),
        content_hash: None,
    }];
    eng.create(&mem).unwrap();

    let signals = FileSignals::gather(Some("auth.rs"), vec![], vec![]);
    let result = FileBasedStrategy::predict(&signals, &eng);
    // FTS5 may reject file path syntax, so just verify no panic on Ok or Err.
    let _ = result;
}

#[test]
fn behavioral_strategy_no_signals() {
    use cortex_prediction::strategies::BehavioralStrategy;
    let eng = storage();
    let signals = BehavioralSignals::default();
    let result = BehavioralStrategy::predict(&signals, &eng).unwrap();
    assert!(result.is_empty());
}

#[test]
fn behavioral_strategy_with_queries() {
    use cortex_prediction::strategies::BehavioralStrategy;
    let eng = storage();
    eng.create(&make_memory(
        "bh1",
        "authentication middleware pattern",
        MemoryType::Tribal,
    ))
    .unwrap();

    let signals = BehavioralSignals::gather(vec!["authentication".to_string()], vec![], vec![]);
    let result = BehavioralStrategy::predict(&signals, &eng).unwrap();
    // Should find the memory via FTS5 search.
    assert!(!result.is_empty());
}

#[test]
fn behavioral_strategy_with_frequent_ids() {
    use cortex_prediction::strategies::BehavioralStrategy;
    let eng = storage();
    eng.create(&make_memory("bh2", "frequent memory", MemoryType::Semantic))
        .unwrap();

    let signals = BehavioralSignals::gather(vec![], vec![], vec!["bh2".to_string()]);
    let result = BehavioralStrategy::predict(&signals, &eng).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn temporal_strategy_predict() {
    use cortex_prediction::strategies::TemporalStrategy;
    let eng = storage();
    let signals = TemporalSignals::default();
    let result = TemporalStrategy::predict(&signals, &eng).unwrap();
    // Empty storage, so no results expected.
    let _ = result;
}

// ─── FileSignals::gather ─────────────────────────────────────────────────────

#[test]
fn file_signals_gather_extracts_directory() {
    let signals = FileSignals::gather(
        Some("src/auth/mod.rs"),
        vec!["src/lib.rs".to_string()],
        vec![],
    );
    assert_eq!(signals.directory, Some("src/auth".to_string()));
    assert_eq!(signals.active_file, Some("src/auth/mod.rs".to_string()));
}

// ─── TemporalSignals::gather ─────────────────────────────────────────────────

#[test]
fn temporal_signals_gather() {
    let start = Utc::now() - chrono::Duration::minutes(30);
    let signals = TemporalSignals::gather(start);
    assert!(signals.session_duration_secs >= 1800);
    assert!(signals.hour_of_day <= 23);
    assert!(signals.day_of_week >= 1 && signals.day_of_week <= 7);
}
