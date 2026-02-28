//! Process feedback: confirm/reject/modify → update confidence.

use cortex_core::memory::{BaseMemory, Confidence};

/// User feedback on a memory.
#[derive(Debug, Clone)]
pub enum Feedback {
    /// User confirms the memory is correct.
    Confirm,
    /// User rejects the memory as incorrect.
    Reject,
    /// User modifies the memory with a correction.
    Modify(String),
}

/// Result of processing feedback.
#[derive(Debug, Clone)]
pub struct FeedbackResult {
    /// The memory ID that was validated.
    pub memory_id: String,
    /// New confidence after feedback.
    pub new_confidence: f64,
    /// Whether the memory should be archived (rejected).
    pub should_archive: bool,
    /// Optional correction text (for Modify feedback).
    pub correction: Option<String>,
}

/// Confidence boost on confirmation.
const CONFIRM_BOOST: f64 = 0.15;
/// Confidence penalty on rejection.
const REJECT_PENALTY: f64 = 0.3;
/// Confidence adjustment on modification.
const MODIFY_ADJUSTMENT: f64 = 0.05;

/// Process user feedback on a memory.
pub fn process_feedback(memory: &BaseMemory, feedback: &Feedback) -> FeedbackResult {
    match feedback {
        Feedback::Confirm => FeedbackResult {
            memory_id: memory.id.clone(),
            new_confidence: (memory.confidence.value() + CONFIRM_BOOST).min(1.0),
            should_archive: false,
            correction: None,
        },
        Feedback::Reject => {
            let new_conf = (memory.confidence.value() - REJECT_PENALTY).max(0.0);
            FeedbackResult {
                memory_id: memory.id.clone(),
                new_confidence: new_conf,
                should_archive: new_conf < Confidence::ARCHIVAL,
                correction: None,
            }
        }
        Feedback::Modify(correction) => FeedbackResult {
            memory_id: memory.id.clone(),
            new_confidence: (memory.confidence.value() + MODIFY_ADJUSTMENT).min(1.0),
            should_archive: false,
            correction: Some(correction.clone()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cortex_core::memory::types::InsightContent;
    use cortex_core::memory::*;

    fn make_memory(confidence: f64) -> BaseMemory {
        let content = TypedContent::Insight(InsightContent {
            observation: "test".to_string(),
            evidence: vec![],
        });
        BaseMemory {
            id: "test-id".to_string(),
            memory_type: MemoryType::Insight,
            content: content.clone(),
            summary: "test".to_string(),
            transaction_time: Utc::now(),
            valid_time: Utc::now(),
            valid_until: None,
            confidence: Confidence::new(confidence),
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
        }
    }

    #[test]
    fn confirm_boosts_confidence() {
        let mem = make_memory(0.5);
        let result = process_feedback(&mem, &Feedback::Confirm);
        assert!(result.new_confidence > 0.5);
        assert!(!result.should_archive);
    }

    #[test]
    fn reject_lowers_confidence() {
        let mem = make_memory(0.5);
        let result = process_feedback(&mem, &Feedback::Reject);
        assert!(result.new_confidence < 0.5);
    }

    #[test]
    fn reject_low_confidence_archives() {
        let mem = make_memory(0.2);
        let result = process_feedback(&mem, &Feedback::Reject);
        assert!(result.should_archive);
    }

    #[test]
    fn modify_adjusts_and_returns_correction() {
        let mem = make_memory(0.5);
        let result = process_feedback(&mem, &Feedback::Modify("updated text".to_string()));
        assert!(result.new_confidence > 0.5);
        assert_eq!(result.correction.unwrap(), "updated text");
    }
}
