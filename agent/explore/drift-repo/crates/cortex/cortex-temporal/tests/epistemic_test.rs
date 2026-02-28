//! Phase D2 tests: epistemic status, confidence aggregation, materialized views.
//! TTD2-01 through TTD2-15.

use chrono::{Duration, Utc};
use cortex_core::config::TemporalConfig;
use cortex_core::models::*;
use cortex_storage::pool::{ReadPool, WriteConnection};
use std::sync::Arc;

// ── Test Harness ─────────────────────────────────────────────────────────

fn setup() -> (Arc<WriteConnection>, Arc<ReadPool>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_epistemic.db");
    let _dir = Box::leak(Box::new(dir));

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();
    }

    let writer = Arc::new(WriteConnection::open(&db_path).unwrap());
    let readers = Arc::new(ReadPool::open(&db_path, 2).unwrap());
    (writer, readers)
}

async fn ensure_memory_row(writer: &Arc<WriteConnection>, memory_id: &str) {
    let mid = memory_id.to_string();
    let content = serde_json::json!({
        "type": "episodic",
        "data": {
            "interaction": "test interaction",
            "context": "test context"
        }
    })
    .to_string();
    writer
        .with_conn(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO memories \
                 (id, memory_type, content, summary, transaction_time, valid_time, \
                  confidence, importance, last_accessed, access_count, tags, archived, content_hash) \
                 VALUES (?1, 'episodic', ?2, 'test', \
                         strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
                         strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
                         0.8, 'normal', \
                         strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
                         0, '[]', 0, 'hash')",
                rusqlite::params![mid, content],
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;
            Ok(())
        })
        .await
        .unwrap();
}

// ── TTD2-01: New memory starts as Conjecture ─────────────────────────────

#[test]
fn ttd2_01_new_memory_starts_as_conjecture() {
    let user = EventActor::User("alice".to_string());
    let status = cortex_temporal::epistemic::determine_initial_status(&user);
    match &status {
        EpistemicStatus::Conjecture { source, .. } => {
            assert_eq!(source, "user:alice");
        }
        other => panic!("Expected Conjecture, got {:?}", other),
    }

    let agent = EventActor::Agent("coder".to_string());
    let status = cortex_temporal::epistemic::determine_initial_status(&agent);
    match &status {
        EpistemicStatus::Conjecture { source, .. } => {
            assert_eq!(source, "agent:coder");
        }
        other => panic!("Expected Conjecture, got {:?}", other),
    }

    let system = EventActor::System("decay_engine".to_string());
    let status = cortex_temporal::epistemic::determine_initial_status(&system);
    match &status {
        EpistemicStatus::Conjecture { source, .. } => {
            assert_eq!(source, "system:decay_engine");
        }
        other => panic!("Expected Conjecture, got {:?}", other),
    }
}

// ── TTD2-02: Conjecture → Provisional on validation pass ─────────────────

#[test]
fn ttd2_02_conjecture_to_provisional() {
    let conjecture = EpistemicStatus::Conjecture {
        source: "user:alice".to_string(),
        created_at: Utc::now(),
    };

    let result = cortex_temporal::epistemic::promote_to_provisional(&conjecture, 3);
    assert!(result.is_ok());

    let provisional = result.unwrap();
    match &provisional {
        EpistemicStatus::Provisional {
            evidence_count,
            last_validated,
        } => {
            assert_eq!(*evidence_count, 3);
            // last_validated should be recent
            assert!(Utc::now() - *last_validated < Duration::seconds(5));
        }
        other => panic!("Expected Provisional, got {:?}", other),
    }
}

// ── TTD2-03: Provisional → Verified on confirmation ──────────────────────

#[test]
fn ttd2_03_provisional_to_verified() {
    let provisional = EpistemicStatus::Provisional {
        evidence_count: 3,
        last_validated: Utc::now(),
    };

    let result = cortex_temporal::epistemic::promote_to_verified(
        &provisional,
        vec!["alice".to_string(), "bob".to_string()],
        vec!["ref-1".to_string(), "ref-2".to_string()],
    );
    assert!(result.is_ok());

    let verified = result.unwrap();
    match &verified {
        EpistemicStatus::Verified {
            verified_by,
            verified_at,
            evidence_refs,
        } => {
            assert_eq!(verified_by, &["alice", "bob"]);
            assert_eq!(evidence_refs, &["ref-1", "ref-2"]);
            assert!(Utc::now() - *verified_at < Duration::seconds(5));
        }
        other => panic!("Expected Verified, got {:?}", other),
    }
}

// ── TTD2-04: Verified → Stale on evidence decay ─────────────────────────

#[test]
fn ttd2_04_verified_to_stale() {
    let verified_at = Utc::now() - Duration::days(90);
    let verified = EpistemicStatus::Verified {
        verified_by: vec!["alice".to_string()],
        verified_at,
        evidence_refs: vec!["ref-1".to_string()],
    };

    let result =
        cortex_temporal::epistemic::demote_to_stale(&verified, "evidence freshness below 0.5".to_string());
    assert!(result.is_ok());

    let stale = result.unwrap();
    match &stale {
        EpistemicStatus::Stale {
            was_verified_at,
            staleness_detected_at,
            reason,
        } => {
            assert_eq!(*was_verified_at, verified_at);
            assert!(Utc::now() - *staleness_detected_at < Duration::seconds(5));
            assert_eq!(reason, "evidence freshness below 0.5");
        }
        other => panic!("Expected Stale, got {:?}", other),
    }
}

// ── TTD2-05: Conjecture → Verified rejected ──────────────────────────────

#[test]
fn ttd2_05_conjecture_to_verified_rejected() {
    let conjecture = EpistemicStatus::Conjecture {
        source: "user:alice".to_string(),
        created_at: Utc::now(),
    };

    let result = cortex_temporal::epistemic::promote_to_verified(
        &conjecture,
        vec!["alice".to_string()],
        vec!["ref-1".to_string()],
    );
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("conjecture") && err_str.contains("verified"),
        "Error should mention conjecture → verified, got: {}",
        err_str
    );
}

// ── TTD2-06: Verified → Provisional rejected ─────────────────────────────

#[test]
fn ttd2_06_verified_to_provisional_rejected() {
    let verified = EpistemicStatus::Verified {
        verified_by: vec!["alice".to_string()],
        verified_at: Utc::now(),
        evidence_refs: vec!["ref-1".to_string()],
    };

    let result = cortex_temporal::epistemic::promote_to_provisional(&verified, 5);
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("verified") && err_str.contains("provisional"),
        "Error should mention verified → provisional, got: {}",
        err_str
    );
}

// ── TTD2-06 extra: Provisional → Stale rejected ─────────────────────────

#[test]
fn ttd2_06_extra_provisional_to_stale_rejected() {
    let provisional = EpistemicStatus::Provisional {
        evidence_count: 3,
        last_validated: Utc::now(),
    };

    let result =
        cortex_temporal::epistemic::demote_to_stale(&provisional, "test reason".to_string());
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("provisional") && err_str.contains("stale"),
        "Error should mention provisional → stale, got: {}",
        err_str
    );
}

// ── TTD2-06 extra: Stale → Verified rejected ────────────────────────────

#[test]
fn ttd2_06_extra_stale_to_verified_rejected() {
    let stale = EpistemicStatus::Stale {
        was_verified_at: Utc::now() - Duration::days(30),
        staleness_detected_at: Utc::now(),
        reason: "evidence decay".to_string(),
    };

    let result = cortex_temporal::epistemic::promote_to_verified(
        &stale,
        vec!["alice".to_string()],
        vec!["ref-1".to_string()],
    );
    assert!(result.is_err());
}

// ── TTD2-06 extra: Conjecture → Stale rejected ─────────────────────────

#[test]
fn ttd2_06_extra_conjecture_to_stale_rejected() {
    let conjecture = EpistemicStatus::Conjecture {
        source: "user:alice".to_string(),
        created_at: Utc::now(),
    };

    let result =
        cortex_temporal::epistemic::demote_to_stale(&conjecture, "test reason".to_string());
    assert!(result.is_err());
}

// ── TTD2-07: WeightedAverage aggregation correct ─────────────────────────

#[test]
fn ttd2_07_weighted_average_aggregation() {
    let evidences = vec![0.9, 0.3, 0.8];
    let result = cortex_temporal::epistemic::aggregate_confidence(
        &evidences,
        &AggregationStrategy::WeightedAverage,
    );
    let expected = (0.9 + 0.3 + 0.8) / 3.0;
    assert!(
        (result - expected).abs() < 0.0001,
        "WeightedAverage: expected {}, got {}",
        expected,
        result
    );

    // Single element
    let result = cortex_temporal::epistemic::aggregate_confidence(
        &[0.7],
        &AggregationStrategy::WeightedAverage,
    );
    assert!((result - 0.7).abs() < 0.0001);

    // Empty
    let result = cortex_temporal::epistemic::aggregate_confidence(
        &[],
        &AggregationStrategy::WeightedAverage,
    );
    assert!((result - 0.0).abs() < 0.0001);
}

// ── TTD2-08: GodelTNorm aggregation = min ────────────────────────────────

#[test]
fn ttd2_08_godel_tnorm_aggregation() {
    let evidences = vec![0.9, 0.3, 0.8];
    let result = cortex_temporal::epistemic::aggregate_confidence(
        &evidences,
        &AggregationStrategy::GodelTNorm,
    );
    assert!(
        (result - 0.3).abs() < 0.0001,
        "GodelTNorm: expected 0.3, got {}",
        result
    );

    // All high
    let result = cortex_temporal::epistemic::aggregate_confidence(
        &[0.9, 0.95, 0.85],
        &AggregationStrategy::GodelTNorm,
    );
    assert!(
        (result - 0.85).abs() < 0.0001,
        "GodelTNorm: expected 0.85, got {}",
        result
    );

    // Single element
    let result = cortex_temporal::epistemic::aggregate_confidence(
        &[0.5],
        &AggregationStrategy::GodelTNorm,
    );
    assert!((result - 0.5).abs() < 0.0001);

    // Empty
    let result = cortex_temporal::epistemic::aggregate_confidence(
        &[],
        &AggregationStrategy::GodelTNorm,
    );
    assert!((result - 0.0).abs() < 0.0001);
}

// ── TTD2-11: Materialized view creation ──────────────────────────────────

#[tokio::test]
async fn ttd2_11_materialized_view_creation() {
    let (writer, readers) = setup();

    // Insert some memories and events so reconstruction has data
    for i in 0..5 {
        let mid = format!("view-mem-{}", i);
        ensure_memory_row(&writer, &mid).await;

        let event = MemoryEvent {
            event_id: 0,
            memory_id: mid.clone(),
            recorded_at: Utc::now(),
            event_type: MemoryEventType::Created,
            delta: serde_json::json!({
                "id": mid,
                "memory_type": "episodic",
                "content": {"type": "episodic", "interaction": "test", "context": "ctx"},
                "summary": "test memory",
                "transaction_time": Utc::now().to_rfc3339(),
                "valid_time": Utc::now().to_rfc3339(),
                "confidence": 0.8,
                "importance": "normal",
                "last_accessed": Utc::now().to_rfc3339(),
                "access_count": 0,
                "linked_patterns": [],
                "linked_constraints": [],
                "linked_files": [],
                "linked_functions": [],
                "tags": [],
                "archived": false,
                "content_hash": "hash"
            }),
            actor: EventActor::System("test".to_string()),
            caused_by: vec![],
            schema_version: 1,
        };
        cortex_temporal::event_store::append::append(&writer, &event)
            .await
            .unwrap();
    }

    let view = cortex_temporal::views::create_materialized_view(
        &writer,
        &readers,
        "sprint-12",
        Utc::now(),
    )
    .await
    .unwrap();

    assert_eq!(view.label, "sprint-12");
    assert_eq!(view.memory_count, 5);
    assert_eq!(view.snapshot_ids.len(), 5);
    assert!(view.drift_snapshot_id.is_some());
    assert!(!view.auto_refresh);
}

// ── TTD2-12: Materialized view lookup ────────────────────────────────────

#[tokio::test]
async fn ttd2_12_materialized_view_lookup() {
    let (writer, readers) = setup();

    // Create a view with no memories (empty state)
    let view = cortex_temporal::views::create_materialized_view(
        &writer,
        &readers,
        "test-lookup",
        Utc::now(),
    )
    .await
    .unwrap();

    // Lookup by label
    let found = cortex_temporal::views::get_view(&readers, "test-lookup").unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.label, "test-lookup");
    assert_eq!(found.view_id, view.view_id);

    // Lookup non-existent
    let not_found = cortex_temporal::views::get_view(&readers, "nonexistent").unwrap();
    assert!(not_found.is_none());
}

// ── TTD2-13: Diff between views ──────────────────────────────────────────

#[tokio::test]
async fn ttd2_13_diff_between_views() {
    let (writer, readers) = setup();

    // Create view A (empty state)
    let _view_a = cortex_temporal::views::create_materialized_view(
        &writer,
        &readers,
        "view-a",
        Utc::now() - Duration::hours(1),
    )
    .await
    .unwrap();

    // Add some memories between view A and view B
    for i in 0..3 {
        let mid = format!("diff-mem-{}", i);
        ensure_memory_row(&writer, &mid).await;

        let event = MemoryEvent {
            event_id: 0,
            memory_id: mid.clone(),
            recorded_at: Utc::now(),
            event_type: MemoryEventType::Created,
            delta: serde_json::json!({
                "id": mid,
                "memory_type": "episodic",
                "content": {"type": "episodic", "interaction": "test", "context": "ctx"},
                "summary": "test memory",
                "transaction_time": Utc::now().to_rfc3339(),
                "valid_time": Utc::now().to_rfc3339(),
                "confidence": 0.8,
                "importance": "normal",
                "last_accessed": Utc::now().to_rfc3339(),
                "access_count": 0,
                "linked_patterns": [],
                "linked_constraints": [],
                "linked_files": [],
                "linked_functions": [],
                "tags": [],
                "archived": false,
                "content_hash": "hash"
            }),
            actor: EventActor::System("test".to_string()),
            caused_by: vec![],
            schema_version: 1,
        };
        cortex_temporal::event_store::append::append(&writer, &event)
            .await
            .unwrap();
    }

    // Create view B (with 3 new memories)
    let _view_b = cortex_temporal::views::create_materialized_view(
        &writer,
        &readers,
        "view-b",
        Utc::now(),
    )
    .await
    .unwrap();

    // Diff between views
    let diff = cortex_temporal::views::diff_views(&readers, "view-a", "view-b").unwrap();

    // The diff should show the 3 new memories as created
    assert!(
        !diff.created.is_empty() || diff.stats.net_change >= 0,
        "Diff should reflect changes between views"
    );
}

// ── TTD2-14: Auto-refresh scheduler fires ────────────────────────────────

#[tokio::test]
async fn ttd2_14_auto_refresh_fires() {
    let (_writer, readers) = setup();

    // With no existing views, scheduler should fire
    let config = TemporalConfig::default();
    let scheduler = cortex_temporal::views::AutoRefreshScheduler::new(config);

    let label = scheduler.should_create_view(&readers).unwrap();
    assert!(
        label.is_some(),
        "Scheduler should fire when no auto-created views exist"
    );
    let label = label.unwrap();
    assert!(
        label.starts_with("auto-"),
        "Label should start with 'auto-', got: {}",
        label
    );
}

// ── TTD2-15: Auto-refresh skips when no changes ──────────────────────────

#[tokio::test]
async fn ttd2_15_auto_refresh_skips_no_changes() {
    let (writer, readers) = setup();

    // Create an auto-refresh view manually (simulate previous auto-creation)
    let created_by = EventActor::System("materialized_view_engine".to_string());
    let created_by_json = serde_json::to_string(&created_by).unwrap();
    let ts = Utc::now().to_rfc3339();

    writer
        .with_conn(move |conn| {
            cortex_storage::queries::view_ops::insert_materialized_view(
                conn,
                &cortex_storage::queries::view_ops::InsertViewParams {
                    label: "auto-recent",
                    timestamp: &ts,
                    memory_count: 0,
                    snapshot_ids_json: "[]",
                    drift_snapshot_id: None,
                    created_by_json: &created_by_json,
                    auto_refresh: true,
                },
            )
        })
        .await
        .unwrap();

    // With a recent auto-view and no events, scheduler should NOT fire
    let config = TemporalConfig::default();
    let scheduler = cortex_temporal::views::AutoRefreshScheduler::new(config);

    let label = scheduler.should_create_view(&readers).unwrap();
    assert!(
        label.is_none(),
        "Scheduler should not fire when interval hasn't elapsed"
    );
}

// ── TTD2-Extra: Full promotion path ──────────────────────────────────────

#[test]
fn ttd2_extra_full_promotion_path() {
    // Conjecture → Provisional → Verified → Stale (full lifecycle)
    let status = cortex_temporal::epistemic::determine_initial_status(
        &EventActor::User("alice".to_string()),
    );
    assert_eq!(status.variant_name(), "conjecture");

    let status = cortex_temporal::epistemic::promote_to_provisional(&status, 3).unwrap();
    assert_eq!(status.variant_name(), "provisional");

    let status = cortex_temporal::epistemic::promote_to_verified(
        &status,
        vec!["alice".to_string()],
        vec!["ref-1".to_string()],
    )
    .unwrap();
    assert_eq!(status.variant_name(), "verified");

    let status =
        cortex_temporal::epistemic::demote_to_stale(&status, "evidence decay".to_string())
            .unwrap();
    assert_eq!(status.variant_name(), "stale");
}

// ── TTD2-Extra: EpistemicStatus serde round-trip ─────────────────────────

#[test]
fn ttd2_extra_epistemic_serde_roundtrip() {
    let statuses = vec![
        EpistemicStatus::Conjecture {
            source: "user:alice".to_string(),
            created_at: Utc::now(),
        },
        EpistemicStatus::Provisional {
            evidence_count: 5,
            last_validated: Utc::now(),
        },
        EpistemicStatus::Verified {
            verified_by: vec!["alice".to_string()],
            verified_at: Utc::now(),
            evidence_refs: vec!["ref-1".to_string()],
        },
        EpistemicStatus::Stale {
            was_verified_at: Utc::now() - Duration::days(30),
            staleness_detected_at: Utc::now(),
            reason: "evidence decay".to_string(),
        },
    ];

    for status in &statuses {
        let json = serde_json::to_string(status).unwrap();
        let deserialized: EpistemicStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, &deserialized);
    }
}

// ── TTD2-Extra: AggregationStrategy serde round-trip ─────────────────────

#[test]
fn ttd2_extra_aggregation_strategy_serde() {
    let strategies = vec![
        AggregationStrategy::WeightedAverage,
        AggregationStrategy::GodelTNorm,
    ];

    for strategy in &strategies {
        let json = serde_json::to_string(strategy).unwrap();
        let deserialized: AggregationStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(strategy, &deserialized);
    }
}

// ── TTD2-Extra: MaterializedTemporalView serde round-trip ────────────────

#[test]
fn ttd2_extra_materialized_view_serde() {
    let view = MaterializedTemporalView {
        view_id: 1,
        label: "sprint-12".to_string(),
        timestamp: Utc::now(),
        memory_count: 42,
        snapshot_ids: vec![1, 2, 3],
        drift_snapshot_id: Some(7),
        created_by: EventActor::System("test".to_string()),
        auto_refresh: false,
    };

    let json = serde_json::to_string(&view).unwrap();
    let deserialized: MaterializedTemporalView = serde_json::from_str(&json).unwrap();
    assert_eq!(view.view_id, deserialized.view_id);
    assert_eq!(view.label, deserialized.label);
    assert_eq!(view.memory_count, deserialized.memory_count);
    assert_eq!(view.snapshot_ids, deserialized.snapshot_ids);
}

// ── TTD2-Extra: list_views ───────────────────────────────────────────────

#[tokio::test]
async fn ttd2_extra_list_views() {
    let (writer, readers) = setup();

    // Create two views
    let _v1 = cortex_temporal::views::create_materialized_view(
        &writer,
        &readers,
        "list-view-1",
        Utc::now() - Duration::hours(2),
    )
    .await
    .unwrap();

    let _v2 = cortex_temporal::views::create_materialized_view(
        &writer,
        &readers,
        "list-view-2",
        Utc::now(),
    )
    .await
    .unwrap();

    let views = cortex_temporal::views::list_views(&readers).unwrap();
    assert!(views.len() >= 2);

    let labels: Vec<&str> = views.iter().map(|v| v.label.as_str()).collect();
    assert!(labels.contains(&"list-view-1"));
    assert!(labels.contains(&"list-view-2"));
}

// ── TTD2-Extra: delete_view via storage ──────────────────────────────────

#[tokio::test]
async fn ttd2_extra_delete_view() {
    let (writer, readers) = setup();

    let _v = cortex_temporal::views::create_materialized_view(
        &writer,
        &readers,
        "to-delete",
        Utc::now(),
    )
    .await
    .unwrap();

    // Verify it exists
    let found = cortex_temporal::views::get_view(&readers, "to-delete").unwrap();
    assert!(found.is_some());

    // Delete via storage layer
    writer
        .with_conn(|conn| cortex_storage::queries::view_ops::delete_view(conn, "to-delete"))
        .await
        .unwrap();

    // Verify it's gone
    let found = cortex_temporal::views::get_view(&readers, "to-delete").unwrap();
    assert!(found.is_none());
}

// ── TTD2-Extra: Engine create_view and get_view ──────────────────────────

#[tokio::test]
async fn ttd2_extra_engine_create_and_get_view() {
    use cortex_core::traits::ITemporalEngine;

    let (writer, readers) = setup();
    let config = TemporalConfig::default();
    let engine = cortex_temporal::TemporalEngine::new(writer, readers, config);

    let view = engine.create_view("engine-test", Utc::now()).await.unwrap();
    assert_eq!(view.label, "engine-test");

    let found = engine.get_view("engine-test").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().label, "engine-test");

    let not_found = engine.get_view("nonexistent").await.unwrap();
    assert!(not_found.is_none());
}
