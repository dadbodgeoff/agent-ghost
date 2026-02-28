//! 10-layer prompt compiler (Req 11 AC2, AC14).
//!
//! Layers:
//! L0: CORP_POLICY.md (immutable, Uncapped)
//! L1: Simulation boundary prompt (platform-injected, Fixed 200)
//! L2: SOUL.md + IDENTITY.md (Fixed 2000)
//! L3: Tool schemas filtered by convergence level (Fixed 3000)
//! L4: Environment context (Fixed 200)
//! L5: Skill index (Fixed 500)
//! L6: Convergence state from read-only pipeline (Fixed 1000)
//! L7: MEMORY.md + daily logs, convergence-filtered (Fixed 4000)
//! L8: Conversation history (Remainder)
//! L9: User message (Uncapped)

use ghost_llm::tokens::TokenCounter;
use once_cell::sync::Lazy;
use regex::Regex;

use super::observation_masker::{ObservationMasker, ObservationMaskerConfig};
use super::spotlighting::{Spotlighter, SpotlightingConfig};
use super::token_budget::{Budget, TokenBudgetAllocator};

/// A single compiled prompt layer.
#[derive(Debug, Clone)]
pub struct PromptLayer {
    pub index: u8,
    pub name: &'static str,
    pub content: String,
    pub token_count: usize,
    pub budget: Budget,
}

/// Input data for prompt compilation.
#[derive(Debug, Clone, Default)]
pub struct PromptInput {
    pub corp_policy: String,
    pub simulation_prompt: String,
    pub soul_identity: String,
    pub tool_schemas: String,
    pub environment: String,
    pub skill_index: String,
    pub convergence_state: String,
    pub memory_logs: String,
    pub conversation_history: String,
    pub user_message: String,
}

/// Compiles 10 prompt layers with budget allocation and truncation.
pub struct PromptCompiler {
    counter: TokenCounter,
    context_window: usize,
    spotlighter: Spotlighter,
    observation_masker: Option<ObservationMasker>,
}

impl PromptCompiler {
    pub fn new(context_window: usize) -> Self {
        Self {
            counter: TokenCounter::default(),
            context_window,
            spotlighter: Spotlighter::new(SpotlightingConfig::default()),
            observation_masker: None,
        }
    }

    /// Create a PromptCompiler with a custom spotlighting configuration.
    pub fn with_spotlighting(context_window: usize, config: SpotlightingConfig) -> Self {
        Self {
            counter: TokenCounter::default(),
            context_window,
            spotlighter: Spotlighter::new(config),
            observation_masker: None,
        }
    }

    /// Create a PromptCompiler with observation masking enabled.
    ///
    /// Masking is applied to L8 (conversation history) BEFORE spotlighting.
    /// Old tool outputs are replaced with compact references, reducing token count.
    pub fn with_observation_masking(
        context_window: usize,
        spotlighting_config: SpotlightingConfig,
        masker_config: ObservationMaskerConfig,
    ) -> Self {
        Self {
            counter: TokenCounter::default(),
            context_window,
            spotlighter: Spotlighter::new(spotlighting_config),
            observation_masker: Some(ObservationMasker::new(masker_config)),
        }
    }

    /// Compile all 10 layers from input data.
    ///
    /// Ordering: observation masking -> spotlighting -> budget allocation -> truncation.
    /// L0 and L1 are NEVER datamarked.
    /// L4 timestamps are sanitized to preserve KV cache stability.
    pub fn compile(&self, input: &PromptInput) -> Vec<PromptLayer> {
        let mut budgets = TokenBudgetAllocator::default_budgets();

        // Adjust budgets for spotlighting: datamarking roughly doubles token count
        let multiplier = self.spotlighter.token_budget_multiplier();
        for (i, budget) in budgets.iter_mut().enumerate() {
            if self.spotlighter.affects_layer(i as u8) {
                if let Budget::Fixed(n) = budget {
                    *budget = Budget::Fixed((*n as f64 * multiplier) as usize);
                }
            }
        }

        let allocated = TokenBudgetAllocator::allocate(self.context_window, &budgets);

        let layer_names: [&str; 10] = [
            "CORP_POLICY",
            "SIMULATION_BOUNDARY",
            "SOUL_IDENTITY",
            "TOOL_SCHEMAS",
            "ENVIRONMENT",
            "SKILL_INDEX",
            "CONVERGENCE_STATE",
            "MEMORY_LOGS",
            "CONVERSATION_HISTORY",
            "USER_MESSAGE",
        ];

        // Sanitize L4 environment timestamps (Task 16.3)
        let sanitized_environment = sanitize_environment_timestamps(&input.environment);

        // Apply observation masking to L8 BEFORE spotlighting (Task 17.3).
        // Masking reduces token count; spotlighting doubles it -- mask first
        // to minimize the doubling impact.
        let conversation_history = match &self.observation_masker {
            Some(masker) => {
                match masker.mask_history(&input.conversation_history) {
                    Ok(masked) => {
                        let original_tokens = self.counter.count(&input.conversation_history);
                        let masked_tokens = self.counter.count(&masked);
                        let saved = original_tokens.saturating_sub(masked_tokens);
                        if saved > 0 {
                            tracing::info!(
                                original_tokens,
                                masked_tokens,
                                saved,
                                "Observation masking applied"
                            );
                        }
                        masked
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Observation masking failed, using unmasked history");
                        input.conversation_history.clone()
                    }
                }
            }
            None => input.conversation_history.clone(),
        };

        let contents: [&str; 10] = [
            &input.corp_policy,
            &input.simulation_prompt,
            &input.soul_identity,
            &input.tool_schemas,
            &sanitized_environment,
            &input.skill_index,
            &input.convergence_state,
            &input.memory_logs,
            &conversation_history,
            &input.user_message,
        ];

        let mut layers: Vec<PromptLayer> = (0..10)
            .map(|i| {
                let content = self.spotlighter.apply(i as u8, contents[i]);
                let token_count = self.counter.count(&content);
                PromptLayer {
                    index: i as u8,
                    name: layer_names[i],
                    content,
                    token_count,
                    budget: budgets[i],
                }
            })
            .collect();

        // Apply truncation if total exceeds context window
        self.apply_truncation(&mut layers, &allocated);

        layers
    }
    fn apply_truncation(&self, layers: &mut [PromptLayer], allocated: &[usize; 10]) {
        let total: usize = layers.iter().map(|l| l.token_count).sum();

        if total <= self.context_window {
            return;
        }

        let mut excess = total - self.context_window;

        for &idx in &TokenBudgetAllocator::truncation_order() {
            if excess == 0 {
                break;
            }

            let layer = &mut layers[idx as usize];
            let budget = allocated[idx as usize];

            if layer.token_count > budget && budget < usize::MAX {
                let can_trim = layer.token_count - budget.min(layer.token_count);
                let trim = can_trim.min(excess);
                let target_tokens = layer.token_count - trim;

                let target_chars = target_tokens * 4;
                if target_chars < layer.content.len() {
                    layer.content.truncate(target_chars);
                    layer.token_count = self.counter.count(&layer.content);
                }

                excess = excess.saturating_sub(trim);
            }
        }
    }

    #[deprecated(
        since = "0.1.0",
        note = "Use tool_constraint_instruction() for L6 constraint instead of L3 content filtering"
    )]
    pub fn filter_tool_schemas(schemas: &str, intervention_level: u8) -> String {
        if intervention_level == 0 {
            return schemas.to_string();
        }

        let lines: Vec<&str> = schemas.lines().collect();
        let mut filtered = Vec::new();

        for line in &lines {
            let should_include = match intervention_level {
                1 => true,
                2 => !line.contains("proactive") && !line.contains("heartbeat"),
                3 => {
                    !line.contains("proactive")
                        && !line.contains("heartbeat")
                        && !line.contains("personal")
                        && !line.contains("emotional")
                }
                _ => {
                    line.contains("read")
                        || line.contains("search")
                        || line.contains("shell")
                        || line.contains("filesystem")
                }
            };

            if should_include {
                filtered.push(*line);
            }
        }

        filtered.join("\n")
    }
}

pub fn tool_constraint_instruction(intervention_level: u8) -> String {
    match intervention_level.min(4) {
        0 | 1 => String::new(),
        2 => "TOOL RESTRICTION: Do not use proactive or heartbeat tools at current convergence level.".into(),
        3 => "TOOL RESTRICTION: Only task-focused tools permitted. Do not use proactive, heartbeat, personal, or emotional tools.".into(),
        _ => "TOOL RESTRICTION: Minimal tools only. Only read, search, shell, and filesystem tools are permitted.".into(),
    }
}

static RE_ISO_SECONDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}):\d{2}(?:\.\d+)?Z?").unwrap()
});

static RE_TIME_SECONDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"((?:^|[\sT,;])\d{2}:\d{2}):\d{2}").unwrap()
});

static RE_UNIX_EPOCH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b\d{10,13}\b").unwrap()
});

pub fn sanitize_environment_timestamps(content: &str) -> String {
    let result = RE_ISO_SECONDS.replace_all(content, "$1");
    let result = RE_TIME_SECONDS.replace_all(&result, "$1");
    let result = RE_UNIX_EPOCH.replace_all(&result, "");

    if result != content {
        tracing::debug!("Sanitized timestamps in L4 environment content");
    }

    result.into_owned()
}