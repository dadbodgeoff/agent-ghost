//! `OAuthBroker` — orchestrates the full OAuth lifecycle.
//!
//! The agent never sees raw tokens. The broker:
//! 1. Manages connect/callback/disconnect flows
//! 2. Stores tokens encrypted via `TokenStore`
//! 3. Executes API calls with transparent token refresh
//! 4. Provides `revoke_all()` for kill switch integration
//!
//! Owned by the gateway, passed to agent-loop via `Arc`.

use std::collections::BTreeMap;
use std::sync::Mutex;

use chrono::Utc;
use secrecy::ExposeSecret;

use crate::error::OAuthError;
use crate::provider::OAuthProvider;
use crate::storage::TokenStore;
use crate::types::{
    ApiRequest, ApiResponse, ConnectionInfo, ConnectionStatus, OAuthRefId, PkceChallenge,
};

/// Pending OAuth flow state (between connect and callback).
struct PendingFlow {
    provider_name: String,
    ref_id: OAuthRefId,
    pkce: PkceChallenge,
    scopes: Vec<String>,
    redirect_uri: String,
    created_at: chrono::DateTime<Utc>,
}

/// The OAuth broker orchestrates connect/callback/execute/disconnect flows.
pub struct OAuthBroker {
    /// Registered OAuth providers keyed by name.
    providers: BTreeMap<String, Box<dyn OAuthProvider>>,
    /// Encrypted token storage.
    token_store: TokenStore,
    /// Pending OAuth flows keyed by state parameter.
    pending_flows: Mutex<BTreeMap<String, PendingFlow>>,
    /// Connection metadata (provider name for each ref_id).
    connections: Mutex<BTreeMap<String, ConnectionMeta>>,
}

/// Metadata about an active connection.
#[derive(Clone)]
struct ConnectionMeta {
    provider_name: String,
    scopes: Vec<String>,
    connected_at: chrono::DateTime<Utc>,
}

impl OAuthBroker {
    /// Create a new broker with the given providers and token store.
    pub fn new(
        providers: BTreeMap<String, Box<dyn OAuthProvider>>,
        token_store: TokenStore,
    ) -> Self {
        Self {
            providers,
            token_store,
            pending_flows: Mutex::new(BTreeMap::new()),
            connections: Mutex::new(BTreeMap::new()),
        }
    }

    /// Initiate an OAuth connect flow.
    ///
    /// Returns `(authorization_url, ref_id)`. The caller redirects the user
    /// to the authorization URL. After the user authorizes, the provider
    /// redirects to the callback URL with a `code` and `state`.
    pub fn connect(
        &self,
        provider_name: &str,
        scopes: &[String],
        redirect_uri: &str,
    ) -> Result<(String, OAuthRefId), OAuthError> {
        let provider = self.providers.get(provider_name).ok_or_else(|| {
            OAuthError::ProviderError(format!("unknown provider: {provider_name}"))
        })?;

        let ref_id = OAuthRefId::new();
        let state = format!("{}:{}", ref_id, uuid::Uuid::new_v4()); // ref_id + CSRF token

        let (auth_url, pkce) = provider.authorization_url(scopes, &state, redirect_uri)?;

        let flow = PendingFlow {
            provider_name: provider_name.to_string(),
            ref_id: ref_id.clone(),
            pkce,
            scopes: scopes.to_vec(),
            redirect_uri: redirect_uri.to_string(),
            created_at: Utc::now(),
        };

        self.pending_flows
            .lock()
            .map_err(|_| OAuthError::FlowFailed("lock poisoned".into()))?
            .insert(state, flow);

        tracing::info!(
            provider = %provider_name,
            ref_id = %ref_id,
            "OAuth connect flow initiated"
        );

        Ok((auth_url, ref_id))
    }

    /// Handle the OAuth callback after user authorization.
    ///
    /// Exchanges the authorization code for tokens, encrypts and stores them.
    pub fn callback(&self, state: &str, code: &str) -> Result<OAuthRefId, OAuthError> {
        let flow = self
            .pending_flows
            .lock()
            .map_err(|_| OAuthError::FlowFailed("lock poisoned".into()))?
            .remove(state)
            .ok_or_else(|| OAuthError::InvalidState("unknown or expired state".into()))?;

        // Reject stale flows (>10 minutes)
        let age = Utc::now() - flow.created_at;
        if age.num_minutes() > 10 {
            return Err(OAuthError::InvalidState("state expired (>10 min)".into()));
        }

        let provider = self.providers.get(&flow.provider_name).ok_or_else(|| {
            OAuthError::ProviderError(format!("provider gone: {}", flow.provider_name))
        })?;

        let verifier = flow.pkce.code_verifier.expose_secret().to_string();
        let token_set = provider.exchange_code(code, &verifier, &flow.redirect_uri)?;

        self.token_store
            .store_token(&flow.ref_id, &flow.provider_name, &token_set)?;

        self.connections
            .lock()
            .map_err(|_| OAuthError::FlowFailed("lock poisoned".into()))?
            .insert(
                flow.ref_id.to_string(),
                ConnectionMeta {
                    provider_name: flow.provider_name.clone(),
                    scopes: flow.scopes.clone(),
                    connected_at: Utc::now(),
                },
            );

        tracing::info!(
            provider = %flow.provider_name,
            ref_id = %flow.ref_id,
            "OAuth callback completed — tokens stored"
        );

        Ok(flow.ref_id)
    }

    /// Execute an API call on behalf of the agent.
    ///
    /// 1. Load token from encrypted storage
    /// 2. If expired, refresh transparently
    /// 3. Inject Bearer token and execute
    /// 4. Token zeroized on drop
    pub fn execute(
        &self,
        ref_id: &OAuthRefId,
        request: &ApiRequest,
    ) -> Result<ApiResponse, OAuthError> {
        let meta = self.get_connection_meta(ref_id)?;
        let provider = self.providers.get(&meta.provider_name).ok_or_else(|| {
            OAuthError::ProviderError(format!("provider gone: {}", meta.provider_name))
        })?;

        // Load token, handling expiry with auto-refresh
        let token_set = match self.token_store.load_token(ref_id, &meta.provider_name) {
            Ok(ts) => ts,
            Err(OAuthError::TokenExpired(_)) => {
                self.refresh_and_store(ref_id, &meta.provider_name, provider.as_ref())?
            }
            Err(e) => return Err(e),
        };

        let access = token_set.access_token.expose_secret();
        provider.execute_api_call(access, request)
    }

    /// Disconnect: revoke at provider + delete local tokens.
    pub fn disconnect(&self, ref_id: &OAuthRefId) -> Result<(), OAuthError> {
        let meta = self.get_connection_meta(ref_id)?;
        let provider = self.providers.get(&meta.provider_name);

        // Best-effort revocation at provider
        if let Some(provider) = provider {
            if let Ok(ts) = self.token_store.load_token(ref_id, &meta.provider_name) {
                if let Err(e) = provider.revoke_token(ts.access_token.expose_secret()) {
                    tracing::warn!(
                        provider = %meta.provider_name,
                        ref_id = %ref_id,
                        error = %e,
                        "provider-side token revocation failed (best-effort, continuing with local cleanup)"
                    );
                }
            }
        }

        // Delete local encrypted tokens
        self.token_store.delete_token(ref_id, &meta.provider_name)?;

        self.connections
            .lock()
            .map_err(|_| OAuthError::FlowFailed("lock poisoned".into()))?
            .remove(&ref_id.to_string());

        tracing::info!(
            provider = %meta.provider_name,
            ref_id = %ref_id,
            "OAuth connection disconnected"
        );
        Ok(())
    }

    /// Revoke ALL connections (kill switch integration).
    ///
    /// On QUARANTINE/KILL_ALL, this makes all ref_ids non-functional.
    pub fn revoke_all(&self) -> Result<(), OAuthError> {
        let all = self.token_store.list_all_connections()?;
        let mut errors = Vec::new();

        for (provider_name, ref_id) in &all {
            if let Some(provider) = self.providers.get(provider_name) {
                if let Ok(ts) = self.token_store.load_token(ref_id, provider_name) {
                    if let Err(e) = provider.revoke_token(ts.access_token.expose_secret()) {
                        tracing::warn!(
                            provider = %provider_name,
                            ref_id = %ref_id,
                            error = %e,
                            "provider-side token revocation failed during revoke_all (best-effort)"
                        );
                    }
                }
            }
            if let Err(e) = self.token_store.delete_token(ref_id, provider_name) {
                errors.push(format!("{ref_id}: {e}"));
            }
        }

        self.connections
            .lock()
            .map_err(|_| OAuthError::FlowFailed("lock poisoned".into()))?
            .clear();

        if errors.is_empty() {
            tracing::warn!(
                count = all.len(),
                "revoke_all: all OAuth connections revoked"
            );
            Ok(())
        } else {
            tracing::error!(errors = ?errors, "revoke_all: some deletions failed");
            Err(OAuthError::StorageError(format!(
                "partial revoke_all failure: {}",
                errors.join("; ")
            )))
        }
    }

    /// List all active connections (agent-visible, no tokens).
    pub fn list_connections(&self) -> Result<Vec<ConnectionInfo>, OAuthError> {
        let conns = self
            .connections
            .lock()
            .map_err(|_| OAuthError::FlowFailed("lock poisoned".into()))?;

        let mut infos = Vec::new();
        for (ref_id_str, meta) in conns.iter() {
            let ref_id: OAuthRefId = ref_id_str
                .parse::<uuid::Uuid>()
                .map(OAuthRefId::from_uuid)
                .unwrap_or_else(|_| OAuthRefId::new());

            let status = match self.token_store.load_token(&ref_id, &meta.provider_name) {
                Ok(_) => ConnectionStatus::Connected,
                Err(OAuthError::TokenExpired(_)) => ConnectionStatus::Expired,
                Err(OAuthError::TokenRevoked(_)) => ConnectionStatus::Revoked,
                Err(_) => ConnectionStatus::Error,
            };

            infos.push(ConnectionInfo {
                ref_id,
                provider: meta.provider_name.clone(),
                scopes: meta.scopes.clone(),
                connected_at: meta.connected_at,
                status,
            });
        }
        Ok(infos)
    }

    /// List configured provider names.
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn get_connection_meta(&self, ref_id: &OAuthRefId) -> Result<ConnectionMeta, OAuthError> {
        self.connections
            .lock()
            .map_err(|_| OAuthError::FlowFailed("lock poisoned".into()))?
            .get(&ref_id.to_string())
            .cloned()
            .ok_or_else(|| OAuthError::NotConnected(ref_id.to_string()))
    }

    /// Refresh an expired token and store the new one.
    fn refresh_and_store(
        &self,
        ref_id: &OAuthRefId,
        provider_name: &str,
        provider: &dyn OAuthProvider,
    ) -> Result<crate::types::TokenSet, OAuthError> {
        // Load the expired token to get the refresh token
        // We need to bypass the expiry check — read raw
        let path = self.token_store.load_token_raw(ref_id, provider_name)?;

        let refresh = path
            .refresh_token
            .as_ref()
            .ok_or_else(|| OAuthError::RefreshFailed("no refresh token available".into()))?;

        let new_ts = provider.refresh_token(refresh.expose_secret())?;
        self.token_store
            .store_token(ref_id, provider_name, &new_ts)?;

        tracing::info!(
            provider = %provider_name,
            ref_id = %ref_id,
            "token refreshed transparently"
        );
        Ok(new_ts)
    }
}
