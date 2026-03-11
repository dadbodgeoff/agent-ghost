//! Phase 9: PC Control Skills.
//!
//! Cross-platform mouse, keyboard, screenshot, OCR, accessibility tree,
//! window management, and clipboard skills. This is the highest-risk
//! skill category — every action touches the real world.
//!
//! ## Safety Architecture (Defense-in-Depth)
//!
//! 1. **Convergence Gate** — `ConvergenceGuard` disables skills when
//!    convergence level exceeds the per-skill maximum.
//! 2. **App Allowlist** — only explicitly allowed applications can be
//!    targeted by PC control actions.
//! 3. **Screen Safe Zone** — mouse coordinates validated against a
//!    rectangular boundary before dispatch.
//! 4. **Action Budget** — per-session caps on each action type.
//! 5. **Blocked Hotkeys** — dangerous key combos (Ctrl+Alt+Delete,
//!    Cmd+Q, Alt+F4) are always rejected.
//! 6. **Circuit Breaker** — rate limiter + failure counter that halts
//!    all PC control when thresholds are exceeded.
//! 7. **Audit Trail** — every action (executed or blocked) is logged
//!    to the `pc_control_actions` table.
//! 8. **Kill Switch** — `pc_control.enabled = false` (the default)
//!    keeps PC control skills runtime-blocked until explicitly enabled.
//!
//! ## Crate Layout
//!
//! - `safety/` — PcControlConfig, InputValidator, PcControlCircuitBreaker
//! - `platform/` — InputBackend, WindowBackend, AccessibilityBackend, OcrBackend
//! - `input/` — mouse_move, mouse_click, mouse_drag, keyboard_type,
//!   keyboard_hotkey, keyboard_press, scroll
//! - `perception/` — screenshot, accessibility_tree, ocr_extract, element types
//! - `window/` — list_windows, focus_window, resize_window, launch_app,
//!   kill_process, list_processes
//! - `clipboard/` — clipboard_read, clipboard_write
//! - `audit` — log_pc_action, log_blocked_action

pub mod audit;
pub mod clipboard;
pub mod input;
pub mod perception;
pub mod platform;
pub mod safety;
pub mod window;

use std::sync::{Arc, Mutex};

use ghost_skills::autonomy::AutonomyLevel;
use ghost_skills::convergence_guard::{ConvergenceGuard, GuardConfig};
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use platform::accessibility_backend::AccessibilityBackend;
use platform::input_backend::{EnigoBackend, InputBackend};
use platform::ocr_backend::OcrBackend;
use platform::window_backend::WindowBackend;
use safety::config::PcControlConfig;
use safety::{InputValidator, PcControlPolicyHandle, PcControlPolicySnapshot};

/// Returns all Phase 9 PC control skills as boxed trait objects.
///
/// This legacy helper only returns skills when `config.enabled` is `true`.
/// The gateway runtime uses `all_pc_control_skills_with_runtime(...)` so the
/// catalog stays stable across enable/disable transitions.
///
/// Each skill is wrapped with `ConvergenceGuard` for convergence-aware
/// safety gating. Input skills additionally carry their own `InputValidator`
/// and `PcControlCircuitBreaker` for defense-in-depth.
///
/// # Arguments
///
/// * `config` — The `pc_control` section from `ghost.yml`.
pub fn all_pc_control_skills(config: &PcControlConfig) -> Vec<Box<dyn Skill>> {
    if !config.enabled {
        tracing::info!("PC control disabled — no skills registered");
        return Vec::new();
    }

    all_pc_control_skills_with_runtime(
        config,
        PcControlPolicyHandle::new(PcControlPolicySnapshot::from_config(config)),
        config.circuit_breaker(),
    )
}

#[allow(clippy::vec_init_then_push)]
pub fn all_pc_control_skills_with_circuit_breaker(
    config: &PcControlConfig,
    circuit_breaker: Arc<Mutex<safety::circuit_breaker::PcControlCircuitBreaker>>,
) -> Vec<Box<dyn Skill>> {
    if !config.enabled {
        tracing::info!("PC control disabled — no skills registered");
        return Vec::new();
    }

    all_pc_control_skills_with_runtime(
        config,
        PcControlPolicyHandle::new(PcControlPolicySnapshot::from_config(config)),
        circuit_breaker,
    )
}

#[allow(clippy::vec_init_then_push)]
pub fn all_pc_control_skills_with_runtime(
    config: &PcControlConfig,
    policy_handle: PcControlPolicyHandle,
    circuit_breaker: Arc<Mutex<safety::circuit_breaker::PcControlCircuitBreaker>>,
) -> Vec<Box<dyn Skill>> {
    let validator = Arc::new(InputValidator::from_runtime_policy(policy_handle.clone()));

    // Build the input backend. Falls back to a no-op if no display server.
    let backend: Arc<Mutex<dyn InputBackend>> = match EnigoBackend::try_new() {
        Ok(b) => {
            tracing::info!("PC control: enigo backend initialized");
            Arc::new(Mutex::new(b))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "PC control: enigo backend unavailable, input skills will fail"
            );
            Arc::new(Mutex::new(platform::input_backend::MockInputBackend::new()))
        }
    };

    // Build the window backend. Platform-specific with mock fallback.
    let window_backend: Arc<dyn WindowBackend> = build_window_backend();

    // Build the accessibility backend. Platform-specific with stub fallback.
    let accessibility_backend: Arc<dyn AccessibilityBackend> = build_accessibility_backend();

    // Build the OCR backend. Platform-specific with stub fallback.
    let ocr_backend: Arc<dyn OcrBackend> = build_ocr_backend();

    // Build a screen capture backend for OCR to use.
    let ocr_screen_capture: Box<dyn perception::screenshot::ScreenCaptureBackend> =
        match perception::screenshot::XcapScreenCapture::try_new() {
            Ok(b) => Box::new(b),
            Err(_) => Box::new(perception::screenshot::MockScreenCapture::new(1920, 1080)),
        };

    // Construct allowed apps vec for ConvergenceGuard app_allowlist.
    let app_allowlist = if config.allowed_apps.is_empty() {
        None
    } else {
        Some(config.allowed_apps.clone())
    };

    let mut skills: Vec<Box<dyn Skill>> = Vec::new();

    // ── Input skills (Medium risk, Level 2, ActWithConfirmation) ─────

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            input::mouse_move::MouseMoveSkill::new(
                validator.clone(),
                circuit_breaker.clone(),
                backend.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(config.budgets.total),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            input::mouse_click::MouseClickSkill::new(
                validator.clone(),
                circuit_breaker.clone(),
                backend.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(config.budgets.mouse_click),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            input::mouse_drag::MouseDragSkill::new(
                validator.clone(),
                circuit_breaker.clone(),
                backend.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(config.budgets.mouse_drag),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            input::keyboard_type::KeyboardTypeSkill::new(
                validator.clone(),
                circuit_breaker.clone(),
                backend.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(config.budgets.keyboard_type),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            input::keyboard_hotkey::KeyboardHotkeySkill::new(
                validator.clone(),
                circuit_breaker.clone(),
                backend.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(config.budgets.keyboard_hotkey),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            input::keyboard_press::KeyboardPressSkill::new(
                validator.clone(),
                circuit_breaker.clone(),
                backend.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(config.budgets.keyboard_type), // same budget as keyboard_type
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            input::scroll::ScrollSkill::new(
                validator.clone(),
                circuit_breaker.clone(),
                backend.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(config.budgets.total),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    // ── Perception skills (Low risk, Level 4, ActAutonomously) ──────

    // Screenshot: try xcap backend, fallback to mock.
    let screenshot_skill =
        perception::screenshot::ScreenshotSkill::try_new_xcap().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "xcap unavailable — screenshot skill will use mock");
            perception::screenshot::ScreenshotSkill::new(Box::new(
                perception::screenshot::MockScreenCapture::new(1920, 1080),
            ))
        });

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            screenshot_skill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                action_budget: None,
                app_allowlist: None,
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            perception::accessibility_tree::AccessibilityTreeSkill::new(accessibility_backend),
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                action_budget: None,
                app_allowlist: None,
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            perception::ocr_extract::OcrExtractSkill::new(ocr_screen_capture, ocr_backend),
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                action_budget: None,
                app_allowlist: None,
            },
        )),
        policy_handle.clone(),
    )));

    // ── Window skills (mixed risk levels) ───────────────────────────

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            window::list_windows::ListWindowsSkill::new(window_backend.clone()),
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                action_budget: None,
                app_allowlist: None,
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            window::focus_window::FocusWindowSkill::new(
                window_backend.clone(),
                validator.clone(),
                circuit_breaker.clone(),
            ),
            GuardConfig {
                max_convergence_level: 3,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: None,
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            window::resize_window::ResizeWindowSkill::new(
                window_backend.clone(),
                validator.clone(),
                circuit_breaker.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: None,
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            window::launch_app::LaunchAppSkill::new(
                window_backend.clone(),
                validator.clone(),
                circuit_breaker.clone(),
            ),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(10),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            window::kill_process::KillProcessSkill::new(validator.clone(), circuit_breaker.clone()),
            GuardConfig {
                max_convergence_level: 1,
                autonomy_level: AutonomyLevel::PlanAndPropose,
                action_budget: Some(5),
                app_allowlist: app_allowlist.clone(),
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            window::list_processes::ListProcessesSkill::new(),
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                action_budget: None,
                app_allowlist: None,
            },
        )),
        policy_handle.clone(),
    )));

    // ── Clipboard skills (Medium risk) ──────────────────────────────

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            clipboard::clipboard_read::ClipboardReadSkill::new(),
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: None,
                app_allowlist: None,
            },
        )),
        policy_handle.clone(),
    )));

    skills.push(Box::new(PcControlEnabledGuard::new(
        Box::new(ConvergenceGuard::new(
            clipboard::clipboard_write::ClipboardWriteSkill::new(circuit_breaker.clone()),
            GuardConfig {
                max_convergence_level: 3,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(100),
                app_allowlist: None,
            },
        )),
        policy_handle.clone(),
    )));

    tracing::info!(
        count = skills.len(),
        allowed_apps = ?config.allowed_apps,
        safe_zone = ?config.safe_zone,
        "PC control: registered {} skills",
        skills.len(),
    );

    skills
}

struct PcControlEnabledGuard {
    inner: Box<dyn Skill>,
    policy_handle: PcControlPolicyHandle,
}

impl PcControlEnabledGuard {
    fn new(inner: Box<dyn Skill>, policy_handle: PcControlPolicyHandle) -> Self {
        Self {
            inner,
            policy_handle,
        }
    }
}

impl Skill for PcControlEnabledGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn removable(&self) -> bool {
        self.inner.removable()
    }

    fn source(&self) -> ghost_skills::registry::SkillSource {
        self.inner.source()
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        if !self.policy_handle.is_enabled() {
            let reason = "pc control disabled by runtime policy".to_string();
            audit::log_blocked_action(
                ctx.db,
                ctx.agent_id,
                ctx.session_id,
                self.inner.name(),
                input,
                &reason,
            );
            return Err(SkillError::PcControlBlocked(reason));
        }
        self.inner.execute(ctx, input)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        self.inner.preview(input)
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }
}

/// Build the window backend for the current platform.
fn build_window_backend() -> Arc<dyn WindowBackend> {
    #[cfg(target_os = "macos")]
    {
        tracing::info!("PC control: using macOS window backend (AppleScript)");
        Arc::new(platform::macos_window_backend::MacOsWindowBackend::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        tracing::warn!("PC control: no window backend available for this platform");
        Arc::new(platform::window_backend::MockWindowBackend::empty())
    }
}

/// Build the accessibility backend for the current platform.
fn build_accessibility_backend() -> Arc<dyn AccessibilityBackend> {
    #[cfg(target_os = "macos")]
    {
        tracing::info!("PC control: using macOS accessibility backend (System Events)");
        Arc::new(platform::macos_accessibility_backend::MacOsAccessibilityBackend::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        tracing::warn!("PC control: no accessibility backend available for this platform");
        Arc::new(platform::accessibility_backend::StubAccessibilityBackend)
    }
}

/// Build the OCR backend for the current platform.
fn build_ocr_backend() -> Arc<dyn OcrBackend> {
    #[cfg(target_os = "macos")]
    {
        tracing::info!("PC control: using macOS OCR backend (Vision framework)");
        Arc::new(platform::macos_ocr_backend::MacOsOcrBackend::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        tracing::warn!("PC control: no OCR backend available for this platform");
        Arc::new(platform::ocr_backend::StubOcrBackend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_returns_no_skills() {
        let config = PcControlConfig::default();
        assert!(!config.enabled);
        let skills = all_pc_control_skills(&config);
        assert!(skills.is_empty());
    }

    #[test]
    fn runtime_registry_keeps_pc_control_skills_available_when_disabled() {
        let config = PcControlConfig::default();
        assert!(!config.enabled);

        let skills = all_pc_control_skills_with_runtime(
            &config,
            PcControlPolicyHandle::new(PcControlPolicySnapshot::from_config(&config)),
            config.circuit_breaker(),
        );

        // The runtime catalog must stay stable so enable/disable flips do not
        // require a skill rebuild or process restart.
        assert_eq!(skills.len(), 18);
    }

    #[test]
    fn enabled_config_returns_all_skills() {
        let config = PcControlConfig {
            enabled: true,
            allowed_apps: vec!["Firefox".into()],
            ..Default::default()
        };
        let skills = all_pc_control_skills(&config);
        // 7 input + 3 perception + 6 window + 2 clipboard = 18 skills
        assert_eq!(skills.len(), 18);
    }

    #[test]
    fn all_skills_have_unique_names() {
        let config = PcControlConfig {
            enabled: true,
            allowed_apps: vec!["TestApp".into()],
            ..Default::default()
        };
        let skills = all_pc_control_skills(&config);
        let mut names: Vec<&str> = skills.iter().map(|s| s.name()).collect();
        let total = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), total, "duplicate skill names found");
    }

    #[test]
    fn all_skills_are_removable() {
        let config = PcControlConfig {
            enabled: true,
            allowed_apps: vec!["TestApp".into()],
            ..Default::default()
        };
        let skills = all_pc_control_skills(&config);
        for skill in &skills {
            assert!(
                skill.removable(),
                "skill '{}' should be removable",
                skill.name()
            );
        }
    }

    #[test]
    fn expected_skill_names_present() {
        let config = PcControlConfig {
            enabled: true,
            allowed_apps: vec!["TestApp".into()],
            ..Default::default()
        };
        let skills = all_pc_control_skills(&config);
        let names: Vec<&str> = skills.iter().map(|s| s.name()).collect();

        let expected = [
            "mouse_move",
            "mouse_click",
            "mouse_drag",
            "keyboard_type",
            "keyboard_hotkey",
            "keyboard_press",
            "scroll",
            "screenshot",
            "accessibility_tree",
            "ocr_extract",
            "list_windows",
            "focus_window",
            "resize_window",
            "launch_app",
            "kill_process",
            "list_processes",
            "clipboard_read",
            "clipboard_write",
        ];

        for name in &expected {
            assert!(names.contains(name), "missing skill: {name}");
        }
    }
}
