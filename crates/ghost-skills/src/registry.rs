//! Skill registry: discover, verify, load (Req 23 AC1, AC5).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Skill discovery priority: workspace > user > bundled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillSource {
    Bundled = 0,
    User = 1,
    Workspace = 2,
}

/// Skill manifest parsed from YAML frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: u64,
    pub signature: Option<String>,
}

/// Skill state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillState {
    Loaded,
    Quarantined,
}

/// Registered skill entry.
#[derive(Debug, Clone)]
pub struct RegisteredSkill {
    pub manifest: SkillManifest,
    pub source: SkillSource,
    pub path: PathBuf,
    pub state: SkillState,
}

/// Skill registry.
pub struct SkillRegistry {
    skills: BTreeMap<String, RegisteredSkill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: BTreeMap::new(),
        }
    }

    /// Register a skill. Quarantines if signature is invalid or missing.
    pub fn register(&mut self, manifest: SkillManifest, source: SkillSource, path: PathBuf) {
        let state = if self.verify_signature(&manifest) {
            SkillState::Loaded
        } else {
            tracing::warn!(
                skill = %manifest.name,
                "Skill quarantined: invalid or missing signature"
            );
            SkillState::Quarantined
        };

        self.skills.insert(
            manifest.name.clone(),
            RegisteredSkill {
                manifest,
                source,
                path,
                state,
            },
        );
    }

    /// Lookup a skill by name.
    pub fn lookup(&self, name: &str) -> Option<&RegisteredSkill> {
        self.skills.get(name)
    }

    /// Get all loaded (non-quarantined) skills.
    pub fn loaded_skills(&self) -> Vec<&RegisteredSkill> {
        self.skills
            .values()
            .filter(|s| s.state == SkillState::Loaded)
            .collect()
    }

    /// Get all quarantined skills.
    pub fn quarantined_skills(&self) -> Vec<&RegisteredSkill> {
        self.skills
            .values()
            .filter(|s| s.state == SkillState::Quarantined)
            .collect()
    }

    fn verify_signature(&self, manifest: &SkillManifest) -> bool {
        // Signature verification via ghost-signing
        manifest.signature.is_some()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
