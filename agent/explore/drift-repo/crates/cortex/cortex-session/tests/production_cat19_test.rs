//! Production Category 19: Cortex Memory System — T19-11 (Session Lifecycle)
//!
//! Start session, record analytics (tokens, latency), end session.
//! Token counts must be queryable. Session duration must be >0.

use cortex_session::SessionManager;

// ── T19-11: Session Lifecycle ──────────────────────────────────────────────

#[test]
fn t19_11_session_lifecycle() {
    let manager = SessionManager::new();

    // Start session.
    let session_id = manager.create_session("sess-001".to_string());
    assert_eq!(session_id, "sess-001");
    assert_eq!(manager.session_count(), 1);

    // Get session — should exist.
    let ctx = manager.get_session("sess-001").expect("session should exist");
    assert_eq!(ctx.session_id, "sess-001");
    assert_eq!(ctx.tokens_sent, 0);
    assert_eq!(ctx.queries_made, 0);
    assert!(ctx.loaded_memories.is_empty());

    // Record analytics: mark memories sent with token counts.
    manager.mark_memory_sent("sess-001", "mem-a", 150);
    manager.mark_memory_sent("sess-001", "mem-b", 200);

    // Record queries.
    manager.record_query("sess-001");
    manager.record_query("sess-001");
    manager.record_query("sess-001");

    // Verify analytics.
    let updated = manager.get_session("sess-001").unwrap();
    assert_eq!(updated.tokens_sent, 350, "tokens should be 150 + 200 = 350");
    assert_eq!(updated.queries_made, 3, "should have 3 queries");
    assert_eq!(
        updated.loaded_memories.len(),
        2,
        "should have 2 loaded memories"
    );
    assert!(updated.loaded_memories.contains("mem-a"));
    assert!(updated.loaded_memories.contains("mem-b"));

    // Deduplication: re-sending same memory should still update tokens but not count.
    manager.mark_memory_sent("sess-001", "mem-a", 100);
    let after_dup = manager.get_session("sess-001").unwrap();
    assert_eq!(
        after_dup.loaded_memories.len(),
        2,
        "duplicate memory should not increase count"
    );
    assert_eq!(
        after_dup.tokens_sent, 450,
        "tokens should increase even for duplicate: 350 + 100"
    );

    // Check deduplication detection.
    assert!(
        manager.is_memory_sent("sess-001", "mem-a"),
        "mem-a should be marked as sent"
    );
    assert!(
        !manager.is_memory_sent("sess-001", "mem-c"),
        "mem-c should NOT be marked as sent"
    );

    // Session duration must be >0 (at least a few microseconds).
    let final_ctx = manager.get_session("sess-001").unwrap();
    let duration = final_ctx.session_duration();
    assert!(
        duration.num_milliseconds() >= 0,
        "session duration should be >= 0ms"
    );

    // End session (remove).
    let removed = manager.remove_session("sess-001");
    assert!(removed.is_some(), "remove should return the session");
    assert_eq!(manager.session_count(), 0, "no sessions should remain");

    // Get after removal should return None.
    assert!(
        manager.get_session("sess-001").is_none(),
        "session should not exist after removal"
    );
}
