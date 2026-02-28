//! Generate validation prompts for user review.

use cortex_core::memory::BaseMemory;

/// A validation prompt for the user.
#[derive(Debug, Clone)]
pub struct ValidationPrompt {
    /// The memory ID being validated.
    pub memory_id: String,
    /// The prompt text to show the user.
    pub prompt: String,
    /// Suggested actions.
    pub actions: Vec<String>,
}

/// Generate a validation prompt for a memory.
pub fn generate_prompt(memory: &BaseMemory) -> ValidationPrompt {
    let prompt = format!(
        "Memory (confidence: {:.0}%): {}\n\nIs this still accurate?",
        memory.confidence.value() * 100.0,
        memory.summary
    );

    ValidationPrompt {
        memory_id: memory.id.clone(),
        prompt,
        actions: vec![
            "Confirm — this is correct".to_string(),
            "Reject — this is wrong".to_string(),
            "Modify — update with corrections".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cortex_core::memory::types::InsightContent;
    use cortex_core::memory::*;

    #[test]
    fn generates_prompt_with_actions() {
        let content = TypedContent::Insight(InsightContent {
            observation: "test".to_string(),
            evidence: vec![],
        });
        let memory = BaseMemory {
            id: "test-id".to_string(),
            memory_type: MemoryType::Insight,
            content: content.clone(),
            summary: "Always use Result for error handling".to_string(),
            transaction_time: Utc::now(),
            valid_time: Utc::now(),
            valid_until: None,
            confidence: Confidence::new(0.4),
            importance: Importance::Normal,
            last_accessed: Utc::now(),
            access_count: 0,
            linked_patterns: vec![],
            linked_constraints: vec![],
            linked_files: vec![],
            linked_functions: vec![],
            tags: vec![],
            archived: false,
            superseded_by: None,
            supersedes: None,
            namespace: Default::default(),
            source_agent: Default::default(),
            content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        };

        let prompt = generate_prompt(&memory);
        assert_eq!(prompt.memory_id, "test-id");
        assert!(prompt.prompt.contains("40%"));
        assert_eq!(prompt.actions.len(), 3);
    }
}
