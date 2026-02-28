use cortex_session::{
    cleanup_stale_sessions, filter_duplicates, SessionAnalytics, SessionContext, SessionManager,
    TokenEfficiency,
};
use std::collections::HashMap;

// ── T9-SESS-01: Deduplication saves tokens ────────────────────────────────

#[test]
fn deduplication_filters_already_sent_memories() {
    let manager = SessionManager::new();
    let sid = manager.create_session("sess1".to_string());

    // Mark mem1 as sent with 100 tokens
    manager.mark_memory_sent(&sid, "mem1", 100);

    let candidates = vec!["mem1".to_string(), "mem2".to_string(), "mem3".to_string()];
    let mut estimates = HashMap::new();
    estimates.insert("mem1".to_string(), 100);
    estimates.insert("mem2".to_string(), 200);
    estimates.insert("mem3".to_string(), 150);

    let result = filter_duplicates(&manager, &sid, &candidates, &estimates);

    assert_eq!(result.to_send.len(), 2, "Should send 2 new memories");
    assert_eq!(result.filtered.len(), 1, "Should filter 1 duplicate");
    assert!(result.filtered.contains(&"mem1".to_string()));
    assert_eq!(result.tokens_saved, 100, "Should save 100 tokens");
}

// ── T9-SESS-02: Cleanup removes stale sessions ───────────────────────────

#[test]
fn cleanup_removes_stale_sessions() {
    let manager = SessionManager::new();
    manager.create_session("active".to_string());
    manager.create_session("stale".to_string());

    // Make the stale session old by setting high token count (exceeds budget)
    if let Some(mut ctx) = manager.get_session("stale") {
        ctx.tokens_sent = 1_000_000; // Exceeds DEFAULT_MAX_TOKENS
        manager.update_session(ctx);
    }

    let removed = cleanup_stale_sessions(
        &manager,
        chrono::Duration::hours(1),
        chrono::Duration::days(7),
        500_000, // max tokens
    );

    assert_eq!(removed, 1, "Should remove 1 stale session");
    assert!(manager.get_session("active").is_some());
    assert!(manager.get_session("stale").is_none());
}

// ── T9-SESS-03: Analytics tracks token efficiency correctly ───────────────

#[test]
fn analytics_tracks_correctly() {
    let mut analytics = SessionAnalytics::default();

    analytics.record_retrieval("mem1");
    analytics.record_retrieval("mem1");
    analytics.record_retrieval("mem2");
    analytics.record_intent("investigate");
    analytics.record_intent("investigate");
    analytics.record_intent("create");
    analytics.record_latency(10.0);
    analytics.record_latency(20.0);

    let most = analytics.most_retrieved(5);
    assert_eq!(most[0].0, "mem1");
    assert_eq!(most[0].1, 2);

    assert!((analytics.avg_latency_ms() - 15.0).abs() < f64::EPSILON);

    assert_eq!(analytics.intent_distribution.get("investigate"), Some(&2));
    assert_eq!(analytics.intent_distribution.get("create"), Some(&1));
}

// ── T9-SESS-04: Concurrent session access via DashMap ─────────────────────

#[test]
fn concurrent_session_access_no_corruption() {
    use std::sync::Arc;
    use std::thread;

    let manager = Arc::new(SessionManager::new());

    // Create 4 sessions
    for i in 0..4 {
        manager.create_session(format!("sess{}", i));
    }

    let mut handles = vec![];

    // 4 threads each operating on their own session
    for i in 0..4 {
        let mgr = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            let sid = format!("sess{}", i);
            for j in 0..100 {
                mgr.mark_memory_sent(&sid, &format!("mem_{}_{}", i, j), 10);
                mgr.record_query(&sid);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify no corruption
    for i in 0..4 {
        let sid = format!("sess{}", i);
        let ctx = manager.get_session(&sid).unwrap();
        assert_eq!(
            ctx.queries_made, 100,
            "Session {} should have 100 queries",
            i
        );
        assert_eq!(
            ctx.loaded_memories.len(),
            100,
            "Session {} should have 100 memories",
            i
        );
        assert_eq!(
            ctx.tokens_sent, 1000,
            "Session {} should have 1000 tokens sent",
            i
        );
    }
}

// ── T9-SESS-05: Token tracking accurate ───────────────────────────────────

#[test]
fn token_tracking_accurate() {
    let manager = SessionManager::new();
    let sid = manager.create_session("sess1".to_string());

    manager.mark_memory_sent(&sid, "mem1", 150);
    manager.mark_memory_sent(&sid, "mem2", 200);
    manager.mark_memory_sent(&sid, "mem3", 350);

    let ctx = manager.get_session(&sid).unwrap();
    assert_eq!(
        ctx.tokens_sent, 700,
        "tokens_sent should be sum of all memory tokens"
    );
    assert_eq!(ctx.loaded_memories.len(), 3);
}

// ── Token efficiency metrics ──────────────────────────────────────────────

#[test]
fn token_efficiency_ratio() {
    let mut eff = TokenEfficiency::default();
    eff.record_sent(1000);
    eff.record_useful(700);
    eff.record_dedup_savings(300);

    assert!((eff.efficiency_ratio() - 0.7).abs() < f64::EPSILON);
    // dedup savings ratio: 300 / (1000 + 300) ≈ 0.2307
    assert!((eff.dedup_savings_ratio() - 300.0 / 1300.0).abs() < f64::EPSILON);
}

#[test]
fn token_efficiency_zero_division() {
    let eff = TokenEfficiency::default();
    assert_eq!(eff.efficiency_ratio(), 0.0);
    assert_eq!(eff.dedup_savings_ratio(), 0.0);
}

// ── Session context lifecycle ─────────────────────────────────────────────

#[test]
fn session_context_lifecycle() {
    let ctx = SessionContext::new("test".to_string());
    assert_eq!(ctx.session_id, "test");
    assert!(ctx.loaded_memories.is_empty());
    assert_eq!(ctx.tokens_sent, 0);
    assert_eq!(ctx.queries_made, 0);
}

#[test]
fn session_manager_crud() {
    let manager = SessionManager::new();
    assert_eq!(manager.session_count(), 0);

    manager.create_session("s1".to_string());
    assert_eq!(manager.session_count(), 1);

    assert!(manager.get_session("s1").is_some());
    assert!(manager.get_session("s2").is_none());

    manager.remove_session("s1");
    assert_eq!(manager.session_count(), 0);
}

#[test]
fn is_memory_sent_check() {
    let manager = SessionManager::new();
    manager.create_session("s1".to_string());

    assert!(!manager.is_memory_sent("s1", "mem1"));
    manager.mark_memory_sent("s1", "mem1", 50);
    assert!(manager.is_memory_sent("s1", "mem1"));
}
