pub mod cloud_config;
pub mod consolidation_config;
pub mod convergence_config;
pub mod decay_config;
pub mod defaults;
pub mod embedding_config;
pub mod multiagent_config;
pub mod observability_config;
pub mod privacy_config;
pub mod retrieval_config;
pub mod storage_config;
pub mod temporal_config;

use serde::{Deserialize, Serialize};

pub use cloud_config::CloudConfig;
pub use consolidation_config::ConsolidationConfig;
pub use convergence_config::ConvergenceConfig;
pub use decay_config::DecayConfig;
pub use embedding_config::EmbeddingConfig;
pub use multiagent_config::MultiAgentConfig;
pub use observability_config::ObservabilityConfig;
pub use privacy_config::PrivacyConfig;
pub use retrieval_config::RetrievalConfig;
pub use storage_config::StorageConfig;
pub use temporal_config::TemporalConfig;

/// Top-level configuration aggregating all subsystem configs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CortexConfig {
    pub storage: StorageConfig,
    pub embedding: EmbeddingConfig,
    pub retrieval: RetrievalConfig,
    pub consolidation: ConsolidationConfig,
    pub decay: DecayConfig,
    pub privacy: PrivacyConfig,
    pub cloud: CloudConfig,
    pub observability: ObservabilityConfig,
    pub temporal: TemporalConfig,
    pub multiagent: MultiAgentConfig,
    pub convergence: ConvergenceConfig,
}

impl CortexConfig {
    /// Load config from a TOML string, falling back to defaults for missing fields.
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }
}
