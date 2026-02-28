//! Gateway configuration loaded from ghost.yml (Req 15 AC2, Req 31).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
    #[error("config parse error: {0}")]
    ParseError(String),
    #[error("validation error: {0}")]
    ValidationError(String),
    #[error("env var not found: {0}")]
    EnvVarNotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Top-level ghost.yml configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostConfig {
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
    #[serde(default)]
    pub channels: Vec<ChannelConfig>,
    #[serde(default)]
    pub convergence: ConvergenceGatewayConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub models: ModelsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_db_path")]
    pub db_path: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            db_path: default_db_path(),
        }
    }
}

fn default_bind() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 18789 }
fn default_db_path() -> String { "~/.ghost/data/ghost.db".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    #[serde(default = "default_spending_cap")]
    pub spending_cap: f64,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub isolation: IsolationMode,
    #[serde(default)]
    pub template: Option<String>,
}

fn default_spending_cap() -> f64 { 5.0 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IsolationMode {
    #[default]
    InProcess,
    Process,
    Container,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub channel_type: String,
    pub agent: String,
    #[serde(default)]
    pub options: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConvergenceGatewayConfig {
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default)]
    pub contacts: Vec<ContactConfig>,
}

fn default_profile() -> String { "standard".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(default = "default_monitor_address")]
    pub address: String,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self { address: default_monitor_address() }
    }
}

fn default_monitor_address() -> String { "127.0.0.1:18790".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactConfig {
    pub contact_type: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default = "default_soul_drift_threshold")]
    pub soul_drift_threshold: f64,
}

fn default_soul_drift_threshold() -> f64 { 0.15 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsConfig {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

impl GhostConfig {
    /// Load configuration from a file path, with env var substitution.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotFound(path.to_path_buf()));
        }
        let raw = std::fs::read_to_string(path)?;
        let substituted = substitute_env_vars(&raw)?;
        let config: GhostConfig =
            serde_yaml::from_str(&substituted).map_err(|e| ConfigError::ParseError(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Load from default locations: CLI arg > env > ~/.ghost/config/ghost.yml > ./ghost.yml
    pub fn load_default(cli_path: Option<&str>) -> Result<Self, ConfigError> {
        if let Some(p) = cli_path {
            return Self::load(Path::new(p));
        }
        if let Ok(p) = std::env::var("GHOST_CONFIG") {
            return Self::load(Path::new(&p));
        }
        let home_config = dirs_path("~/.ghost/config/ghost.yml");
        if home_config.exists() {
            return Self::load(&home_config);
        }
        let local = Path::new("ghost.yml");
        if local.exists() {
            return Self::load(local);
        }
        // Return default config if no file found
        Ok(GhostConfig::default())
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for agent in &self.agents {
            if agent.name.is_empty() {
                return Err(ConfigError::ValidationError(
                    "Agent name cannot be empty".into(),
                ));
            }
            if agent.spending_cap < 0.0 {
                return Err(ConfigError::ValidationError(format!(
                    "Agent '{}' has negative spending cap",
                    agent.name
                )));
            }
        }
        Ok(())
    }
}

impl Default for GhostConfig {
    fn default() -> Self {
        Self {
            gateway: GatewayConfig::default(),
            agents: Vec::new(),
            channels: Vec::new(),
            convergence: ConvergenceGatewayConfig::default(),
            security: SecurityConfig::default(),
            models: ModelsConfig::default(),
        }
    }
}

/// Substitute ${VAR} patterns with environment variable values.
fn substitute_env_vars(input: &str) -> Result<String, ConfigError> {
    let mut result = input.to_string();
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        let value = std::env::var(var_name)
            .map_err(|_| ConfigError::EnvVarNotFound(var_name.to_string()))?;
        result = result.replace(&cap[0], &value);
    }
    Ok(result)
}

fn dirs_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs_home() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
