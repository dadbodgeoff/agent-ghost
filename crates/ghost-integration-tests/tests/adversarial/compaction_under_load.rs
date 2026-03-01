//! Adversarial: Compaction under load (Task 7.3).
//!
//! Tests compaction behavior with simultaneous message arrival,
//! tight token budgets, and edge cases.

use ghost_gateway::session::compaction::{CompactionConfig, SessionCompactor};

// ── Compaction triggers correctly ───────────────────────────────────────

/// Compaction triggers at 70% context window.
#[test]
fn triggers_at_70_percent() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let context_window = 10000;
    // 7000/10000 = 0.70 exactly
    let current_tokens = 7000;
    assert!(
        compactor.should_compact(current_tokens, context_window),
        "Should trigger at 70% of context window"
    );
}

/// Compaction does NOT trigger at 69%.
#[test]
fn does_not_trigger_at_69_percent() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let context_window = 10000;
    // 6999/10000 = 0.6999 < 0.70
    let current_tokens = 6999;
    assert!(
        !compactor.should_compact(current_tokens, context_window),
        "Should NOT trigger at 69% of context window"
    );
}

// ── Token reduction invariant ───────────────────────────────────────────

/// Post-compaction token count < pre-compaction (for meaningful input).
#[test]
fn compact_reduces_tokens() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let mut history: Vec<String> = vec![
        "Hello, how are you?".to_string(),
        "I'm doing well, thanks for asking! Let me help you with that.".to_string(),
        "Can you help me with Rust lifetimes and borrowing?".to_string(),
        "Of course! Lifetimes in Rust ensure references are valid for as long as needed.".to_string(),
        "I need to understand how the borrow checker works in practice.".to_string(),
        "The borrow checker enforces ownership rules at compile time, preventing data races and use-after-free bugs. It tracks how references are used across scopes.".to_string(),
    ];

    let pre_tokens: usize = history.iter().map(|m| m.len()).sum();
    let result = compactor.compact(&mut history, 1, None);

    assert!(result.is_ok(), "Compaction must succeed: {:?}", result.err());
    let new_tokens: usize = history.iter().map(|m| m.len()).sum();
    assert!(
        new_tokens < pre_tokens,
        "Post-compaction tokens ({}) must be < pre-compaction ({})",
        new_tokens,
        pre_tokens
    );
}


// ── Max passes enforced ─────────────────────────────────────────────────

/// Max 3 passes — 4th pass returns error.
#[test]
fn max_passes_enforced() {
    let config = CompactionConfig {
        max_passes: 3,
        ..CompactionConfig::default()
    };
    let compactor = SessionCompactor::new(config);

    let mut history: Vec<String> = vec![
        "a".repeat(300),
        "b".repeat(300),
    ];

    // Pass 4 must be rejected
    let result = compactor.compact(&mut history, 4, None);
    assert!(
        result.is_err(),
        "Pass 4 must be rejected when max_passes=3"
    );
}

// ── CompactionBlock never re-compressed ─────────────────────────────────

/// CompactionBlock in history is never re-compressed on subsequent pass.
#[test]
fn compaction_block_never_recompressed() {
    let compactor = SessionCompactor::new(CompactionConfig::default());

    // Create a history with an existing CompactionBlock (serialized JSON)
    let block_json = serde_json::json!({
        "summary": "[Compacted 5 messages in pass 1]",
        "original_token_count": 500,
        "compressed_token_count": 50,
        "pass_number": 1,
        "timestamp": "2026-01-01T00:00:00Z"
    });
    let mut history: Vec<String> = vec![
        block_json.to_string(),
        "New message after compaction that is long enough to be meaningful for compression purposes".to_string(),
        "Response to new message with additional context and detail to ensure meaningful compression".to_string(),
    ];

    let result = compactor.compact(&mut history, 2, None);
    assert!(result.is_ok(), "Compaction must succeed: {:?}", result.err());

    // The original CompactionBlock JSON should still be in history
    let has_original_block = history.iter().any(|m| m.contains("\"pass_number\":1") || m.contains("\"pass_number\": 1"));
    assert!(
        has_original_block,
        "Original CompactionBlock must be preserved, not re-compressed"
    );
}

// ── Tight budget edge case ──────────────────────────────────────────────

/// Very small history — compaction with tight budget should not panic.
#[test]
fn tight_budget_no_panic() {
    let config = CompactionConfig {
        max_passes: 3,
        ..CompactionConfig::default()
    };
    let compactor = SessionCompactor::new(config);

    let mut history: Vec<String> = vec![
        "Hello".to_string(),
        "Hi".to_string(),
    ];

    // Should not panic even with minimal content (may error, that's fine)
    let _ = compactor.compact(&mut history, 1, None);
}

// ── Session pruning ─────────────────────────────────────────────────────

/// Tool results are pruned from history.
#[test]
fn prune_tool_results() {
    let mut history: Vec<String> = vec![
        "Run the tests".to_string(),
        r#"{"type": "tool_result", "content": "All 42 tests passed"}"#.to_string(),
        "Tests passed!".to_string(),
    ];

    let pruned = SessionCompactor::prune_tool_results(&mut history);
    assert!(
        pruned.tokens_freed > 0,
        "Tool results should be pruned, freeing tokens"
    );
    assert!(
        pruned.results_pruned >= 1,
        "At least one tool_result should be pruned"
    );
}

/// Non-tool messages preserved during pruning.
#[test]
fn prune_preserves_non_tool_messages() {
    let mut history: Vec<String> = vec![
        "Important question".to_string(),
        r#"{"type": "tool_result", "content": "Tool output data"}"#.to_string(),
        "Follow-up question".to_string(),
    ];

    let original_non_tool_count = history.iter()
        .filter(|m| {
            serde_json::from_str::<serde_json::Value>(m)
                .ok()
                .and_then(|v| v.get("type")?.as_str().map(|t| t == "tool_result"))
                .unwrap_or(false)
                == false
        })
        .count();
    SessionCompactor::prune_tool_results(&mut history);

    let remaining_count = history.len();
    assert_eq!(
        remaining_count, original_non_tool_count,
        "Non-tool messages must be preserved during pruning"
    );
}
