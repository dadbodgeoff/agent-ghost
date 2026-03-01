//! Web search tool — multi-backend web search with SearXNG, Tavily, and Brave.
//!
//! SearXNG (self-hosted, free, unlimited) is the default backend.
//! Tavily and Brave are optional for users who don't want to run Docker.
//! All backends return normalized `SearchResult` structs.
//!
//! Results are sanitized: HTML tags stripped, snippets truncated to
//! `max_snippet_len`, and total results capped at `max_results`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum WebSearchError {
    #[error("search API unavailable: {0}")]
    Unavailable(String),
    #[error("search request failed: {0}")]
    RequestFailed(String),
    #[error("search response parse error: {0}")]
    ParseError(String),
    #[error("search API auth failed: {0}")]
    AuthFailed(String),
    #[error("search rate limited — retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}

// ── Result types ────────────────────────────────────────────────────────

/// A single search result, normalized across all backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

// ── Configuration ───────────────────────────────────────────────────────

/// Which search backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchBackend {
    /// Self-hosted SearXNG instance (free, unlimited).
    #[default]
    SearXNG,
    /// Tavily Search API (1,000 free credits/month).
    Tavily,
    /// Brave Search API ($5/month free credits).
    Brave,
}

/// Web search tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchConfig {
    /// Active backend.
    #[serde(default)]
    pub backend: SearchBackend,

    /// Maximum results to return (applies to all backends).
    #[serde(default = "default_max_results")]
    pub max_results: usize,

    /// Maximum snippet length in characters (truncated with "…").
    #[serde(default = "default_max_snippet_len")]
    pub max_snippet_len: usize,

    /// Request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// SearXNG instance URL (e.g. "http://localhost:8888").
    #[serde(default)]
    pub searxng_url: String,

    /// Tavily API key.
    #[serde(default)]
    pub tavily_api_key: String,

    /// Brave Search API key.
    #[serde(default)]
    pub brave_api_key: String,
}

fn default_max_results() -> usize { 5 }
fn default_max_snippet_len() -> usize { 300 }
fn default_timeout_secs() -> u64 { 10 }

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            backend: SearchBackend::default(),
            max_results: default_max_results(),
            max_snippet_len: default_max_snippet_len(),
            timeout_secs: default_timeout_secs(),
            searxng_url: String::new(),
            tavily_api_key: String::new(),
            brave_api_key: String::new(),
        }
    }
}

// ── Public API ──────────────────────────────────────────────────────────

/// Execute a web search query using the configured backend.
pub async fn search(
    query: &str,
    config: &WebSearchConfig,
) -> Result<Vec<SearchResult>, WebSearchError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let raw_results = match config.backend {
        SearchBackend::SearXNG => search_searxng(query, config).await?,
        SearchBackend::Tavily => search_tavily(query, config).await?,
        SearchBackend::Brave => search_brave(query, config).await?,
    };

    // Normalize: sanitize HTML, truncate snippets, cap results.
    let results = raw_results
        .into_iter()
        .take(config.max_results)
        .map(|mut r| {
            r.title = strip_html_tags(&r.title);
            r.snippet = truncate_snippet(
                &strip_html_tags(&r.snippet),
                config.max_snippet_len,
            );
            r
        })
        .collect();

    Ok(results)
}

// ── SearXNG backend ─────────────────────────────────────────────────────
//
// SearXNG is a self-hosted meta-search engine. It aggregates results from
// Google, Bing, DuckDuckGo, Wikipedia, etc. Free, unlimited, no API key.
//
// JSON API: GET {url}/search?q={query}&format=json&categories=general
// Docs: https://docs.searxng.org/dev/search_api.html

/// SearXNG JSON response (subset of fields we care about).
#[derive(Debug, Deserialize)]
struct SearXNGResponse {
    #[serde(default)]
    results: Vec<SearXNGResult>,
}

#[derive(Debug, Deserialize)]
struct SearXNGResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    /// SearXNG calls the snippet "content".
    #[serde(default)]
    content: String,
}

async fn search_searxng(
    query: &str,
    config: &WebSearchConfig,
) -> Result<Vec<SearchResult>, WebSearchError> {
    if config.searxng_url.is_empty() {
        return Err(WebSearchError::Unavailable(
            "SearXNG URL not configured. Set `searxng_url` in search config \
             (e.g. \"http://localhost:8888\"). Run SearXNG via: \
             docker run -p 8888:8080 searxng/searxng"
                .into(),
        ));
    }

    let base_url = config.searxng_url.trim_end_matches('/');
    let url = format!(
        "{}/search?q={}&format=json&categories=general",
        base_url,
        urlencoded(query),
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .build()
        .map_err(|e| WebSearchError::RequestFailed(e.to_string()))?;

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                WebSearchError::RequestFailed(format!(
                    "SearXNG request timed out after {}s — is the instance running at {}?",
                    config.timeout_secs, base_url
                ))
            } else if e.is_connect() {
                WebSearchError::Unavailable(format!(
                    "Cannot connect to SearXNG at {} — is the instance running?",
                    base_url
                ))
            } else {
                WebSearchError::RequestFailed(e.to_string())
            }
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(WebSearchError::RequestFailed(format!(
            "SearXNG returned HTTP {}: {}",
            status.as_u16(),
            truncate_snippet(&body, 200),
        )));
    }

    let parsed: SearXNGResponse = resp
        .json()
        .await
        .map_err(|e| WebSearchError::ParseError(format!("SearXNG JSON parse: {}", e)))?;

    Ok(parsed
        .results
        .into_iter()
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.content,
        })
        .collect())
}

// ── Tavily backend ──────────────────────────────────────────────────────
//
// Tavily is a search API built for AI agents. Returns cleaned, extracted
// content rather than just URLs/snippets. 1,000 free credits/month.
//
// POST https://api.tavily.com/search
// Body: { "api_key": "...", "query": "...", "max_results": 5 }

#[derive(Debug, Serialize)]
struct TavilyRequest<'a> {
    api_key: &'a str,
    query: &'a str,
    max_results: usize,
    /// "basic" returns title/url/content. "advanced" adds raw_content.
    search_depth: &'a str,
}

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

async fn search_tavily(
    query: &str,
    config: &WebSearchConfig,
) -> Result<Vec<SearchResult>, WebSearchError> {
    if config.tavily_api_key.is_empty() {
        return Err(WebSearchError::AuthFailed(
            "Tavily API key not configured. Get a free key at https://tavily.com \
             (1,000 credits/month free, no credit card)."
                .into(),
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .build()
        .map_err(|e| WebSearchError::RequestFailed(e.to_string()))?;

    let body = TavilyRequest {
        api_key: &config.tavily_api_key,
        query,
        max_results: config.max_results,
        search_depth: "basic",
    };

    let resp = client
        .post("https://api.tavily.com/search")
        .json(&body)
        .send()
        .await
        .map_err(|e| WebSearchError::RequestFailed(e.to_string()))?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(WebSearchError::AuthFailed(
            "Tavily API key is invalid or expired.".into(),
        ));
    }
    if status.as_u16() == 429 {
        return Err(WebSearchError::RateLimited {
            retry_after_secs: 60,
        });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(WebSearchError::RequestFailed(format!(
            "Tavily returned HTTP {}: {}",
            status.as_u16(),
            truncate_snippet(&body, 200),
        )));
    }

    let parsed: TavilyResponse = resp
        .json()
        .await
        .map_err(|e| WebSearchError::ParseError(format!("Tavily JSON parse: {}", e)))?;

    Ok(parsed
        .results
        .into_iter()
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.content,
        })
        .collect())
}

// ── Brave backend ───────────────────────────────────────────────────────
//
// Brave Search API. $5/month free credits (~1,000 queries).
//
// GET https://api.search.brave.com/res/v1/web/search?q={query}&count={n}
// Header: X-Subscription-Token: {api_key}

#[derive(Debug, Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

async fn search_brave(
    query: &str,
    config: &WebSearchConfig,
) -> Result<Vec<SearchResult>, WebSearchError> {
    if config.brave_api_key.is_empty() {
        return Err(WebSearchError::AuthFailed(
            "Brave Search API key not configured. Get $5/month free credits \
             at https://brave.com/search/api/"
                .into(),
        ));
    }

    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlencoded(query),
        config.max_results,
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .build()
        .map_err(|e| WebSearchError::RequestFailed(e.to_string()))?;

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip")
        .header("X-Subscription-Token", &config.brave_api_key)
        .send()
        .await
        .map_err(|e| WebSearchError::RequestFailed(e.to_string()))?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(WebSearchError::AuthFailed(
            "Brave Search API key is invalid.".into(),
        ));
    }
    if status.as_u16() == 429 {
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(WebSearchError::RateLimited {
            retry_after_secs: retry_after,
        });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(WebSearchError::RequestFailed(format!(
            "Brave returned HTTP {}: {}",
            status.as_u16(),
            truncate_snippet(&body, 200),
        )));
    }

    let parsed: BraveResponse = resp
        .json()
        .await
        .map_err(|e| WebSearchError::ParseError(format!("Brave JSON parse: {}", e)))?;

    let results = parsed
        .web
        .map(|w| w.results)
        .unwrap_or_default()
        .into_iter()
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.description,
        })
        .collect();

    Ok(results)
}

// ── Utilities ───────────────────────────────────────────────────────────

/// Minimal URL encoding for query parameters.
/// Encodes spaces, ampersands, and other special characters.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push(char::from(HEX_CHARS[(b >> 4) as usize]));
                out.push(char::from(HEX_CHARS[(b & 0x0f) as usize]));
            }
        }
    }
    out
}

const HEX_CHARS: [u8; 16] = *b"0123456789ABCDEF";

/// Strip HTML tags from a string. Simple state machine — not a full parser,
/// but sufficient for search result snippets.
fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Collapse multiple whitespace into single space.
    let collapsed: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
}

/// Truncate a string to `max_len` characters, appending "…" if truncated.
fn truncate_snippet(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    // Find a char boundary near max_len.
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = s[..end].to_string();
    truncated.push('…');
    truncated
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Unit tests (no network) ─────────────────────────────────────

    #[test]
    fn strip_html_tags_basic() {
        assert_eq!(
            strip_html_tags("<b>Hello</b> <i>world</i>"),
            "Hello world"
        );
    }

    #[test]
    fn strip_html_tags_nested() {
        assert_eq!(
            strip_html_tags("<div><p>Some <strong>bold</strong> text</p></div>"),
            "Some bold text"
        );
    }

    #[test]
    fn strip_html_tags_no_tags() {
        assert_eq!(strip_html_tags("plain text"), "plain text");
    }

    #[test]
    fn strip_html_tags_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn truncate_snippet_short() {
        assert_eq!(truncate_snippet("hello", 10), "hello");
    }

    #[test]
    fn truncate_snippet_exact() {
        assert_eq!(truncate_snippet("hello", 5), "hello");
    }

    #[test]
    fn truncate_snippet_long() {
        let result = truncate_snippet("hello world this is long", 11);
        assert!(result.ends_with('…'));
        assert!(result.len() <= 15); // 11 + "…" (3 bytes)
    }

    #[test]
    fn truncate_snippet_unicode() {
        // Ensure we don't split in the middle of a multi-byte char.
        let result = truncate_snippet("héllo wörld", 6);
        assert!(result.ends_with('…'));
        // Should not panic on char boundary.
    }

    #[test]
    fn urlencoded_basic() {
        assert_eq!(urlencoded("hello world"), "hello%20world");
        assert_eq!(urlencoded("rust+lang"), "rust%2Blang");
        assert_eq!(urlencoded("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn urlencoded_passthrough() {
        assert_eq!(urlencoded("simple"), "simple");
        assert_eq!(urlencoded("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn default_config_is_searxng() {
        let config = WebSearchConfig::default();
        assert_eq!(config.backend, SearchBackend::SearXNG);
        assert_eq!(config.max_results, 5);
        assert_eq!(config.max_snippet_len, 300);
    }

    #[tokio::test]
    async fn empty_query_returns_empty() {
        let config = WebSearchConfig {
            searxng_url: "http://localhost:9999".into(),
            ..Default::default()
        };
        let results = search("", &config).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn whitespace_query_returns_empty() {
        let config = WebSearchConfig {
            searxng_url: "http://localhost:9999".into(),
            ..Default::default()
        };
        let results = search("   ", &config).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn searxng_missing_url_returns_unavailable() {
        let config = WebSearchConfig::default();
        let err = search("test", &config).await.unwrap_err();
        assert!(matches!(err, WebSearchError::Unavailable(_)));
    }

    #[tokio::test]
    async fn tavily_missing_key_returns_auth_failed() {
        let config = WebSearchConfig {
            backend: SearchBackend::Tavily,
            ..Default::default()
        };
        let err = search("test", &config).await.unwrap_err();
        assert!(matches!(err, WebSearchError::AuthFailed(_)));
    }

    #[tokio::test]
    async fn brave_missing_key_returns_auth_failed() {
        let config = WebSearchConfig {
            backend: SearchBackend::Brave,
            ..Default::default()
        };
        let err = search("test", &config).await.unwrap_err();
        assert!(matches!(err, WebSearchError::AuthFailed(_)));
    }

    #[tokio::test]
    async fn searxng_unreachable_returns_error() {
        let config = WebSearchConfig {
            searxng_url: "http://127.0.0.1:1".into(), // nothing listening
            timeout_secs: 1,
            ..Default::default()
        };
        let err = search("test", &config).await.unwrap_err();
        // Should be either Unavailable (connect refused) or RequestFailed (timeout).
        assert!(
            matches!(err, WebSearchError::Unavailable(_) | WebSearchError::RequestFailed(_))
        );
    }

    // ── SearXNG JSON parsing ────────────────────────────────────────

    #[test]
    fn parse_searxng_response() {
        let json = r#"{
            "results": [
                {
                    "title": "Rust Programming",
                    "url": "https://www.rust-lang.org",
                    "content": "A language empowering everyone to build reliable software."
                },
                {
                    "title": "Rust (Wikipedia)",
                    "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)",
                    "content": "Rust is a <b>multi-paradigm</b> programming language."
                }
            ]
        }"#;
        let parsed: SearXNGResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.results.len(), 2);
        assert_eq!(parsed.results[0].title, "Rust Programming");
        assert_eq!(parsed.results[1].content, "Rust is a <b>multi-paradigm</b> programming language.");
    }

    #[test]
    fn parse_searxng_empty_response() {
        let json = r#"{"results": []}"#;
        let parsed: SearXNGResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.results.is_empty());
    }

    #[test]
    fn parse_searxng_missing_fields() {
        // SearXNG sometimes returns results with missing fields.
        let json = r#"{"results": [{"url": "https://example.com"}]}"#;
        let parsed: SearXNGResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].title, "");
        assert_eq!(parsed.results[0].url, "https://example.com");
        assert_eq!(parsed.results[0].content, "");
    }

    // ── Tavily JSON parsing ─────────────────────────────────────────

    #[test]
    fn parse_tavily_response() {
        let json = r#"{
            "results": [
                {
                    "title": "Rust Lang",
                    "url": "https://www.rust-lang.org",
                    "content": "Rust is a systems programming language.",
                    "score": 0.95
                }
            ]
        }"#;
        let parsed: TavilyResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].title, "Rust Lang");
    }

    // ── Brave JSON parsing ──────────────────────────────────────────

    #[test]
    fn parse_brave_response() {
        let json = r#"{
            "web": {
                "results": [
                    {
                        "title": "Rust Programming Language",
                        "url": "https://www.rust-lang.org",
                        "description": "A language for reliable software."
                    }
                ]
            }
        }"#;
        let parsed: BraveResponse = serde_json::from_str(json).unwrap();
        let results = parsed.web.unwrap().results;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Programming Language");
    }

    #[test]
    fn parse_brave_empty_web() {
        let json = r#"{"web": null}"#;
        let parsed: BraveResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.web.is_none());
    }

    // ── Integration tests (require live services, run manually) ─────

    #[tokio::test]
    #[ignore = "requires running SearXNG instance at localhost:8888"]
    async fn live_searxng_search() {
        let config = WebSearchConfig {
            searxng_url: "http://localhost:8888".into(),
            ..Default::default()
        };
        let results = search("rust programming language", &config).await.unwrap();
        assert!(!results.is_empty());
        for r in &results {
            assert!(!r.title.is_empty());
            assert!(!r.url.is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "requires TAVILY_API_KEY env var"]
    async fn live_tavily_search() {
        let api_key = std::env::var("TAVILY_API_KEY").expect("TAVILY_API_KEY not set");
        let config = WebSearchConfig {
            backend: SearchBackend::Tavily,
            tavily_api_key: api_key,
            ..Default::default()
        };
        let results = search("rust programming language", &config).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires BRAVE_API_KEY env var"]
    async fn live_brave_search() {
        let api_key = std::env::var("BRAVE_API_KEY").expect("BRAVE_API_KEY not set");
        let config = WebSearchConfig {
            backend: SearchBackend::Brave,
            brave_api_key: api_key,
            ..Default::default()
        };
        let results = search("rust programming language", &config).await.unwrap();
        assert!(!results.is_empty());
    }
}
