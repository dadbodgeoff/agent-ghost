//! Tests for Phase 17 -- Observation Masking (Tasks 17.1, 17.2, 17.3).

use ghost_agent_loop::context::observation_masker::{ObservationMasker, ObservationMaskerConfig};
use ghost_agent_loop::context::prompt_compiler::{PromptCompiler, PromptInput};
use ghost_agent_loop::context::spotlighting::SpotlightingConfig;

// ====================================================================
// Task 17.1 -- ToolOutputCache
// ====================================================================

#[test]
fn cache_store_creates_file() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
    let r = cache.store("call-1", "shell", "Hello, world!").unwrap();
    assert!(!r.hash.is_empty());
    assert_eq!(r.tool_name, "shell");
    assert_eq!(r.tool_call_id, "call-1");
    assert_eq!(r.byte_count, 13);
    assert!(r.token_count > 0);
    assert!(dir.path().join(format!("{}.txt", r.hash)).exists());
}

#[test]
fn cache_store_then_load_roundtrip() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
    let content = "fn main() { println!(\"Hello\"); }";
    let r = cache.store("c1", "file_read", content).unwrap();
    assert_eq!(cache.load(&r.hash).unwrap(), content);
}

#[test]
fn cache_same_content_deduplicates() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
    let r1 = cache.store("c1", "shell", "dup").unwrap();
    let r2 = cache.store("c2", "shell", "dup").unwrap();
    assert_eq!(r1.hash, r2.hash);
    let count = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .map(|x| x == "txt")
                .unwrap_or(false)
        })
        .count();
    assert_eq!(count, 1);
}

#[test]
fn cache_reference_string_format() {
    use ghost_agent_loop::context::tool_output_cache::{CacheRef, ToolOutputCache};
    let r = CacheRef {
        hash: "abcdef0123456789".into(),
        tool_name: "web_fetch".into(),
        tool_call_id: "c42".into(),
        token_count: 1500,
        byte_count: 6000,
    };
    let expected = format!(
        "[tool_result: web_fetch {} 1500 tokens, ref:abcdef01]",
        "\u{2192}"
    );
    assert_eq!(ToolOutputCache::reference_string(&r), expected);
}

#[test]
fn cache_cleanup_removes_old() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
    cache.store("c1", "shell", "old").unwrap();
    assert_eq!(cache.cleanup_older_than(Duration::from_secs(0)).unwrap(), 1);
}

#[test]
fn cache_cleanup_keeps_recent() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
    cache.store("c1", "shell", "recent").unwrap();
    assert_eq!(
        cache.cleanup_older_than(Duration::from_secs(3600)).unwrap(),
        0
    );
}

#[test]
fn cache_large_output_1mb() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
    let big = "x".repeat(1_000_000);
    let r = cache.store("c1", "file_read", &big).unwrap();
    assert_eq!(cache.load(&r.hash).unwrap().len(), 1_000_000);
}

#[test]
fn cache_null_bytes() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
    let content = "before\0null\0after";
    let r = cache.store("c1", "shell", content).unwrap();
    assert_eq!(cache.load(&r.hash).unwrap(), content);
}

#[test]
fn cache_creates_dir_automatically() {
    use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("deep").join("nested");
    let cache = ToolOutputCache::with_dir(nested.clone());
    assert!(!nested.exists());
    cache.store("c1", "shell", "test").unwrap();
    assert!(nested.exists());
}

#[test]
fn cache_concurrent_store_no_corruption() {
    use ghost_agent_loop::context::tool_output_cache::{CacheRef, ToolOutputCache};
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(ToolOutputCache::with_dir(dir.path().to_path_buf()));
    let content = "shared content";
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let c: Arc<ToolOutputCache> = Arc::clone(&cache);
            let s = content.to_string();
            std::thread::spawn(move || c.store(&format!("c{i}"), "shell", &s).unwrap())
        })
        .collect();
    let refs: Vec<CacheRef> = handles
        .into_iter()
        .map(|h: std::thread::JoinHandle<CacheRef>| h.join().unwrap())
        .collect();
    for r in &refs {
        assert_eq!(&r.hash, &refs[0].hash);
    }
    assert_eq!(cache.load(&refs[0].hash).unwrap(), content);
}

mod cache_proptests {
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn store_load_roundtrip(content in "[a-zA-Z0-9 ]{1,5000}") {
            use ghost_agent_loop::context::tool_output_cache::ToolOutputCache;
            let dir = tempfile::tempdir().unwrap();
            let cache = ToolOutputCache::with_dir(dir.path().to_path_buf());
            let r = cache.store("cp", "test", &content).unwrap();
            prop_assert_eq!(cache.load(&r.hash).unwrap(), content);
        }
    }
}

// ====================================================================
// Task 17.2 -- ObservationMasker
// ====================================================================

fn make_masker(
    recency: usize,
    threshold: usize,
    enabled: bool,
) -> (tempfile::TempDir, ObservationMasker) {
    let dir = tempfile::tempdir().unwrap();
    let config = ObservationMaskerConfig {
        enabled,
        recency_window: recency,
        min_token_threshold: threshold,
        cache_dir: dir.path().to_path_buf(),
    };
    (dir, ObservationMasker::new(config))
}

#[test]
fn masker_5_results_recency_3_masks_first_2() {
    let (_dir, masker) = make_masker(3, 1, true);
    let history = "Assistant: Turn 1\ntool_result tool_name:shell tool_call_id:c1\nOutput from tool call 1 with enough content to be above threshold for masking purposes\nAssistant: Turn 2\ntool_result tool_name:file_read tool_call_id:c2\nOutput from tool call 2 with enough content to be above threshold for masking purposes\nAssistant: Turn 3\ntool_result tool_name:search tool_call_id:c3\nOutput from tool call 3 with enough content to be above threshold\nAssistant: Turn 4\ntool_result tool_name:web_fetch tool_call_id:c4\nOutput from tool call 4 with enough content to be above threshold\nAssistant: Turn 5\ntool_result tool_name:api_call tool_call_id:c5\nOutput from tool call 5 with enough content to be above threshold";
    let masked = masker.mask_history(history).unwrap();
    assert!(masked.contains("[tool_result: shell"));
    assert!(masked.contains("[tool_result: file_read"));
    assert!(masked.contains("Output from tool call 3"));
    assert!(masked.contains("Output from tool call 4"));
    assert!(masked.contains("Output from tool call 5"));
}

#[test]
fn masker_1_result_within_window_nothing_masked() {
    let (_dir, masker) = make_masker(3, 1, true);
    let history = "Assistant: Turn 1\ntool_result tool_name:shell tool_call_id:c1\nSingle output";
    let masked = masker.mask_history(history).unwrap();
    assert!(masked.contains("Single output"));
    assert!(!masked.contains("[tool_result:"));
}

#[test]
fn masker_below_threshold_not_masked() {
    let (_dir, masker) = make_masker(0, 99999, true);
    let history = "Assistant: T1\ntool_result tool_name:shell tool_call_id:c1\nShort";
    let masked = masker.mask_history(history).unwrap();
    assert!(masked.contains("Short"));
    assert!(!masked.contains("[tool_result:"));
}

#[test]
fn masker_unmask_recovers_content() {
    let (_dir, masker) = make_masker(0, 1, true);
    let orig = "Full tool output that should be recoverable after masking operation";
    let history = format!("Assistant: T1\ntool_result tool_name:shell tool_call_id:c1\n{orig}");
    let masked = masker.mask_history(&history).unwrap();
    let start = masked.find("[tool_result:").unwrap();
    let end = masked[start..].find(']').unwrap() + start + 1;
    let recovered = masker.unmask_reference(&masked[start..end]).unwrap();
    assert!(recovered.contains(orig));
}

#[test]
fn masker_non_tool_messages_never_modified() {
    let (_dir, masker) = make_masker(0, 1, true);
    let history = "User: Hello\nAssistant: Hi\nUser: Bye\nAssistant: Goodbye";
    let masked = masker.mask_history(history).unwrap();
    assert!(masked.contains("Hello"));
    assert!(masked.contains("Hi"));
    assert!(masked.contains("Bye"));
    assert!(masked.contains("Goodbye"));
}

#[test]
fn masker_disabled_returns_unchanged() {
    let (_dir, masker) = make_masker(0, 1, false);
    let history = "Assistant: T1\ntool_result tool_name:shell tool_call_id:c1\nContent";
    assert_eq!(masker.mask_history(history).unwrap(), history);
}

#[test]
fn masker_masked_history_shorter() {
    let (_dir, masker) = make_masker(0, 1, true);
    let long = "x".repeat(5000);
    let history = format!("Assistant: T1\ntool_result tool_name:shell tool_call_id:c1\n{long}");
    let masked = masker.mask_history(&history).unwrap();
    assert!(masked.len() < history.len());
}

#[test]
fn masker_malformed_tool_result_no_panic() {
    let (_dir, masker) = make_masker(0, 1, true);
    let history = "Assistant: T1\ntool_result\nSome output without metadata";
    assert!(masker.mask_history(history).is_ok());
}

#[test]
fn masker_unmask_cache_miss_graceful_error() {
    let (_dir, masker) = make_masker(3, 200, true);
    let arrow = "\u{2192}";
    let reference = format!("[tool_result: shell {} 500 tokens, ref:deadbeef]", arrow);
    assert!(masker.unmask_reference(&reference).is_err());
}

mod masker_proptests {
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn masked_count_correct(num_turns in 1usize..=15, recency in 0usize..=5) {
            let (_dir, masker) = super::make_masker(recency, 1, true);
            let mut history = String::new();
            for i in 0..num_turns {
                history.push_str(&format!("Assistant: Turn {}\n", i + 1));
                history.push_str(&format!("tool_result tool_name:t{i} tool_call_id:c{i}\n"));
                history.push_str(&format!("Output from tool {i} with enough content to exceed threshold\n"));
            }
            let masked = masker.mask_history(&history).unwrap();
            let expected = num_turns.saturating_sub(recency);
            let actual = masked.matches("[tool_result:").count();
            prop_assert_eq!(actual, expected);
        }
    }
}

// ====================================================================
// Task 17.3 -- PromptCompiler + ObservationMasker Integration
// ====================================================================

fn default_input_with_history(history: &str) -> PromptInput {
    PromptInput {
        corp_policy: "No harm.".into(),
        simulation_prompt: "You are a simulation.".into(),
        soul_identity: "I am Ghost.".into(),
        tool_schemas: "shell, filesystem".into(),
        environment: "macOS".into(),
        skill_index: "skill1, skill2".into(),
        convergence_state: "score=0.1 level=0".into(),
        memory_logs: "memory entry 1".into(),
        conversation_history: history.into(),
        user_message: "What is Rust?".into(),
    }
}

fn build_history(num_turns: usize, content_size: usize) -> String {
    let mut history = String::new();
    for i in 0..num_turns {
        history.push_str(&format!("Assistant: Turn {}\n", i + 1));
        history.push_str(&format!(
            "tool_result tool_name:tool_{i} tool_call_id:call_{i}\n"
        ));
        history.push_str(&"x".repeat(content_size));
        history.push('\n');
    }
    history
}

#[test]
fn prompt_compiler_new_no_masking_backward_compat() {
    let compiler = PromptCompiler::new(128_000);
    let history = build_history(5, 500);
    let input = default_input_with_history(&history);
    let (layers, _stats) = compiler.compile(&input);
    assert!(
        layers[8].content.contains('x'),
        "Raw content should be present in L8"
    );
    assert!(
        !layers[8].content.contains("ref:"),
        "No masking references should appear"
    );
}

#[test]
fn prompt_compiler_with_observation_masking_masks_l8() {
    let dir = tempfile::tempdir().unwrap();
    let masker_config = ObservationMaskerConfig {
        enabled: true,
        recency_window: 2,
        min_token_threshold: 1,
        cache_dir: dir.path().to_path_buf(),
    };
    let spot_config = SpotlightingConfig {
        enabled: false,
        ..SpotlightingConfig::default()
    };
    let compiler = PromptCompiler::with_observation_masking(128_000, spot_config, masker_config);
    let history = build_history(5, 500);
    let input = default_input_with_history(&history);
    let (layers, _stats) = compiler.compile(&input);
    assert!(
        layers[8].content.contains("[tool_result:"),
        "Old turns should have compact refs"
    );
    assert!(
        layers[8].content.contains("Turn 5"),
        "Turn 5 should be inline"
    );
}

#[test]
fn prompt_compiler_masking_plus_spotlighting_masked_content_datamarked() {
    use ghost_agent_loop::context::spotlighting::SpotlightMode;
    let dir = tempfile::tempdir().unwrap();
    let spot_config = SpotlightingConfig {
        enabled: true,
        mode: SpotlightMode::Datamarking,
        marker: '\u{2195}',
        layers: vec![7, 8],
    };
    let masker_config = ObservationMaskerConfig {
        enabled: true,
        recency_window: 1,
        min_token_threshold: 1,
        cache_dir: dir.path().to_path_buf(),
    };
    let compiler = PromptCompiler::with_observation_masking(128_000, spot_config, masker_config);
    let history = build_history(3, 500);
    let input = default_input_with_history(&history);
    let (layers, _stats) = compiler.compile(&input);
    let l8 = &layers[8].content;
    assert!(l8.contains('\u{2195}'), "L8 should be datamarked");
}

#[test]
fn prompt_compiler_l8_token_count_reduced_after_masking() {
    let dir = tempfile::tempdir().unwrap();
    let masker_config = ObservationMaskerConfig {
        enabled: true,
        recency_window: 1,
        min_token_threshold: 1,
        cache_dir: dir.path().to_path_buf(),
    };
    let spot_off = SpotlightingConfig {
        enabled: false,
        ..SpotlightingConfig::default()
    };
    let compiler_no_mask = PromptCompiler::with_spotlighting(128_000, spot_off.clone());
    let compiler_mask = PromptCompiler::with_observation_masking(
        128_000,
        SpotlightingConfig {
            enabled: false,
            ..SpotlightingConfig::default()
        },
        masker_config,
    );
    let history = build_history(5, 2000);
    let input = default_input_with_history(&history);
    let (layers_no_mask, _) = compiler_no_mask.compile(&input);
    let (layers_mask, _) = compiler_mask.compile(&input);
    assert!(
        layers_mask[8].token_count < layers_no_mask[8].token_count,
        "Masked L8 ({}) should have fewer tokens than unmasked ({})",
        layers_mask[8].token_count,
        layers_no_mask[8].token_count
    );
}

#[test]
fn prompt_compiler_l0_through_l7_and_l9_unaffected_by_masking() {
    let dir = tempfile::tempdir().unwrap();
    let masker_config = ObservationMaskerConfig {
        enabled: true,
        recency_window: 1,
        min_token_threshold: 1,
        cache_dir: dir.path().to_path_buf(),
    };
    let spot_off = SpotlightingConfig {
        enabled: false,
        ..SpotlightingConfig::default()
    };
    let compiler_no_mask = PromptCompiler::with_spotlighting(128_000, spot_off.clone());
    let compiler_mask = PromptCompiler::with_observation_masking(
        128_000,
        SpotlightingConfig {
            enabled: false,
            ..SpotlightingConfig::default()
        },
        masker_config,
    );
    let history = build_history(5, 500);
    let input = default_input_with_history(&history);
    let (layers_no_mask, _) = compiler_no_mask.compile(&input);
    let (layers_mask, _) = compiler_mask.compile(&input);
    for i in 0..8 {
        assert_eq!(
            layers_no_mask[i].content, layers_mask[i].content,
            "Layer {} should be unaffected by masking",
            i
        );
    }
    assert_eq!(
        layers_no_mask[9].content, layers_mask[9].content,
        "L9 should be unaffected"
    );
}

#[test]
fn prompt_compiler_masker_error_falls_back_to_unmasked() {
    let dir = tempfile::tempdir().unwrap();
    let bad_path = dir.path().join("not_a_dir");
    std::fs::write(&bad_path, "blocker").unwrap();
    let masker_config = ObservationMaskerConfig {
        enabled: true,
        recency_window: 0,
        min_token_threshold: 1,
        cache_dir: bad_path.join("subdir"),
    };
    let compiler = PromptCompiler::with_observation_masking(
        128_000,
        SpotlightingConfig {
            enabled: false,
            ..SpotlightingConfig::default()
        },
        masker_config,
    );
    let history = build_history(3, 500);
    let input = default_input_with_history(&history);
    let (layers, _stats) = compiler.compile(&input);
    assert!(!layers[8].content.is_empty());
}

#[test]
fn prompt_compiler_full_compile_with_masking_references_for_old_inline_for_recent() {
    let dir = tempfile::tempdir().unwrap();
    let masker_config = ObservationMaskerConfig {
        enabled: true,
        recency_window: 2,
        min_token_threshold: 1,
        cache_dir: dir.path().to_path_buf(),
    };
    let compiler = PromptCompiler::with_observation_masking(
        128_000,
        SpotlightingConfig {
            enabled: false,
            ..SpotlightingConfig::default()
        },
        masker_config,
    );
    let history = build_history(6, 500);
    let input = default_input_with_history(&history);
    let (layers, _stats) = compiler.compile(&input);
    let l8 = &layers[8].content;
    let ref_count = l8.matches("[tool_result:").count();
    assert_eq!(
        ref_count, 4,
        "Expected 4 masked references, got {ref_count}"
    );
    assert!(l8.contains("Turn 5"), "Turn 5 should be inline");
    assert!(l8.contains("Turn 6"), "Turn 6 should be inline");
    assert_eq!(layers.len(), 10);
}
