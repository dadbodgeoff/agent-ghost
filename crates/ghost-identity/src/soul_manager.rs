//! SoulManager — loads SOUL.md, tracks versions, stores baseline embedding (Req 24 AC1).

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SoulError {
    #[error("SOUL.md not found at {path}")]
    NotFound { path: String },
    #[error("failed to read SOUL.md: {0}")]
    ReadError(String),
    #[error("SOUL.md is empty")]
    Empty,
}

/// Loaded soul document with version tracking.
#[derive(Debug, Clone)]
pub struct SoulDocument {
    pub content: String,
    pub path: PathBuf,
    pub hash: [u8; 32],
}

/// Manages the SOUL.md document (read-only to agent).
pub struct SoulManager {
    document: Option<SoulDocument>,
    /// Baseline embedding for drift detection.
    baseline_embedding: Option<Vec<f64>>,
}

impl SoulManager {
    pub fn new() -> Self {
        Self {
            document: None,
            baseline_embedding: None,
        }
    }

    /// Write a default SOUL.md template to the given path (T-3.1.1).
    pub fn create_template(path: &Path) -> Result<(), SoulError> {
        let template = "\
# SOUL.md — Agent Identity Document

## Purpose
Define the core purpose of this agent. What is it designed to do?
What problems does it solve for its users?

## Values
- Honesty and transparency in all interactions
- Respect for user privacy and data boundaries
- Helpfulness balanced with appropriate caution
- Consistency in behavior and communication

## Boundaries
- Never disclose secrets or credentials
- Do not impersonate real individuals
- Respect rate limits and resource constraints
- Escalate when uncertain rather than guessing

## Communication Style
Describe the agent's tone, voice, and preferred interaction patterns.
Should it be formal or casual? Concise or verbose? Technical or approachable?
";
        std::fs::write(path, template)
            .map_err(|e| SoulError::ReadError(format!("failed to write template: {e}")))?;
        Ok(())
    }

    /// Load SOUL.md from the given path.
    pub fn load(&mut self, path: &Path) -> Result<&SoulDocument, SoulError> {
        if !path.exists() {
            return Err(SoulError::NotFound {
                path: path.display().to_string(),
            });
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| SoulError::ReadError(e.to_string()))?;

        if content.is_empty() {
            return Err(SoulError::Empty);
        }

        let hash = *blake3::hash(content.as_bytes()).as_bytes();

        self.document = Some(SoulDocument {
            content,
            path: path.to_path_buf(),
            hash,
        });

        // SAFETY: we just assigned `Some` to self.document above
        Ok(self.document.as_ref().expect("document was just set"))
    }

    /// Get the loaded soul document.
    pub fn document(&self) -> Option<&SoulDocument> {
        self.document.as_ref()
    }

    /// Set the baseline embedding for drift detection.
    pub fn set_baseline_embedding(&mut self, embedding: Vec<f64>) {
        self.baseline_embedding = Some(embedding);
    }

    /// Get the baseline embedding.
    pub fn baseline_embedding(&self) -> Option<&[f64]> {
        self.baseline_embedding.as_deref()
    }
}

impl Default for SoulManager {
    fn default() -> Self {
        Self::new()
    }
}
