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
}

impl PromptCompiler {
    pub fn new(context_window: usize) -> Self {
        Self {
            counter: TokenCounter::default(),
            context_window,
        }
    }

    /// Compile all 10 layers from input data.
    pub fn compile(&self, input: &PromptInput) -> Vec<PromptLayer> {
        let budgets = TokenBudgetAllocator::default_budgets();
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

        let contents = [
            &input.corp_policy,
            &input.simulation_prompt,
            &input.soul_identity,
            &input.tool_schemas,
            &input.environment,
            &input.skill_index,
            &input.convergence_state,
            &input.memory_logs,
            &input.conversation_history,
            &input.user_message,
        ];

        let mut layers: Vec<PromptLayer> = (0..10)
            .map(|i| {
                let content = contents[i].clone();
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

    fn apply_truncation(&self, layers: &mut [PromptLayer], _allocated: &[usize; 10]) {
        let total: usize = layers.iter().map(|l| l.token_count).sum();

        if total <= self.context_window {
            return;
        }

        let mut excess = total - self.context_window;

        // Truncation priority: L8 > L7 > L5 > L2. NEVER truncate L0, L1, L9.
        for &idx in &TokenBudgetAllocator::truncation_order() {
            if excess == 0 {
                break;
            }

            let layer = &mut layers[idx as usize];

            // How much can we trim from this layer?
            // Keep at least 1 token (or 0 if layer is empty).
            let min_tokens = if layer.token_count > 0 { 1 } else { 0 };
            let trimmable = layer.token_count.saturating_sub(min_tokens);
            let trim = trimmable.min(excess);

            if trim > 0 {
                let target_tokens = layer.token_count - trim;
                // Truncate content to fit target token count (approximate: 4 chars/token)
                let target_chars = target_tokens * 4;
                if target_chars < layer.content.len() {
                    layer.content.truncate(target_chars);
                    layer.token_count = self.counter.count(&layer.content);
                }
                excess = excess.saturating_sub(trim);
            }
        }
    }

    /// Filter tool schemas by intervention level.
    /// Higher level → fewer tools exposed.
    pub fn filter_tool_schemas(schemas: &str, intervention_level: u8) -> String {
        if intervention_level == 0 {
            return schemas.to_string();
        }

        // At higher levels, filter out non-essential tools
        let lines: Vec<&str> = schemas.lines().collect();
        let mut filtered = Vec::new();

        for line in &lines {
            let should_include = match intervention_level {
                1 => true, // L1: all tools
                2 => {
                    // L2: exclude proactive tools
                    !line.contains("proactive") && !line.contains("heartbeat")
                }
                3 => {
                    // L3: task-focused tools only
                    !line.contains("proactive")
                        && !line.contains("heartbeat")
                        && !line.contains("personal")
                        && !line.contains("emotional")
                }
                _ => {
                    // L4: minimal tools
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
