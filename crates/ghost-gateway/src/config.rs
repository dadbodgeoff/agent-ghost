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
    #[serde(default)]
    pub secrets: SecretsConfig,
    /// Mesh networking configuration (Task 22.1). Disabled by default.
    #[serde(default)]
    pub mesh: MeshConfig,
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
    /// Per-agent network egress control (Phase 11).
    #[serde(default)]
    pub network: Option<NetworkEgressGatewayConfig>,
}

fn default_spending_cap() -> f64 { 5.0 }

/// Per-agent network egress configuration in ghost.yml (Phase 11).
///
/// Maps to `ghost_egress::AgentEgressConfig` at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEgressGatewayConfig {
    /// Policy mode: "allowlist", "blocklist", or "unrestricted".
    #[serde(default = "default_egress_policy")]
    pub egress_policy: String,
    /// Domains allowed when policy is allowlist. Supports wildcards: `*.slack.com`.
    #[serde(default = "default_egress_allowed_domains")]
    pub allowed_domains: Vec<String>,
    /// Domains blocked when policy is blocklist.
    #[serde(default)]
    pub blocked_domains: Vec<String>,
    /// Whether to log violation events.
    #[serde(default = "default_true_config")]
    pub log_violations: bool,
    /// Whether to emit a TriggerEvent on violation.
    #[serde(default)]
    pub alert_on_violation: bool,
    /// Number of violations in window before QUARANTINE.
    #[serde(default = "default_egress_violation_threshold")]
    pub violation_threshold: u32,
    /// Time window (minutes) for violation counting.
    #[serde(default = "default_egress_violation_window")]
    pub violation_window_minutes: u32,
}

impl Default for NetworkEgressGatewayConfig {
    fn default() -> Self {
        Self {
            egress_policy: default_egress_policy(),
            allowed_domains: default_egress_allowed_domains(),
            blocked_domains: Vec::new(),
            log_violations: true,
            alert_on_violation: false,
            violation_threshold: default_egress_violation_threshold(),
            violation_window_minutes: default_egress_violation_window(),
        }
    }
}

fn default_egress_policy() -> String { "unrestricted".into() }

fn default_egress_allowed_domains() -> Vec<String> {
    ghost_egress::config::DEFAULT_ALLOWED_DOMAINS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_true_config() -> bool { true }

fn default_egress_violation_threshold() -> u32 { 5 }

fn default_egress_violation_window() -> u32 { 10 }

/// Convert gateway config to ghost-egress config.
pub fn build_egress_config(
    network: &NetworkEgressGatewayConfig,
) -> ghost_egress::AgentEgressConfig {
    let policy = match network.egress_policy.as_str() {
        "allowlist" => ghost_egress::EgressPolicyMode::Allowlist,
        "blocklist" => ghost_egress::EgressPolicyMode::Blocklist,
        _ => ghost_egress::EgressPolicyMode::Unrestricted,
    };

    ghost_egress::AgentEgressConfig {
        policy,
        allowed_domains: network.allowed_domains.clone(),
        blocked_domains: network.blocked_domains.clone(),
        log_violations: network.log_violations,
        alert_on_violation: network.alert_on_violation,
        violation_threshold: network.violation_threshold,
        violation_window_minutes: network.violation_window_minutes,
    }
}

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

/// Secrets infrastructure configuration (Phase 10).
///
/// Defaults to `env` provider if not specified (backward compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Provider backend: "env", "keychain", or "vault".
    #[serde(default = "default_secrets_provider")]
    pub provider: String,
    /// Keychain-specific settings.
    #[serde(default)]
    pub keychain: Option<KeychainSecretsConfig>,
    /// Vault-specific settings.
    #[serde(default)]
    pub vault: Option<VaultSecretsConfig>,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            provider: default_secrets_provider(),
            keychain: None,
            vault: None,
        }
    }
}

fn default_secrets_provider() -> String {
    "env".into()
}

/// Keychain provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeychainSecretsConfig {
    #[serde(default = "default_keychain_service")]
    pub service_name: String,
}

fn default_keychain_service() -> String {
    "ghost-platform".into()
}

/// Vault provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSecretsConfig {
    pub endpoint: String,
    #[serde(default = "default_vault_mount")]
    pub mount: String,
    /// Env var name containing the Vault token (bootstrap problem).
    #[serde(default = "default_vault_token_env")]
    pub token_env: String,
}

fn default_vault_mount() -> String {
    "secret".into()
}

fn default_vault_token_env() -> String {
    "VAULT_TOKEN".into()
}

/// Mesh networking configuration (Task 22.1).
/// Disabled by default — opt-in via `mesh.enabled: true` in ghost.yml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// Whether mesh networking is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Known agents for discovery and delegation.
    #[serde(default)]
    pub known_agents: Vec<KnownAgent>,
    /// Minimum trust score required for delegation (default 0.3).
    #[serde(default = "default_min_trust")]
    pub min_trust_for_delegation: f64,
    /// Maximum delegation chain depth (default 3).
    #[serde(default = "default_max_delegation_depth")]
    pub max_delegation_depth: u32,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            known_agents: Vec::new(),
            min_trust_for_delegation: default_min_trust(),
            max_delegation_depth: default_max_delegation_depth(),
        }
    }
}

fn default_min_trust() -> f64 { 0.3 }
fn default_max_delegation_depth() -> u32 { 3 }

/// A known agent for mesh discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownAgent {
    pub name: String,
    pub endpoint: String,
    /// Base64-encoded Ed25519 public key.
    pub public_key: String,
}

/// Build a `SecretProvider` from the parsed `SecretsConfig`.
pub fn build_secret_provider(
    config: &SecretsConfig,
) -> Result<Box<dyn ghost_secrets::SecretProvider>, ConfigError> {
    match config.provider.as_str() {
        "env" => Ok(Box::new(ghost_secrets::EnvProvider)),
        #[cfg(feature = "keychain")]
        "keychain" => {
            let service = config
                .keychain
                .as_ref()
                .map(|k| k.service_name.as_str())
                .unwrap_or("ghost-platform");
            Ok(Box::new(ghost_secrets::KeychainProvider::new(service)))
        }
        #[cfg(not(feature = "keychain"))]
        "keychain" => Err(ConfigError::ValidationError(
            "keychain provider requested but 'keychain' feature is not enabled".into(),
        )),
        "vault" => {
            #[cfg(feature = "vault")]
            {
                let vault_cfg = config.vault.as_ref().ok_or_else(|| {
                    ConfigError::ValidationError(
                        "secrets.provider is 'vault' but secrets.vault section is missing".into(),
                    )
                })?;
                let token_value = std::env::var(&vault_cfg.token_env).map_err(|_| {
                    ConfigError::EnvVarNotFound(vault_cfg.token_env.clone())
                })?;
                let token = ghost_secrets::SecretString::from(token_value);
                let provider = ghost_secrets::VaultProvider::new(
                    &vault_cfg.endpoint,
                    &vault_cfg.mount,
                    token,
                )
                .map_err(|e| ConfigError::ValidationError(format!("Vault provider init: {e}")))?;
                Ok(Box::new(provider))
            }
            #[cfg(not(feature = "vault"))]
            {
                Err(ConfigError::ValidationError(
                    "vault provider requested but 'vault' feature is not enabled".into(),
                ))
            }
        }
        other => Err(ConfigError::ValidationError(format!(
            "unknown secrets provider: '{other}' (expected 'env', 'keychain', or 'vault')"
        ))),
    }
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
        let mut seen_names = std::collections::BTreeSet::new();
        for agent in &self.agents {
            if agent.name.is_empty() {
                return Err(ConfigError::ValidationError(
                    "Agent name cannot be empty".into(),
                ));
            }
            if !seen_names.insert(&agent.name) {
                return Err(ConfigError::ValidationError(format!(
                    "Duplicate agent name: '{}'",
                    agent.name
                )));
            }
            if agent.spending_cap < 0.0 {
                return Err(ConfigError::ValidationError(format!(
                    "Agent '{}' has negative spending cap",
                    agent.name
                )));
            }
        }
        if self.gateway.db_path.is_empty() {
            return Err(ConfigError::ValidationError(
                "gateway.db_path cannot be empty".into(),
            ));
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
            secrets: SecretsConfig::default(),
            mesh: MeshConfig::default(),
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
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}
