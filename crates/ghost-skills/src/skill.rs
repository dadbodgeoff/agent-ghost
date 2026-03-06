//! Core Skill trait and supporting types.
//!
//! Every executable skill — bundled, user-installed, or platform-managed —
//! implements this trait. Skills receive a `SkillContext` with access to
//! the database, agent identity, and session state, and return a JSON
//! result or a typed `SkillError`.
//!
//! Phase 5: Convergence safety skills are the first implementors.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::registry::SkillSource;

/// Result type for skill execution.
pub type SkillResult = Result<serde_json::Value, SkillError>;

/// Errors that can occur during skill execution.
#[derive(Debug, Error)]
pub enum SkillError {
    /// The agent's convergence level exceeds the skill's maximum.
    #[error("convergence too high: current level {current}, maximum allowed {maximum}")]
    ConvergenceTooHigh { current: u8, maximum: u8 },

    /// The skill's per-session action budget has been exhausted.
    #[error("action budget exhausted: {used}/{budget} actions used")]
    BudgetExhausted { used: u32, budget: u32 },

    /// The targeted application is not in the allowlist.
    #[error("app not allowed: '{app}' not in allowlist {allowed:?}")]
    AppNotAllowed { app: String, allowed: Vec<String> },

    /// The user denied a confirmation prompt.
    #[error("user denied confirmation")]
    UserDenied,

    /// A required input field is missing or has the wrong type.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Database or storage error during skill execution.
    #[error("storage error: {0}")]
    Storage(String),

    /// The skill is not available on this platform.
    #[error("platform not supported: {0}")]
    PlatformNotSupported(String),

    /// Authorization denied for this operation.
    #[error("authorization denied: {0}")]
    AuthorizationDenied(String),

    /// A reflection constraint was violated (cooldown, depth, session limit).
    #[error("reflection constraint: {0}")]
    ReflectionConstraint(String),

    /// A delegation operation failed (invalid state transition, constraint violation).
    #[error("delegation failed: {0}")]
    DelegationFailed(String),

    /// A PC control action was blocked by the safety layer (safe zone, blocked hotkey, etc.).
    #[error("pc control blocked: {0}")]
    PcControlBlocked(String),

    /// The PC control circuit breaker has tripped.
    #[error("circuit breaker open: {0}")]
    CircuitBreakerOpen(String),

    /// Internal error within the skill.
    #[error("internal: {0}")]
    Internal(String),
}

impl SkillError {
    /// Machine-readable error code for API responses.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ConvergenceTooHigh { .. } => "CONVERGENCE_TOO_HIGH",
            Self::BudgetExhausted { .. } => "BUDGET_EXHAUSTED",
            Self::AppNotAllowed { .. } => "APP_NOT_ALLOWED",
            Self::UserDenied => "USER_DENIED",
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::PlatformNotSupported(_) => "PLATFORM_NOT_SUPPORTED",
            Self::AuthorizationDenied(_) => "AUTHORIZATION_DENIED",
            Self::ReflectionConstraint(_) => "REFLECTION_CONSTRAINT",
            Self::DelegationFailed(_) => "DELEGATION_FAILED",
            Self::PcControlBlocked(_) => "PC_CONTROL_BLOCKED",
            Self::CircuitBreakerOpen(_) => "CIRCUIT_BREAKER_OPEN",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

/// Execution context passed to every skill invocation.
///
/// Carries the database connection, agent identity, and session state.
/// The context lifetime is bounded by the database lock scope in the
/// calling API handler.
pub struct SkillContext<'a> {
    /// SQLite database connection (borrowed from the Mutex-locked AppState).
    pub db: &'a rusqlite::Connection,

    /// The agent executing this skill.
    pub agent_id: Uuid,

    /// Current session identifier.
    pub session_id: Uuid,

    /// Active convergence profile name (e.g., "standard", "research").
    pub convergence_profile: &'a str,
}

/// The core Skill trait.
///
/// All skills implement this trait. Skills are invoked synchronously
/// within the database lock scope — the API handler acquires the lock,
/// creates the context, calls `execute`, and releases the lock.
///
/// For skills that need async I/O (network, LLM queries), the handler
/// should pre-fetch data and pass it through the input JSON.
pub trait Skill: Send + Sync {
    /// Unique, stable name of this skill (e.g., "convergence_check").
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Whether this skill can be uninstalled by the user.
    /// Platform-managed skills (Phase 5 safety) return `false`.
    fn removable(&self) -> bool;

    /// Source classification (Bundled, User, Workspace).
    fn source(&self) -> SkillSource;

    /// Execute the skill with the given context and input parameters.
    ///
    /// # Arguments
    /// * `ctx` — Execution context with DB access and agent identity.
    /// * `input` — Skill-specific input parameters as JSON.
    ///
    /// # Returns
    /// A JSON value on success, or a `SkillError` on failure.
    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult;

    /// Preview what the skill would do, for confirmation dialogs.
    /// Returns a human-readable description of the proposed action,
    /// or `None` if preview is not supported.
    fn preview(&self, _input: &serde_json::Value) -> Option<String> {
        None
    }

    /// JSON Schema describing the skill's input parameters.
    /// Used to generate LLM tool schemas when this skill is exposed
    /// as a callable tool. The default returns a permissive schema;
    /// override for better LLM guidance on required fields.
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": true
        })
    }
}

/// Metadata for a registered skill, combining static trait info
/// with runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub removable: bool,
    pub source: String,
}
