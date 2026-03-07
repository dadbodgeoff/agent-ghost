//! Export analyzer — orchestrates import, parsing, signal computation (Req 35 AC1, AC4).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::parsers;
use crate::timeline::{ReconstructedSession, TimelineReconstructor};
use crate::{ExportError, ExportResult};

/// Result of analyzing an export file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportAnalysisResult {
    pub source_format: String,
    pub total_messages: usize,
    pub total_sessions: usize,
    pub per_session_scores: Vec<SessionScore>,
    pub recommended_level: u8,
    pub flagged_sessions: Vec<String>,
}

/// Per-session convergence score estimate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionScore {
    pub session_id: String,
    pub message_count: usize,
    pub duration_seconds: i64,
    pub estimated_score: f64,
}

/// Orchestrates the full export analysis pipeline.
pub struct ExportAnalyzer {
    timeline: TimelineReconstructor,
}

impl Default for ExportAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportAnalyzer {
    pub fn new() -> Self {
        Self {
            timeline: TimelineReconstructor::default(),
        }
    }

    /// Analyze an export file: detect format, parse, reconstruct timeline, compute scores.
    pub fn analyze(&self, path: &Path) -> ExportResult<ExportAnalysisResult> {
        let parsers = parsers::all_parsers();

        // Detect format
        let parser = parsers
            .iter()
            .find(|p| p.detect(path))
            .ok_or_else(|| ExportError::UnsupportedFormat(
                format!("No parser detected for: {}", path.display()),
            ))?;

        tracing::info!(parser = parser.name(), "Detected export format");

        // Parse messages
        let messages = parser.parse(path)?;
        if messages.is_empty() {
            return Ok(ExportAnalysisResult {
                source_format: parser.name().to_string(),
                total_messages: 0,
                total_sessions: 0,
                per_session_scores: Vec::new(),
                recommended_level: 0,
                flagged_sessions: Vec::new(),
            });
        }

        // Reconstruct timeline
        let sessions = self.timeline.reconstruct(&messages);

        // Compute per-session scores
        let per_session_scores: Vec<SessionScore> = sessions
            .iter()
            .map(|s| self.score_session(s))
            .collect();

        // Determine flagged sessions and recommended level
        let flagged: Vec<String> = per_session_scores
            .iter()
            .filter(|s| s.estimated_score > 0.5)
            .map(|s| s.session_id.clone())
            .collect();

        let max_score = per_session_scores
            .iter()
            .map(|s| s.estimated_score)
            .fold(0.0f64, f64::max);

        let recommended_level = if max_score > 0.85 {
            4
        } else if max_score > 0.7 {
            3
        } else if max_score > 0.5 {
            2
        } else if max_score > 0.3 {
            1
        } else {
            0
        };

        Ok(ExportAnalysisResult {
            source_format: parser.name().to_string(),
            total_messages: messages.len(),
            total_sessions: sessions.len(),
            per_session_scores,
            recommended_level,
            flagged_sessions: flagged,
        })
    }

    fn score_session(&self, session: &ReconstructedSession) -> SessionScore {
        let duration = (session.end - session.start).num_seconds();

        // Simple heuristic scoring based on session characteristics
        let duration_signal = (duration as f64 / 21600.0).min(1.0); // 6h max
        let msg_density = if duration > 0 {
            (session.message_count as f64 / (duration as f64 / 60.0)).min(1.0)
        } else {
            0.0
        };

        let estimated_score = ((duration_signal * 0.5) + (msg_density * 0.5)).clamp(0.0, 1.0);

        SessionScore {
            session_id: session.session_id.clone(),
            message_count: session.message_count,
            duration_seconds: duration,
            estimated_score,
        }
    }
}
