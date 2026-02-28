//! IdentityManager — loads IDENTITY.md (name, voice, emoji, channel behavior) (Req 24 AC2).

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("IDENTITY.md not found at {path}")]
    NotFound { path: String },
    #[error("failed to read IDENTITY.md: {0}")]
    ReadError(String),
}

/// Parsed identity configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub name: String,
    pub voice: String,
    pub emoji: Option<String>,
    pub channel_behavior: serde_json::Value,
    pub raw_content: String,
}

/// Manages the IDENTITY.md document (read-only to agent).
pub struct IdentityManager {
    identity: Option<AgentIdentity>,
}

impl IdentityManager {
    pub fn new() -> Self {
        Self { identity: None }
    }

    /// Load IDENTITY.md from the given path.
    pub fn load(&mut self, path: &Path) -> Result<&AgentIdentity, IdentityError> {
        if !path.exists() {
            return Err(IdentityError::NotFound {
                path: path.display().to_string(),
            });
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| IdentityError::ReadError(e.to_string()))?;

        // Parse YAML frontmatter if present, otherwise use defaults
        let identity = if content.starts_with("---") {
            // Simple frontmatter extraction
            let parts: Vec<&str> = content.splitn(3, "---").collect();
            if parts.len() >= 3 {
                let frontmatter = parts[1].trim();
                let name = extract_field(frontmatter, "name")
                    .unwrap_or_else(|| "Agent".to_string());
                let voice = extract_field(frontmatter, "voice")
                    .unwrap_or_else(|| "neutral".to_string());
                let emoji = extract_field(frontmatter, "emoji");
                AgentIdentity {
                    name,
                    voice,
                    emoji,
                    channel_behavior: serde_json::Value::Null,
                    raw_content: content,
                }
            } else {
                default_identity(content)
            }
        } else {
            default_identity(content)
        };

        self.identity = Some(identity);
        // SAFETY: We just assigned Some above, so this branch is unreachable.
        // Structured to avoid unwrap() per Task 7.1 conventions.
        match self.identity.as_ref() {
            Some(id) => Ok(id),
            None => Err(IdentityError::ReadError(
                "internal error: identity not set after assignment".into(),
            )),
        }
    }

    pub fn identity(&self) -> Option<&AgentIdentity> {
        self.identity.as_ref()
    }
}

impl Default for IdentityManager {
    fn default() -> Self {
        Self::new()
    }
}

fn default_identity(content: String) -> AgentIdentity {
    AgentIdentity {
        name: "Agent".to_string(),
        voice: "neutral".to_string(),
        emoji: None,
        channel_behavior: serde_json::Value::Null,
        raw_content: content,
    }
}

fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(value) = rest.strip_prefix(':') {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}
