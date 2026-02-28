//! Snapshot formatter — serializes to prompt-ready text (Req 20 AC4).

use crate::snapshot::AgentSnapshot;

/// Serializes an `AgentSnapshot` to prompt-ready text blocks with
/// per-section token allocation. Consumed by PromptCompiler at Layer L6.
pub struct SnapshotFormatter {
    /// Approximate characters per token (rough estimate for budgeting).
    chars_per_token: usize,
}

impl SnapshotFormatter {
    pub fn new() -> Self {
        Self { chars_per_token: 4 }
    }

    /// Format the snapshot into a prompt-ready string, respecting the token budget.
    pub fn format(&self, snapshot: &AgentSnapshot, token_budget: usize) -> String {
        let char_budget = token_budget * self.chars_per_token;
        let mut output = String::with_capacity(char_budget.min(16384));

        // Section 1: Convergence state
        let state = snapshot.convergence_state();
        let convergence_section = format!(
            "[Convergence] score={:.3} level={}\n",
            state.score, state.level
        );
        output.push_str(&convergence_section);

        // Section 2: Simulation boundary prompt
        let sim_prompt = snapshot.simulation_prompt();
        if !sim_prompt.is_empty() && output.len() + sim_prompt.len() + 20 < char_budget {
            output.push_str("[SimulationBoundary]\n");
            output.push_str(sim_prompt);
            output.push('\n');
        }

        // Section 3: Active goals
        if !snapshot.goals().is_empty() {
            output.push_str("[Goals]\n");
            for goal in snapshot.goals() {
                let line = format!("- {} ({:?}/{:?})\n", goal.goal_text, goal.scope, goal.origin);
                if output.len() + line.len() > char_budget {
                    break;
                }
                output.push_str(&line);
            }
        }

        // Section 4: Recent reflections
        if !snapshot.reflections().is_empty() {
            output.push_str("[Reflections]\n");
            for refl in snapshot.reflections().iter().take(5) {
                let line = format!("- {}\n", refl.reflection_text);
                if output.len() + line.len() > char_budget {
                    break;
                }
                output.push_str(&line);
            }
        }

        // Section 5: Memory summary
        output.push_str(&format!(
            "[Memories] {} available\n",
            snapshot.memories().len()
        ));

        // Truncate to budget
        if output.len() > char_budget {
            output.truncate(char_budget);
        }

        output
    }
}

impl Default for SnapshotFormatter {
    fn default() -> Self {
        Self::new()
    }
}
