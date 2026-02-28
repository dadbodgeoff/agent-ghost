//! Stable Prefix Hash Validator (Task 16.1).
//!
//! Validates that L0-L5 content is identical across turns within a session.
//! KV cache providers (Anthropic, OpenAI) cache based on prefix token identity —
//! any mutation in L0-L5 invalidates the cache from that point forward.
//! blake3 hashing ensures fast, deterministic comparison.

use std::sync::{Arc, Mutex};

use super::prompt_compiler::PromptInput;

/// Number of stable prefix layers (L0-L5).
const STABLE_LAYER_COUNT: usize = 6;

/// Result of prefix validation against cached state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixValidation {
    /// First turn in session — hash stored, no comparison possible.
    FirstTurn,
    /// L0-L5 content is identical to cached — KV cache hit expected.
    CacheHit,
    /// L0-L5 content differs from cached at the specified layer.
    CacheMiss {
        layer: u8,
        reason: String,
    },
}

/// Per-session cache of the stable prefix (L0-L5) content and hash.
///
/// Thread-safe via `Arc<Mutex<>>` for sharing across async tasks.
#[derive(Debug, Clone)]
pub struct StablePrefixCache {
    inner: Arc<Mutex<CacheInner>>,
}

#[derive(Debug)]
struct CacheInner {
    prefix_hash: Option<[u8; 32]>,
    cached_layers: Option<[String; STABLE_LAYER_COUNT]>,
}

impl StablePrefixCache {
    /// Create a new empty cache for a session.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CacheInner {
                prefix_hash: None,
                cached_layers: None,
            })),
        }
    }

    /// Validate the current input's L0-L5 against the cached prefix.
    pub fn validate(&self, input: &PromptInput) -> PrefixValidation {
        let current_layers = Self::extract_layers(input);
        let current_hash = Self::compute_hash(&current_layers);

        let mut inner = self.inner.lock().expect("StablePrefixCache lock poisoned");

        match inner.prefix_hash {
            None => {
                // First turn — store and return FirstTurn
                inner.prefix_hash = Some(current_hash);
                inner.cached_layers = Some(current_layers);
                PrefixValidation::FirstTurn
            }
            Some(cached_hash) => {
                if cached_hash == current_hash {
                    PrefixValidation::CacheHit
                } else {
                    // Find which layer changed
                    let cached = inner.cached_layers.as_ref().expect("cached_layers set with hash");
                    for i in 0..STABLE_LAYER_COUNT {
                        if cached[i] != current_layers[i] {
                            let diff_preview = Self::diff_preview(&cached[i], &current_layers[i]);
                            tracing::warn!(
                                layer = i,
                                diff = %diff_preview,
                                "Stable prefix cache miss — L{} changed",
                                i
                            );
                            return PrefixValidation::CacheMiss {
                                layer: i as u8,
                                reason: diff_preview,
                            };
                        }
                    }
                    // Hash mismatch but no layer diff found (shouldn't happen)
                    PrefixValidation::CacheMiss {
                        layer: 0,
                        reason: "hash mismatch with no visible layer diff".into(),
                    }
                }
            }
        }
    }

    /// Clear the cache (e.g., at session boundary).
    pub fn reset(&self) {
        let mut inner = self.inner.lock().expect("StablePrefixCache lock poisoned");
        inner.prefix_hash = None;
        inner.cached_layers = None;
    }

    /// Extract L0-L5 content from a `PromptInput`.
    fn extract_layers(input: &PromptInput) -> [String; STABLE_LAYER_COUNT] {
        [
            input.corp_policy.clone(),
            input.simulation_prompt.clone(),
            input.soul_identity.clone(),
            input.tool_schemas.clone(),
            input.environment.clone(),
            input.skill_index.clone(),
        ]
    }

    /// Compute blake3 hash of concatenated L0-L5 content.
    fn compute_hash(layers: &[String; STABLE_LAYER_COUNT]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        for (i, layer) in layers.iter().enumerate() {
            // Include layer index as separator to avoid collisions
            // e.g., L0="ab" + L1="cd" vs L0="abc" + L1="d"
            hasher.update(&[i as u8]);
            hasher.update(layer.as_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    /// Generate a short diff preview (first 50 chars that differ).
    fn diff_preview(cached: &str, current: &str) -> String {
        let cached_chars: Vec<char> = cached.chars().collect();
        let current_chars: Vec<char> = current.chars().collect();

        for (i, (a, b)) in cached_chars.iter().zip(current_chars.iter()).enumerate() {
            if a != b {
                let start = i;
                let end = (i + 50).min(current_chars.len());
                let snippet: String = current_chars[start..end].iter().collect();
                return format!("differs at char {}: \"{}\"", start, snippet);
            }
        }

        if cached_chars.len() != current_chars.len() {
            return format!(
                "length changed: {} -> {}",
                cached_chars.len(),
                current_chars.len()
            );
        }

        "unknown diff".into()
    }
}

impl Default for StablePrefixCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(layers: [&str; 6]) -> PromptInput {
        PromptInput {
            corp_policy: layers[0].into(),
            simulation_prompt: layers[1].into(),
            soul_identity: layers[2].into(),
            tool_schemas: layers[3].into(),
            environment: layers[4].into(),
            skill_index: layers[5].into(),
            convergence_state: "state".into(),
            memory_logs: "mem".into(),
            conversation_history: "hist".into(),
            user_message: "msg".into(),
        }
    }

    #[test]
    fn first_turn_returns_first_turn() {
        let cache = StablePrefixCache::new();
        let input = make_input(["a", "b", "c", "d", "e", "f"]);
        assert_eq!(cache.validate(&input), PrefixValidation::FirstTurn);
    }

    #[test]
    fn second_turn_identical_returns_cache_hit() {
        let cache = StablePrefixCache::new();
        let input = make_input(["a", "b", "c", "d", "e", "f"]);
        assert_eq!(cache.validate(&input), PrefixValidation::FirstTurn);
        assert_eq!(cache.validate(&input), PrefixValidation::CacheHit);
    }

    #[test]
    fn second_turn_l3_changed_returns_cache_miss_layer_3() {
        let cache = StablePrefixCache::new();
        let input1 = make_input(["a", "b", "c", "d", "e", "f"]);
        let input2 = make_input(["a", "b", "c", "CHANGED", "e", "f"]);
        assert_eq!(cache.validate(&input1), PrefixValidation::FirstTurn);
        match cache.validate(&input2) {
            PrefixValidation::CacheMiss { layer, .. } => assert_eq!(layer, 3),
            other => panic!("expected CacheMiss, got {:?}", other),
        }
    }

    #[test]
    fn second_turn_l0_changed_returns_cache_miss_layer_0() {
        let cache = StablePrefixCache::new();
        let input1 = make_input(["a", "b", "c", "d", "e", "f"]);
        let input2 = make_input(["CHANGED", "b", "c", "d", "e", "f"]);
        assert_eq!(cache.validate(&input1), PrefixValidation::FirstTurn);
        match cache.validate(&input2) {
            PrefixValidation::CacheMiss { layer, .. } => assert_eq!(layer, 0),
            other => panic!("expected CacheMiss, got {:?}", other),
        }
    }

    #[test]
    fn reset_clears_cache_next_call_returns_first_turn() {
        let cache = StablePrefixCache::new();
        let input = make_input(["a", "b", "c", "d", "e", "f"]);
        assert_eq!(cache.validate(&input), PrefixValidation::FirstTurn);
        assert_eq!(cache.validate(&input), PrefixValidation::CacheHit);
        cache.reset();
        assert_eq!(cache.validate(&input), PrefixValidation::FirstTurn);
    }

    #[test]
    fn hash_is_deterministic() {
        let layers: [String; 6] = [
            "a".into(), "b".into(), "c".into(),
            "d".into(), "e".into(), "f".into(),
        ];
        let h1 = StablePrefixCache::compute_hash(&layers);
        let h2 = StablePrefixCache::compute_hash(&layers);
        assert_eq!(h1, h2);
    }

    #[test]
    fn empty_layers_valid_hash_no_panic() {
        let cache = StablePrefixCache::new();
        let input = make_input(["", "", "", "", "", ""]);
        assert_eq!(cache.validate(&input), PrefixValidation::FirstTurn);
        assert_eq!(cache.validate(&input), PrefixValidation::CacheHit);
    }

    #[test]
    fn large_l0_content_completes_quickly() {
        let cache = StablePrefixCache::new();
        let large = "x".repeat(100_000);
        let input = make_input([&large, "b", "c", "d", "e", "f"]);
        let start = std::time::Instant::now();
        cache.validate(&input);
        cache.validate(&input);
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 10, "took {}ms", elapsed.as_millis());
    }

    #[test]
    fn mutable_layers_l6_l7_l8_l9_do_not_affect_prefix() {
        let cache = StablePrefixCache::new();
        let input1 = PromptInput {
            corp_policy: "policy".into(),
            simulation_prompt: "sim".into(),
            soul_identity: "soul".into(),
            tool_schemas: "tools".into(),
            environment: "env".into(),
            skill_index: "skills".into(),
            convergence_state: "state_v1".into(),
            memory_logs: "mem_v1".into(),
            conversation_history: "hist_v1".into(),
            user_message: "msg_v1".into(),
        };
        let input2 = PromptInput {
            convergence_state: "state_v2_CHANGED".into(),
            memory_logs: "mem_v2_CHANGED".into(),
            conversation_history: "hist_v2_CHANGED".into(),
            user_message: "msg_v2_CHANGED".into(),
            ..input1.clone()
        };
        assert_eq!(cache.validate(&input1), PrefixValidation::FirstTurn);
        assert_eq!(cache.validate(&input2), PrefixValidation::CacheHit);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn identical_l0_l5_always_cache_hit(
            l0 in ".*",
            l1 in ".*",
            l2 in ".*",
            l3 in ".*",
            l4 in ".*",
            l5 in ".*",
            // Mutable layers vary
            l6a in ".*", l6b in ".*",
            l7a in ".*", l7b in ".*",
        ) {
            let cache = StablePrefixCache::new();
            let input1 = PromptInput {
                corp_policy: l0.clone(),
                simulation_prompt: l1.clone(),
                soul_identity: l2.clone(),
                tool_schemas: l3.clone(),
                environment: l4.clone(),
                skill_index: l5.clone(),
                convergence_state: l6a,
                memory_logs: l7a,
                conversation_history: String::new(),
                user_message: String::new(),
            };
            let input2 = PromptInput {
                corp_policy: l0,
                simulation_prompt: l1,
                soul_identity: l2,
                tool_schemas: l3,
                environment: l4,
                skill_index: l5,
                convergence_state: l6b,
                memory_logs: l7b,
                conversation_history: String::new(),
                user_message: String::new(),
            };
            prop_assert_eq!(cache.validate(&input1), PrefixValidation::FirstTurn);
            prop_assert_eq!(cache.validate(&input2), PrefixValidation::CacheHit);
        }

        #[test]
        fn one_layer_differs_always_cache_miss(
            base in "[a-z]{1,20}",
            diff_layer in 0..6usize,
        ) {
            let cache = StablePrefixCache::new();
            let layers: [String; 6] = std::array::from_fn(|_| base.clone());
            let input1 = PromptInput {
                corp_policy: layers[0].clone(),
                simulation_prompt: layers[1].clone(),
                soul_identity: layers[2].clone(),
                tool_schemas: layers[3].clone(),
                environment: layers[4].clone(),
                skill_index: layers[5].clone(),
                ..Default::default()
            };

            let mut modified = layers.clone();
            modified[diff_layer] = format!("{}_MODIFIED", base);
            let input2 = PromptInput {
                corp_policy: modified[0].clone(),
                simulation_prompt: modified[1].clone(),
                soul_identity: modified[2].clone(),
                tool_schemas: modified[3].clone(),
                environment: modified[4].clone(),
                skill_index: modified[5].clone(),
                ..Default::default()
            };

            prop_assert_eq!(cache.validate(&input1), PrefixValidation::FirstTurn);
            match cache.validate(&input2) {
                PrefixValidation::CacheMiss { layer, .. } => {
                    prop_assert_eq!(layer as usize, diff_layer);
                }
                other => prop_assert!(false, "expected CacheMiss, got {:?}", other),
            }
        }
    }
}
