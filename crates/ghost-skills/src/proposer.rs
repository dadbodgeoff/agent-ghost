//! Skill proposer (Task 21.2).
//!
//! After seeing the same workflow pattern 3+ times, proposes creating a skill.
//! Skill proposals require human approval (same flow as goal proposals).

use std::collections::{BTreeMap, BTreeSet};

use crate::recorder::CompletedWorkflow;
use crate::registry::SkillManifest;

/// A proposed skill from observed workflow patterns.
#[derive(Debug, Clone)]
pub struct SkillProposal {
    /// Auto-generated kebab-case name from tool sequence.
    pub name: String,
    /// Auto-generated description from trigger messages.
    pub description: String,
    /// The workflow that triggered the proposal.
    pub workflow: CompletedWorkflow,
    /// Estimated tokens saved per replay (67% of workflow tokens).
    pub estimated_tokens_saved: usize,
    /// Number of times this pattern was observed.
    pub occurrences: u32,
}

/// Skill proposer — observes completed workflows and proposes skills.
pub struct SkillProposer {
    /// similarity_hash → occurrence count.
    pattern_counts: BTreeMap<[u8; 32], u32>,
    /// Propose skill after N occurrences (default 3).
    proposal_threshold: u32,
    /// Already-proposed patterns (don't re-propose).
    proposed_skills: BTreeSet<[u8; 32]>,
    /// Cumulative tokens saved by skill reuse.
    pub tokens_saved_by_skills: u64,
}

impl SkillProposer {
    pub fn new() -> Self {
        Self {
            pattern_counts: BTreeMap::new(),
            proposal_threshold: 3,
            proposed_skills: BTreeSet::new(),
            tokens_saved_by_skills: 0,
        }
    }

    /// Create with a custom proposal threshold.
    pub fn with_threshold(threshold: u32) -> Self {
        Self {
            proposal_threshold: threshold,
            ..Self::new()
        }
    }

    /// Observe a completed workflow. Returns a proposal if threshold is met.
    pub fn observe(&mut self, workflow: &CompletedWorkflow) -> Option<SkillProposal> {
        let hash = workflow.similarity_hash;
        let count = self.pattern_counts.entry(hash).or_insert(0);
        let prev = *count;
        *count = count.saturating_add(1);

        if prev == u32::MAX {
            tracing::warn!(
                hash = %format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    hash[0], hash[1], hash[2], hash[3],
                    hash[4], hash[5], hash[6], hash[7]),
                "pattern count saturated at u32::MAX — no further counting for this pattern"
            );
        }

        if *count >= self.proposal_threshold && !self.proposed_skills.contains(&hash) {
            self.proposed_skills.insert(hash);

            let name = generate_skill_name(&workflow.recording);
            let description = format!(
                "Auto-generated skill from {} occurrences of: {}",
                count, workflow.recording.trigger_message
            );

            tracing::info!(
                name = %name,
                occurrences = *count,
                "skill proposal generated"
            );

            Some(SkillProposal {
                name,
                description,
                workflow: workflow.clone(),
                estimated_tokens_saved: (workflow.total_tokens_used as f64 * 0.67) as usize,
                occurrences: *count,
            })
        } else {
            None
        }
    }

    /// Convert an approved proposal to a SkillManifest for registration.
    pub fn approve(proposal: &SkillProposal) -> SkillManifest {
        SkillManifest {
            name: proposal.name.clone(),
            version: "1.0.0".into(),
            description: proposal.description.clone(),
            capabilities: proposal
                .workflow
                .recording
                .steps
                .iter()
                .map(|s| s.tool_name.clone())
                .collect(),
            timeout_seconds: 30,
            signature: None, // Needs signing before registration
        }
    }
}

impl Default for SkillProposer {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a kebab-case skill name from the tool sequence.
fn generate_skill_name(recording: &crate::recorder::WorkflowRecording) -> String {
    let parts: Vec<&str> = recording
        .steps
        .iter()
        .map(|s| s.tool_name.as_str())
        .collect();

    if parts.is_empty() {
        return "empty-workflow".into();
    }

    // Deduplicate consecutive same tools
    let mut deduped: Vec<&str> = Vec::new();
    for part in &parts {
        if deduped.last() != Some(part) {
            deduped.push(part);
        }
    }

    // Join with "then" and convert to kebab-case
    deduped
        .iter()
        .map(|s| s.replace('_', "-"))
        .collect::<Vec<_>>()
        .join("-then-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::{RecordingStatus, WorkflowRecording, WorkflowStep};
    use uuid::Uuid;

    fn make_workflow(tools: &[&str]) -> CompletedWorkflow {
        let steps: Vec<WorkflowStep> = tools
            .iter()
            .map(|t| WorkflowStep {
                tool_name: t.to_string(),
                arguments_template: serde_json::json!({}),
                output_summary: "output".into(),
                duration_ms: 100,
                succeeded: true,
            })
            .collect();

        let recording = WorkflowRecording {
            session_id: Uuid::now_v7(),
            started_at: chrono::Utc::now(),
            trigger_message: "test workflow".into(),
            steps,
            status: RecordingStatus::Completed,
        };

        let hash = *blake3::hash(b"test").as_bytes();
        CompletedWorkflow {
            recording,
            total_tokens_used: 1000,
            similarity_hash: hash,
        }
    }

    #[test]
    fn first_occurrence_no_proposal() {
        let mut proposer = SkillProposer::new();
        let wf = make_workflow(&["file_read"]);
        assert!(proposer.observe(&wf).is_none());
    }

    #[test]
    fn second_occurrence_no_proposal() {
        let mut proposer = SkillProposer::new();
        let wf = make_workflow(&["file_read"]);
        proposer.observe(&wf);
        assert!(proposer.observe(&wf).is_none());
    }

    #[test]
    fn third_occurrence_generates_proposal() {
        let mut proposer = SkillProposer::new();
        let wf = make_workflow(&["file_read"]);
        proposer.observe(&wf);
        proposer.observe(&wf);
        let proposal = proposer.observe(&wf);
        assert!(proposal.is_some());
        assert_eq!(proposal.unwrap().occurrences, 3);
    }

    #[test]
    fn fourth_occurrence_no_re_proposal() {
        let mut proposer = SkillProposer::new();
        let wf = make_workflow(&["file_read"]);
        for _ in 0..3 {
            proposer.observe(&wf);
        }
        // 4th occurrence — already proposed
        assert!(proposer.observe(&wf).is_none());
    }

    #[test]
    fn estimated_tokens_saved_is_67_percent() {
        let mut proposer = SkillProposer::new();
        let wf = make_workflow(&["file_read"]);
        for _ in 0..2 {
            proposer.observe(&wf);
        }
        let proposal = proposer.observe(&wf).unwrap();
        assert_eq!(proposal.estimated_tokens_saved, 670); // 67% of 1000
    }

    #[test]
    fn approve_produces_valid_manifest() {
        let mut proposer = SkillProposer::new();
        let wf = make_workflow(&["file_read", "web_search"]);
        for _ in 0..2 {
            proposer.observe(&wf);
        }
        let proposal = proposer.observe(&wf).unwrap();
        let manifest = SkillProposer::approve(&proposal);
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.capabilities.len(), 2);
    }

    #[test]
    fn generated_name_is_kebab_case() {
        let name = generate_skill_name(&WorkflowRecording {
            session_id: Uuid::nil(),
            started_at: chrono::Utc::now(),
            trigger_message: "test".into(),
            steps: vec![
                WorkflowStep {
                    tool_name: "file_read".into(),
                    arguments_template: serde_json::json!({}),
                    output_summary: "".into(),
                    duration_ms: 0,
                    succeeded: true,
                },
                WorkflowStep {
                    tool_name: "web_search".into(),
                    arguments_template: serde_json::json!({}),
                    output_summary: "".into(),
                    duration_ms: 0,
                    succeeded: true,
                },
            ],
            status: RecordingStatus::Completed,
        });
        assert_eq!(name, "file-read-then-web-search");
    }

    #[test]
    fn pattern_count_saturates() {
        let mut proposer = SkillProposer::new();
        let wf = make_workflow(&["file_read"]);
        // Won't overflow even with many observations
        for _ in 0..1000 {
            proposer.observe(&wf);
        }
    }
}
