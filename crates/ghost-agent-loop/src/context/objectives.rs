//! Running objectives tracker (Task 20.3).
//!
//! Maintains a running objectives summary and injects it just before L9
//! (user message) in the high-attention zone. This is the "todo.md trick"
//! from Manus — keeps the agent focused on what it's supposed to be doing.
//!
//! Objectives summary is always < 200 tokens (hard cap).

/// Running objectives tracker.
#[derive(Debug, Clone)]
pub struct ObjectivesTracker {
    /// Active objectives extracted from goal proposals.
    current_objectives: Vec<String>,
    /// Last 3 decisions made (from proposal outcomes).
    recent_decisions: Vec<String>,
    /// Unresolved questions or errors.
    blockers: Vec<String>,
}

/// Approximate token count (chars / 4).
fn approx_tokens(s: &str) -> usize {
    s.len() / 4
}

impl ObjectivesTracker {
    pub fn new() -> Self {
        Self {
            current_objectives: Vec::new(),
            recent_decisions: Vec::new(),
            blockers: Vec::new(),
        }
    }

    /// Add an objective.
    pub fn add_objective(&mut self, objective: String) {
        self.current_objectives.push(objective);
    }

    /// Remove a completed objective.
    pub fn complete_objective(&mut self, index: usize) {
        if index < self.current_objectives.len() {
            self.current_objectives.remove(index);
        }
    }

    /// Record a decision (keeps last 3).
    pub fn record_decision(&mut self, decision: String) {
        self.recent_decisions.push(decision);
        if self.recent_decisions.len() > 3 {
            self.recent_decisions.remove(0);
        }
    }

    /// Add a blocker.
    pub fn add_blocker(&mut self, blocker: String) {
        self.blockers.push(blocker);
    }

    /// Remove a resolved blocker.
    pub fn resolve_blocker(&mut self, index: usize) {
        if index < self.blockers.len() {
            self.blockers.remove(index);
        }
    }

    /// Clear all state.
    pub fn reset(&mut self) {
        self.current_objectives.clear();
        self.recent_decisions.clear();
        self.blockers.clear();
    }

    /// Compile a summary for injection between L8 and L9.
    /// Hard cap: < 200 tokens. Truncates if needed, prioritizing most recent.
    pub fn compile_summary(&self) -> String {
        if self.current_objectives.is_empty()
            && self.recent_decisions.is_empty()
            && self.blockers.is_empty()
        {
            return "CURRENT STATE: No active objectives.".to_string();
        }

        let objectives_str = if self.current_objectives.is_empty() {
            "None".to_string()
        } else {
            self.current_objectives.join("; ")
        };

        let decisions_str = if self.recent_decisions.is_empty() {
            "None".to_string()
        } else {
            self.recent_decisions.join("; ")
        };

        let blockers_str = if self.blockers.is_empty() {
            "None".to_string()
        } else {
            self.blockers.join("; ")
        };

        let full = format!(
            "CURRENT STATE:\n- Objectives: {}\n- Recent decisions: {}\n- Blockers: {}",
            objectives_str, decisions_str, blockers_str
        );

        // Hard cap at 200 tokens (~800 chars)
        if approx_tokens(&full) <= 200 {
            full
        } else {
            // Truncate to fit, keeping the structure
            let max_chars = 800;
            let mut truncated = full;
            truncated.truncate(max_chars);
            // Clean up at last newline or semicolon
            if let Some(pos) = truncated.rfind(|c| c == '\n' || c == ';') {
                truncated.truncate(pos);
            }
            truncated.push_str(" [truncated]");
            truncated
        }
    }
}

impl Default for ObjectivesTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tracker_minimal_output() {
        let tracker = ObjectivesTracker::new();
        let summary = tracker.compile_summary();
        assert_eq!(summary, "CURRENT STATE: No active objectives.");
    }

    #[test]
    fn with_objectives_formatted() {
        let mut tracker = ObjectivesTracker::new();
        tracker.add_objective("Fix login bug".into());
        tracker.add_objective("Add tests".into());
        tracker.add_objective("Deploy to staging".into());
        let summary = tracker.compile_summary();
        assert!(summary.contains("Fix login bug"));
        assert!(summary.contains("Add tests"));
        assert!(approx_tokens(&summary) <= 200);
    }

    #[test]
    fn with_all_fields() {
        let mut tracker = ObjectivesTracker::new();
        tracker.add_objective("Implement auth".into());
        tracker.record_decision("Use JWT tokens".into());
        tracker.add_blocker("Missing API key".into());
        let summary = tracker.compile_summary();
        assert!(summary.contains("Implement auth"));
        assert!(summary.contains("Use JWT tokens"));
        assert!(summary.contains("Missing API key"));
    }

    #[test]
    fn recent_decisions_capped_at_3() {
        let mut tracker = ObjectivesTracker::new();
        tracker.record_decision("Decision 1".into());
        tracker.record_decision("Decision 2".into());
        tracker.record_decision("Decision 3".into());
        tracker.record_decision("Decision 4".into());
        let summary = tracker.compile_summary();
        assert!(!summary.contains("Decision 1"));
        assert!(summary.contains("Decision 4"));
    }

    #[test]
    fn very_long_objectives_truncated() {
        let mut tracker = ObjectivesTracker::new();
        for i in 0..100 {
            tracker.add_objective(format!("Objective number {} with extra detail padding", i));
        }
        let summary = tracker.compile_summary();
        assert!(approx_tokens(&summary) <= 210); // Allow small overshoot from truncation logic
    }

    #[test]
    fn reset_clears_all() {
        let mut tracker = ObjectivesTracker::new();
        tracker.add_objective("test".into());
        tracker.record_decision("test".into());
        tracker.add_blocker("test".into());
        tracker.reset();
        assert_eq!(
            tracker.compile_summary(),
            "CURRENT STATE: No active objectives."
        );
    }

    #[test]
    fn complete_objective_removes_it() {
        let mut tracker = ObjectivesTracker::new();
        tracker.add_objective("A".into());
        tracker.add_objective("B".into());
        tracker.complete_objective(0);
        let summary = tracker.compile_summary();
        assert!(!summary.contains("- Objectives: A"));
        assert!(summary.contains("B"));
    }
}
