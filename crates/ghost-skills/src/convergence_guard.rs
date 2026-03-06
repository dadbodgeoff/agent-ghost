//! ConvergenceGuard — decorator that wraps any `Skill` with
//! convergence-aware safety checks.
//!
//! Every skill that touches the real world gets wrapped. The guard
//! enforces:
//!
//! 1. **Convergence level gate** — skill disabled above a threshold
//! 2. **Action budget** — maximum actions per session
//! 3. **App allowlist** — restrict which apps the skill can target
//! 4. **Autonomy downshift** — convergence constrains autonomy level
//! 5. **Audit logging** — every execution is logged
//!
//! Phase 5 safety skills are themselves unguarded (they ARE the
//! safety infrastructure). The guard is applied to Phase 7+ skills.

use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::autonomy::AutonomyLevel;
use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Configuration for a ConvergenceGuard instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardConfig {
    /// Skill is disabled when convergence level exceeds this value.
    /// Level 0-4, where 4 is the most restrictive.
    pub max_convergence_level: u8,

    /// Maximum actions per session (None = unlimited).
    pub action_budget: Option<u32>,

    /// Application allowlist for targeted skills (None = no restriction).
    pub app_allowlist: Option<Vec<String>>,

    /// User-configured autonomy level (ceiling; convergence can lower it).
    pub autonomy_level: AutonomyLevel,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            max_convergence_level: 2,
            action_budget: None,
            app_allowlist: None,
            autonomy_level: AutonomyLevel::ActWithConfirmation,
        }
    }
}

/// ConvergenceGuard wraps any `Skill` with convergence-aware safety.
///
/// Implements `Skill` itself so it can be used wherever a Skill is
/// expected (decorator pattern).
pub struct ConvergenceGuard<S: Skill> {
    inner: S,
    config: GuardConfig,
    /// Per-session action counter. Reset when the session changes.
    action_count: AtomicU32,
    /// Session ID that the counter tracks. If the session changes,
    /// the counter resets.
    tracked_session: std::sync::Mutex<Uuid>,
}

impl<S: Skill> ConvergenceGuard<S> {
    /// Wrap a skill with convergence guard using the given configuration.
    pub fn new(inner: S, config: GuardConfig) -> Self {
        Self {
            inner,
            config,
            action_count: AtomicU32::new(0),
            tracked_session: std::sync::Mutex::new(Uuid::nil()),
        }
    }

    /// Builder: set maximum convergence level.
    pub fn with_max_convergence_level(mut self, level: u8) -> Self {
        self.config.max_convergence_level = level;
        self
    }

    /// Builder: set action budget.
    pub fn with_action_budget(mut self, budget: u32) -> Self {
        self.config.action_budget = Some(budget);
        self
    }

    /// Builder: set app allowlist.
    pub fn with_app_allowlist(mut self, apps: Vec<String>) -> Self {
        self.config.app_allowlist = Some(apps);
        self
    }

    /// Builder: set autonomy level.
    pub fn with_autonomy_level(mut self, level: AutonomyLevel) -> Self {
        self.config.autonomy_level = level;
        self
    }

    /// Current action count for this session.
    pub fn actions_used(&self) -> u32 {
        self.action_count.load(Ordering::Relaxed)
    }

    /// Reference to the inner skill.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// Reference to the guard configuration.
    pub fn config(&self) -> &GuardConfig {
        &self.config
    }

    /// Reset the action counter (e.g., on session change).
    fn reset_if_new_session(&self, session_id: Uuid) {
        if let Ok(mut tracked) = self.tracked_session.lock() {
            if *tracked != session_id {
                *tracked = session_id;
                self.action_count.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Fetch the current convergence level from the database.
    fn current_convergence_level(ctx: &SkillContext<'_>) -> u8 {
        let agent_id_str = ctx.agent_id.to_string();
        cortex_storage::queries::convergence_score_queries::latest_by_agent(
            ctx.db,
            &agent_id_str,
        )
        .ok()
        .flatten()
        .map(|row| row.level as u8)
        .unwrap_or(0)
    }

    /// Fetch the current convergence score from the database.
    fn current_convergence_score(ctx: &SkillContext<'_>) -> f64 {
        let agent_id_str = ctx.agent_id.to_string();
        cortex_storage::queries::convergence_score_queries::latest_by_agent(
            ctx.db,
            &agent_id_str,
        )
        .ok()
        .flatten()
        .map(|row| row.composite_score)
        .unwrap_or(0.0)
    }
}

impl<S: Skill> Skill for ConvergenceGuard<S> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn removable(&self) -> bool {
        self.inner.removable()
    }

    fn source(&self) -> SkillSource {
        self.inner.source()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        // 1. Check convergence level
        let level = Self::current_convergence_level(ctx);
        if level > self.config.max_convergence_level {
            tracing::warn!(
                skill = self.inner.name(),
                current_level = level,
                max_level = self.config.max_convergence_level,
                agent_id = %ctx.agent_id,
                "Skill blocked by convergence level"
            );
            return Err(SkillError::ConvergenceTooHigh {
                current: level,
                maximum: self.config.max_convergence_level,
            });
        }

        // 2. Check action budget
        self.reset_if_new_session(ctx.session_id);
        if let Some(budget) = self.config.action_budget {
            let used = self.action_count.load(Ordering::Relaxed);
            if used >= budget {
                tracing::warn!(
                    skill = self.inner.name(),
                    used = used,
                    budget = budget,
                    agent_id = %ctx.agent_id,
                    "Skill blocked by action budget"
                );
                return Err(SkillError::BudgetExhausted { used, budget });
            }
        }

        // 3. Check app allowlist (if the input specifies a target app)
        if let Some(ref allowed) = self.config.app_allowlist {
            if let Some(target_app) = input.get("target_app").and_then(|v| v.as_str()) {
                if !allowed.iter().any(|a| a == target_app) {
                    tracing::warn!(
                        skill = self.inner.name(),
                        target_app = target_app,
                        agent_id = %ctx.agent_id,
                        "Skill blocked by app allowlist"
                    );
                    return Err(SkillError::AppNotAllowed {
                        app: target_app.to_string(),
                        allowed: allowed.clone(),
                    });
                }
            }
        }

        // 4. Check effective autonomy level
        let score = Self::current_convergence_score(ctx);
        let effective = self.config.autonomy_level.effective(score);
        if !effective.can_execute() {
            tracing::info!(
                skill = self.inner.name(),
                effective_autonomy = %effective,
                convergence_score = score,
                agent_id = %ctx.agent_id,
                "Skill in observe-only mode — returning preview"
            );
            let preview = self.inner.preview(input).unwrap_or_else(|| {
                format!("Skill '{}' would execute with input: {}", self.inner.name(), input)
            });
            return Ok(serde_json::json!({
                "mode": "observe_only",
                "preview": preview,
                "effective_autonomy": effective.as_u8(),
                "convergence_score": score,
            }));
        }

        // 5. Execute the inner skill
        let result = self.inner.execute(ctx, input)?;

        // 6. Increment action counter
        self.action_count.fetch_add(1, Ordering::Relaxed);

        // 7. Log to audit trail
        tracing::info!(
            skill = self.inner.name(),
            agent_id = %ctx.agent_id,
            session_id = %ctx.session_id,
            convergence_level = level,
            convergence_score = score,
            actions_used = self.action_count.load(Ordering::Relaxed),
            "Skill executed successfully"
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        self.inner.preview(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillContext;

    /// Minimal test skill that returns its input.
    struct EchoSkill;

    impl Skill for EchoSkill {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "Returns input as output" }
        fn removable(&self) -> bool { true }
        fn source(&self) -> SkillSource { SkillSource::Bundled }

        fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
            Ok(input.clone())
        }
    }

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    #[test]
    fn guard_passes_when_convergence_low() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let guard = ConvergenceGuard::new(
            EchoSkill,
            GuardConfig {
                max_convergence_level: 2,
                ..Default::default()
            },
        );

        let input = serde_json::json!({"hello": "world"});
        let result = guard.execute(&ctx, &input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), input);
    }

    #[test]
    fn guard_blocks_when_convergence_too_high() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        // Insert a high convergence score
        let score_id = Uuid::now_v7().to_string();
        let agent_id_str = agent_id.to_string();
        let session_id_str = session_id.to_string();
        cortex_storage::queries::convergence_score_queries::insert_score(
            &db,
            &score_id,
            &agent_id_str,
            Some(session_id_str.as_str()),
            0.8,
            "{}",
            3,
            "standard",
            "2026-01-01T00:00:00Z",
            &[0u8; 32],
            &[0u8; 32],
        )
        .unwrap();

        let ctx = SkillContext {
            db: &db,
            agent_id,
            session_id,
            convergence_profile: "standard",
        };

        let guard = ConvergenceGuard::new(
            EchoSkill,
            GuardConfig {
                max_convergence_level: 2,
                ..Default::default()
            },
        );

        let result = guard.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::ConvergenceTooHigh { current: 3, maximum: 2 } => {}
            other => panic!("Expected ConvergenceTooHigh, got: {other:?}"),
        }
    }

    #[test]
    fn guard_enforces_action_budget() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let guard = ConvergenceGuard::new(
            EchoSkill,
            GuardConfig {
                action_budget: Some(2),
                ..Default::default()
            },
        );

        let input = serde_json::json!({});

        // First two should succeed
        assert!(guard.execute(&ctx, &input).is_ok());
        assert!(guard.execute(&ctx, &input).is_ok());

        // Third should fail
        let result = guard.execute(&ctx, &input);
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::BudgetExhausted { used: 2, budget: 2 } => {}
            other => panic!("Expected BudgetExhausted, got: {other:?}"),
        }
    }

    #[test]
    fn guard_resets_budget_on_new_session() {
        let db = test_db();
        let session1 = Uuid::now_v7();
        let session2 = Uuid::now_v7();
        let agent_id = Uuid::nil();

        let guard = ConvergenceGuard::new(
            EchoSkill,
            GuardConfig {
                action_budget: Some(1),
                ..Default::default()
            },
        );

        let input = serde_json::json!({});

        // Session 1: use the budget
        let ctx1 = SkillContext {
            db: &db, agent_id, session_id: session1,
            convergence_profile: "standard",
        };
        assert!(guard.execute(&ctx1, &input).is_ok());
        assert!(guard.execute(&ctx1, &input).is_err());

        // Session 2: budget resets
        let ctx2 = SkillContext {
            db: &db, agent_id, session_id: session2,
            convergence_profile: "standard",
        };
        assert!(guard.execute(&ctx2, &input).is_ok());
    }

    #[test]
    fn guard_enforces_app_allowlist() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let guard = ConvergenceGuard::new(
            EchoSkill,
            GuardConfig {
                app_allowlist: Some(vec!["Firefox".into(), "VS Code".into()]),
                ..Default::default()
            },
        );

        // Allowed app
        let input = serde_json::json!({"target_app": "Firefox"});
        assert!(guard.execute(&ctx, &input).is_ok());

        // Disallowed app
        let input = serde_json::json!({"target_app": "Terminal"});
        let result = guard.execute(&ctx, &input);
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::AppNotAllowed { ref app, .. } if app == "Terminal" => {}
            other => panic!("Expected AppNotAllowed, got: {other:?}"),
        }

        // No target_app in input — no restriction
        let input = serde_json::json!({"data": 42});
        assert!(guard.execute(&ctx, &input).is_ok());
    }
}
