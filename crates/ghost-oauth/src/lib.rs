//! # ghost-oauth
//!
//! Self-hosted OAuth 2.0 PKCE broker for third-party APIs (Google, GitHub,
//! Slack, Microsoft). The agent never sees raw tokens — only opaque `OAuthRefId`
//! references. Tokens are encrypted at rest via `ghost-secrets`.
//!
//! Kill switch integration: `OAuthBroker::revoke_all()` revokes every connection.

pub mod error;
pub mod types;
pub mod provider;
pub mod storage;
pub mod providers;
pub mod broker;

// Re-exports for convenience.
pub use error::OAuthError;
pub use types::{
    ApiRequest, ApiResponse, ConnectionInfo, ConnectionStatus, OAuthRefId,
    PkceChallenge, ProviderConfig, TokenSet,
};
pub use provider::OAuthProvider;
pub use storage::TokenStore;
pub use broker::OAuthBroker;
