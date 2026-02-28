//! Golden fixture tests — frozen expected outputs for format regression detection.
//!
//! These tests load JSON fixtures from `test-fixtures/golden/temporal/` and verify
//! that the temporal system produces exactly the expected output. If a serde attribute
//! changes, a field is renamed, or a type conversion silently drops data — these tests
//! catch it because they compare against a frozen expected output.

use chrono::{DateTime, Duration, Utc};
use cortex_core::config::TemporalConfig;
use cortex_core::memory::*;
use cortex_core::models::*;
use cortex_storage::pool::{ReadPool, WriteConnection};
use std::sync::Arc;
use test_fixtures::load_fixture_value;

// ═══════════════════════════════════════════════════════════════════════════
// Test Infrastructure
// ═══════════════════════════════════════════════════════════════════════════

fn setup() -> (Arc<WriteConnection>, Arc<ReadPool>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("golden_test.db");
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
    writer
        .with_conn(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO memories \
                 (id, memory_type, content, summary, transaction_time, valid_time, \
                  confidence, importance, last_accessed, access_count, archived, content_hash) \
                 VALUES (?1, 'episodic', '{}', 'test', \
                         strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
                         strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
                         0.8, 'normal', \
                         strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
                         0, 0, 'hash')",
                rusqlite::params![mid],
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;
            Ok(())
        })
        .await
        .unwrap();
}

fn make_shell(id: &str) -> BaseMemory {
    let content = TypedContent::Episodic(cortex_core::memory::types::EpisodicContent {
        interaction: String::new(),
        context: String::new(),
        outcome: None,
    });
    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Episodic,
        content,
        summary: String::new(),
        transaction_time: DateTime::UNIX_EPOCH.with_timezone(&Utc),
        valid_time: DateTime::UNIX_EPOCH.with_timezone(&Utc),
        valid_until: None,
        confidence: Confidence::new(0.5),
        importance: Importance::Normal,
        last_accessed: DateTime::UNIX_EPOCH.with_timezone(&Utc),
        access_count: 0,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec![],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: String::new(),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

fn make_event_at(
    memory_id: &str,
    event_type: MemoryEventType,
    delta: serde_json::Value,
    at: DateTime<Utc>,
) -> MemoryEvent {
    MemoryEvent {
        event_id: 0,
        memory_id: memory_id.to_string(),
        recorded_at: at,
        event_type,
        delta,
        actor: EventActor::System("golden-test".to_string()),
        caused_by: vec![],
        schema_version: 1,
    }
}

fn parse_event_type(s: &str) -> MemoryEventType {
    serde_json::from_str(&format!("\"{}\"", s)).expect("valid event type")
}

// ═══════════════════════════════════════════════════════════════════════════
// PTF-GOLD-01: reconstruction_simple — 10 events, 3 checkpoints
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn golden_reconstruction_simple() {
    let fixture = load_fixture_value("golden/temporal/reconstruction_simple.json");
    let mem_id = fixture["memory_id"].as_str().unwrap();
    let events_json = fixture["events"].as_array().unwrap();

    // Build MemoryEvent list from fixture
    let events: Vec<MemoryEvent> = events_json
        .iter()
        .map(|ej| {
            let et = parse_event_type(ej["event_type"].as_str().unwrap());
            let recorded_at: DateTime<Utc> = ej["recorded_at"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            make_event_at(mem_id, et, ej["delta"].clone(), recorded_at)
        })
        .collect();

    let checkpoints = fixture["checkpoints"].as_array().unwrap();

    for cp in checkpoints {
        let after_idx = cp["after_event_index"].as_u64().unwrap() as usize;
        let expected = &cp["expected_state"];

        // Replay events up to checkpoint
        let shell = make_shell(mem_id);
        let state =
            cortex_temporal::event_store::replay::replay_events(&events[..after_idx], shell);

        // Verify field-by-field against fixture
        assert_eq!(
            state.id,
            expected["id"].as_str().unwrap(),
            "Checkpoint {}: id mismatch",
            after_idx
        );
        assert_eq!(
            state.summary,
            expected["summary"].as_str().unwrap(),
            "Checkpoint {}: summary mismatch",
            after_idx
        );

        let expected_conf = expected["confidence"].as_f64().unwrap();
        assert!(
            (state.confidence.value() - expected_conf).abs() < 0.001,
            "Checkpoint {}: confidence mismatch: got {} expected {}",
            after_idx,
            state.confidence.value(),
            expected_conf
        );

        let expected_importance = expected["importance"].as_str().unwrap();
        let actual_importance = serde_json::to_string(&state.importance)
            .unwrap()
            .trim_matches('"')
            .to_string();
        assert_eq!(
            actual_importance, expected_importance,
            "Checkpoint {}: importance mismatch",
            after_idx
        );

        let expected_tags: Vec<String> = expected["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let mut actual_tags = state.tags.clone();
        actual_tags.sort();
        let mut expected_sorted = expected_tags.clone();
        expected_sorted.sort();
        assert_eq!(
            actual_tags, expected_sorted,
            "Checkpoint {}: tags mismatch",
            after_idx
        );

        assert_eq!(
            state.archived,
            expected["archived"].as_bool().unwrap(),
            "Checkpoint {}: archived mismatch",
            after_idx
        );

        assert_eq!(
            state.content_hash,
            expected["content_hash"].as_str().unwrap(),
            "Checkpoint {}: content_hash mismatch",
            after_idx
        );

        if expected["superseded_by"].is_null() {
            assert!(
                state.superseded_by.is_none(),
                "Checkpoint {}: superseded_by should be None",
                after_idx
            );
        } else {
            assert_eq!(
                state.superseded_by.as_deref(),
                expected["superseded_by"].as_str(),
                "Checkpoint {}: superseded_by mismatch",
                after_idx
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PTF-GOLD-02: reconstruction_with_snapshot — snapshot+replay == full replay
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_reconstruction_with_snapshot() {
    let fixture = load_fixture_value("golden/temporal/reconstruction_with_snapshot.json");
    let mem_id = fixture["memory_id"].as_str().unwrap();
    let initial_conf = fixture["initial_confidence"].as_f64().unwrap();
    let decrement = fixture["confidence_decrement"].as_f64().unwrap();
    let total = fixture["total_events"].as_u64().unwrap() as usize;
    let snap_at = fixture["snapshot_at_event"].as_u64().unwrap() as usize;

    let (writer, readers) = setup();
    ensure_memory_row(&writer, mem_id).await;

    let base_time = Utc::now() - Duration::hours(total as i64);

    // Build and append events: Created + (total-1) confidence changes
    let mut shell_mem = make_shell(mem_id);
    shell_mem.confidence = Confidence::new(initial_conf);
    shell_mem.id = mem_id.to_string();

    let created = make_event_at(
        mem_id,
        MemoryEventType::Created,
        serde_json::to_value(&shell_mem).unwrap(),
        base_time,
    );
    cortex_temporal::event_store::append::append(&writer, &created)
        .await
        .unwrap();

    let mut conf = initial_conf;
    for i in 1..total {
        let new_conf = conf - decrement;
        let e = make_event_at(
            mem_id,
            MemoryEventType::ConfidenceChanged,
            serde_json::json!({"old": conf, "new": new_conf}),
            base_time + Duration::minutes(i as i64),
        );
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();

        // Create snapshot at the designated event
        if i == snap_at - 1 {
            let mut snap_state = shell_mem.clone();
            snap_state.confidence = Confidence::new(new_conf);
            cortex_temporal::snapshot::create::create_snapshot(
                &writer,
                mem_id,
                &snap_state,
                SnapshotReason::EventThreshold,
            )
            .await
            .unwrap();
        }

        conf = new_conf;
    }

    // Verify each checkpoint
    let checkpoints = fixture["checkpoints"].as_array().unwrap();
    for cp in checkpoints {
        let after_idx = cp["after_event_index"].as_u64().unwrap() as usize;
        let expected_conf = cp["expected_confidence"].as_f64().unwrap();

        let target = base_time + Duration::minutes(after_idx as i64) + Duration::seconds(30);

        // Full replay
        let all_events =
            cortex_temporal::event_store::query::get_events(&readers, mem_id, Some(target))
                .unwrap();
        let full_replay =
            cortex_temporal::event_store::replay::replay_events(&all_events, make_shell(mem_id));

        // Snapshot + replay reconstruction
        let reconstructed =
            cortex_temporal::snapshot::reconstruct::reconstruct_at(&readers, mem_id, target)
                .unwrap()
                .unwrap();

        // Both must match expected confidence
        assert!(
            (full_replay.confidence.value() - expected_conf).abs() < 0.02,
            "Checkpoint {}: full_replay confidence {} != expected {}",
            after_idx,
            full_replay.confidence.value(),
            expected_conf
        );

        assert!(
            (reconstructed.confidence.value() - expected_conf).abs() < 0.02,
            "Checkpoint {}: reconstructed confidence {} != expected {}",
            after_idx,
            reconstructed.confidence.value(),
            expected_conf
        );

        // Invariant: snapshot+replay == full replay
        assert!(
            (full_replay.confidence.value() - reconstructed.confidence.value()).abs() < 0.001,
            "Checkpoint {}: snapshot+replay ({}) != full replay ({})",
            after_idx,
            reconstructed.confidence.value(),
            full_replay.confidence.value()
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PTF-GOLD-07: diff_empty — diff(T, T) == empty
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn golden_diff_empty() {
    let fixture = load_fixture_value("golden/temporal/diff_empty.json");
    let time_a: DateTime<Utc> = fixture["time_a"].as_str().unwrap().parse().unwrap();
    let time_b: DateTime<Utc> = fixture["time_b"].as_str().unwrap().parse().unwrap();
    let expected = &fixture["expected_output"];

    assert_eq!(time_a, time_b, "Fixture should have time_a == time_b");

    // Construct the query and verify the expected output structure
    let expected_stats = &expected["stats"];
    assert_eq!(expected_stats["net_change"].as_i64().unwrap(), 0);
    assert!((expected_stats["confidence_trend"].as_f64().unwrap() - 0.0).abs() < 0.001);
    assert!((expected_stats["knowledge_churn_rate"].as_f64().unwrap() - 0.0).abs() < 0.001);

    // Execute against a real DB
    let (_, readers) = setup();

    let query = TemporalDiffQuery {
        time_a,
        time_b,
        scope: DiffScope::All,
    };
    let result = readers
        .with_conn(|conn| cortex_temporal::query::diff::execute_diff(conn, &query))
        .unwrap();

    // Verify against fixture
    assert!(result.created.is_empty(), "diff(T,T) created should be empty");
    assert!(result.archived.is_empty(), "diff(T,T) archived should be empty");
    assert!(result.modified.is_empty(), "diff(T,T) modified should be empty");
    assert_eq!(result.stats.net_change, 0, "diff(T,T) net_change should be 0");
    assert!(
        (result.stats.confidence_trend - 0.0).abs() < 0.001,
        "diff(T,T) confidence_trend should be 0.0"
    );
    assert!(
        (result.stats.knowledge_churn_rate - 0.0).abs() < 0.001,
        "diff(T,T) churn_rate should be 0.0"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// PTF-GOLD-11: drift_stable — stable KB, KSI ≈ 1.0
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_drift_stable() {
    let fixture = load_fixture_value("golden/temporal/drift_stable.json");
    let mem_count = fixture["memory_count"].as_u64().unwrap() as usize;
    let expected = &fixture["expected_output"];

    let (writer, readers) = setup();

    // Create memories with no changes in the window
    let creation_time = Utc::now() - Duration::days(30);
    for i in 0..mem_count {
        let mid = format!("stable-{}", i);
        let mid_clone = mid.clone();
        writer
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO memories \
                     (id, memory_type, content, summary, transaction_time, valid_time, \
                      confidence, importance, last_accessed, access_count, archived, content_hash) \
                     VALUES (?1, 'episodic', '{}', 'stable memory', ?2, ?2, \
                             0.8, 'normal', ?2, 0, 0, 'hash')",
                    rusqlite::params![mid_clone, creation_time.to_rfc3339()],
                )
                .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;
                Ok(())
            })
            .await
            .unwrap();

        // Emit a Created event at creation_time (outside the window)
        let e = make_event_at(
            &mid,
            MemoryEventType::Created,
            serde_json::json!({"id": mid}),
            creation_time,
        );
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    // Compute KSI for the last 7 days (no events in this window)
    let now = Utc::now();
    let window_start = now - Duration::days(7);
    let ksi = cortex_temporal::drift::metrics::compute_ksi(&readers, None, window_start, now)
        .unwrap();

    let expected_ksi = expected["global"]["overall_ksi"].as_f64().unwrap();
    assert!(
        (ksi - expected_ksi).abs() < 0.001,
        "KSI should be {}, got {}",
        expected_ksi,
        ksi
    );

    // Compute contradiction density
    let cd = cortex_temporal::drift::metrics::compute_contradiction_density(
        &readers,
        None,
        window_start,
        now,
    )
    .unwrap();

    let expected_cd = expected["global"]["overall_contradiction_density"].as_f64().unwrap();
    assert!(
        (cd - expected_cd).abs() < 0.001,
        "Contradiction density should be {}, got {}",
        expected_cd,
        cd
    );

    // Verify no alerts fire for a stable dataset
    let snapshot = cortex_temporal::drift::metrics::compute_all_metrics(&readers, window_start, now)
        .unwrap();
    let config = TemporalConfig::default();
    let alerts =
        cortex_temporal::drift::alerting::evaluate_drift_alerts(&snapshot, &config, &[]);

    // Filter out ConfidenceErosion alerts (avg_confidence check is separate from stability)
    let non_erosion_alerts: Vec<_> = alerts
        .iter()
        .filter(|a| a.category != DriftAlertCategory::ConfidenceErosion)
        .collect();
    assert!(
        non_erosion_alerts.is_empty(),
        "Stable dataset should not trigger non-erosion alerts: {:?}",
        non_erosion_alerts
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// PTF-GOLD-05: epistemic_lifecycle — full state machine traversal
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn golden_epistemic_lifecycle() {
    let fixture = load_fixture_value("golden/temporal/epistemic_lifecycle.json");
    let stages = fixture["stages"].as_array().unwrap();

    // Stage 1: Initial — Conjecture
    let initial_source = stages[0]["expected_status"]["source"].as_str().unwrap();
    let status = cortex_temporal::epistemic::determine_initial_status(&EventActor::User(
        "alice".to_string(),
    ));
    match &status {
        EpistemicStatus::Conjecture { source, .. } => {
            assert_eq!(source, initial_source, "Initial source mismatch");
        }
        _ => panic!("Expected Conjecture, got {:?}", status),
    }

    // Stage 2: Promote to Provisional
    let evidence_count = stages[1]["expected_status"]["evidence_count"]
        .as_u64()
        .unwrap() as u32;
    let provisional =
        cortex_temporal::epistemic::promote_to_provisional(&status, evidence_count).unwrap();
    match &provisional {
        EpistemicStatus::Provisional {
            evidence_count: ec, ..
        } => {
            assert_eq!(*ec, evidence_count, "Evidence count mismatch");
        }
        _ => panic!("Expected Provisional, got {:?}", provisional),
    }

    // Stage 3: Promote to Verified
    let expected_verified_by: Vec<String> = stages[2]["expected_status"]["verified_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let expected_evidence_refs: Vec<String> = stages[2]["expected_status"]["evidence_refs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let verified = cortex_temporal::epistemic::promote_to_verified(
        &provisional,
        expected_verified_by.clone(),
        expected_evidence_refs.clone(),
    )
    .unwrap();
    match &verified {
        EpistemicStatus::Verified {
            verified_by,
            evidence_refs,
            ..
        } => {
            assert_eq!(verified_by, &expected_verified_by);
            assert_eq!(evidence_refs, &expected_evidence_refs);
        }
        _ => panic!("Expected Verified, got {:?}", verified),
    }

    // Stage 4: Demote to Stale
    let expected_reason = stages[3]["expected_status"]["reason"].as_str().unwrap();
    let stale =
        cortex_temporal::epistemic::demote_to_stale(&verified, expected_reason.to_string())
            .unwrap();
    match &stale {
        EpistemicStatus::Stale { reason, .. } => {
            assert_eq!(reason, expected_reason);
        }
        _ => panic!("Expected Stale, got {:?}", stale),
    }

    // Verify invalid transitions from fixture
    let invalid = fixture["invalid_transitions"].as_array().unwrap();
    for inv in invalid {
        let from_str = inv["from"].as_str().unwrap();
        let to_str = inv["to"].as_str().unwrap();

        let from_status = match from_str {
            "conjecture" => EpistemicStatus::Conjecture {
                source: "test".to_string(),
                created_at: Utc::now(),
            },
            "provisional" => EpistemicStatus::Provisional {
                evidence_count: 1,
                last_validated: Utc::now(),
            },
            "verified" => EpistemicStatus::Verified {
                verified_by: vec!["x".to_string()],
                verified_at: Utc::now(),
                evidence_refs: vec![],
            },
            "stale" => EpistemicStatus::Stale {
                was_verified_at: Utc::now(),
                staleness_detected_at: Utc::now(),
                reason: "test".to_string(),
            },
            _ => panic!("Unknown status: {}", from_str),
        };

        let result = match to_str {
            "provisional" => {
                cortex_temporal::epistemic::promote_to_provisional(&from_status, 1).map(|_| ())
            }
            "verified" => {
                cortex_temporal::epistemic::promote_to_verified(&from_status, vec![], vec![])
                    .map(|_| ())
            }
            "stale" => {
                cortex_temporal::epistemic::demote_to_stale(&from_status, "test".to_string())
                    .map(|_| ())
            }
            _ => panic!("Unknown target status: {}", to_str),
        };

        assert!(
            result.is_err(),
            "Transition {} → {} should be rejected",
            from_str, to_str
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Fixture file existence verification
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn golden_all_5_temporal_fixture_files_exist() {
    let files = [
        "golden/temporal/reconstruction_simple.json",
        "golden/temporal/reconstruction_with_snapshot.json",
        "golden/temporal/diff_empty.json",
        "golden/temporal/drift_stable.json",
        "golden/temporal/epistemic_lifecycle.json",
    ];
    for f in &files {
        assert!(
            test_fixtures::fixture_exists(f),
            "Missing temporal golden fixture: {}",
            f
        );
    }
}

#[test]
fn golden_all_temporal_fixtures_parse_as_valid_json() {
    let files = test_fixtures::list_fixtures("golden/temporal");
    assert!(
        files.len() >= 5,
        "Expected at least 5 temporal golden files, found {}",
        files.len()
    );
    for file in &files {
        let content = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", file.display(), e));
        let val: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", file.display(), e));
        assert!(
            val["description"].is_string(),
            "Fixture {} must have a description",
            file.display()
        );
    }
}