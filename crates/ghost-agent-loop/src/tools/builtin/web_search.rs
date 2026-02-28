//! Web search tool — API-based web search.
//!
//! Delegates to a configured search API (e.g., SearXNG, Brave Search).
//! Results are sanitized before returning to the agent.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebSearchError {
    #[error("search API unavailable: {0}")]
    Unavailable(String),
    #[error("search failed: {0}")]
    Failed(String),
}

/// A single search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Web search tool configuration.
#[derive(Debug, Clone)]
pub struct WebSearchConfig {
    /// Search API endpoint.
    pub api_url: String,
    /// Maximum results to return.
    pub max_results: usize,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            api_url: String::new(),
            max_results: 5,
        }
    }
}

/// Execute a web search query.
///
/// In production, this calls the configured search API.
/// Stub implementation returns empty results.
pub async fn search(
    query: &str,
    config: &WebSearchConfig,
) -> Result<Vec<SearchResult>, WebSearchError> {
    if config.api_url.is_empty() {
        return Err(WebSearchError::Unavailable(
            "no search API configured".into(),
        ));
    }

    // Stub: real implementation calls HTTP API
    tracing::info!(query, "web search executed");
    Ok(Vec::new())
}
