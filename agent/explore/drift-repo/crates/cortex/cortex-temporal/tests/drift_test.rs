//! Phase D1 tests: drift metrics, alerting, evidence freshness, snapshots, patterns.
//! TTD1-01 through TTD1-17 (TTD1-18..TTD1-21 are benchmarks in benches/).

use chrono::{Duration, Utc};
use cortex_core::config::TemporalConfig;
use cortex_core::memory::*;
use cortex_core::models::*;
use cortex_storage::pool::{ReadPool, WriteConnection};
use std::sync::Arc;

// ── Test Harness ─────────────────────────────────────────────────────────

fn setup() -> (Arc<WriteConnection>, Arc<ReadPool>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_drift.db");
    let _dir = Box::leak(Box::new(dir));

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();
    }

    let writer = Arc::new(WriteConnection::open(&db_path).unwrap());
    let readers = Arc::new(ReadPool::open(&db_path, 2).unwrap());
    (writer, readers)
}

#[allow(dead_code)]
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

/// Insert a memory row with specific type and confidence.
async fn insert_typed_memory(
    writer: &Arc<WriteConnection>,
    id: &str,
    memory_type: &str,
    confidence: f64,
    archived: bool,
) {
    insert_typed_memory_at(writer, id, memory_type, confidence, archived, None).await;
}

/// Insert a memory row with specific type, confidence, and optional transaction_time.
async fn insert_typed_memory_at(
    writer: &Arc<WriteConnection>,
    id: &str,
    memory_type: &str,
    confidence: f64,
    archived: bool,
    transaction_time: Option<chrono::DateTime<Utc>>,
) {
    let mid = id.to_string();
    let mt = memory_type.to_string();
    let arch = if archived { 1 } else { 0 };
    let tt = transaction_time
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    writer
        .with_conn(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO memories \
                 (id, memory_type, content, summary, transaction_time, valid_time, \
                  confidence, importance, last_accessed, access_count, archived, content_hash) \
                 VALUES (?1, ?2, '{}', 'test', \
                         ?3, ?3, \
                         ?4, 'normal', \
                         ?3, \
                         0, ?5, 'hash')",
                rusqlite::params![mid, mt, tt, confidence, arch],
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;
            Ok(())
        })
        .await
        .unwrap();
}

fn make_test_event(memory_id: &str, event_type: MemoryEventType) -> MemoryEvent {
    MemoryEvent {
        event_id: 0,
        memory_id: memory_id.to_string(),
        recorded_at: Utc::now(),
        event_type,
        delta: serde_json::json!({}),
        actor: EventActor::System("test".to_string()),
        caused_by: vec![],
        schema_version: 1,
    }
}

// ── TTD1-01: KSI = 1.0 for stable dataset ───────────────────────────────

#[tokio::test]
async fn ttd1_01_ksi_stable_dataset() {
    let (writer, readers) = setup();

    // Insert 10 memories, no events in window → KSI should be 1.0
    for i in 0..10 {
        insert_typed_memory(&writer, &format!("stable-{}", i), "episodic", 0.8, false).await;
    }

    let window_start = Utc::now() - Duration::hours(1);
    let window_end = Utc::now();

    let ksi = cortex_temporal::drift::compute_ksi(&readers, None, window_start, window_end)
        .unwrap();
    assert!(
        (ksi - 1.0).abs() < 0.001,
        "KSI should be 1.0 for stable dataset, got {}",
        ksi
    );
}

// ── TTD1-02: KSI bounds [0.0, 1.0] ──────────────────────────────────────

#[tokio::test]
async fn ttd1_02_ksi_bounds() {
    let (writer, readers) = setup();

    // Insert memories and many change events to push KSI low
    for i in 0..5 {
        let mid = format!("bounds-{}", i);
        insert_typed_memory(&writer, &mid, "episodic", 0.8, false).await;
    }

    let window_start = Utc::now() - Duration::hours(1);
    let window_end = Utc::now() + Duration::seconds(1);

    // Insert many change events in window
    for i in 0..50 {
        let mid = format!("bounds-{}", i % 5);
        let mut e = make_test_event(&mid, MemoryEventType::ContentUpdated);
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let ksi = cortex_temporal::drift::compute_ksi(&readers, None, window_start, window_end)
        .unwrap();
    assert!(ksi >= 0.0, "KSI must be >= 0.0, got {}", ksi);
    assert!(ksi <= 1.0, "KSI must be <= 1.0, got {}", ksi);
}

// ── TTD1-03: KSI per type is independent ─────────────────────────────────

#[tokio::test]
async fn ttd1_03_ksi_per_type_independent() {
    let (writer, readers) = setup();

    // Insert core and episodic memories
    for i in 0..5 {
        insert_typed_memory(&writer, &format!("core-{}", i), "core", 0.9, false).await;
        insert_typed_memory(&writer, &format!("epi-{}", i), "episodic", 0.7, false).await;
    }

    let window_start = Utc::now() - Duration::hours(1);

    // Only change episodic memories
    for i in 0..20 {
        let mid = format!("epi-{}", i % 5);
        let mut e = make_test_event(&mid, MemoryEventType::ContentUpdated);
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let window_end = Utc::now() + Duration::seconds(1);

    let core_ksi = cortex_temporal::drift::compute_ksi(
        &readers,
        Some(MemoryType::Core),
        window_start,
        window_end,
    )
    .unwrap();

    let epi_ksi = cortex_temporal::drift::compute_ksi(
        &readers,
        Some(MemoryType::Episodic),
        window_start,
        window_end,
    )
    .unwrap();

    // Core KSI should be higher (no changes) than episodic KSI
    assert!(
        core_ksi > epi_ksi,
        "Core KSI ({}) should be > Episodic KSI ({})",
        core_ksi,
        epi_ksi
    );
    assert!(
        (core_ksi - 1.0).abs() < 0.001,
        "Core KSI should be ~1.0, got {}",
        core_ksi
    );
}

// ── TTD1-04: Confidence trajectory tracks correctly ──────────────────────

#[tokio::test]
async fn ttd1_04_confidence_trajectory() {
    let (writer, readers) = setup();

    // Insert memories with known confidence
    for i in 0..10 {
        insert_typed_memory(&writer, &format!("traj-{}", i), "episodic", 0.8, false).await;
    }

    let window_start = Utc::now() - Duration::hours(2);
    let window_end = Utc::now();

    let trajectory = cortex_temporal::drift::compute_confidence_trajectory(
        &readers,
        None,
        window_start,
        window_end,
        5,
    )
    .unwrap();

    assert_eq!(trajectory.len(), 5, "Should have 5 sample points");
    // All points should be around 0.8 since we haven't changed confidence
    for (i, val) in trajectory.iter().enumerate() {
        assert!(
            *val >= 0.0 && *val <= 1.0,
            "Trajectory point {} = {} out of bounds",
            i,
            val
        );
    }
}

// ── TTD1-05: Contradiction density = 0 for clean dataset ─────────────────

#[tokio::test]
async fn ttd1_05_contradiction_density_clean() {
    let (writer, readers) = setup();

    for i in 0..10 {
        insert_typed_memory(&writer, &format!("clean-{}", i), "episodic", 0.8, false).await;
    }

    let window_start = Utc::now() - Duration::hours(1);
    let window_end = Utc::now();

    let density = cortex_temporal::drift::compute_contradiction_density(
        &readers,
        None,
        window_start,
        window_end,
    )
    .unwrap();

    assert!(
        (density - 0.0).abs() < 0.001,
        "Contradiction density should be 0 for clean dataset, got {}",
        density
    );
}

// ── TTD1-06: Consolidation efficiency computes correctly ─────────────────

#[tokio::test]
async fn ttd1_06_consolidation_efficiency() {
    let (writer, readers) = setup();

    // Insert semantic and episodic memories
    for i in 0..5 {
        insert_typed_memory(&writer, &format!("sem-{}", i), "semantic", 0.8, false).await;
        insert_typed_memory(&writer, &format!("epi-c-{}", i), "episodic", 0.7, false).await;
    }

    let window_start = Utc::now() - Duration::hours(1);
    let window_end = Utc::now() + Duration::seconds(1);

    // Create 3 semantic memories and archive 6 episodic → efficiency = 3/6 = 0.5
    for i in 0..3 {
        let mid = format!("sem-new-{}", i);
        insert_typed_memory(&writer, &mid, "semantic", 0.8, false).await;
        let mut e = make_test_event(&mid, MemoryEventType::Created);
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    for i in 0..6 {
        let mid = format!("epi-arch-{}", i);
        insert_typed_memory(&writer, &mid, "episodic", 0.5, true).await;
        let mut e = make_test_event(&mid, MemoryEventType::Archived);
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let efficiency = cortex_temporal::drift::compute_consolidation_efficiency(
        &readers,
        window_start,
        window_end,
    )
    .unwrap();

    assert!(
        (efficiency - 0.5).abs() < 0.01,
        "Consolidation efficiency should be ~0.5, got {}",
        efficiency
    );
}

// ── TTD1-07: Evidence freshness = 1.0 for fresh evidence ─────────────────

#[test]
fn ttd1_07_evidence_freshness_fresh() {
    // All factors fresh → product = 1.0
    let factors = vec![1.0, 1.0, 1.0];
    let freshness = cortex_temporal::drift::evidence_freshness::compute_evidence_freshness(&factors);
    assert!((freshness - 1.0).abs() < 0.001);

    // Empty factors → 1.0
    let empty: Vec<f64> = vec![];
    let freshness = cortex_temporal::drift::evidence_freshness::compute_evidence_freshness(&empty);
    assert!((freshness - 1.0).abs() < 0.001);
}

// ── TTD1-08: Evidence freshness < 1.0 for stale links ────────────────────

#[test]
fn ttd1_08_evidence_freshness_stale() {
    use cortex_core::memory::FileLink;

    // File link with mismatched hash → 0.5
    let link = FileLink {
        file_path: "src/main.rs".to_string(),
        line_start: Some(1),
        line_end: Some(10),
        content_hash: Some("old_hash".to_string()),
    };
    let freshness =
        cortex_temporal::drift::evidence_freshness::file_link_freshness(&link, Some("new_hash"));
    assert!((freshness - 0.5).abs() < 0.001);

    // File link with matching hash → 1.0
    let freshness =
        cortex_temporal::drift::evidence_freshness::file_link_freshness(&link, Some("old_hash"));
    assert!((freshness - 1.0).abs() < 0.001);

    // Pattern link inactive → 0.3
    let pattern = cortex_core::memory::PatternLink {
        pattern_id: "p1".to_string(),
        pattern_name: "test".to_string(),
    };
    let freshness =
        cortex_temporal::drift::evidence_freshness::pattern_link_freshness(&pattern, false);
    assert!((freshness - 0.3).abs() < 0.001);

    // Pattern link active → 1.0
    let freshness =
        cortex_temporal::drift::evidence_freshness::pattern_link_freshness(&pattern, true);
    assert!((freshness - 1.0).abs() < 0.001);

    // Product aggregation with stale factor
    let factors = vec![1.0, 0.5, 0.3];
    let freshness = cortex_temporal::drift::evidence_freshness::compute_evidence_freshness(&factors);
    assert!((freshness - 0.15).abs() < 0.001, "Expected 0.15, got {}", freshness);
}

// ── TTD1-09: Evidence freshness bounds [0.0, 1.0] ───────────────────────

#[test]
fn ttd1_09_evidence_freshness_bounds() {
    // Test with various factor combinations
    let test_cases: Vec<Vec<f64>> = vec![
        vec![0.0],
        vec![1.0],
        vec![0.5, 0.5],
        vec![0.1, 0.2, 0.3],
        vec![1.0, 1.0, 1.0, 1.0],
        vec![0.0, 1.0],
        vec![],
    ];

    for factors in test_cases {
        let freshness =
            cortex_temporal::drift::evidence_freshness::compute_evidence_freshness(&factors);
        assert!(
            (0.0..=1.0).contains(&freshness),
            "Freshness {} out of bounds for factors {:?}",
            freshness,
            factors
        );
    }

    // User validation freshness bounds
    let now = Utc::now();
    for days_ago in [0, 1, 30, 90, 180, 365, 1000] {
        let validated_at = now - Duration::days(days_ago);
        let freshness =
            cortex_temporal::drift::evidence_freshness::user_validation_freshness(validated_at, now);
        assert!(
            (0.0..=1.0).contains(&freshness),
            "User validation freshness {} out of bounds for {} days ago",
            freshness,
            days_ago
        );
    }
}

// ── TTD1-10: Alert fires when KSI below threshold ───────────────────────

#[test]
fn ttd1_10_alert_fires_ksi_below_threshold() {
    use std::collections::HashMap;

    let config = TemporalConfig::default(); // alert_ksi_threshold = 0.3

    // Create a snapshot with low KSI for Core type
    let mut type_metrics = HashMap::new();
    type_metrics.insert(
        MemoryType::Core,
        TypeDriftMetrics {
            count: 10,
            avg_confidence: 0.7,
            ksi: 0.2, // Below threshold of 0.3
            contradiction_density: 0.0,
            consolidation_efficiency: 0.0,
            evidence_freshness_index: 0.9,
        },
    );

    let snapshot = DriftSnapshot {
        timestamp: Utc::now(),
        window_hours: 168,
        type_metrics,
        module_metrics: HashMap::new(),
        global: GlobalDriftMetrics {
            total_memories: 10,
            active_memories: 10,
            archived_memories: 0,
            avg_confidence: 0.7,
            overall_ksi: 0.5,
            overall_contradiction_density: 0.0,
            overall_evidence_freshness: 0.9,
        },
    };

    let alerts = cortex_temporal::drift::evaluate_drift_alerts(&snapshot, &config, &[]);
    assert!(
        alerts.iter().any(|a| a.category == DriftAlertCategory::KnowledgeChurn),
        "Should fire KnowledgeChurn alert when KSI < threshold"
    );
}

// ── TTD1-11: Alert dampening works ───────────────────────────────────────

#[test]
fn ttd1_11_alert_dampening() {
    use std::collections::HashMap;

    let config = TemporalConfig::default();

    let mut type_metrics = HashMap::new();
    type_metrics.insert(
        MemoryType::Core,
        TypeDriftMetrics {
            count: 10,
            avg_confidence: 0.7,
            ksi: 0.2,
            contradiction_density: 0.0,
            consolidation_efficiency: 0.0,
            evidence_freshness_index: 0.9,
        },
    );

    let snapshot = DriftSnapshot {
        timestamp: Utc::now(),
        window_hours: 168,
        type_metrics,
        module_metrics: HashMap::new(),
        global: GlobalDriftMetrics {
            total_memories: 10,
            active_memories: 10,
            archived_memories: 0,
            avg_confidence: 0.7,
            overall_ksi: 0.5,
            overall_contradiction_density: 0.0,
            overall_evidence_freshness: 0.9,
        },
    };

    // First evaluation — should fire
    let alerts1 = cortex_temporal::drift::evaluate_drift_alerts(&snapshot, &config, &[]);
    assert!(!alerts1.is_empty(), "First evaluation should fire alerts");

    // Second evaluation with recent alerts — should be dampened
    let alerts2 = cortex_temporal::drift::evaluate_drift_alerts(&snapshot, &config, &alerts1);
    let churn_alerts: Vec<_> = alerts2
        .iter()
        .filter(|a| a.category == DriftAlertCategory::KnowledgeChurn)
        .collect();
    assert!(
        churn_alerts.is_empty(),
        "Same alert within cooldown should be dampened"
    );
}

// ── TTD1-12: Critical alert has shorter cooldown ─────────────────────────

#[test]
fn ttd1_12_critical_shorter_cooldown() {
    use std::collections::HashMap;

    let config = TemporalConfig::default();

    // Snapshot with contradiction spike (Critical severity)
    let snapshot = DriftSnapshot {
        timestamp: Utc::now(),
        window_hours: 168,
        type_metrics: HashMap::new(),
        module_metrics: HashMap::new(),
        global: GlobalDriftMetrics {
            total_memories: 100,
            active_memories: 100,
            archived_memories: 0,
            avg_confidence: 0.7,
            overall_ksi: 0.8,
            overall_contradiction_density: 0.15, // Above 0.10 threshold
            overall_evidence_freshness: 0.9,
        },
    };

    let alerts = cortex_temporal::drift::evaluate_drift_alerts(&snapshot, &config, &[]);
    let critical: Vec<_> = alerts
        .iter()
        .filter(|a| a.severity == AlertSeverity::Critical)
        .collect();
    assert!(
        !critical.is_empty(),
        "Should fire Critical alert for contradiction spike"
    );

    // Simulate an old critical alert (2 hours ago — past 1h cooldown)
    let mut old_alert = critical[0].clone().clone();
    old_alert.detected_at = Utc::now() - Duration::hours(2);

    let alerts2 =
        cortex_temporal::drift::evaluate_drift_alerts(&snapshot, &config, &[old_alert]);
    let critical2: Vec<_> = alerts2
        .iter()
        .filter(|a| a.severity == AlertSeverity::Critical)
        .collect();
    assert!(
        !critical2.is_empty(),
        "Critical alert should re-fire after 1h cooldown (2h elapsed)"
    );
}

// ── TTD1-13: Drift snapshot round-trip ───────────────────────────────────

#[tokio::test]
async fn ttd1_13_drift_snapshot_round_trip() {
    use std::collections::HashMap;

    let (writer, readers) = setup();

    let mut type_metrics = HashMap::new();
    type_metrics.insert(
        MemoryType::Episodic,
        TypeDriftMetrics {
            count: 42,
            avg_confidence: 0.75,
            ksi: 0.85,
            contradiction_density: 0.01,
            consolidation_efficiency: 0.6,
            evidence_freshness_index: 0.92,
        },
    );

    let original = DriftSnapshot {
        timestamp: Utc::now(),
        window_hours: 168,
        type_metrics,
        module_metrics: HashMap::new(),
        global: GlobalDriftMetrics {
            total_memories: 100,
            active_memories: 80,
            archived_memories: 20,
            avg_confidence: 0.75,
            overall_ksi: 0.85,
            overall_contradiction_density: 0.01,
            overall_evidence_freshness: 0.92,
        },
    };

    // Store
    let id = cortex_temporal::drift::store_drift_snapshot(&writer, &original).await.unwrap();
    assert!(id > 0);

    // Retrieve
    let retrieved = cortex_temporal::drift::get_latest_drift_snapshot(&readers)
        .unwrap()
        .expect("Should find stored snapshot");

    assert_eq!(original, retrieved, "Round-trip must be lossless");
}

// ── TTD1-14: Crystallization detection ───────────────────────────────────

#[tokio::test]
async fn ttd1_14_crystallization_detection() {
    let (writer, readers) = setup();

    let window_start = Utc::now() - Duration::hours(24);
    let window_end = Utc::now() + Duration::seconds(1);

    // Insert a memory that was reclassified from episodic to semantic
    let mid = "crystal-1";
    insert_typed_memory(&writer, mid, "semantic", 0.8, false).await;
    let mut e = make_test_event(mid, MemoryEventType::Reclassified);
    e.delta = serde_json::json!({
        "old_type": "episodic",
        "new_type": "semantic",
        "confidence": 0.8
    });
    e.recorded_at = Utc::now();
    cortex_temporal::event_store::append::append(&writer, &e)
        .await
        .unwrap();

    let result =
        cortex_temporal::drift::detect_crystallization(&readers, window_start, window_end)
            .unwrap();

    assert!(result.is_some(), "Should detect crystallization pattern");
    let pattern = result.unwrap();
    assert!(!pattern.memory_ids.is_empty());
}

// ── TTD1-15: Erosion detection ───────────────────────────────────────────

#[tokio::test]
async fn ttd1_15_erosion_detection() {
    let (writer, readers) = setup();

    let window_start = Utc::now() - Duration::hours(24);
    let window_end = Utc::now() + Duration::seconds(1);

    // Insert a memory with multiple decay events
    let mid = "erode-1";
    insert_typed_memory(&writer, mid, "episodic", 0.5, false).await;

    for i in 0..5 {
        let mut e = make_test_event(mid, MemoryEventType::Decayed);
        e.delta = serde_json::json!({
            "old_confidence": 0.8 - (i as f64 * 0.05),
            "new_confidence": 0.8 - ((i + 1) as f64 * 0.05)
        });
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let result =
        cortex_temporal::drift::detect_erosion(&readers, window_start, window_end).unwrap();

    assert!(result.is_some(), "Should detect erosion pattern");
    let pattern = result.unwrap();
    assert!(!pattern.affected_memories.is_empty());
    assert!(pattern.declining_windows >= 2);
}

// ── TTD1-16: Explosion detection ─────────────────────────────────────────

#[tokio::test]
async fn ttd1_16_explosion_detection() {
    let (writer, readers) = setup();

    // Create a baseline: 10 memories created 5 days ago (within 3× window)
    for i in 0..10 {
        let mid = format!("baseline-{}", i);
        insert_typed_memory(&writer, &mid, "episodic", 0.8, false).await;
        let mut e = make_test_event(&mid, MemoryEventType::Created);
        e.recorded_at = Utc::now() - Duration::days(3);
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    // Create a spike: 50 memories in the last day
    for i in 0..50 {
        let mid = format!("spike-{}", i);
        insert_typed_memory(&writer, &mid, "episodic", 0.8, false).await;
        let mut e = make_test_event(&mid, MemoryEventType::Created);
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let window_start = Utc::now() - Duration::days(1);
    let window_end = Utc::now() + Duration::seconds(1);

    let result = cortex_temporal::drift::detect_explosion(
        &readers,
        window_start,
        window_end,
        3.0, // sigma threshold
    )
    .unwrap();

    assert!(result.is_some(), "Should detect explosion pattern");
    let pattern = result.unwrap();
    assert!(pattern.current_rate > pattern.baseline_rate);
    assert!(pattern.sigma_above > 3.0);
}

// ── TTD1-17: Conflict wave detection ─────────────────────────────────────

#[tokio::test]
async fn ttd1_17_conflict_wave_detection() {
    let (writer, readers) = setup();

    let window_start = Utc::now() - Duration::hours(24);
    let window_end = Utc::now() + Duration::seconds(1);

    // Insert memories and contradiction events
    for i in 0..10 {
        let mid = format!("conflict-{}", i);
        insert_typed_memory(&writer, &mid, "semantic", 0.8, false).await;

        let mut e = make_test_event(&mid, MemoryEventType::RelationshipAdded);
        e.delta = serde_json::json!({
            "source": mid,
            "target": format!("conflict-{}", (i + 1) % 10),
            "relation_type": "contradicts",
            "strength": 0.9
        });
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let result =
        cortex_temporal::drift::detect_conflict_wave(&readers, window_start, window_end)
            .unwrap();

    assert!(result.is_some(), "Should detect conflict wave pattern");
    let pattern = result.unwrap();
    assert!(pattern.density > pattern.baseline_density);
}

// ── TTD1-Extra: compute_all_metrics assembles full snapshot ──────────────

#[tokio::test]
async fn ttd1_extra_compute_all_metrics() {
    let (writer, readers) = setup();

    for i in 0..5 {
        insert_typed_memory(&writer, &format!("all-{}", i), "episodic", 0.8, false).await;
    }

    let window_start = Utc::now() - Duration::hours(1);
    let window_end = Utc::now();

    let snapshot =
        cortex_temporal::drift::metrics::compute_all_metrics(&readers, window_start, window_end)
            .unwrap();

    assert_eq!(snapshot.global.active_memories, 5);
    assert!(snapshot.global.overall_ksi >= 0.0 && snapshot.global.overall_ksi <= 1.0);
    assert!(snapshot.global.avg_confidence > 0.0);
}

// ── TTD1-Extra: Engine compute_drift_metrics ─────────────────────────────

#[tokio::test]
async fn ttd1_extra_engine_compute_drift_metrics() {
    use cortex_core::traits::ITemporalEngine;

    let (writer, readers) = setup();

    for i in 0..3 {
        insert_typed_memory(&writer, &format!("eng-{}", i), "episodic", 0.8, false).await;
    }

    let config = TemporalConfig::default();
    let engine = cortex_temporal::TemporalEngine::new(writer, readers, config);

    let snapshot = engine.compute_drift_metrics(168).await.unwrap();
    assert!(snapshot.global.active_memories >= 3);
}

// ── TTD1-Extra: Engine get_drift_alerts ──────────────────────────────────

#[tokio::test]
async fn ttd1_extra_engine_get_drift_alerts() {
    use cortex_core::traits::ITemporalEngine;

    let (writer, readers) = setup();

    for i in 0..3 {
        insert_typed_memory(&writer, &format!("alert-eng-{}", i), "episodic", 0.8, false).await;
    }

    let config = TemporalConfig::default();
    let engine = cortex_temporal::TemporalEngine::new(writer, readers, config);

    // Should not error even with minimal data
    let alerts = engine.get_drift_alerts().await.unwrap();
    // Alerts may or may not fire depending on data — just verify no panic
    let _ = alerts;
}

// ── TTD1-Extra: Drift snapshot range query ───────────────────────────────

#[tokio::test]
async fn ttd1_extra_drift_snapshot_range_query() {
    use std::collections::HashMap;

    let (writer, readers) = setup();

    // Store 3 snapshots at different times
    for i in 0..3 {
        let snapshot = DriftSnapshot {
            timestamp: Utc::now() - Duration::hours(i),
            window_hours: 168,
            type_metrics: HashMap::new(),
            module_metrics: HashMap::new(),
            global: GlobalDriftMetrics {
                total_memories: 100 + i as usize,
                active_memories: 80,
                archived_memories: 20,
                avg_confidence: 0.75,
                overall_ksi: 0.85,
                overall_contradiction_density: 0.01,
                overall_evidence_freshness: 0.92,
            },
        };
        cortex_temporal::drift::store_drift_snapshot(&writer, &snapshot).await.unwrap();
    }

    let from = Utc::now() - Duration::hours(5);
    let to = Utc::now() + Duration::hours(1);
    let snapshots = cortex_temporal::drift::get_drift_snapshots(&readers, from, to).unwrap();
    assert_eq!(snapshots.len(), 3);
}
