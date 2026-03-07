//! PC control configuration, deserializable from the `pc_control`
//! section of `ghost.yml`.
//!
//! All fields have safe defaults: disabled, empty allowlist, standard
//! budgets, and a set of always-blocked dangerous hotkeys.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::circuit_breaker::PcControlCircuitBreaker;
use super::input_validator::{InputValidator, ScreenRegion};

/// Top-level PC control configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcControlConfig {
    /// Master switch — must be explicitly set to `true` to enable PC control.
    #[serde(default)]
    pub enabled: bool,

    /// Application allowlist. Only these apps can be targeted by PC control
    /// skills. Default: empty (no apps allowed).
    #[serde(default)]
    pub allowed_apps: Vec<String>,

    /// Optional screen safe zone. If set, all mouse actions are validated
    /// against this rectangular region.
    #[serde(default)]
    pub safe_zone: Option<ScreenRegion>,

    /// Per-action-type session budgets.
    #[serde(default)]
    pub budgets: ActionBudgets,

    /// Hotkeys that are always blocked, regardless of other settings.
    #[serde(default = "default_blocked_hotkeys")]
    pub blocked_hotkeys: Vec<String>,

    /// Perception stack preferences.
    #[serde(default)]
    pub perception: PerceptionConfig,

    /// Circuit breaker tuning.
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
}

impl Default for PcControlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_apps: Vec::new(),
            safe_zone: None,
            budgets: ActionBudgets::default(),
            blocked_hotkeys: default_blocked_hotkeys(),
            perception: PerceptionConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

impl PcControlConfig {
    /// Build an `InputValidator` from this configuration.
    pub fn input_validator(&self) -> Arc<InputValidator> {
        Arc::new(InputValidator::new(
            self.allowed_apps.clone(),
            self.safe_zone.clone(),
            self.blocked_hotkeys.clone(),
        ))
    }

    /// Build a `PcControlCircuitBreaker` from this configuration.
    pub fn circuit_breaker(&self) -> Arc<std::sync::Mutex<PcControlCircuitBreaker>> {
        Arc::new(std::sync::Mutex::new(PcControlCircuitBreaker::new(
            self.circuit_breaker.max_actions_per_second,
            self.circuit_breaker.failure_threshold,
            std::time::Duration::from_secs(self.circuit_breaker.cooldown_seconds),
        )))
    }
}

/// Per-action-type session budgets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionBudgets {
    pub mouse_click: u32,
    pub keyboard_type: u32,
    pub keyboard_hotkey: u32,
    pub mouse_drag: u32,
    pub total: u32,
}

impl Default for ActionBudgets {
    fn default() -> Self {
        Self {
            mouse_click: 200,
            keyboard_type: 500,
            keyboard_hotkey: 50,
            mouse_drag: 20,
            total: 1000,
        }
    }
}

/// Perception stack preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptionConfig {
    /// Try accessibility tree first (fastest, most semantic).
    #[serde(default = "default_true")]
    pub prefer_accessibility_tree: bool,

    /// Enable OCR fallback layer.
    #[serde(default = "default_true")]
    pub ocr_enabled: bool,

    /// Enable vision model fallback layer (requires LLM provider).
    #[serde(default = "default_true")]
    pub vision_model_enabled: bool,

    /// Which model to use for vision queries.
    #[serde(default = "default_vision_model")]
    pub vision_model: String,
}

impl Default for PerceptionConfig {
    fn default() -> Self {
        Self {
            prefer_accessibility_tree: true,
            ocr_enabled: true,
            vision_model_enabled: true,
            vision_model: default_vision_model(),
        }
    }
}

/// Circuit breaker tuning parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Maximum actions per second before tripping.
    #[serde(default = "default_max_actions_per_second")]
    pub max_actions_per_second: u32,

    /// Consecutive failures before tripping.
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Cooldown period in seconds after tripping.
    #[serde(default = "default_cooldown_seconds")]
    pub cooldown_seconds: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_actions_per_second: default_max_actions_per_second(),
            failure_threshold: default_failure_threshold(),
            cooldown_seconds: default_cooldown_seconds(),
        }
    }
}

fn default_blocked_hotkeys() -> Vec<String> {
    vec![
        "Ctrl+Alt+Delete".into(),
        "Cmd+Q".into(),
        "Alt+F4".into(),
        "Ctrl+Shift+Delete".into(),
        "Cmd+Shift+Q".into(),
    ]
}

fn default_true() -> bool {
    true
}

fn default_vision_model() -> String {
    "claude-sonnet".into()
}

fn default_max_actions_per_second() -> u32 {
    5
}

fn default_failure_threshold() -> u32 {
    3
}

fn default_cooldown_seconds() -> u64 {
    30
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let config = PcControlConfig::default();
        assert!(!config.enabled);
        assert!(config.allowed_apps.is_empty());
        assert!(config.safe_zone.is_none());
    }

    #[test]
    fn default_budgets() {
        let budgets = ActionBudgets::default();
        assert_eq!(budgets.mouse_click, 200);
        assert_eq!(budgets.keyboard_type, 500);
        assert_eq!(budgets.keyboard_hotkey, 50);
        assert_eq!(budgets.mouse_drag, 20);
        assert_eq!(budgets.total, 1000);
    }

    #[test]
    fn default_blocked_hotkeys_populated() {
        let config = PcControlConfig::default();
        assert!(config
            .blocked_hotkeys
            .contains(&"Ctrl+Alt+Delete".to_string()));
        assert!(config.blocked_hotkeys.contains(&"Cmd+Q".to_string()));
        assert!(config.blocked_hotkeys.contains(&"Alt+F4".to_string()));
    }

    #[test]
    fn deserialize_minimal_yaml() {
        let yaml = r#"
enabled: true
allowed_apps:
  - Firefox
  - VS Code
"#;
        let config: PcControlConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.allowed_apps.len(), 2);
        assert_eq!(config.budgets.mouse_click, 200);
        assert!(config.blocked_hotkeys.contains(&"Alt+F4".to_string()));
    }

    #[test]
    fn deserialize_full_yaml() {
        let yaml = r#"
enabled: true
allowed_apps: ["Firefox"]
safe_zone:
  x: 0
  y: 0
  width: 1920
  height: 1080
budgets:
  mouse_click: 100
  keyboard_type: 250
  keyboard_hotkey: 25
  mouse_drag: 10
  total: 500
blocked_hotkeys:
  - "Ctrl+Alt+Delete"
circuit_breaker:
  max_actions_per_second: 10
  failure_threshold: 5
  cooldown_seconds: 60
"#;
        let config: PcControlConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.budgets.mouse_click, 100);
        assert_eq!(config.circuit_breaker.max_actions_per_second, 10);
        assert!(config.safe_zone.is_some());
        let zone = config.safe_zone.unwrap();
        assert_eq!(zone.width, 1920);
    }
}
