//! # ghost-secrets
//!
//! Cross-platform credential storage with OS keychain, HashiCorp Vault,
//! and environment variable fallback.
//!
//! Leaf crate — zero dependencies on any `ghost-*` or `cortex-*` crate.
//! All secret values wrapped in `SecretString` (zeroized on drop via `secrecy`).

pub mod env_provider;
pub mod error;
pub mod provider;

#[cfg(feature = "keychain")]
pub mod keychain_provider;

#[cfg(feature = "vault")]
pub mod vault_provider;

// Re-exports for convenience.
pub use env_provider::EnvProvider;
pub use error::SecretsError;
pub use provider::{ProviderConfig, SecretProvider};
pub use secrecy::ExposeSecret;
pub use secrecy::SecretString;

#[cfg(feature = "keychain")]
pub use keychain_provider::KeychainProvider;

#[cfg(feature = "vault")]
pub use vault_provider::VaultProvider;
