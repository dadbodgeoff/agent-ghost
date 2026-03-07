//! # ghost-oauth
//!
//! Self-hosted OAuth 2.0 PKCE broker for third-party APIs (Google, GitHub,
//! Slack, Microsoft). The agent never sees raw tokens — only opaque `OAuthRefId`
//! references. Tokens are encrypted at rest via `ghost-secrets`.
//!
//! Kill switch integration: `OAuthBroker::revoke_all()` revokes every connection.

pub mod broker;
pub mod error;
pub mod provider;
pub mod providers;
pub mod storage;
pub mod types;

// Re-exports for convenience.
pub use broker::OAuthBroker;
pub use error::OAuthError;
pub use provider::OAuthProvider;
pub use storage::TokenStore;
pub use types::{
    ApiRequest, ApiResponse, ConnectionInfo, ConnectionStatus, OAuthRefId, PkceChallenge,
    ProviderConfig, TokenSet,
};
