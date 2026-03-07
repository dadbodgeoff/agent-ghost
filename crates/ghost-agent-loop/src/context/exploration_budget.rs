//! Information-theoretic exploration budget (Task 22.4 — stretch goal).
//!
//! Tracks bits-per-token for each tool call category and enforces a
//! 20% exploration / 80% exploitation split of the token budget.
//! Advisory only — the agent can override the budget.

use std::collections::BTreeMap;

/// Tool call classification: exploration (gathering info) vs exploitation (acting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallType {
    /// Gathering new information (file_read, web_search, api_call).
    Exploration,
    /// Acting on known information (file_write, shell_execute, memory_write).
    Exploitation,
}

/// Per-category information density tracking.
#[derive(Debug, Clone, Default)]
pub struct InformationDensity {
    pub total_calls: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// How much agent behavior changed after this category's output.
    pub behavioral_change_score: f64,
    /// Approximate bits per token: behavioral_change_score / total_output_tokens.
    pub bits_per_token: f64,
}

/// Exploration/exploitation budget tracker.
#[derive(Debug, Clone)]
pub struct ExplorationBudget {
    /// Fraction of token budget for exploration (default 0.20).
    pub exploration_ratio: f64,
    /// Fraction of token budget for exploitation (default 0.80).
    pub exploitation_ratio: f64,
    /// Per-tool-category density tracking.
    pub per_category_density: BTreeMap<String, InformationDensity>,
    /// Tokens used for exploration in current session.
    exploration_tokens: usize,
    /// Tokens used for exploitation in current session.
    exploitation_tokens: usize,
    /// Recent bits_per_token values for diminishing returns detection.
    recent_exploration_bpt: Vec<f64>,
    /// Threshold below which exploration is considered low-value.
    diminishing_returns_threshold: f64,
}

impl Default for ExplorationBudget {
    fn default() -> Self {
        Self {
            exploration_ratio: 0.20,
            exploitation_ratio: 0.80,
            per_category_density: BTreeMap::new(),
            exploration_tokens: 0,
            exploitation_tokens: 0,
            recent_exploration_bpt: Vec::new(),
            diminishing_returns_threshold: 0.001,
        }
    }
}

impl ExplorationBudget {
    /// Create with custom ratios.
    pub fn new(exploration_ratio: f64, exploitation_ratio: f64) -> Self {
        Self {
            exploration_ratio,
            exploitation_ratio,
            ..Default::default()
        }
    }

    /// Classify a tool call as exploration or exploitation.
    pub fn classify(tool_name: &str) -> ToolCallType {
        match tool_name {
            n if n.contains("read")
                || n.contains("search")
                || n.contains("fetch")
                || n.contains("list")
                || n.contains("get")
                || n.contains("query")
                || n.contains("discover")
                || n.contains("inspect") =>
            {
                ToolCallType::Exploration
            }
            _ => ToolCallType::Exploitation,
        }
    }

    /// Check if a tool call should be allowed given the current budget.
    ///
    /// Advisory only — returns false if the budget for this type is exhausted,
    /// but the agent can override.
    pub fn should_allow(&self, tool_type: ToolCallType, session_token_budget: usize) -> bool {
        let total_used = self.exploration_tokens + self.exploitation_tokens;
        if total_used == 0 {
            return true;
        }

        match tool_type {
            ToolCallType::Exploration => {
                let max_exploration =
                    (session_token_budget as f64 * self.exploration_ratio) as usize;
                if self.exploration_tokens >= max_exploration {
                    tracing::info!(
                        exploration_tokens = self.exploration_tokens,
                        max = max_exploration,
                        "Exploration budget exhausted — suggest switching to exploitation"
                    );
                    return false;
                }
                true
            }
            ToolCallType::Exploitation => {
                // Exploitation is always allowed — if exploitation budget is
                // exceeded, we allow rebalancing by permitting exploration.
                true
            }
        }
    }

    /// Record a tool call with its token usage and behavioral change.
    pub fn record(
        &mut self,
        tool_name: &str,
        input_tokens: usize,
        output_tokens: usize,
        behavioral_change: f64,
    ) {
        let tool_type = Self::classify(tool_name);
        match tool_type {
            ToolCallType::Exploration => self.exploration_tokens += input_tokens + output_tokens,
            ToolCallType::Exploitation => self.exploitation_tokens += input_tokens + output_tokens,
        }

        let category = tool_name.to_string();
        let density = self.per_category_density.entry(category).or_default();
        density.total_calls += 1;
        density.total_input_tokens += input_tokens as u64;
        density.total_output_tokens += output_tokens as u64;
        density.behavioral_change_score += behavioral_change;
        if density.total_output_tokens > 0 {
            density.bits_per_token =
                density.behavioral_change_score / density.total_output_tokens as f64;
        }

        // Track recent exploration bits_per_token for diminishing returns.
        if tool_type == ToolCallType::Exploration && output_tokens > 0 {
            let bpt = behavioral_change / output_tokens as f64;
            self.recent_exploration_bpt.push(bpt);
            // Keep only last 10.
            if self.recent_exploration_bpt.len() > 10 {
                self.recent_exploration_bpt.remove(0);
            }
            self.check_diminishing_returns();
        }
    }

    /// Check if recent exploration calls show diminishing returns.
    fn check_diminishing_returns(&self) {
        if self.recent_exploration_bpt.len() < 5 {
            return;
        }
        let last_5 = &self.recent_exploration_bpt[self.recent_exploration_bpt.len() - 5..];
        let all_low = last_5
            .iter()
            .all(|&bpt| bpt < self.diminishing_returns_threshold);
        if all_low {
            tracing::info!(
                last_5_bpt = ?last_5,
                threshold = self.diminishing_returns_threshold,
                "Diminishing returns detected — last 5 exploration calls had low information density"
            );
        }
    }

    /// Get the current exploration/exploitation ratio.
    pub fn current_ratio(&self) -> (f64, f64) {
        let total = (self.exploration_tokens + self.exploitation_tokens) as f64;
        if total == 0.0 {
            return (0.0, 0.0);
        }
        (
            self.exploration_tokens as f64 / total,
            self.exploitation_tokens as f64 / total,
        )
    }

    /// Reset session counters.
    pub fn reset_session(&mut self) {
        self.exploration_tokens = 0;
        self.exploitation_tokens = 0;
        self.recent_exploration_bpt.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_is_20_80() {
        let budget = ExplorationBudget::default();
        assert!((budget.exploration_ratio - 0.20).abs() < f64::EPSILON);
        assert!((budget.exploitation_ratio - 0.80).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_file_read_is_exploration() {
        assert_eq!(
            ExplorationBudget::classify("file_read"),
            ToolCallType::Exploration
        );
    }

    #[test]
    fn classify_file_write_is_exploitation() {
        assert_eq!(
            ExplorationBudget::classify("file_write"),
            ToolCallType::Exploitation
        );
    }

    #[test]
    fn classify_web_search_is_exploration() {
        assert_eq!(
            ExplorationBudget::classify("web_search"),
            ToolCallType::Exploration
        );
    }

    #[test]
    fn classify_shell_execute_is_exploitation() {
        assert_eq!(
            ExplorationBudget::classify("shell_execute"),
            ToolCallType::Exploitation
        );
    }

    #[test]
    fn should_allow_when_budget_not_exhausted() {
        let budget = ExplorationBudget::default();
        assert!(budget.should_allow(ToolCallType::Exploration, 10000));
        assert!(budget.should_allow(ToolCallType::Exploitation, 10000));
    }

    #[test]
    fn should_deny_exploration_when_budget_exhausted() {
        let mut budget = ExplorationBudget::default();
        // Use up 20% of a 1000-token budget with exploration.
        budget.exploration_tokens = 200;
        budget.exploitation_tokens = 0;
        assert!(!budget.should_allow(ToolCallType::Exploration, 1000));
    }

    #[test]
    fn exploitation_always_allowed() {
        let mut budget = ExplorationBudget::default();
        budget.exploitation_tokens = 900;
        budget.exploration_tokens = 0;
        // Even when exploitation budget is "exceeded", it's still allowed.
        assert!(budget.should_allow(ToolCallType::Exploitation, 1000));
    }

    #[test]
    fn record_updates_density() {
        let mut budget = ExplorationBudget::default();
        budget.record("file_read", 10, 100, 0.5);
        let density = budget.per_category_density.get("file_read").unwrap();
        assert_eq!(density.total_calls, 1);
        assert_eq!(density.total_output_tokens, 100);
        assert!((density.bits_per_token - 0.005).abs() < 0.0001);
    }

    #[test]
    fn current_ratio_tracks_usage() {
        let mut budget = ExplorationBudget::default();
        budget.exploration_tokens = 200;
        budget.exploitation_tokens = 800;
        let (exp, expl) = budget.current_ratio();
        assert!((exp - 0.2).abs() < 0.001);
        assert!((expl - 0.8).abs() < 0.001);
    }

    #[test]
    fn reset_session_clears_counters() {
        let mut budget = ExplorationBudget::default();
        budget.exploration_tokens = 100;
        budget.exploitation_tokens = 400;
        budget.recent_exploration_bpt.push(0.01);
        budget.reset_session();
        assert_eq!(budget.exploration_tokens, 0);
        assert_eq!(budget.exploitation_tokens, 0);
        assert!(budget.recent_exploration_bpt.is_empty());
    }
}
