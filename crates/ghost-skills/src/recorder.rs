//! Workflow recorder (Task 21.1).
//!
//! Records successful multi-tool-call sequences for potential skill creation.
//! A "successful sequence" is: all tool calls succeeded, no policy violations,
//! user didn't intervene to correct, and the final outcome was accepted.
//! Recording is passive — it observes tool calls, doesn't modify them.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Recording status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingStatus {
    Active,
    Completed,
    Abandoned,
}

/// A single step in a recorded workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub tool_name: String,
    /// Arguments with concrete values replaced by placeholders.
    pub arguments_template: serde_json::Value,
    /// Compressed summary of tool output (not full output).
    pub output_summary: String,
    pub duration_ms: u64,
    pub succeeded: bool,
}

/// An active workflow recording.
#[derive(Debug, Clone)]
pub struct WorkflowRecording {
    pub session_id: Uuid,
    pub started_at: DateTime<Utc>,
    /// The user message that initiated the workflow.
    pub trigger_message: String,
    pub steps: Vec<WorkflowStep>,
    pub status: RecordingStatus,
}

/// A completed workflow ready for skill proposal.
#[derive(Debug, Clone)]
pub struct CompletedWorkflow {
    pub recording: WorkflowRecording,
    pub total_tokens_used: usize,
    /// blake3 hash of tool sequence pattern (names + argument shapes).
    pub similarity_hash: [u8; 32],
}

/// Workflow recorder — observes tool calls and records successful sequences.
pub struct WorkflowRecorder {
    active_recordings: BTreeMap<Uuid, WorkflowRecording>,
    completed_recordings: Vec<CompletedWorkflow>,
}

impl WorkflowRecorder {
    pub fn new() -> Self {
        Self {
            active_recordings: BTreeMap::new(),
            completed_recordings: Vec::new(),
        }
    }

    /// Begin recording a workflow for a session.
    pub fn start_recording(&mut self, session_id: Uuid, trigger: &str) {
        self.active_recordings.insert(
            session_id,
            WorkflowRecording {
                session_id,
                started_at: Utc::now(),
                trigger_message: trigger.to_string(),
                steps: Vec::new(),
                status: RecordingStatus::Active,
            },
        );
    }

    /// Add a step to an active recording.
    pub fn record_step(&mut self, session_id: Uuid, step: WorkflowStep) {
        if let Some(recording) = self.active_recordings.get_mut(&session_id) {
            if recording.status == RecordingStatus::Active {
                recording.steps.push(step);
            } else {
                tracing::debug!(
                    session_id = %session_id,
                    status = ?recording.status,
                    tool = %step.tool_name,
                    "record_step skipped — recording is not active"
                );
            }
        } else {
            tracing::debug!(
                session_id = %session_id,
                tool = %step.tool_name,
                "record_step skipped — no active recording for session"
            );
        }
    }

    /// Finalize a recording as completed.
    pub fn complete(&mut self, session_id: Uuid) -> Option<CompletedWorkflow> {
        let mut recording = self.active_recordings.remove(&session_id)?;
        recording.status = RecordingStatus::Completed;

        let similarity_hash = compute_similarity_hash(&recording);
        let total_tokens = recording
            .steps
            .iter()
            .map(|s| s.output_summary.len() / 4)
            .sum();

        let completed = CompletedWorkflow {
            recording,
            total_tokens_used: total_tokens,
            similarity_hash,
        };

        self.completed_recordings.push(completed.clone());
        Some(completed)
    }

    /// Abandon a recording (user intervened, policy violation, etc.).
    pub fn abandon(&mut self, session_id: Uuid) {
        self.active_recordings.remove(&session_id);
    }

    /// Get completed recordings pending skill proposal.
    pub fn completed(&self) -> &[CompletedWorkflow] {
        &self.completed_recordings
    }

    /// Drain completed recordings.
    pub fn drain_completed(&mut self) -> Vec<CompletedWorkflow> {
        std::mem::take(&mut self.completed_recordings)
    }
}

impl Default for WorkflowRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute similarity hash from tool sequence pattern.
/// Same tool sequence with different concrete arguments → same hash.
fn compute_similarity_hash(recording: &WorkflowRecording) -> [u8; 32] {
    let mut hasher_input = String::new();
    for step in &recording.steps {
        hasher_input.push_str(&step.tool_name);
        hasher_input.push('|');
        // Hash argument shape (keys only, not values)
        if let serde_json::Value::Object(map) = &step.arguments_template {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                hasher_input.push_str(key);
                hasher_input.push(',');
            }
        }
        hasher_input.push(';');
    }
    *blake3::hash(hasher_input.as_bytes()).as_bytes()
}

/// Template concrete values in arguments with placeholders.
pub fn templatize_arguments(args: &serde_json::Value) -> serde_json::Value {
    match args {
        serde_json::Value::String(s) => {
            // Replace file paths
            if s.contains('/')
                && (s.contains(".rs")
                    || s.contains(".ts")
                    || s.contains(".py")
                    || s.contains(".js"))
            {
                return serde_json::Value::String("{file_path}".into());
            }
            // Replace URLs
            if s.starts_with("http://") || s.starts_with("https://") {
                return serde_json::Value::String("{url}".into());
            }
            serde_json::Value::String(s.clone())
        }
        serde_json::Value::Object(map) => {
            let templated: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), templatize_arguments(v)))
                .collect();
            serde_json::Value::Object(templated)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_record_complete() {
        let mut recorder = WorkflowRecorder::new();
        let sid = Uuid::now_v7();
        recorder.start_recording(sid, "fix the bug");
        recorder.record_step(
            sid,
            WorkflowStep {
                tool_name: "file_read".into(),
                arguments_template: serde_json::json!({"path": "{file_path}"}),
                output_summary: "file contents".into(),
                duration_ms: 100,
                succeeded: true,
            },
        );
        let completed = recorder.complete(sid);
        assert!(completed.is_some());
        let c = completed.unwrap();
        assert_eq!(c.recording.steps.len(), 1);
        assert_eq!(c.recording.status, RecordingStatus::Completed);
    }

    #[test]
    fn abandon_removes_recording() {
        let mut recorder = WorkflowRecorder::new();
        let sid = Uuid::now_v7();
        recorder.start_recording(sid, "test");
        recorder.abandon(sid);
        assert!(recorder.complete(sid).is_none());
    }

    #[test]
    fn similarity_hash_same_for_same_sequence() {
        let mut recorder = WorkflowRecorder::new();
        let sid1 = Uuid::now_v7();
        let sid2 = Uuid::now_v7();

        for sid in [sid1, sid2] {
            recorder.start_recording(sid, "test");
            recorder.record_step(
                sid,
                WorkflowStep {
                    tool_name: "file_read".into(),
                    arguments_template: serde_json::json!({"path": "different_path"}),
                    output_summary: "output".into(),
                    duration_ms: 50,
                    succeeded: true,
                },
            );
        }

        let c1 = recorder.complete(sid1).unwrap();
        let c2 = recorder.complete(sid2).unwrap();
        assert_eq!(c1.similarity_hash, c2.similarity_hash);
    }

    #[test]
    fn similarity_hash_differs_for_different_sequence() {
        let mut recorder = WorkflowRecorder::new();
        let sid1 = Uuid::now_v7();
        let sid2 = Uuid::now_v7();

        recorder.start_recording(sid1, "test");
        recorder.record_step(
            sid1,
            WorkflowStep {
                tool_name: "file_read".into(),
                arguments_template: serde_json::json!({}),
                output_summary: "".into(),
                duration_ms: 0,
                succeeded: true,
            },
        );

        recorder.start_recording(sid2, "test");
        recorder.record_step(
            sid2,
            WorkflowStep {
                tool_name: "web_search".into(),
                arguments_template: serde_json::json!({}),
                output_summary: "".into(),
                duration_ms: 0,
                succeeded: true,
            },
        );

        let c1 = recorder.complete(sid1).unwrap();
        let c2 = recorder.complete(sid2).unwrap();
        assert_ne!(c1.similarity_hash, c2.similarity_hash);
    }

    #[test]
    fn templatize_file_path() {
        let args = serde_json::json!({"path": "/home/user/project/src/main.rs"});
        let templated = templatize_arguments(&args);
        assert_eq!(templated["path"], "{file_path}");
    }

    #[test]
    fn templatize_url() {
        let args = serde_json::json!({"url": "https://api.example.com/v1/data"});
        let templated = templatize_arguments(&args);
        assert_eq!(templated["url"], "{url}");
    }

    #[test]
    fn empty_recording_completes() {
        let mut recorder = WorkflowRecorder::new();
        let sid = Uuid::now_v7();
        recorder.start_recording(sid, "test");
        let completed = recorder.complete(sid);
        assert!(completed.is_some());
        assert_eq!(completed.unwrap().recording.steps.len(), 0);
    }
}
