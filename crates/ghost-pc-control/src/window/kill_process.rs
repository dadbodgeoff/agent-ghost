//! `kill_process` — terminate a running process.
//!
//! High risk. Convergence max: Level 1. Budget: 5 per session.
//! App allowlist enforced. Requires Plan and Propose autonomy.
//!
//! ## Input
//!
//! | Field  | Type   | Required | Description                  |
//! |--------|--------|----------|------------------------------|
//! | `pid`  | u32    | no       | Process ID to kill           |
//! | `app`  | string | no       | App name (finds PID)         |
//! | `force`| bool   | no       | Force kill (SIGKILL/TerminateProcess) |
//!
//! At least one of `pid` or `app` must be provided.

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};
use sysinfo::System;

use crate::audit;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct KillProcessSkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
}

impl KillProcessSkill {
    pub fn new(
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    ) -> Self {
        Self {
            validator,
            circuit_breaker,
        }
    }
}

impl Skill for KillProcessSkill {
    fn name(&self) -> &str {
        "kill_process"
    }
    fn description(&self) -> &str {
        "Terminate a running process"
    }
    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let pid = input.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
        let app_name = input.get("app").and_then(|v| v.as_str());
        let force = input
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if pid.is_none() && app_name.is_none() {
            return Err(SkillError::InvalidInput(
                "at least one of 'pid' or 'app' must be provided".into(),
            ));
        }

        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        // Resolve the target PID and process name.
        let (target_pid, resolved_name) = if let Some(p) = pid {
            let name = sys
                .process(sysinfo::Pid::from_u32(p))
                .map(|proc| proc.name().to_string_lossy().to_string())
                .unwrap_or_else(|| format!("PID {p}"));
            (p, name)
        } else {
            let app = app_name.unwrap();
            let proc = sys
                .processes()
                .values()
                .find(|p| p.name().to_string_lossy().to_lowercase() == app.to_lowercase())
                .ok_or_else(|| {
                    SkillError::InvalidInput(format!("no process found matching app name '{app}'"))
                })?;
            (
                proc.pid().as_u32(),
                proc.name().to_string_lossy().to_string(),
            )
        };

        // Validate against app allowlist.
        if let ValidationResult::Denied(reason) = self.validator.validate_app(&resolved_name) {
            audit::log_blocked_action(
                ctx.db,
                ctx.agent_id,
                ctx.session_id,
                "kill_process",
                input,
                &reason,
            );
            return Err(SkillError::PcControlBlocked(reason));
        }

        // Circuit breaker check.
        {
            self.circuit_breaker.lock().unwrap().check("kill_process")?;
        }

        // Send signal via kill command (avoids unsafe libc).
        let signal = if force { "KILL" } else { "TERM" };
        let status = std::process::Command::new("kill")
            .args(["-s", signal, &target_pid.to_string()])
            .status()
            .map_err(|e| {
                self.circuit_breaker.lock().unwrap().record_failure();
                SkillError::Internal(format!("failed to execute kill command: {e}"))
            })?;

        if !status.success() {
            self.circuit_breaker.lock().unwrap().record_failure();
            return Err(SkillError::Internal(format!(
                "kill -{signal} {target_pid} failed with exit code {:?}",
                status.code()
            )));
        }

        // Record success.
        {
            self.circuit_breaker.lock().unwrap().record_success();
        }

        let result = serde_json::json!({
            "pid": target_pid,
            "name": resolved_name,
            "signal": signal,
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "kill_process",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let target = input
            .get("app")
            .and_then(|v| v.as_str())
            .map(|a| a.to_string())
            .or_else(|| {
                input
                    .get("pid")
                    .and_then(|v| v.as_u64())
                    .map(|p| format!("PID {p}"))
            })
            .unwrap_or_else(|| "process".into());
        let force = input
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if force {
            Some(format!("Force kill: {target}"))
        } else {
            Some(format!("Kill: {target}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::input_validator::ScreenRegion;
    use std::time::Duration;
    use uuid::Uuid;

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

    fn test_skill() -> KillProcessSkill {
        let validator = Arc::new(InputValidator::new(
            vec!["Firefox".into(), "TestApp".into()],
            Some(ScreenRegion {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }),
            vec![],
        ));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(
            100,
            10,
            Duration::from_secs(30),
        )));
        KillProcessSkill::new(validator, cb)
    }

    #[test]
    fn rejects_missing_pid_and_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_nonexistent_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(
            &ctx,
            &serde_json::json!({"app": "nonexistent_app_xyz_12345"}),
        );
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn preview_force_kill() {
        let skill = test_skill();
        let preview = skill.preview(&serde_json::json!({"app": "Firefox", "force": true}));
        assert_eq!(preview, Some("Force kill: Firefox".into()));
    }

    #[test]
    fn preview_normal_kill_by_pid() {
        let skill = test_skill();
        let preview = skill.preview(&serde_json::json!({"pid": 1234}));
        assert_eq!(preview, Some("Kill: PID 1234".into()));
    }

    #[test]
    fn skill_metadata() {
        let skill = test_skill();
        assert_eq!(skill.name(), "kill_process");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
