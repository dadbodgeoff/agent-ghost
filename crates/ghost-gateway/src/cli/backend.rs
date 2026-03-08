//! CLI backend abstraction — HTTP vs direct DB (Task 6.6 — §3.2, F.12).

use std::sync::Arc;

use crate::bootstrap::shellexpand_tilde;
use crate::config::GhostConfig;
use crate::db_pool::DbPool;

use super::error::CliError;
use super::http_client::GhostHttpClient;

/// The backend a CLI command uses to interact with the platform.
pub enum CliBackend {
    /// Talk to a running gateway over HTTP.
    Http { client: GhostHttpClient },
    /// Open the database directly (gateway not required).
    Direct {
        config: Box<GhostConfig>,
        db: Arc<DbPool>,
    },
}

/// What kind of backend a command requires.
#[derive(Debug, Clone, Copy)]
pub enum BackendRequirement {
    /// Command only works via HTTP (needs running gateway).
    HttpOnly,
    /// Prefer HTTP but fall back to direct DB.
    PreferHttp,
    /// Command only works with direct DB access.
    DirectOnly,
}

impl CliBackend {
    /// Auto-detect the best available backend.
    ///
    /// 1. Probe HTTP health (2s timeout).
    /// 2. Fall back to direct DB access.
    /// 3. Return `NoBackend` if neither works.
    pub async fn detect(
        config: &GhostConfig,
        gateway_url: Option<&str>,
        token: Option<String>,
    ) -> Result<Self, CliError> {
        let base_url = gateway_url
            .map(String::from)
            .unwrap_or_else(|| format!("http://{}:{}", config.gateway.bind, config.gateway.port));

        // Try HTTP first.
        if GhostHttpClient::health_check(&base_url).await {
            let client = GhostHttpClient::new(base_url, token);
            client.assert_compatible().await?;
            return Ok(Self::Http { client });
        }

        // Fall back to direct DB.
        Self::open_direct(config)
    }

    /// Open a direct DB backend without trying HTTP first.
    pub fn open_direct(config: &GhostConfig) -> Result<Self, CliError> {
        let db_path = shellexpand_tilde(&config.gateway.db_path);
        let path = std::path::Path::new(&db_path);
        if !path.exists() {
            return Err(CliError::NoBackend);
        }

        let pool = crate::db_pool::create_existing_pool(std::path::PathBuf::from(&db_path))
            .map_err(|e| CliError::Database(format!("open db pool: {e}")))?;

        Ok(Self::Direct {
            config: Box::new(config.clone()),
            db: pool,
        })
    }

    /// Check that this backend satisfies the given requirement.
    pub fn require(&self, req: BackendRequirement) -> Result<(), CliError> {
        match (req, self) {
            (BackendRequirement::HttpOnly, Self::Direct { .. }) => Err(CliError::GatewayRequired),
            (BackendRequirement::DirectOnly, Self::Http { .. }) => Err(CliError::Usage(
                "this command requires direct DB access (gateway must not be the only backend)"
                    .into(),
            )),
            _ => Ok(()),
        }
    }

    /// Returns `true` if this is an HTTP backend.
    pub fn is_http(&self) -> bool {
        matches!(self, Self::Http { .. })
    }

    /// Get a reference to the HTTP client (panics if Direct).
    pub fn http(&self) -> &GhostHttpClient {
        match self {
            Self::Http { client } => client,
            Self::Direct { .. } => panic!("expected HTTP backend"),
        }
    }

    /// Get a reference to the DB pool (panics if Http).
    pub fn db(&self) -> &Arc<DbPool> {
        match self {
            Self::Direct { db, .. } => db,
            Self::Http { .. } => panic!("expected Direct backend"),
        }
    }
}
