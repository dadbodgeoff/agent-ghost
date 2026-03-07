//! Tests for ghost-audit (Task 6.1).

use ghost_audit::aggregation::AuditAggregation;
use ghost_audit::export::{AuditExporter, ExportFormat};
use ghost_audit::query_engine::{AuditEntry, AuditFilter, AuditQueryEngine};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    let engine = AuditQueryEngine::new(&conn);
    engine.ensure_table().unwrap();
    conn
}

fn make_entry(id: &str, event_type: &str, severity: &str, agent: &str) -> AuditEntry {
    AuditEntry {
        id: id.to_string(),
        timestamp: "2026-02-28T12:00:00Z".to_string(),
        agent_id: agent.to_string(),
        event_type: event_type.to_string(),
        severity: severity.to_string(),
        tool_name: Some("test_tool".to_string()),
        details: "test details".to_string(),
        session_id: Some("sess-1".to_string()),
    }
}

#[test]
fn insert_and_query_with_filter() {
    let conn = setup_db();
    let engine = AuditQueryEngine::new(&conn);
    engine
        .insert(&make_entry("1", "violation", "high", "agent-a"))
        .unwrap();
    engine
        .insert(&make_entry("2", "policy_denial", "medium", "agent-b"))
        .unwrap();
    engine
        .insert(&make_entry("3", "violation", "low", "agent-a"))
        .unwrap();

    let mut filter = AuditFilter::new();
    filter.agent_id = Some("agent-a".to_string());
    let result = engine.query(&filter).unwrap();
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.total, 2);
}

#[test]
fn pagination_page1_and_page2() {
    let conn = setup_db();
    let engine = AuditQueryEngine::new(&conn);
    for i in 0..10 {
        engine
            .insert(&make_entry(&format!("id-{}", i), "violation", "high", "a"))
            .unwrap();
    }

    let mut filter = AuditFilter::new();
    filter.page_size = 3;
    filter.page = 1;
    let p1 = engine.query(&filter).unwrap();
    assert_eq!(p1.items.len(), 3);
    assert_eq!(p1.total, 10);

    filter.page = 2;
    let p2 = engine.query(&filter).unwrap();
    assert_eq!(p2.items.len(), 3);
}

#[test]
fn full_text_search() {
    let conn = setup_db();
    let engine = AuditQueryEngine::new(&conn);
    let mut entry = make_entry("1", "violation", "high", "a");
    entry.details = "soul drift detected at threshold".to_string();
    engine.insert(&entry).unwrap();
    engine
        .insert(&make_entry("2", "violation", "high", "a"))
        .unwrap();

    let mut filter = AuditFilter::new();
    filter.search = Some("soul drift".to_string());
    let result = engine.query(&filter).unwrap();
    assert_eq!(result.items.len(), 1);
    assert!(result.items[0].details.contains("soul drift"));
}

#[test]
fn aggregation_returns_correct_counts() {
    let conn = setup_db();
    let engine = AuditQueryEngine::new(&conn);
    engine
        .insert(&make_entry("1", "violation", "high", "a"))
        .unwrap();
    engine
        .insert(&make_entry("2", "violation", "high", "a"))
        .unwrap();
    engine
        .insert(&make_entry("3", "policy_denial", "medium", "a"))
        .unwrap();

    let agg = AuditAggregation::new(&conn);
    let result = agg.summarize(None).unwrap();
    assert_eq!(result.total_entries, 3);
}

#[test]
fn export_json_valid() {
    let entries = vec![make_entry("1", "violation", "high", "a")];
    let mut buf = Vec::new();
    AuditExporter::export(&entries, ExportFormat::Json, &mut buf).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[test]
fn export_csv_valid_with_headers() {
    let entries = vec![make_entry("1", "violation", "high", "a")];
    let mut buf = Vec::new();
    AuditExporter::export(&entries, ExportFormat::Csv, &mut buf).unwrap();
    let csv = String::from_utf8(buf).unwrap();
    assert!(
        csv.starts_with("id,timestamp,agent_id,event_type,severity,tool_name,details,session_id")
    );
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 2); // header + 1 data row
}

#[test]
fn export_jsonl_valid() {
    let entries = vec![
        make_entry("1", "violation", "high", "a"),
        make_entry("2", "policy_denial", "low", "b"),
    ];
    let mut buf = Vec::new();
    AuditExporter::export(&entries, ExportFormat::Jsonl, &mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    // Each line is valid JSON
    for line in &lines {
        serde_json::from_str::<serde_json::Value>(line).unwrap();
    }
}

#[test]
fn query_no_results_returns_empty_list() {
    let conn = setup_db();
    let engine = AuditQueryEngine::new(&conn);
    let filter = AuditFilter::new();
    let result = engine.query(&filter).unwrap();
    assert!(result.items.is_empty());
    assert_eq!(result.total, 0);
}

#[test]
fn query_page_beyond_data_returns_empty() {
    let conn = setup_db();
    let engine = AuditQueryEngine::new(&conn);
    engine
        .insert(&make_entry("1", "violation", "high", "a"))
        .unwrap();

    let mut filter = AuditFilter::new();
    filter.page = 100;
    let result = engine.query(&filter).unwrap();
    assert!(result.items.is_empty());
    assert_eq!(result.total, 1); // total still reflects actual count
}
