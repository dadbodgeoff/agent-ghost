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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    pub oauth: OAuthConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,
    /// Mesh networking configuration (Task 22.1). Disabled by default.
    #[serde(default)]
    pub mesh: MeshConfig,
    /// PC control configuration (Phase 9). Disabled by default.
    #[serde(default)]
    pub pc_control: ghost_pc_control::safety::PcControlConfig,
    /// Tool configuration — web_search, web_fetch, http_request, shell.
    #[serde(default)]
    pub tools: ToolsConfig,
    /// External skill ingestion, trust roots, and managed artifact storage.
    #[serde(default)]
    pub external_skills: ExternalSkillsConfig,
    /// OpenTelemetry configuration (WP9-A). Only active with `otel` feature.
    #[serde(default)]
    pub otel: OtelConfig,
}

/// OpenTelemetry exporter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelConfig {
    /// Enable OTEL exporter. Requires `otel` feature flag at compile time.
    #[serde(default)]
    pub enabled: bool,
    /// OTLP endpoint (default: http://localhost:4317 for gRPC).
    #[serde(default = "default_otel_endpoint")]
    pub endpoint: String,
    /// Service name reported to the collector.
    #[serde(default = "default_otel_service_name")]
    pub service_name: String,
}

fn default_otel_endpoint() -> String {
    "http://localhost:4317".into()
}

fn default_otel_service_name() -> String {
    "ghost-gateway".into()
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_otel_endpoint(),
            service_name: default_otel_service_name(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default)]
    pub rate_limit_scope: RateLimitScope,
    /// WP7-B: WebSocket broadcast channel capacity.
    #[serde(default = "default_ws_broadcast_capacity")]
    pub ws_broadcast_capacity: usize,
    /// WP7-C: WebSocket event replay buffer size.
    #[serde(default = "default_ws_replay_buffer_size")]
    pub ws_replay_buffer_size: usize,
    /// Require short-lived WebSocket tickets and reject legacy bearer upgrade auth.
    #[serde(default)]
    pub ws_ticket_auth_only: bool,
    /// WP9-D: Session TTL in days. Sessions inactive beyond this are soft-deleted.
    /// Hard-deleted after 2x TTL. Default: 90 days.
    #[serde(default = "default_session_ttl_days")]
    pub session_ttl_days: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            db_path: default_db_path(),
            rate_limit_scope: RateLimitScope::default(),
            ws_broadcast_capacity: default_ws_broadcast_capacity(),
            ws_replay_buffer_size: default_ws_replay_buffer_size(),
            ws_ticket_auth_only: false,
            session_ttl_days: default_session_ttl_days(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitScope {
    Process,
    #[default]
    Database,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSkillsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rescan_on_boot: bool,
    #[serde(default)]
    pub execution_enabled: bool,
    #[serde(default = "default_external_skill_storage_path")]
    pub managed_storage_path: String,
    #[serde(default)]
    pub approved_roots: Vec<ExternalSkillRootConfig>,
    #[serde(default)]
    pub trusted_signers: Vec<TrustedSkillSignerConfig>,
}

impl Default for ExternalSkillsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rescan_on_boot: false,
            execution_enabled: false,
            managed_storage_path: default_external_skill_storage_path(),
            approved_roots: Vec::new(),
            trusted_signers: Vec::new(),
        }
    }
}

fn default_external_skill_storage_path() -> String {
    "~/.ghost/data/skill-artifacts".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSkillRootConfig {
    pub source: ExternalSkillSourceConfig,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSkillSourceConfig {
    #[default]
    User,
    Workspace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedSkillSignerConfig {
    pub key_id: String,
    pub publisher: String,
    /// Base64-encoded Ed25519 public key bytes.
    pub public_key: String,
    #[serde(default)]
    pub revoked: bool,
}

/// WP9-D: Default session TTL in days.
fn default_session_ttl_days() -> u32 {
    90
}

fn default_bind() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    39780
}
fn default_db_path() -> String {
    "~/.ghost/data/ghost.db".into()
}

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
    /// Per-agent skill allowlist. When None, all registered skills are available.
    /// When Some, acts as an allowlist (safety skills are always included).
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    /// Per-agent network egress control (Phase 11).
    #[serde(default)]
    pub network: Option<NetworkEgressGatewayConfig>,
}

fn default_spending_cap() -> f64 {
    5.0
}

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

fn default_egress_policy() -> String {
    "unrestricted".into()
}

fn default_egress_allowed_domains() -> Vec<String> {
    ghost_egress::config::DEFAULT_ALLOWED_DOMAINS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_true_config() -> bool {
    true
}

fn default_egress_violation_threshold() -> u32 {
    5
}

fn default_egress_violation_window() -> u32 {
    10
}

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

fn default_profile() -> String {
    "standard".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Whether convergence monitor health checking is enabled.
    /// When false, the gateway skips monitor checks and starts as Healthy.
    #[serde(default)]
    pub enabled: bool,
    /// Whether live execution should fail closed when convergence protection
    /// is missing, stale, or corrupted.
    #[serde(default)]
    pub block_on_degraded: bool,
    /// Maximum acceptable age for a convergence state file before it is
    /// surfaced as stale rather than healthy.
    #[serde(default = "default_convergence_state_stale_after_secs")]
    pub stale_after_secs: u64,
    #[serde(default = "default_monitor_address")]
    pub address: String,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            block_on_degraded: false,
            stale_after_secs: default_convergence_state_stale_after_secs(),
            address: default_monitor_address(),
        }
    }
}

fn default_convergence_state_stale_after_secs() -> u64 {
    300
}

fn default_monitor_address() -> String {
    "127.0.0.1:18790".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactConfig {
    pub contact_type: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default = "default_soul_drift_threshold")]
    pub soul_drift_threshold: f64,
    /// WP6-A: Allowed CORS origins. When empty, falls back to env var / dev defaults.
    #[serde(default)]
    pub cors_origins: Vec<String>,
}

/// WP7-B: WebSocket broadcast channel capacity.
fn default_ws_broadcast_capacity() -> usize {
    1024
}
/// WP7-C: WebSocket event replay buffer size.
fn default_ws_replay_buffer_size() -> usize {
    1000
}

fn default_soul_drift_threshold() -> f64 {
    0.15
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsConfig {
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OAuthConfig {
    #[serde(default)]
    pub providers: Vec<OAuthProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    pub name: String,
    pub client_id: String,
    pub client_secret_env: String,
    pub auth_url: String,
    pub token_url: String,
    #[serde(default)]
    pub revoke_url: Option<String>,
}

/// Tool configuration — web_search, web_fetch, http_request, shell.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    #[serde(default)]
    pub web_search: WebSearchToolConfig,
    #[serde(default)]
    pub web_fetch: WebFetchToolConfig,
    #[serde(default)]
    pub http_request: HttpRequestToolConfig,
    #[serde(default)]
    pub shell: ShellToolOverrides,
}

/// Web search tool config (maps to ghost_agent_loop WebSearchConfig).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchToolConfig {
    /// Backend: "searxng", "tavily", or "brave".
    #[serde(default = "default_search_backend")]
    pub backend: String,
    /// SearXNG instance URL.
    #[serde(default = "default_searxng_url")]
    pub searxng_url: String,
    /// Tavily API key (or env var name via ${VAR}).
    #[serde(default)]
    pub tavily_api_key: String,
    /// Brave API key (or env var name via ${VAR}).
    #[serde(default)]
    pub brave_api_key: String,
    /// Max results per query.
    #[serde(default = "default_search_max_results")]
    pub max_results: usize,
}

fn default_search_backend() -> String {
    "searxng".into()
}
fn default_searxng_url() -> String {
    "http://localhost:8888".into()
}
fn default_search_max_results() -> usize {
    5
}

impl Default for WebSearchToolConfig {
    fn default() -> Self {
        Self {
            backend: default_search_backend(),
            searxng_url: default_searxng_url(),
            tavily_api_key: String::new(),
            brave_api_key: String::new(),
            max_results: default_search_max_results(),
        }
    }
}

/// Web fetch tool config (maps to ghost_agent_loop FetchConfig).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchToolConfig {
    #[serde(default)]
    pub allow_http: bool,
    #[serde(default = "default_fetch_max_bytes")]
    pub max_body_bytes: u64,
    #[serde(default = "default_fetch_timeout")]
    pub timeout_secs: u64,
}

fn default_fetch_max_bytes() -> u64 {
    1_048_576
}
fn default_fetch_timeout() -> u64 {
    15
}

impl Default for WebFetchToolConfig {
    fn default() -> Self {
        Self {
            allow_http: false,
            max_body_bytes: default_fetch_max_bytes(),
            timeout_secs: default_fetch_timeout(),
        }
    }
}

/// HTTP request tool config (maps to ghost_agent_loop HttpRequestConfig).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpRequestToolConfig {
    #[serde(default)]
    pub allow_http: bool,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

/// Shell tool overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShellToolOverrides {
    #[serde(default)]
    pub allowed_prefixes: Vec<String>,
    #[serde(default = "default_shell_timeout")]
    pub timeout_secs: u64,
}

fn default_shell_timeout() -> u64 {
    30
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
    /// Distributed kill remains feature-gated for this remediation milestone.
    #[serde(default)]
    pub distributed_kill_enabled: bool,
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
            distributed_kill_enabled: false,
            known_agents: Vec::new(),
            min_trust_for_delegation: default_min_trust(),
            max_delegation_depth: default_max_delegation_depth(),
        }
    }
}

fn default_min_trust() -> f64 {
    0.3
}
fn default_max_delegation_depth() -> u32 {
    3
}

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
                let token_value = std::env::var(&vault_cfg.token_env)
                    .map_err(|_| ConfigError::EnvVarNotFound(vault_cfg.token_env.clone()))?;
                let token = ghost_secrets::SecretString::from(token_value);
                let provider =
                    ghost_secrets::VaultProvider::new(&vault_cfg.endpoint, &vault_cfg.mount, token)
                        .map_err(|e| {
                            ConfigError::ValidationError(format!("Vault provider init: {e}"))
                        })?;
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
    /// Create a minimal test configuration.
    ///
    /// Uses the given port, binds to 127.0.0.1, sets `db_path` to the
    /// provided path (should be a tempfile), and disables the convergence
    /// monitor and mesh networking.
    pub fn test_config(port: u16, db_path: &str) -> Self {
        Self {
            gateway: GatewayConfig {
                port,
                bind: "127.0.0.1".to_string(),
                db_path: db_path.to_string(),
                ..Default::default()
            },
            convergence: ConvergenceGatewayConfig {
                monitor: MonitorConfig {
                    enabled: false,
                    ..Default::default()
                },
                ..Default::default()
            },
            mesh: MeshConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Load configuration from a file path, with env var substitution.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotFound(path.to_path_buf()));
        }
        let raw = std::fs::read_to_string(path)?;
        let substituted = substitute_env_vars(&raw)?;
        let config: GhostConfig = serde_yaml::from_str(&substituted)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Resolve the preferred config path:
    /// CLI arg > env > ~/.ghost/config/ghost.yml > ./ghost.yml > <exe_dir>/ghost.yml.
    ///
    /// If no file exists yet, this returns the home config path as the default
    /// creation target.
    pub fn default_path(cli_path: Option<&str>) -> PathBuf {
        if let Some(p) = cli_path {
            return PathBuf::from(p);
        }
        if let Ok(p) = std::env::var("GHOST_CONFIG") {
            return PathBuf::from(p);
        }

        let home_config = PathBuf::from(crate::bootstrap::shellexpand_tilde(
            "~/.ghost/config/ghost.yml",
        ));
        if home_config.exists() {
            return home_config;
        }

        let local = PathBuf::from("ghost.yml");
        if local.exists() {
            return local;
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let beside_exe = exe_dir.join("ghost.yml");
                if beside_exe.exists() {
                    return beside_exe;
                }

                let mut ancestor = exe_dir.to_path_buf();
                for _ in 0..3 {
                    if let Some(parent) = ancestor.parent() {
                        ancestor = parent.to_path_buf();
                        let candidate = ancestor.join("ghost.yml");
                        if candidate.exists() {
                            return candidate;
                        }
                    }
                }
            }
        }

        home_config
    }

    /// Load from default locations:
    /// CLI arg > env > ~/.ghost/config/ghost.yml > ./ghost.yml > <exe_dir>/ghost.yml
    pub fn load_default(cli_path: Option<&str>) -> Result<Self, ConfigError> {
        if cli_path.is_some() || std::env::var("GHOST_CONFIG").is_ok() {
            return Self::load(&Self::default_path(cli_path));
        }

        let path = Self::default_path(None);
        if path.exists() {
            return Self::load(&path);
        }

        // Return default config if no file found
        Ok(GhostConfig::default())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
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
        if self.external_skills.enabled {
            if self.external_skills.managed_storage_path.trim().is_empty() {
                return Err(ConfigError::ValidationError(
                    "external_skills.managed_storage_path cannot be empty".into(),
                ));
            }
            for root in &self.external_skills.approved_roots {
                if root.path.trim().is_empty() {
                    return Err(ConfigError::ValidationError(
                        "external_skills.approved_roots[].path cannot be empty".into(),
                    ));
                }
            }
            for signer in &self.external_skills.trusted_signers {
                if signer.key_id.trim().is_empty() {
                    return Err(ConfigError::ValidationError(
                        "external_skills.trusted_signers[].key_id cannot be empty".into(),
                    ));
                }
                if signer.publisher.trim().is_empty() {
                    return Err(ConfigError::ValidationError(
                        "external_skills.trusted_signers[].publisher cannot be empty".into(),
                    ));
                }
                if signer.public_key.trim().is_empty() {
                    return Err(ConfigError::ValidationError(
                        "external_skills.trusted_signers[].public_key cannot be empty".into(),
                    ));
                }
            }
        }
        let mut seen_oauth_provider_names = std::collections::BTreeSet::new();
        for provider in &self.oauth.providers {
            if provider.name.trim().is_empty() {
                return Err(ConfigError::ValidationError(
                    "oauth.providers[].name cannot be empty".into(),
                ));
            }
            if !seen_oauth_provider_names.insert(provider.name.as_str()) {
                return Err(ConfigError::ValidationError(format!(
                    "Duplicate oauth provider name: '{}'",
                    provider.name
                )));
            }
            if provider.client_id.trim().is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "oauth provider '{}' client_id cannot be empty",
                    provider.name
                )));
            }
            if provider.client_secret_env.trim().is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "oauth provider '{}' client_secret_env cannot be empty",
                    provider.name
                )));
            }
            if provider.auth_url.trim().is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "oauth provider '{}' auth_url cannot be empty",
                    provider.name
                )));
            }
            if provider.token_url.trim().is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "oauth provider '{}' token_url cannot be empty",
                    provider.name
                )));
            }
        }
        Ok(())
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
