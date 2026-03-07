//! E2E: Full compaction lifecycle.
//!
//! Validates: threshold exceeded → snapshot → flush turn → compression
//! → CompactionBlock → verification.

use ghost_gateway::session::compaction::{CompactionConfig, SessionCompactor};

/// Full compaction lifecycle: threshold check → compact → verify reduction.
#[test]
fn full_compaction_lifecycle() {
    let config = CompactionConfig::default();
    let compactor = SessionCompactor::new(config);
    let context_window = 2000;

    // Build a history that exceeds 70% threshold (70% of 2000 = 1400)
    let mut history: Vec<String> = (0..20)
        .map(|i| format!("Message {} with enough content to be meaningful for compaction testing purposes and to exceed the threshold", i))
        .collect();

    let pre_tokens: usize = history.iter().map(|m| m.len()).sum();
    assert!(
        compactor.should_compact(pre_tokens, context_window),
        "History should exceed 70% threshold: {} / {}",
        pre_tokens,
        context_window
    );

    // Execute compaction pass 1
    let result = compactor.compact(&mut history, 1, None);
    assert!(
        result.is_ok(),
        "Compaction pass 1 should succeed: {:?}",
        result.err()
    );

    let block = result.unwrap();
    assert_eq!(block.pass_number, 1);
    assert!(block.compressed_token_count < block.original_token_count);

    // Verify post-compaction is smaller
    let post_tokens: usize = history.iter().map(|m| m.len()).sum();
    assert!(
        post_tokens < pre_tokens,
        "Post-compaction ({}) must be < pre-compaction ({})",
        post_tokens,
        pre_tokens
    );

    // Verify CompactionBlock is in history
    let has_block = history.iter().any(|m| m.contains("\"pass_number\""));
    assert!(
        has_block,
        "CompactionBlock must be in history after compaction"
    );
}

/// Compaction rollback on max passes exceeded.
#[test]
fn compaction_rollback_on_max_passes() {
    let config = CompactionConfig {
        max_passes: 2,
        ..CompactionConfig::default()
    };
    let compactor = SessionCompactor::new(config);

    let mut history: Vec<String> = vec!["test message".into()];
    let snapshot = history.clone();

    // Pass 3 exceeds max_passes=2
    let result = compactor.compact(&mut history, 3, None);
    assert!(result.is_err());

    // History should be unchanged (no partial modification)
    assert_eq!(history, snapshot);
}

/// CompactionBlock is never re-compressed in subsequent passes.
#[test]
fn compaction_block_preserved_across_passes() {
    let compactor = SessionCompactor::new(CompactionConfig::default());

    // First pass
    let mut history: Vec<String> = (0..10)
        .map(|i| {
            format!(
                "Original message {} with sufficient content for meaningful compaction testing",
                i
            )
        })
        .collect();

    let result1 = compactor.compact(&mut history, 1, None);
    assert!(result1.is_ok());

    let block_count_after_pass1 = history
        .iter()
        .filter(|m| m.contains("\"pass_number\""))
        .count();
    assert_eq!(block_count_after_pass1, 1);

    // Add new messages
    for i in 0..5 {
        history.push(format!(
            "New message {} after first compaction with enough content to trigger another pass",
            i
        ));
    }

    // Second pass — original CompactionBlock must be preserved
    let result2 = compactor.compact(&mut history, 2, None);
    if result2.is_ok() {
        let block_count_after_pass2 = history
            .iter()
            .filter(|m| m.contains("\"pass_number\""))
            .count();
        // Should have both pass 1 and pass 2 blocks
        assert!(
            block_count_after_pass2 >= 2,
            "Both CompactionBlocks should be preserved: found {}",
            block_count_after_pass2
        );
    }
}

/// Compaction aborts on shutdown signal.
#[test]
fn compaction_aborts_on_shutdown() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let shutdown = std::sync::atomic::AtomicBool::new(true);

    let mut history: Vec<String> = (0..10)
        .map(|i| format!("Message {} for shutdown test", i))
        .collect();
    let snapshot = history.clone();

    let result = compactor.compact(&mut history, 1, Some(&shutdown));
    assert!(
        result.is_err(),
        "Compaction should abort on shutdown signal"
    );

    // History should be unchanged
    assert_eq!(history, snapshot);
}

/// Tool result pruning frees tokens.
///
/// The implementation uses JSON deserialization: a message is a tool_result
/// if it deserializes as a JSON object with `"type": "tool_result"`.
#[test]
fn tool_result_pruning() {
    let mut history: Vec<String> = vec![
        "User question".into(),
        r#"{"type": "tool_result", "content": "output data from tool execution"}"#.into(),
        "Agent response".into(),
        r#"{"type": "tool_result", "content": "more tool output"}"#.into(),
    ];

    let result = SessionCompactor::prune_tool_results(&mut history);
    assert_eq!(result.results_pruned, 2);
    assert!(result.tokens_freed > 0);
    assert_eq!(history.len(), 2); // Only non-tool messages remain
}
