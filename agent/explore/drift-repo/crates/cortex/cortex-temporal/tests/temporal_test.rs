//! Phase A tests: event store + snapshot + reconstruction.
//! TTA-01 through TTA-21 (TTA-22..TTA-27 are benchmarks in benches/).

use chrono::{Duration, Utc};
use cortex_core::config::TemporalConfig;
use cortex_core::memory::*;
use cortex_core::models::*;
use cortex_storage::pool::{ReadPool, WriteConnection};
use std::sync::Arc;

/// Test harness: file-backed DB with migrations, FK-safe.
///
/// Architecture:
///   1. Open a raw `rusqlite::Connection`, run migrations, close it.
///      Raw connections have `foreign_keys = OFF` by default in SQLite,
///      so no FK issues during DDL. This also avoids the `blocking_lock`
///      panic that `WriteConnection::with_conn_sync` would trigger inside
///      a tokio runtime.
///   2. Re-open the same file via `WriteConnection` + `ReadPool`.
///      `WriteConnection::open` applies pragmas including `foreign_keys = ON`,
///      which is correct — production code should honour FKs.
///   3. Return the `Arc`-wrapped handles the `TemporalEngine` expects.
///
/// FK compliance: tests that insert events for a memory_id MUST first
/// call `ensure_memory_row` to satisfy the FK on `memory_events`.
fn setup() -> (Arc<WriteConnection>, Arc<ReadPool>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_temporal.db");
    let _dir = Box::leak(Box::new(dir)); // prevent cleanup while DB is open

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();
    }

    let writer = Arc::new(WriteConnection::open(&db_path).unwrap());
    let readers = Arc::new(ReadPool::open(&db_path, 2).unwrap());
    (writer, readers)
}

/// Insert a minimal memory row so that FK on `memory_events` is satisfied.
/// Idempotent (INSERT OR IGNORE).
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

/// Synchronous version for `#[test]` (non-async) functions.
/// Uses a raw connection to the same DB file.
fn ensure_memory_row_sync(conn: &rusqlite::Connection, memory_id: &str) {
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
        rusqlite::params![memory_id],
    )
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

fn make_test_memory() -> BaseMemory {
    let content = TypedContent::Episodic(cortex_core::memory::types::EpisodicContent {
        interaction: "test interaction".to_string(),
        context: "test context".to_string(),
        outcome: None,
    });
    BaseMemory {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type: MemoryType::Episodic,
        content: content.clone(),
        summary: "test summary".to_string(),
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
        tags: vec!["test".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

// ── TTA-01: Event append round-trip ──────────────────────────────────────

#[tokio::test]
async fn tta_01_event_append_round_trip() {
    let (writer, readers) = setup();
    let mem_id = "mem-001";
    ensure_memory_row(&writer, mem_id).await;
    let event = make_test_event(mem_id, MemoryEventType::Created);

    let event_id = cortex_temporal::event_store::append::append(&writer, &event)
        .await
        .unwrap();
    assert!(event_id > 0);

    let events = cortex_temporal::event_store::query::get_events(&readers, mem_id, None).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, event_id);
    assert_eq!(events[0].memory_id, mem_id);
    assert_eq!(events[0].event_type, MemoryEventType::Created);
    assert_eq!(events[0].schema_version, 1);
}

// ── TTA-02: Event batch append ───────────────────────────────────────────

#[tokio::test]
async fn tta_02_event_batch_append() {
    let (writer, readers) = setup();
    let mem_id = "mem-batch";
    ensure_memory_row(&writer, mem_id).await;
    let events: Vec<MemoryEvent> = (0..100)
        .map(|i| {
            let mut e = make_test_event(mem_id, MemoryEventType::ContentUpdated);
            e.delta = serde_json::json!({ "index": i });
            e
        })
        .collect();

    let ids = cortex_temporal::event_store::append::append_batch(&writer, &events)
        .await
        .unwrap();
    assert_eq!(ids.len(), 100);

    let queried = cortex_temporal::event_store::query::get_events(&readers, mem_id, None).unwrap();
    assert_eq!(queried.len(), 100);
}

// ── TTA-03: Event query by time range ────────────────────────────────────

#[tokio::test]
async fn tta_03_event_query_by_time_range() {
    let (writer, readers) = setup();
    let mem_id = "mem-time";
    ensure_memory_row(&writer, mem_id).await;

    let t1 = Utc::now() - Duration::hours(3);
    let t2 = Utc::now() - Duration::hours(2);
    let t3 = Utc::now() - Duration::hours(1);

    for (t, et) in [
        (t1, MemoryEventType::Created),
        (t2, MemoryEventType::ContentUpdated),
        (t3, MemoryEventType::TagsModified),
    ] {
        let mut e = make_test_event(mem_id, et);
        e.recorded_at = t;
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    // Query before t2 + 30min should return only t1 and t2 events
    let cutoff = t2 + Duration::minutes(30);
    let events =
        cortex_temporal::event_store::query::get_events(&readers, mem_id, Some(cutoff)).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, MemoryEventType::Created);
    assert_eq!(events[1].event_type, MemoryEventType::ContentUpdated);
}

// ── TTA-04: Event query by type ──────────────────────────────────────────

#[tokio::test]
async fn tta_04_event_query_by_type() {
    let (writer, readers) = setup();

    // Append mixed types across different memories
    for mid in ["m1", "m2", "m3"] {
        ensure_memory_row(&writer, mid).await;
    }
    for (mid, et) in [
        ("m1", MemoryEventType::Created),
        ("m2", MemoryEventType::Created),
        ("m1", MemoryEventType::ContentUpdated),
        ("m3", MemoryEventType::TagsModified),
    ] {
        let e = make_test_event(mid, et);
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let created = cortex_temporal::event_store::query::get_events_by_type(
        &readers,
        &MemoryEventType::Created,
        None,
    )
    .unwrap();
    assert_eq!(created.len(), 2);

    let tags = cortex_temporal::event_store::query::get_events_by_type(
        &readers,
        &MemoryEventType::TagsModified,
        None,
    )
    .unwrap();
    assert_eq!(tags.len(), 1);
}

// ── TTA-05: Event replay produces current state ──────────────────────────

#[tokio::test]
async fn tta_05_event_replay_produces_current_state() {
    let mem = make_test_memory();
    let mem_id = mem.id.clone();

    // Simulate: Created → 5 confidence changes → 3 tag modifications → archive → restore
    let mut events = Vec::new();

    // Created event with full state
    let mut created = make_test_event(&mem_id, MemoryEventType::Created);
    created.delta = serde_json::to_value(&mem).unwrap();
    events.push(created);

    // Confidence changes
    for i in 1..=5 {
        let mut e = make_test_event(&mem_id, MemoryEventType::ConfidenceChanged);
        let new_conf = 0.8 - (i as f64 * 0.05);
        e.delta = serde_json::json!({ "old": 0.8, "new": new_conf });
        events.push(e);
    }

    // Tag modifications
    let mut tag_add = make_test_event(&mem_id, MemoryEventType::TagsModified);
    tag_add.delta = serde_json::json!({ "added": ["important", "reviewed"], "removed": [] });
    events.push(tag_add);

    let mut tag_rm = make_test_event(&mem_id, MemoryEventType::TagsModified);
    tag_rm.delta = serde_json::json!({ "added": [], "removed": ["test"] });
    events.push(tag_rm);

    let mut tag_add2 = make_test_event(&mem_id, MemoryEventType::TagsModified);
    tag_add2.delta = serde_json::json!({ "added": ["final"], "removed": [] });
    events.push(tag_add2);

    // Archive then restore
    events.push(make_test_event(&mem_id, MemoryEventType::Archived));
    events.push(make_test_event(&mem_id, MemoryEventType::Restored));

    let shell = cortex_core::memory::BaseMemory {
        id: mem_id.clone(),
        ..make_test_memory()
    };
    let result = cortex_temporal::event_store::replay::replay_events(&events, shell);

    // Final confidence: 0.8 - 5*0.05 = 0.55
    assert!((result.confidence.value() - 0.55).abs() < 0.001);
    // Tags: started with ["test"], added ["important","reviewed"], removed ["test"], added ["final"]
    assert!(result.tags.contains(&"important".to_string()));
    assert!(result.tags.contains(&"reviewed".to_string()));
    assert!(result.tags.contains(&"final".to_string()));
    assert!(!result.tags.contains(&"test".to_string()));
    // Not archived (restored)
    assert!(!result.archived);
}

// ── TTA-06: Event replay handles all 17 types ───────────────────────────

#[test]
fn tta_06_replay_all_17_event_types() {
    use cortex_temporal::event_store::replay::apply_event;

    let mem = make_test_memory();
    let mid = mem.id.clone();

    // Created
    let mut e = make_test_event(&mid, MemoryEventType::Created);
    e.delta = serde_json::to_value(&mem).unwrap();
    let state = apply_event(make_test_memory(), &e);
    assert_eq!(state.id, mem.id);

    // ContentUpdated
    let mut e = make_test_event(&mid, MemoryEventType::ContentUpdated);
    e.delta = serde_json::json!({ "new_summary": "updated summary", "new_content_hash": "abc123" });
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.summary, "updated summary");
    assert_eq!(state.content_hash, "abc123");

    // ConfidenceChanged
    let mut e = make_test_event(&mid, MemoryEventType::ConfidenceChanged);
    e.delta = serde_json::json!({ "old": 0.8, "new": 0.6 });
    let state = apply_event(mem.clone(), &e);
    assert!((state.confidence.value() - 0.6).abs() < 0.001);

    // ImportanceChanged
    let mut e = make_test_event(&mid, MemoryEventType::ImportanceChanged);
    e.delta = serde_json::json!({ "old": "normal", "new": "critical" });
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.importance, Importance::Critical);

    // TagsModified
    let mut e = make_test_event(&mid, MemoryEventType::TagsModified);
    e.delta = serde_json::json!({ "added": ["new_tag"], "removed": ["test"] });
    let state = apply_event(mem.clone(), &e);
    assert!(state.tags.contains(&"new_tag".to_string()));
    assert!(!state.tags.contains(&"test".to_string()));

    // LinkAdded (no BaseMemory field change)
    let e = make_test_event(&mid, MemoryEventType::LinkAdded);
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.id, mid);

    // LinkRemoved (no BaseMemory field change)
    let e = make_test_event(&mid, MemoryEventType::LinkRemoved);
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.id, mid);

    // RelationshipAdded (no BaseMemory field change)
    let e = make_test_event(&mid, MemoryEventType::RelationshipAdded);
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.id, mid);

    // RelationshipRemoved (no BaseMemory field change)
    let e = make_test_event(&mid, MemoryEventType::RelationshipRemoved);
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.id, mid);

    // StrengthUpdated (no BaseMemory field change)
    let e = make_test_event(&mid, MemoryEventType::StrengthUpdated);
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.id, mid);

    // Archived
    let e = make_test_event(&mid, MemoryEventType::Archived);
    let state = apply_event(mem.clone(), &e);
    assert!(state.archived);

    // Restored
    let mut archived = mem.clone();
    archived.archived = true;
    let e = make_test_event(&mid, MemoryEventType::Restored);
    let state = apply_event(archived, &e);
    assert!(!state.archived);

    // Decayed
    let mut e = make_test_event(&mid, MemoryEventType::Decayed);
    e.delta = serde_json::json!({ "old_confidence": 0.8, "new_confidence": 0.5 });
    let state = apply_event(mem.clone(), &e);
    assert!((state.confidence.value() - 0.5).abs() < 0.001);

    // Validated (no direct field change)
    let e = make_test_event(&mid, MemoryEventType::Validated);
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.id, mid);

    // Consolidated
    let mut e = make_test_event(&mid, MemoryEventType::Consolidated);
    e.delta = serde_json::json!({ "merged_into": "mem-target" });
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.superseded_by, Some("mem-target".to_string()));

    // Reclassified
    let mut e = make_test_event(&mid, MemoryEventType::Reclassified);
    e.delta = serde_json::json!({ "old_type": "episodic", "new_type": "semantic" });
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.memory_type, MemoryType::Semantic);

    // Superseded
    let mut e = make_test_event(&mid, MemoryEventType::Superseded);
    e.delta = serde_json::json!({ "superseded_by": "mem-newer" });
    let state = apply_event(mem.clone(), &e);
    assert_eq!(state.superseded_by, Some("mem-newer".to_string()));
}

// ── TTA-07: Upcaster registry no-op for current version ─────────────────

#[test]
fn tta_07_upcaster_noop_for_current_version() {
    use cortex_storage::queries::event_ops::RawEvent;
    use cortex_temporal::event_store::upcaster::UpcasterRegistry;

    let registry = UpcasterRegistry::with_defaults();
    let raw = RawEvent {
        event_id: 1,
        memory_id: "m1".to_string(),
        recorded_at: Utc::now().to_rfc3339(),
        event_type: "created".to_string(),
        delta: "{}".to_string(),
        actor_type: "system".to_string(),
        actor_id: "test".to_string(),
        caused_by: None,
        schema_version: 1,
    };

    let upcasted = registry.upcast_event(raw.clone());
    assert_eq!(upcasted.event_id, raw.event_id);
    assert_eq!(upcasted.event_type, raw.event_type);
    assert_eq!(upcasted.schema_version, 1);
}

// ── TTA-08: Compaction moves old events ──────────────────────────────────

#[tokio::test]
async fn tta_08_compaction_moves_old_events() {
    let (writer, readers) = setup();
    let mem_id = "mem-compact";
    ensure_memory_row(&writer, mem_id).await;

    // Insert events at various times
    let old_time = Utc::now() - Duration::days(90);
    let recent_time = Utc::now() - Duration::hours(1);

    // 5 old events
    for i in 0..5 {
        let mut e = make_test_event(mem_id, MemoryEventType::ContentUpdated);
        e.recorded_at = old_time + Duration::minutes(i);
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    // Create a snapshot after the old events
    let mem = make_test_memory();
    let snap_id = cortex_temporal::snapshot::create::create_snapshot(
        &writer,
        mem_id,
        &mem,
        SnapshotReason::EventThreshold,
    )
    .await
    .unwrap();
    assert!(snap_id > 0);

    // 3 recent events
    for _ in 0..3 {
        let mut e = make_test_event(mem_id, MemoryEventType::TagsModified);
        e.recorded_at = recent_time;
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    // Count before compaction
    let count_before =
        cortex_temporal::event_store::query::get_event_count(&readers, mem_id).unwrap();
    assert_eq!(count_before, 8);

    // Compact events older than 30 days, with verified snapshot at event_id 5
    let cutoff = Utc::now() - Duration::days(30);
    let result = cortex_temporal::event_store::compaction::compact_events(&writer, cutoff, 5)
        .await
        .unwrap();
    assert_eq!(result.events_moved, 5);

    // After compaction, only recent events remain in main table
    let count_after =
        cortex_temporal::event_store::query::get_event_count(&readers, mem_id).unwrap();
    assert_eq!(count_after, 3);

    // Verify archive table has the old events
    let archive_count: u64 = readers
        .with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM memory_events_archive WHERE memory_id = ?1",
                [mem_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c as u64)
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))
        })
        .unwrap();
    assert_eq!(archive_count, 5);
}

// ── TTA-09: Snapshot creation + lookup ───────────────────────────────────

#[tokio::test]
async fn tta_09_snapshot_creation_and_lookup() {
    let (writer, readers) = setup();
    let mem = make_test_memory();
    let mem_id = mem.id.clone();
    ensure_memory_row(&writer, &mem_id).await;

    // Append an event first so the snapshot has an event_id reference
    let e = make_test_event(&mem_id, MemoryEventType::Created);
    cortex_temporal::event_store::append::append(&writer, &e)
        .await
        .unwrap();

    // Create snapshot
    let snap_id = cortex_temporal::snapshot::create::create_snapshot(
        &writer,
        &mem_id,
        &mem,
        SnapshotReason::OnDemand,
    )
    .await
    .unwrap();
    assert!(snap_id > 0);

    // Lookup
    let found = cortex_temporal::snapshot::lookup::get_nearest_snapshot(
        &readers,
        &mem_id,
        Utc::now() + Duration::seconds(1),
    )
    .unwrap();
    assert!(found.is_some());
    let snap = found.unwrap();
    assert_eq!(snap.memory_id, mem_id);
    assert_eq!(snap.snapshot_reason, SnapshotReason::OnDemand);

    // Decompress and verify state
    let restored = cortex_temporal::snapshot::create::decompress_snapshot(&snap.state).unwrap();
    assert_eq!(restored.id, mem.id);
    assert_eq!(restored.summary, mem.summary);
    assert!((restored.confidence.value() - mem.confidence.value()).abs() < 0.001);
}

// ── TTA-10: Snapshot zstd round-trip ─────────────────────────────────────

#[test]
fn tta_10_snapshot_zstd_round_trip() {
    let mem = make_test_memory();
    let json = serde_json::to_vec(&mem).unwrap();
    let compressed = zstd::encode_all(json.as_slice(), 3).unwrap();

    // Compressed should be smaller (or at least not much larger)
    assert!(compressed.len() < json.len() * 2);

    let decompressed = cortex_temporal::snapshot::create::decompress_snapshot(&compressed).unwrap();
    assert_eq!(decompressed.id, mem.id);
    assert_eq!(decompressed.summary, mem.summary);
    assert_eq!(decompressed.memory_type, mem.memory_type);
    assert_eq!(decompressed.tags, mem.tags);
    assert!((decompressed.confidence.value() - mem.confidence.value()).abs() < 0.001);
}

// ── TTA-11: Reconstruction from events only ──────────────────────────────

#[tokio::test]
async fn tta_11_reconstruction_from_events_only() {
    let (writer, readers) = setup();
    let mem = make_test_memory();
    let mem_id = mem.id.clone();
    ensure_memory_row(&writer, &mem_id).await;

    // Append Created event with full state
    let mut created = make_test_event(&mem_id, MemoryEventType::Created);
    created.delta = serde_json::to_value(&mem).unwrap();
    cortex_temporal::event_store::append::append(&writer, &created)
        .await
        .unwrap();

    // Append some mutations
    let mut conf = make_test_event(&mem_id, MemoryEventType::ConfidenceChanged);
    conf.delta = serde_json::json!({ "old": 0.8, "new": 0.6 });
    cortex_temporal::event_store::append::append(&writer, &conf)
        .await
        .unwrap();

    let mut tags = make_test_event(&mem_id, MemoryEventType::TagsModified);
    tags.delta = serde_json::json!({ "added": ["reconstructed"], "removed": [] });
    cortex_temporal::event_store::append::append(&writer, &tags)
        .await
        .unwrap();

    // Reconstruct (no snapshots exist)
    let result = cortex_temporal::snapshot::reconstruct::reconstruct_at(
        &readers,
        &mem_id,
        Utc::now() + Duration::seconds(1),
    )
    .unwrap();
    assert!(result.is_some());
    let state = result.unwrap();
    assert_eq!(state.id, mem_id);
    assert!((state.confidence.value() - 0.6).abs() < 0.001);
    assert!(state.tags.contains(&"reconstructed".to_string()));
}

// ── TTA-12: Reconstruction from snapshot + events ────────────────────────

#[tokio::test]
async fn tta_12_reconstruction_from_snapshot_plus_events() {
    let (writer, readers) = setup();
    let mem = make_test_memory();
    let mem_id = mem.id.clone();
    ensure_memory_row(&writer, &mem_id).await;

    // Append Created event
    let mut created = make_test_event(&mem_id, MemoryEventType::Created);
    created.delta = serde_json::to_value(&mem).unwrap();
    cortex_temporal::event_store::append::append(&writer, &created)
        .await
        .unwrap();

    // Create snapshot at current state
    cortex_temporal::snapshot::create::create_snapshot(
        &writer,
        &mem_id,
        &mem,
        SnapshotReason::EventThreshold,
    )
    .await
    .unwrap();

    // Append more events AFTER snapshot
    let mut conf = make_test_event(&mem_id, MemoryEventType::ConfidenceChanged);
    conf.delta = serde_json::json!({ "old": 0.8, "new": 0.3 });
    cortex_temporal::event_store::append::append(&writer, &conf)
        .await
        .unwrap();

    let arch = make_test_event(&mem_id, MemoryEventType::Archived);
    cortex_temporal::event_store::append::append(&writer, &arch)
        .await
        .unwrap();

    // Reconstruct — should use snapshot + replay 2 events
    let result = cortex_temporal::snapshot::reconstruct::reconstruct_at(
        &readers,
        &mem_id,
        Utc::now() + Duration::seconds(1),
    )
    .unwrap();
    assert!(result.is_some());
    let state = result.unwrap();
    assert!((state.confidence.value() - 0.3).abs() < 0.001);
    assert!(state.archived);
}

// ── TTA-13: Reconstruction snapshot+replay == full replay (property-like) ─

#[tokio::test]
async fn tta_13_snapshot_replay_equals_full_replay() {
    let (writer, readers) = setup();
    let mem = make_test_memory();
    let mem_id = mem.id.clone();
    ensure_memory_row(&writer, &mem_id).await;

    // Append Created
    let mut created = make_test_event(&mem_id, MemoryEventType::Created);
    created.delta = serde_json::to_value(&mem).unwrap();
    cortex_temporal::event_store::append::append(&writer, &created)
        .await
        .unwrap();

    // 10 confidence changes
    for i in 1..=10 {
        let mut e = make_test_event(&mem_id, MemoryEventType::ConfidenceChanged);
        let new_conf = 0.8 - (i as f64 * 0.05);
        e.delta = serde_json::json!({ "old": 0.8, "new": new_conf });
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();

        // Create snapshot at event 5
        if i == 5 {
            let mut snap_state = mem.clone();
            snap_state.confidence = Confidence::new(new_conf);
            cortex_temporal::snapshot::create::create_snapshot(
                &writer,
                &mem_id,
                &snap_state,
                SnapshotReason::EventThreshold,
            )
            .await
            .unwrap();
        }
    }

    let target = Utc::now() + Duration::seconds(1);

    // Full replay (get all events, replay from shell)
    let all_events =
        cortex_temporal::event_store::query::get_events(&readers, &mem_id, Some(target)).unwrap();
    let shell = make_test_memory();
    let full_replay = cortex_temporal::event_store::replay::replay_events(&all_events, shell);

    // Snapshot + replay reconstruction
    let reconstructed =
        cortex_temporal::snapshot::reconstruct::reconstruct_at(&readers, &mem_id, target)
            .unwrap()
            .unwrap();

    // Both should produce the same confidence
    assert!(
        (full_replay.confidence.value() - reconstructed.confidence.value()).abs() < 0.001,
        "full_replay={} vs reconstructed={}",
        full_replay.confidence.value(),
        reconstructed.confidence.value()
    );
}

// ── TTA-14: Retention policy deletes old snapshots ───────────────────────

#[tokio::test]
async fn tta_14_retention_policy_deletes_old_snapshots() {
    let (writer, readers) = setup();
    let mem = make_test_memory();
    let mem_id = mem.id.clone();
    ensure_memory_row(&writer, &mem_id).await;

    // Create snapshots at various ages by inserting directly
    let ages_days = [1, 10, 40, 100, 200, 400];
    for days in ages_days {
        let snapshot_at = (Utc::now() - Duration::days(days)).to_rfc3339();
        let state_json = serde_json::to_vec(&mem).unwrap();
        let compressed = zstd::encode_all(state_json.as_slice(), 3).unwrap();
        writer
            .with_conn({
                let mid = mem_id.clone();
                let snap_at = snapshot_at.clone();
                let comp = compressed.clone();
                move |conn| {
                    cortex_storage::queries::snapshot_ops::insert_snapshot(
                        conn, &mid, &snap_at, &comp, 0, "periodic",
                    )
                }
            })
            .await
            .unwrap();
    }

    // Verify all 6 exist
    let before =
        cortex_temporal::snapshot::lookup::get_snapshots_for_memory(&readers, &mem_id).unwrap();
    assert_eq!(before.len(), 6);

    // Apply retention: full_days=30, monthly_days=180
    let config = TemporalConfig {
        snapshot_retention_full_days: 30,
        snapshot_retention_monthly_days: 180,
        ..Default::default()
    };
    let result = cortex_temporal::snapshot::retention::apply_retention_policy(&writer, &config)
        .await
        .unwrap();
    assert!(result.snapshots_deleted > 0);

    // After retention, very old snapshots (>180 days) should be gone
    let after =
        cortex_temporal::snapshot::lookup::get_snapshots_for_memory(&readers, &mem_id).unwrap();
    assert!(after.len() < before.len());
}

// ── TTA-15: Adaptive trigger fires at threshold ──────────────────────────

#[tokio::test]
async fn tta_15_adaptive_trigger_fires_at_threshold() {
    let (writer, readers) = setup();
    let mem_id = "mem-trigger";
    ensure_memory_row(&writer, mem_id).await;

    let config = TemporalConfig {
        snapshot_event_threshold: 10, // Lower threshold for testing
        ..Default::default()
    };
    let trigger = cortex_temporal::snapshot::triggers::AdaptiveSnapshotTrigger::new(config);

    // Insert 9 events — should NOT trigger
    for _ in 0..9 {
        let e = make_test_event(mem_id, MemoryEventType::ContentUpdated);
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }
    let should = trigger.should_snapshot(&readers, mem_id).unwrap();
    assert!(should.is_none());

    // Insert 1 more — should trigger
    let e = make_test_event(mem_id, MemoryEventType::ContentUpdated);
    cortex_temporal::event_store::append::append(&writer, &e)
        .await
        .unwrap();
    let should = trigger.should_snapshot(&readers, mem_id).unwrap();
    assert!(should.is_some());
    assert_eq!(should.unwrap(), SnapshotReason::EventThreshold);
}

// ── TTA-16: Mutation paths emit events ───────────────────────────────────

#[test]
fn tta_16_mutation_paths_emit_events() {
    let (writer, _readers) = setup();
    // with_conn_sync is safe here — no tokio runtime active.
    writer
        .with_conn_sync(|conn| {
            let mem = make_test_memory();
            ensure_memory_row_sync(conn, &mem.id);

            // Emit events via the temporal_events helper
            let delta = serde_json::json!({ "full_state": true });
            cortex_storage::temporal_events::emit_event(
                conn, &mem.id, "created", &delta, "system", "test",
            )?;

            let delta2 = serde_json::json!({ "old": 0.8, "new": 0.6 });
            cortex_storage::temporal_events::emit_event(
                conn,
                &mem.id,
                "confidence_changed",
                &delta2,
                "system",
                "test",
            )?;

            let delta3 = serde_json::json!({ "archived": true });
            cortex_storage::temporal_events::emit_event(
                conn, &mem.id, "archived", &delta3, "system", "test",
            )?;

            // Verify events exist
            let count = cortex_storage::queries::event_ops::get_event_count(conn, &mem.id)?;
            assert_eq!(count, 3);

            let events =
                cortex_storage::queries::event_ops::get_events_for_memory(conn, &mem.id, None)?;
            assert_eq!(events.len(), 3);
            assert_eq!(events[0].event_type, "created");
            assert_eq!(events[1].event_type, "confidence_changed");
            assert_eq!(events[2].event_type, "archived");

            Ok(())
        })
        .unwrap();
}

// ── TTA-17: Migration v014 runs cleanly ──────────────────────────────────

#[test]
fn tta_17_migration_v014_runs_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_migration.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Run all migrations
    cortex_storage::migrations::run_migrations(&conn).unwrap();

    // Verify temporal tables exist
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert!(tables.contains(&"memory_events".to_string()));
    assert!(tables.contains(&"memory_events_archive".to_string()));
    assert!(tables.contains(&"memory_snapshots".to_string()));
    assert!(tables.contains(&"drift_snapshots".to_string()));
    assert!(tables.contains(&"materialized_views".to_string()));

    // Verify we can insert into memory_events (need a memory row first for FK)
    conn.execute(
        "INSERT INTO memories (id, memory_type, content, summary, transaction_time, valid_time, confidence, importance, last_accessed, access_count, archived, content_hash)
         VALUES ('test', 'Episodic', '{}', 'test', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 0.8, 'Normal', '2025-01-01T00:00:00Z', 0, 0, 'hash')",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO memory_events (memory_id, recorded_at, event_type, delta, actor_type, actor_id, schema_version)
         VALUES ('test', '2025-01-01T00:00:00Z', 'created', '{}', 'system', 'test', 1)",
        [],
    )
    .unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

// ── TTA-18: No existing test regressions ─────────────────────────────────
// This is verified by running `cargo test --workspace` — not a unit test.
// Placeholder to document the requirement.

#[test]
fn tta_18_workspace_regression_marker() {
    // This test exists to mark TTA-18. The actual verification is:
    // `cargo test --workspace --exclude cortex-privacy` passes.
    // Intentionally empty — the real check is the CI workspace test run.
}

// ── TTA-19: Property test: replay consistency ────────────────────────────
// (replay(events) == apply_one_by_one(events))

#[test]
fn tta_19_replay_consistency() {
    let mem = make_test_memory();
    let mid = mem.id.clone();

    let events: Vec<MemoryEvent> = vec![
        {
            let mut e = make_test_event(&mid, MemoryEventType::Created);
            e.delta = serde_json::to_value(&mem).unwrap();
            e
        },
        {
            let mut e = make_test_event(&mid, MemoryEventType::ConfidenceChanged);
            e.delta = serde_json::json!({ "old": 0.8, "new": 0.7 });
            e
        },
        {
            let mut e = make_test_event(&mid, MemoryEventType::TagsModified);
            e.delta = serde_json::json!({ "added": ["a", "b"], "removed": [] });
            e
        },
        {
            let mut e = make_test_event(&mid, MemoryEventType::ConfidenceChanged);
            e.delta = serde_json::json!({ "old": 0.7, "new": 0.5 });
            e
        },
        make_test_event(&mid, MemoryEventType::Archived),
        make_test_event(&mid, MemoryEventType::Restored),
    ];

    // Batch replay
    let batch_result =
        cortex_temporal::event_store::replay::replay_events(&events, make_test_memory());

    // One-by-one replay
    let mut one_by_one = make_test_memory();
    for event in &events {
        one_by_one = cortex_temporal::event_store::replay::apply_event(one_by_one, event);
    }

    assert!((batch_result.confidence.value() - one_by_one.confidence.value()).abs() < 0.001);
    assert_eq!(batch_result.archived, one_by_one.archived);
    assert_eq!(batch_result.tags, one_by_one.tags);
}

// ── TTA-20: Property test: temporal monotonicity ─────────────────────────

#[tokio::test]
async fn tta_20_temporal_monotonicity() {
    let (writer, readers) = setup();
    let mem_id = "mem-mono";
    ensure_memory_row(&writer, mem_id).await;

    let mut ids = Vec::new();
    for _ in 0..20 {
        let e = make_test_event(mem_id, MemoryEventType::ContentUpdated);
        let id = cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
        ids.push(id);
    }

    // event_ids must be strictly increasing
    for window in ids.windows(2) {
        assert!(
            window[1] > window[0],
            "event_ids not monotonically increasing: {} <= {}",
            window[1],
            window[0]
        );
    }

    // Also verify via query
    let events = cortex_temporal::event_store::query::get_events(&readers, mem_id, None).unwrap();
    for window in events.windows(2) {
        assert!(window[1].event_id > window[0].event_id);
    }
}

// ── TTA-21: Property test: event count conservation ──────────────────────

#[tokio::test]
async fn tta_21_event_count_conservation() {
    let (writer, readers) = setup();
    let mem_id = "mem-count";
    ensure_memory_row(&writer, mem_id).await;
    let n = 42u64;

    for _ in 0..n {
        let e = make_test_event(mem_id, MemoryEventType::ContentUpdated);
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let count = cortex_temporal::event_store::query::get_event_count(&readers, mem_id).unwrap();
    assert_eq!(count, n);

    let events = cortex_temporal::event_store::query::get_events(&readers, mem_id, None).unwrap();
    assert_eq!(events.len() as u64, n);
}

// ── TTA-Extra: Reconstruction returns None for unknown memory ────────────

#[tokio::test]
async fn reconstruction_returns_none_for_unknown_memory() {
    let (_writer, readers) = setup();
    let result =
        cortex_temporal::snapshot::reconstruct::reconstruct_at(&readers, "nonexistent", Utc::now())
            .unwrap();
    assert!(result.is_none());
}

// ── TTA-Extra: Event count for empty memory ──────────────────────────────

#[tokio::test]
async fn event_count_zero_for_empty_memory() {
    let (_writer, readers) = setup();
    let count =
        cortex_temporal::event_store::query::get_event_count(&readers, "nonexistent").unwrap();
    assert_eq!(count, 0);
}

// ── TTA-Extra: Events in range across memories ───────────────────────────

#[tokio::test]
async fn events_in_range_across_memories() {
    let (writer, readers) = setup();
    let t_start = Utc::now() - Duration::hours(2);
    let t_end = Utc::now() + Duration::hours(1);

    for mid in ["m1", "m2", "m3"] {
        ensure_memory_row(&writer, mid).await;
        let mut e = make_test_event(mid, MemoryEventType::Created);
        e.recorded_at = Utc::now();
        cortex_temporal::event_store::append::append(&writer, &e)
            .await
            .unwrap();
    }

    let events =
        cortex_temporal::event_store::query::get_events_in_range(&readers, t_start, t_end).unwrap();
    assert_eq!(events.len(), 3);
}

// ── TTA-Extra: Batch snapshots ───────────────────────────────────────────

#[tokio::test]
async fn batch_snapshot_creation() {
    let (writer, readers) = setup();
    let memories: Vec<(String, BaseMemory)> = (0..5)
        .map(|_| {
            let m = make_test_memory();
            (m.id.clone(), m)
        })
        .collect();

    for (mid, _) in &memories {
        ensure_memory_row(&writer, mid).await;
    }

    let ids = cortex_temporal::snapshot::create::create_batch_snapshots(
        &writer,
        &memories,
        SnapshotReason::Periodic,
    )
    .await
    .unwrap();
    assert_eq!(ids.len(), 5);

    // Verify each snapshot exists
    for (mid, _) in &memories {
        let snaps =
            cortex_temporal::snapshot::lookup::get_snapshots_for_memory(&readers, mid).unwrap();
        assert_eq!(snaps.len(), 1);
    }
}

// ── TTA-Extra: reconstruct_all_at ────────────────────────────────────────

#[tokio::test]
async fn reconstruct_all_at_returns_non_archived() {
    let (writer, readers) = setup();

    // Create 3 memories via events
    for i in 0..3 {
        let mem = make_test_memory();
        let mid = format!("mem-all-{}", i);
        ensure_memory_row(&writer, &mid).await;
        let mut created = make_test_event(&mid, MemoryEventType::Created);
        let mut m = mem.clone();
        m.id = mid.clone();
        created.delta = serde_json::to_value(&m).unwrap();
        cortex_temporal::event_store::append::append(&writer, &created)
            .await
            .unwrap();

        // Archive the third one
        if i == 2 {
            let arch = make_test_event(&mid, MemoryEventType::Archived);
            cortex_temporal::event_store::append::append(&writer, &arch)
                .await
                .unwrap();
        }
    }

    let results = cortex_temporal::snapshot::reconstruct::reconstruct_all_at(
        &readers,
        Utc::now() + Duration::seconds(1),
    )
    .unwrap();
    // Only 2 non-archived memories
    assert_eq!(results.len(), 2);
}

// ── TTA-Extra: Engine implements ITemporalEngine ─────────────────────────

#[tokio::test]
async fn engine_record_and_get_events() {
    use cortex_core::traits::ITemporalEngine;

    let (writer, readers) = setup();
    let mem_id = "engine-test";
    ensure_memory_row(&writer, mem_id).await;
    let config = TemporalConfig::default();
    let engine = cortex_temporal::TemporalEngine::new(writer, readers, config);

    let event = make_test_event(mem_id, MemoryEventType::Created);

    let id = engine.record_event(event).await.unwrap();
    assert!(id > 0);

    let events = engine.get_events(mem_id, None).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, MemoryEventType::Created);
}
