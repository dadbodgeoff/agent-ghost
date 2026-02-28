//! Caller identity for authorization at the NAPI boundary.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::convergence_config::ConvergenceConfig;
use crate::memory::importance::Importance;
use crate::memory::types::MemoryType;

/// Who is making this request?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum CallerType {
    /// The platform itself (full access).
    Platform,
    /// An AI agent (restricted access).
    Agent { agent_id: String },
    /// A human user (full access, different audit trail).
    Human { user_id: String },
}

impl CallerType {
    pub fn is_platform(&self) -> bool {
        matches!(self, CallerType::Platform)
    }

    pub fn is_agent(&self) -> bool {
        matches!(self, CallerType::Agent { .. })
    }

    /// Check if this caller can create the given memory type.
    pub fn can_create_type(
        &self,
        memory_type: MemoryType,
        config: &ConvergenceConfig,
    ) -> bool {
        if self.is_platform() {
            return true;
        }
        !config.restricted_types.contains(&memory_type)
    }

    /// Check if this caller can assign the given importance.
    pub fn can_assign_importance(
        &self,
        importance: Importance,
        config: &ConvergenceConfig,
    ) -> bool {
        if self.is_platform() {
            return true;
        }
        !config.restricted_importance.contains(&importance)
    }
}
