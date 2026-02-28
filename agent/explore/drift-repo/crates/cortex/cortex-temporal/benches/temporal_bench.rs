//! Temporal benchmarks — Phase A baselines (TTA-22 through TTA-27).

use criterion::{criterion_group, criterion_main, Criterion};

use chrono::Utc;
use cortex_core::memory::*;
use cortex_core::models::*;
use cortex_storage::pool::{ReadPool, WriteConnection};
use std::sync::Arc;

fn setup() -> (Arc<WriteConnection>, Arc<ReadPool>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bench_temporal.db");
    let _dir = Box::leak(Box::new(dir));
    let writer = Arc::new(WriteConnection::open(&db_path).unwrap());
    let readers = Arc::new(ReadPool::open(&db_path, 2).unwrap());
    writer
        .with_conn_sync(|conn| {
            cortex_storage::migrations::run_migrations(conn)?;
            Ok(())
        })
        .unwrap();
    (writer, readers)
}

fn make_event(memory_id: &str, et: MemoryEventType) -> MemoryEvent {
    MemoryEvent {
        event_id: 0,
        memory_id: memory_id.to_string(),
        recorded_at: Utc::now(),
        event_type: et,
        delta: serde_json::json!({}),
        actor: EventActor::System("bench".to_string()),
        caused_by: vec![],
        schema_version: 1,
    }
}

fn make_memory() -> BaseMemory {
    let content = TypedContent::Episodic(cortex_core::memory::types::EpisodicContent {
        interaction: "bench interaction".to_string(),
        context: "bench context".to_string(),
        outcome: None,
    });
    BaseMemory {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type: MemoryType::Episodic,
        content: content.clone(),
        summary: "bench summary".to_string(),
        transaction_time: Utc::now(),
        valid_time: Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Normal,
        last_accessed: Utc::now(),
        access_count: 0,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec!["bench".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

// TTA-22: event append (single) < 0.1ms
fn bench_event_append_single(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, _) = setup();

    c.bench_function("event_append_single", |b| {
        b.iter(|| {
            let e = make_event("bench-single", MemoryEventType::ContentUpdated);
            rt.block_on(cortex_temporal::event_store::append::append(&writer, &e))
                .unwrap();
        });
    });
}

// TTA-23: event append (batch of 100) < 5ms
fn bench_event_append_batch(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, _) = setup();

    c.bench_function("event_append_batch_100", |b| {
        b.iter(|| {
            let events: Vec<_> = (0..100)
                .map(|_| make_event("bench-batch", MemoryEventType::ContentUpdated))
                .collect();
            rt.block_on(cortex_temporal::event_store::append::append_batch(
                &writer, &events,
            ))
            .unwrap();
        });
    });
}

// TTA-24: reconstruction 50 events < 5ms
fn bench_reconstruction_50_events(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, readers) = setup();
    let mem = make_memory();
    let mid = "bench-recon-50";

    // Setup: insert 50 events
    rt.block_on(async {
        let mut created = make_event(mid, MemoryEventType::Created);
        created.delta = serde_json::to_value(&mem).unwrap();
        cortex_temporal::event_store::append::append(&writer, &created)
            .await
            .unwrap();
        for _ in 0..49 {
            let mut e = make_event(mid, MemoryEventType::ConfidenceChanged);
            e.delta = serde_json::json!({ "old": 0.8, "new": 0.7 });
            cortex_temporal::event_store::append::append(&writer, &e)
                .await
                .unwrap();
        }
    });

    c.bench_function("reconstruction_50_events", |b| {
        b.iter(|| {
            cortex_temporal::snapshot::reconstruct::reconstruct_at(
                &readers,
                mid,
                Utc::now() + chrono::Duration::seconds(1),
            )
            .unwrap();
        });
    });
}

// TTA-25: reconstruction snapshot + 10 events < 1ms
fn bench_reconstruction_snapshot_plus_10(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, readers) = setup();
    let mem = make_memory();
    let mid = "bench-snap-10";

    rt.block_on(async {
        let mut created = make_event(mid, MemoryEventType::Created);
        created.delta = serde_json::to_value(&mem).unwrap();
        cortex_temporal::event_store::append::append(&writer, &created)
            .await
            .unwrap();

        // Create snapshot
        cortex_temporal::snapshot::create::create_snapshot(
            &writer,
            mid,
            &mem,
            SnapshotReason::EventThreshold,
        )
        .await
        .unwrap();

        // 10 more events after snapshot
        for _ in 0..10 {
            let mut e = make_event(mid, MemoryEventType::ConfidenceChanged);
            e.delta = serde_json::json!({ "old": 0.8, "new": 0.7 });
            cortex_temporal::event_store::append::append(&writer, &e)
                .await
                .unwrap();
        }
    });

    c.bench_function("reconstruction_snapshot_plus_10", |b| {
        b.iter(|| {
            cortex_temporal::snapshot::reconstruct::reconstruct_at(
                &readers,
                mid,
                Utc::now() + chrono::Duration::seconds(1),
            )
            .unwrap();
        });
    });
}

// TTA-26: snapshot creation (single memory) < 2ms
fn bench_snapshot_creation_single(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, _) = setup();
    let mem = make_memory();

    c.bench_function("snapshot_creation_single", |b| {
        b.iter(|| {
            rt.block_on(cortex_temporal::snapshot::create::create_snapshot(
                &writer,
                &mem.id,
                &mem,
                SnapshotReason::OnDemand,
            ))
            .unwrap();
        });
    });
}

// TTA-27: snapshot batch creation (100 memories) < 200ms
fn bench_snapshot_batch_100(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, _) = setup();

    let memories: Vec<(String, BaseMemory)> = (0..100)
        .map(|_| {
            let m = make_memory();
            (m.id.clone(), m)
        })
        .collect();

    c.bench_function("snapshot_batch_100", |b| {
        b.iter(|| {
            rt.block_on(cortex_temporal::snapshot::create::create_batch_snapshots(
                &writer,
                &memories,
                SnapshotReason::Periodic,
            ))
            .unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_event_append_single,
    bench_event_append_batch,
    bench_reconstruction_50_events,
    bench_reconstruction_snapshot_plus_10,
    bench_snapshot_creation_single,
    bench_snapshot_batch_100,
);
criterion_main!(benches, phase_b_benches, phase_c_benches, phase_d1_benches);

// ── Phase B Benchmarks ───────────────────────────────────────────────────

fn insert_memory_with_times(
    conn: &rusqlite::Connection,
    id: &str,
    memory_type: &str,
    transaction_time: &str,
    valid_time: &str,
    valid_until: Option<&str>,
    confidence: f64,
) {
    let valid_until_val = valid_until.unwrap_or("");
    conn.execute(
        "INSERT OR REPLACE INTO memories \
         (id, memory_type, content, summary, transaction_time, valid_time, valid_until, \
          confidence, importance, last_accessed, access_count, archived, content_hash, tags) \
         VALUES (?1, ?2, '{}', 'bench summary', ?3, ?4, \
                 CASE WHEN ?5 = '' THEN NULL ELSE ?5 END, \
                 ?6, 'normal', ?3, 0, 0, 'hash', '[]')",
        rusqlite::params![
            id,
            memory_type,
            transaction_time,
            valid_time,
            valid_until_val,
            confidence
        ],
    )
    .unwrap();
}

// TTB-27: point-in-time single memory < 5ms cold, < 1ms warm
fn bench_as_of_single_memory(c: &mut Criterion) {
    let (_writer, readers) = setup();
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    readers
        .with_conn(|conn| {
            insert_memory_with_times(
                conn,
                "bench-single",
                "episodic",
                &now_str,
                &now_str,
                None,
                0.8,
            );
            Ok::<_, cortex_core::errors::CortexError>(())
        })
        .unwrap();

    let query = cortex_core::models::AsOfQuery {
        system_time: now + chrono::Duration::seconds(1),
        valid_time: now + chrono::Duration::seconds(1),
        filter: None,
    };

    c.bench_function("as_of_single_memory", |b| {
        b.iter(|| {
            readers
                .with_conn(|conn| cortex_temporal::query::as_of::execute_as_of(conn, &query))
                .unwrap();
        });
    });
}

// TTB-28: point-in-time all 10K memories < 500ms cold, < 50ms warm
fn bench_as_of_10k_memories(c: &mut Criterion) {
    let (_writer, readers) = setup();
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Insert 10K memories
    readers
        .with_conn(|conn| {
            for i in 0..10_000 {
                insert_memory_with_times(
                    conn,
                    &format!("bench-10k-{}", i),
                    "episodic",
                    &now_str,
                    &now_str,
                    None,
                    0.8,
                );
            }
            Ok::<_, cortex_core::errors::CortexError>(())
        })
        .unwrap();

    let query = cortex_core::models::AsOfQuery {
        system_time: now + chrono::Duration::seconds(1),
        valid_time: now + chrono::Duration::seconds(1),
        filter: None,
    };

    c.bench_function("as_of_10k_memories", |b| {
        b.iter(|| {
            readers
                .with_conn(|conn| cortex_temporal::query::as_of::execute_as_of(conn, &query))
                .unwrap();
        });
    });
}

// TTB-29: temporal diff < 1s cold, < 100ms warm
fn bench_temporal_diff(c: &mut Criterion) {
    let (_writer, readers) = setup();
    let t1 = chrono::Utc::now() - chrono::Duration::hours(2);
    let t2 = chrono::Utc::now();
    let t1_str = t1.to_rfc3339();
    let mid_str = (t1 + chrono::Duration::hours(1)).to_rfc3339();

    // Insert memories at different times
    readers
        .with_conn(|conn| {
            for i in 0..100 {
                insert_memory_with_times(
                    conn,
                    &format!("bench-diff-a-{}", i),
                    "episodic",
                    &t1_str,
                    &t1_str,
                    None,
                    0.8,
                );
            }
            for i in 0..50 {
                insert_memory_with_times(
                    conn,
                    &format!("bench-diff-b-{}", i),
                    "semantic",
                    &mid_str,
                    &mid_str,
                    None,
                    0.9,
                );
            }
            Ok::<_, cortex_core::errors::CortexError>(())
        })
        .unwrap();

    let query = cortex_core::models::TemporalDiffQuery {
        time_a: t1,
        time_b: t2,
        scope: cortex_core::models::DiffScope::All,
    };

    c.bench_function("temporal_diff_150_memories", |b| {
        b.iter(|| {
            readers
                .with_conn(|conn| cortex_temporal::query::diff::execute_diff(conn, &query))
                .unwrap();
        });
    });
}

// TTB-30: range query Overlaps < 50ms
fn bench_range_overlaps(c: &mut Criterion) {
    let (_writer, readers) = setup();
    let now = chrono::Utc::now();
    let past = (now - chrono::Duration::days(30)).to_rfc3339();

    // Insert memories with various validity ranges
    readers
        .with_conn(|conn| {
            for i in 0..1000 {
                let valid_from = (now - chrono::Duration::days(i % 60)).to_rfc3339();
                let valid_until = if i % 3 == 0 {
                    Some((now + chrono::Duration::days(30)).to_rfc3339())
                } else {
                    None
                };
                insert_memory_with_times(
                    conn,
                    &format!("bench-range-{}", i),
                    "episodic",
                    &past,
                    &valid_from,
                    valid_until.as_deref(),
                    0.8,
                );
            }
            Ok::<_, cortex_core::errors::CortexError>(())
        })
        .unwrap();

    let query = cortex_core::models::TemporalRangeQuery {
        from: now - chrono::Duration::days(15),
        to: now + chrono::Duration::days(15),
        mode: cortex_core::models::TemporalRangeMode::Overlaps,
    };

    c.bench_function("range_overlaps_1k_memories", |b| {
        b.iter(|| {
            readers
                .with_conn(|conn| cortex_temporal::query::range::execute_range(conn, &query))
                .unwrap();
        });
    });
}

criterion_group!(
    phase_b_benches,
    bench_as_of_single_memory,
    bench_as_of_10k_memories,
    bench_temporal_diff,
    bench_range_overlaps,
);

// ── Phase C Benchmarks ───────────────────────────────────────────────────

// TTC-14: decision replay < 200ms warm
fn bench_decision_replay(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, readers) = setup();
    let now = chrono::Utc::now();

    // Setup: insert a decision memory + some context
    rt.block_on(async {
        let decision_content = cortex_core::memory::TypedContent::Decision(
            cortex_core::memory::types::DecisionContent {
                decision: "Use PostgreSQL for auth".to_string(),
                rationale: "Better ACID compliance".to_string(),
                alternatives: vec![],
            },
        );
        let decision = BaseMemory {
            id: "bench-decision".to_string(),
            memory_type: MemoryType::Decision,
            content: decision_content.clone(),
            summary: "Use PostgreSQL for auth: Better ACID compliance".to_string(),
            transaction_time: now,
            valid_time: now,
            valid_until: None,
            confidence: Confidence::new(0.9),
            importance: Importance::High,
            last_accessed: now,
            access_count: 0,
            linked_patterns: vec![],
            linked_constraints: vec![],
            linked_files: vec![],
            linked_functions: vec![],
            tags: vec!["auth".into(), "database".into()],
            archived: false,
            superseded_by: None,
            supersedes: None,
            content_hash: BaseMemory::compute_content_hash(&decision_content).unwrap(),
            namespace: Default::default(),
            source_agent: Default::default(),
        };
        writer
            .with_conn(move |conn| {
                cortex_storage::queries::memory_crud::insert_memory(conn, &decision)
            })
            .await
            .unwrap();

        // Insert created event
        let mut e = make_event("bench-decision", MemoryEventType::Created);
        e.recorded_at = now;
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();

        // Insert some context memories
        for i in 0..20 {
            let mem = make_memory();
            let mid = format!("bench-ctx-{}", i);
            let mut m = BaseMemory {
                id: mid.clone(),
                ..mem
            };
            m.transaction_time = now - chrono::Duration::hours(1);
            writer
                .with_conn(move |conn| {
                    cortex_storage::queries::memory_crud::insert_memory(conn, &m)
                })
                .await
                .unwrap();

            let mut e = make_event(&mid, MemoryEventType::Created);
            e.recorded_at = now - chrono::Duration::hours(1);
            cortex_temporal::event_store::append::append(&writer, &e)
                .await
                .unwrap();
        }
    });

    let query = cortex_core::models::DecisionReplayQuery {
        decision_memory_id: "bench-decision".to_string(),
        budget_override: Some(2000),
    };

    c.bench_function("decision_replay", |b| {
        b.iter(|| {
            cortex_temporal::query::replay::execute_replay(&readers, &query).unwrap();
        });
    });
}

// TTC-15: temporal causal traversal < 20ms warm
fn bench_temporal_causal_traversal(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (writer, readers) = setup();
    let now = chrono::Utc::now();

    // Setup: insert relationship events forming a chain
    rt.block_on(async {
        for i in 0..50 {
            let mid = format!("bench-tc-{}", i);
            let mem = BaseMemory {
                id: mid.clone(),
                ..make_memory()
            };
            writer
                .with_conn(move |conn| {
                    cortex_storage::queries::memory_crud::insert_memory(conn, &mem)
                })
                .await
                .unwrap();

            if i > 0 {
                let prev = format!("bench-tc-{}", i - 1);
                let mut e = make_event(&mid, MemoryEventType::RelationshipAdded);
                e.delta = serde_json::json!({
                    "source": prev,
                    "target": mid,
                    "relation_type": "caused",
                    "strength": 0.8
                });
                cortex_temporal::event_store::append::append(&writer, &e)
                    .await
                    .unwrap();
            }
        }
    });

    let query = cortex_core::models::TemporalCausalQuery {
        memory_id: "bench-tc-0".to_string(),
        as_of: now + chrono::Duration::seconds(1),
        direction: cortex_core::models::TraversalDirection::Forward,
        max_depth: 10,
    };

    c.bench_function("temporal_causal_traversal", |b| {
        b.iter(|| {
            cortex_temporal::query::temporal_causal::execute_temporal_causal(&readers, &query)
                .unwrap();
        });
    });
}

// TTC-16: graph reconstruction 1K edges < 10ms cold, < 2ms warm
fn bench_graph_reconstruction_1k(c: &mut Criterion) {
    use cortex_causal::graph::temporal_graph;
    use cortex_core::models::{EventActor, MemoryEvent, MemoryEventType};

    let now = chrono::Utc::now();

    // Build 1K relationship events
    let events: Vec<MemoryEvent> = (0..1000)
        .map(|i| MemoryEvent {
            event_id: i as u64,
            memory_id: "graph".to_string(),
            recorded_at: now - chrono::Duration::seconds(1000 - i),
            event_type: MemoryEventType::RelationshipAdded,
            delta: serde_json::json!({
                "source": format!("node-{}", i),
                "target": format!("node-{}", i + 1),
                "relation_type": "caused",
                "strength": 0.8
            }),
            actor: EventActor::System("bench".to_string()),
            caused_by: vec![],
            schema_version: 1,
        })
        .collect();

    c.bench_function("graph_reconstruction_1k_edges", |b| {
        b.iter(|| {
            temporal_graph::reconstruct_graph_at(&events, now + chrono::Duration::seconds(1));
        });
    });
}

criterion_group!(
    phase_c_benches,
    bench_decision_replay,
    bench_temporal_causal_traversal,
    bench_graph_reconstruction_1k,
);

// ── Phase D1 Benchmarks ──────────────────────────────────────────────────

// TTD1-18: KSI computation 10K memories < 100ms
fn bench_ksi_10k_memories(c: &mut Criterion) {
    let (_writer, readers) = setup();
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Insert 10K memories
    readers
        .with_conn(|conn| {
            for i in 0..10_000 {
                insert_memory_with_times(
                    conn,
                    &format!("bench-ksi-{}", i),
                    "episodic",
                    &now_str,
                    &now_str,
                    None,
                    0.8,
                );
            }
            Ok::<_, cortex_core::errors::CortexError>(())
        })
        .unwrap();

    let window_start = now - chrono::Duration::hours(168);

    c.bench_function("ksi_10k_memories", |b| {
        b.iter(|| {
            cortex_temporal::drift::compute_ksi(&readers, None, window_start, now).unwrap();
        });
    });
}

// TTD1-19: full drift metrics 10K memories < 500ms
fn bench_drift_metrics_10k(c: &mut Criterion) {
    let (_writer, readers) = setup();
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    readers
        .with_conn(|conn| {
            for i in 0..10_000 {
                insert_memory_with_times(
                    conn,
                    &format!("bench-drift-{}", i),
                    "episodic",
                    &now_str,
                    &now_str,
                    None,
                    0.8,
                );
            }
            Ok::<_, cortex_core::errors::CortexError>(())
        })
        .unwrap();

    let window_start = now - chrono::Duration::hours(168);

    c.bench_function("drift_metrics_10k_memories", |b| {
        b.iter(|| {
            cortex_temporal::drift::metrics::compute_all_metrics(&readers, window_start, now)
                .unwrap();
        });
    });
}

// TTD1-20: evidence freshness single memory < 1ms
fn bench_evidence_freshness_single(c: &mut Criterion) {
    c.bench_function("evidence_freshness_single", |b| {
        let factors = vec![1.0, 0.8, 0.9, 0.7, 1.0];
        b.iter(|| {
            cortex_temporal::drift::evidence_freshness::compute_evidence_freshness(&factors);
        });
    });
}

// TTD1-21: alert evaluation (100 metrics) < 10ms
fn bench_alert_evaluation(c: &mut Criterion) {
    use std::collections::HashMap;

    let config = cortex_core::config::TemporalConfig::default();

    // Build a snapshot with metrics for multiple types
    let mut type_metrics = HashMap::new();
    for mt in cortex_core::memory::MemoryType::ALL.iter().take(10) {
        type_metrics.insert(
            *mt,
            cortex_core::models::TypeDriftMetrics {
                count: 100,
                avg_confidence: 0.7,
                ksi: 0.5,
                contradiction_density: 0.05,
                consolidation_efficiency: 0.4,
                evidence_freshness_index: 0.6,
            },
        );
    }

    let snapshot = cortex_core::models::DriftSnapshot {
        timestamp: chrono::Utc::now(),
        window_hours: 168,
        type_metrics,
        module_metrics: HashMap::new(),
        global: cortex_core::models::GlobalDriftMetrics {
            total_memories: 1000,
            active_memories: 800,
            archived_memories: 200,
            avg_confidence: 0.7,
            overall_ksi: 0.5,
            overall_contradiction_density: 0.05,
            overall_evidence_freshness: 0.6,
        },
    };

    c.bench_function("alert_evaluation_10_types", |b| {
        b.iter(|| {
            cortex_temporal::drift::alerting::evaluate_drift_alerts(&snapshot, &config, &[]);
        });
    });
}

criterion_group!(
    phase_d1_benches,
    bench_ksi_10k_memories,
    bench_drift_metrics_10k,
    bench_evidence_freshness_single,
    bench_alert_evaluation,
);
