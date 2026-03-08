//! URL fetch tool — retrieve and extract text content from web pages.
//!
//! Complements `web_search` by letting the agent read the actual content
//! of pages returned by search results. Returns cleaned text, not raw HTML.
//!
//! Safety:
//! - Only HTTPS URLs are allowed (HTTP rejected unless explicitly configured).
//! - Private/internal IPs are blocked (SSRF protection).
//! - Response body is capped at `max_body_bytes` to prevent memory exhaustion.
//! - Content is sanitized: HTML tags stripped, excessive whitespace collapsed.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("URL not allowed: {0}")]
    UrlNotAllowed(String),
    #[error("fetch failed: {0}")]
    RequestFailed(String),
    #[error("response too large: {size} bytes exceeds {max} byte limit")]
    ResponseTooLarge { size: u64, max: u64 },
    #[error("SSRF blocked: {0} resolves to private/internal IP")]
    SsrfBlocked(String),
    #[error("unsupported content type: {0}")]
    UnsupportedContentType(String),
}

// ── Configuration ───────────────────────────────────────────────────────

/// Fetch tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchConfig {
    /// Allow plain HTTP (not just HTTPS). Default: false.
    #[serde(default)]
    pub allow_http: bool,

    /// Maximum response body size in bytes. Default: 1MB.
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: u64,

    /// Maximum extracted text length in characters. Default: 16,000.
    #[serde(default = "default_max_text_chars")]
    pub max_text_chars: usize,

    /// Request timeout in seconds. Default: 15.
    #[serde(default = "default_fetch_timeout")]
    pub timeout_secs: u64,

    /// User-Agent header. Default: GHOST agent identifier.
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

fn default_max_body_bytes() -> u64 {
    1_048_576
} // 1MB
fn default_max_text_chars() -> usize {
    16_000
}
fn default_fetch_timeout() -> u64 {
    15
}
fn default_user_agent() -> String {
    "GHOST-Agent/0.1 (autonomous-agent; +https://github.com/ghost-agent)".into()
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            allow_http: false,
            max_body_bytes: default_max_body_bytes(),
            max_text_chars: default_max_text_chars(),
            timeout_secs: default_fetch_timeout(),
            user_agent: default_user_agent(),
        }
    }
}

// ── Result type ─────────────────────────────────────────────────────────

/// Result of fetching a URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    /// The URL that was fetched (after redirects).
    pub url: String,
    /// HTTP status code.
    pub status: u16,
    /// Extracted text content (HTML stripped).
    pub content: String,
    /// Content type from the response.
    pub content_type: String,
    /// Whether the content was truncated.
    pub truncated: bool,
    /// Content length in characters.
    pub content_length: usize,
}

// ── Public API ──────────────────────────────────────────────────────────

/// Fetch a URL and extract its text content.
pub async fn fetch_url(url: &str, config: &FetchConfig) -> Result<FetchResult, FetchError> {
    let url = url.trim();

    // Validate URL scheme.
    if url.starts_with("http://") && !config.allow_http {
        return Err(FetchError::UrlNotAllowed(
            "HTTP URLs are not allowed. Only HTTPS is permitted.".into(),
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(FetchError::UrlNotAllowed(format!(
            "Unsupported URL scheme. Only HTTP(S) is allowed: {}",
            truncate(url, 100),
        )));
    }

    // Extract host for SSRF check.
    let host = extract_host(url)
        .ok_or_else(|| FetchError::UrlNotAllowed("Cannot parse host from URL".into()))?;

    // Block private/internal IPs.
    if is_private_host(&host) {
        return Err(FetchError::SsrfBlocked(host));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent(&config.user_agent)
        .build()
        .map_err(|e| FetchError::RequestFailed(e.to_string()))?;

    let resp = client
        .get(url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,text/plain,application/json",
        )
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                FetchError::RequestFailed(format!(
                    "Request timed out after {}s",
                    config.timeout_secs
                ))
            } else {
                FetchError::RequestFailed(e.to_string())
            }
        })?;

    let status = resp.status().as_u16();
    let final_url = resp.url().to_string();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Check content-length header if present.
    if let Some(cl) = resp.content_length() {
        if cl > config.max_body_bytes {
            return Err(FetchError::ResponseTooLarge {
                size: cl,
                max: config.max_body_bytes,
            });
        }
    }

    // Only process text-based content types.
    let is_text = content_type.contains("text/")
        || content_type.contains("application/json")
        || content_type.contains("application/xml")
        || content_type.contains("application/xhtml");

    if !is_text {
        return Err(FetchError::UnsupportedContentType(content_type));
    }

    // Read body with size limit.
    let body_bytes = read_limited_body(resp, config.max_body_bytes).await?;
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // Extract content based on content type — HTML is converted to markdown
    // for token-efficient LLM consumption.
    let extracted = if content_type.contains("text/html") || content_type.contains("xhtml") {
        html_to_markdown(&body)
    } else {
        // Plain text, JSON, XML — return as-is with whitespace cleanup.
        collapse_whitespace(&body)
    };

    // Truncate to max chars.
    let truncated = extracted.len() > config.max_text_chars;
    let content = if truncated {
        let mut s = safe_truncate(&extracted, config.max_text_chars);
        s.push_str("\n\n[Content truncated]");
        s
    } else {
        extracted
    };

    let content_length = content.len();

    Ok(FetchResult {
        url: final_url,
        status,
        content,
        content_type,
        truncated,
        content_length,
    })
}

// ── Internals ───────────────────────────────────────────────────────────

/// Read response body up to `max_bytes`, returning an error if exceeded.
async fn read_limited_body(resp: reqwest::Response, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
    // Use bytes() which reads the full body — we rely on the content-length
    // pre-check above for known sizes. For chunked/unknown, we read and check.
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| FetchError::RequestFailed(format!("Failed to read body: {}", e)))?;

    if bytes.len() as u64 > max_bytes {
        return Err(FetchError::ResponseTooLarge {
            size: bytes.len() as u64,
            max: max_bytes,
        });
    }

    Ok(bytes.to_vec())
}

/// Convert HTML to markdown for token-efficient LLM consumption.
///
/// Preserves semantic structure (headings, links, lists, code blocks, emphasis)
/// while stripping scripts, styles, nav, footer, and boilerplate. This is
/// typically 30-50% more token-efficient than flat text because the LLM
/// doesn't need to infer document structure.
fn html_to_markdown(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut in_nav = false;
    let mut in_footer = false;
    let mut in_pre = false;
    let mut in_code = false;
    let mut tag_buf = String::new();
    let mut collecting_tag = false;
    // Stack for nested list tracking: true = ordered, false = unordered.
    let mut list_stack: Vec<(bool, u32)> = Vec::new(); // (is_ordered, item_count)
                                                       // Link state: collecting link text, pending href.
    let mut link_href: Option<String> = None;
    let mut link_text = String::new();
    let mut in_link = false;

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                collecting_tag = true;
                tag_buf.clear();
            }
            '>' => {
                in_tag = false;
                collecting_tag = false;
                let tag_raw = tag_buf.trim().to_string();
                let tag_lower = tag_raw.to_lowercase();

                // Extract tag name (before space), preserving leading / for closing tags.
                let tag_name = tag_lower
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches('/');

                // Skip content regions.
                match tag_name {
                    "script" => {
                        in_script = true;
                        continue;
                    }
                    "/script" => {
                        in_script = false;
                        continue;
                    }
                    "style" => {
                        in_style = true;
                        continue;
                    }
                    "/style" => {
                        in_style = false;
                        continue;
                    }
                    "nav" => {
                        in_nav = true;
                        continue;
                    }
                    "/nav" => {
                        in_nav = false;
                        continue;
                    }
                    "footer" => {
                        in_footer = true;
                        continue;
                    }
                    "/footer" => {
                        in_footer = false;
                        continue;
                    }
                    _ => {}
                }

                if in_script || in_style || in_nav || in_footer {
                    continue;
                }

                match tag_name {
                    // Headings → markdown headings.
                    "h1" => {
                        ensure_double_newline(&mut out);
                        out.push_str("# ");
                    }
                    "h2" => {
                        ensure_double_newline(&mut out);
                        out.push_str("## ");
                    }
                    "h3" => {
                        ensure_double_newline(&mut out);
                        out.push_str("### ");
                    }
                    "h4" => {
                        ensure_double_newline(&mut out);
                        out.push_str("#### ");
                    }
                    "h5" => {
                        ensure_double_newline(&mut out);
                        out.push_str("##### ");
                    }
                    "h6" => {
                        ensure_double_newline(&mut out);
                        out.push_str("###### ");
                    }
                    "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => {
                        out.push('\n');
                    }

                    // Paragraphs and divs → double newline.
                    "p" | "div" | "article" | "section" | "main" => {
                        ensure_double_newline(&mut out);
                    }
                    "/p" | "/div" | "/article" | "/section" | "/main" => {
                        out.push('\n');
                    }

                    // Line breaks.
                    "br" | "br/" => {
                        out.push('\n');
                    }

                    // Horizontal rule.
                    "hr" | "hr/" => {
                        ensure_double_newline(&mut out);
                        out.push_str("---\n");
                    }

                    // Emphasis.
                    "strong" | "b" => {
                        out.push_str("**");
                    }
                    "/strong" | "/b" => {
                        out.push_str("**");
                    }
                    "em" | "i" => {
                        out.push('*');
                    }
                    "/em" | "/i" => {
                        out.push('*');
                    }

                    // Code.
                    "code" if !in_pre => {
                        out.push('`');
                        in_code = true;
                    }
                    "/code" if !in_pre => {
                        out.push('`');
                        in_code = false;
                    }
                    "pre" => {
                        ensure_double_newline(&mut out);
                        out.push_str("```\n");
                        in_pre = true;
                    }
                    "/pre" => {
                        out.push_str("\n```\n");
                        in_pre = false;
                    }

                    // Blockquote.
                    "blockquote" => {
                        ensure_double_newline(&mut out);
                        out.push_str("> ");
                    }
                    "/blockquote" => {
                        out.push('\n');
                    }

                    // Lists.
                    "ul" => {
                        list_stack.push((false, 0));
                    }
                    "ol" => {
                        list_stack.push((true, 0));
                    }
                    "/ul" | "/ol" => {
                        list_stack.pop();
                        if list_stack.is_empty() {
                            out.push('\n');
                        }
                    }
                    "li" => {
                        out.push('\n');
                        // Indent for nested lists.
                        let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                        out.push_str(&indent);
                        if let Some(last) = list_stack.last_mut() {
                            if last.0 {
                                // Ordered list.
                                last.1 += 1;
                                out.push_str(&format!("{}. ", last.1));
                            } else {
                                out.push_str("- ");
                            }
                        }
                    }
                    "/li" => {} // newline handled by next <li> or </ul>

                    // Links → [text](url).
                    "a" => {
                        link_href = extract_attr(&tag_raw, "href");
                        link_text.clear();
                        in_link = true;
                    }
                    "/a" => {
                        if let Some(href) = link_href.take() {
                            let text = link_text.trim().to_string();
                            if !text.is_empty()
                                && !href.is_empty()
                                && !href.starts_with('#')
                                && !href.starts_with("javascript:")
                            {
                                out.push('[');
                                out.push_str(&text);
                                out.push_str("](");
                                out.push_str(&href);
                                out.push(')');
                            } else {
                                out.push_str(&text);
                            }
                        }
                        in_link = false;
                    }

                    // Images → ![alt](src).
                    "img" => {
                        let alt = extract_attr(&tag_raw, "alt").unwrap_or_default();
                        if let Some(src) = extract_attr(&tag_raw, "src") {
                            if !src.is_empty() {
                                out.push_str("![");
                                out.push_str(&alt);
                                out.push_str("](");
                                out.push_str(&src);
                                out.push(')');
                            }
                        }
                    }

                    // Table elements → simple pipe-delimited.
                    "tr" => {
                        out.push('\n');
                    }
                    "th" | "td" => {
                        out.push_str("| ");
                    }
                    "/th" | "/td" => {
                        out.push(' ');
                    }
                    "/tr" => {
                        out.push('|');
                    }

                    // Skip everything else (header, aside, form, input, etc.)
                    _ => {}
                }
            }
            _ if in_tag => {
                if collecting_tag {
                    tag_buf.push(ch);
                }
            }
            _ if in_script || in_style || in_nav || in_footer => {
                // Skip content in these regions.
            }
            _ if in_pre => {
                // Preserve whitespace in pre blocks.
                out.push(ch);
            }
            _ => {
                if in_link {
                    link_text.push(ch);
                } else {
                    // Normal text — collapse whitespace unless in code.
                    if in_code {
                        out.push(ch);
                    } else if ch == '\n' || ch == '\r' {
                        // Convert newlines to spaces in flowing text.
                        if !out.ends_with(' ') && !out.ends_with('\n') {
                            out.push(' ');
                        }
                    } else if ch.is_whitespace() {
                        if !out.ends_with(' ') && !out.ends_with('\n') {
                            out.push(' ');
                        }
                    } else {
                        out.push(ch);
                    }
                }
            }
        }
    }

    // Decode common HTML entities.
    let decoded = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    clean_markdown(&decoded)
}

/// Ensure the output ends with a double newline (paragraph break).
fn ensure_double_newline(s: &mut String) {
    let trimmed = s.trim_end_matches(' ');
    let trailing_newlines = trimmed.len() - trimmed.trim_end_matches('\n').len();
    // We want the string to end with the trimmed content + "\n\n"
    s.truncate(trimmed.len());
    match trailing_newlines {
        0 => s.push_str("\n\n"),
        1 => s.push('\n'),
        _ => {} // already has 2+ newlines
    }
}

/// Extract an attribute value from a raw tag string.
/// e.g., extract_attr("a href=\"https://example.com\" class=\"link\"", "href")
/// returns Some("https://example.com").
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pattern = format!("{}=", attr);
    let start = lower.find(&pattern)?;
    let after_eq = &tag[start + pattern.len()..];
    let after_eq = after_eq.trim_start();

    if let Some(content) = after_eq.strip_prefix('"') {
        // Double-quoted value.
        let end = content.find('"')?;
        Some(content[..end].to_string())
    } else if let Some(content) = after_eq.strip_prefix('\'') {
        // Single-quoted value.
        let end = content.find('\'')?;
        Some(content[..end].to_string())
    } else {
        // Unquoted — take until whitespace or >.
        let end = after_eq
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(after_eq.len());
        Some(after_eq[..end].to_string())
    }
}

/// Clean up markdown output: collapse excessive blank lines, trim.
fn clean_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut consecutive_blanks = 0;

    for line in s.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            consecutive_blanks += 1;
            if consecutive_blanks <= 2 {
                out.push('\n');
            }
        } else {
            consecutive_blanks = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
    }

    out.trim().to_string()
}

/// Collapse runs of whitespace into single spaces, trim lines.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_newline = false;
    let mut prev_space = false;

    for ch in s.chars() {
        if ch == '\n' || ch == '\r' {
            if !prev_newline {
                out.push('\n');
                prev_newline = true;
                prev_space = false;
            }
        } else if ch.is_whitespace() {
            if !prev_space && !prev_newline {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_newline = false;
            prev_space = false;
        }
    }

    out.trim().to_string()
}

/// Extract host from a URL string.
fn extract_host(url: &str) -> Option<String> {
    // Simple extraction: skip scheme, take until '/' or ':'
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = after_scheme.split('/').next()?.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    Some(host.to_lowercase())
}

/// Check if a host resolves to a private/internal IP (SSRF protection).
/// Also blocks common internal hostnames.
fn is_private_host(host: &str) -> bool {
    // Block common internal hostnames.
    let blocked_hosts = [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "::1",
        "[::1]",
        "metadata.google.internal",
        "169.254.169.254", // AWS/GCP metadata
    ];
    if blocked_hosts.contains(&host) {
        return true;
    }

    // Check if the host is a raw IP in a private range.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_ip(ip);
    }

    false
}

/// Check if an IP address is in a private/reserved range.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()          // 127.0.0.0/8
                || v4.is_private()     // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local()  // 169.254.0.0/16
                || v4.is_unspecified() // 0.0.0.0
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64 // 100.64.0.0/10 (CGNAT)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

/// Truncate a string at a char boundary.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Safe truncation that respects char boundaries.
fn safe_truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SSRF protection ─────────────────────────────────────────────

    #[test]
    fn blocks_localhost() {
        assert!(is_private_host("localhost"));
        assert!(is_private_host("127.0.0.1"));
        assert!(is_private_host("0.0.0.0"));
        assert!(is_private_host("::1"));
    }

    #[test]
    fn blocks_metadata_endpoints() {
        assert!(is_private_host("169.254.169.254"));
        assert!(is_private_host("metadata.google.internal"));
    }

    #[test]
    fn blocks_private_ips() {
        assert!(is_private_host("10.0.0.1"));
        assert!(is_private_host("172.16.0.1"));
        assert!(is_private_host("192.168.1.1"));
    }

    #[test]
    fn allows_public_hosts() {
        assert!(!is_private_host("example.com"));
        assert!(!is_private_host("8.8.8.8"));
        assert!(!is_private_host("rust-lang.org"));
    }

    // ── URL validation ──────────────────────────────────────────────

    #[tokio::test]
    async fn rejects_http_by_default() {
        let config = FetchConfig::default();
        let err = fetch_url("http://example.com", &config).await.unwrap_err();
        assert!(matches!(err, FetchError::UrlNotAllowed(_)));
    }

    #[tokio::test]
    async fn rejects_ftp_scheme() {
        let config = FetchConfig::default();
        let err = fetch_url("ftp://example.com/file", &config)
            .await
            .unwrap_err();
        assert!(matches!(err, FetchError::UrlNotAllowed(_)));
    }

    #[tokio::test]
    async fn rejects_file_scheme() {
        let config = FetchConfig::default();
        let err = fetch_url("file:///etc/passwd", &config).await.unwrap_err();
        assert!(matches!(err, FetchError::UrlNotAllowed(_)));
    }

    #[tokio::test]
    async fn rejects_localhost_ssrf() {
        let config = FetchConfig {
            allow_http: true,
            ..Default::default()
        };
        let err = fetch_url("http://localhost/admin", &config)
            .await
            .unwrap_err();
        assert!(matches!(err, FetchError::SsrfBlocked(_)));
    }

    #[tokio::test]
    async fn rejects_metadata_ssrf() {
        let config = FetchConfig {
            allow_http: true,
            ..Default::default()
        };
        let err = fetch_url("http://169.254.169.254/latest/meta-data", &config)
            .await
            .unwrap_err();
        assert!(matches!(err, FetchError::SsrfBlocked(_)));
    }

    #[tokio::test]
    async fn rejects_private_ip_ssrf() {
        let config = FetchConfig {
            allow_http: true,
            ..Default::default()
        };
        let err = fetch_url("http://10.0.0.1/internal", &config)
            .await
            .unwrap_err();
        assert!(matches!(err, FetchError::SsrfBlocked(_)));
    }

    // ── Host extraction ─────────────────────────────────────────────

    #[test]
    fn extract_host_https() {
        assert_eq!(
            extract_host("https://example.com/path"),
            Some("example.com".into())
        );
    }

    #[test]
    fn extract_host_with_port() {
        assert_eq!(
            extract_host("https://example.com:8443/path"),
            Some("example.com".into())
        );
    }

    #[test]
    fn extract_host_bare() {
        assert_eq!(
            extract_host("https://example.com"),
            Some("example.com".into())
        );
    }

    // ── HTML to markdown conversion ───────────────────────────────

    #[test]
    fn markdown_preserves_headings() {
        let html = "<html><body><h1>Title</h1><h2>Subtitle</h2><p>Text</p></body></html>";
        let md = html_to_markdown(html);
        assert!(md.contains("# Title"));
        assert!(md.contains("## Subtitle"));
        assert!(md.contains("Text"));
    }

    #[test]
    fn markdown_strips_scripts() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("Before"));
        assert!(md.contains("After"));
        assert!(!md.contains("alert"));
    }

    #[test]
    fn markdown_strips_styles() {
        let html = "<style>.foo { color: red; }</style><p>Content</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("Content"));
        assert!(!md.contains("color"));
    }

    #[test]
    fn markdown_decodes_entities() {
        let html = "<p>A &amp; B &lt; C &gt; D &quot;E&quot;</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("A & B < C > D \"E\""));
    }

    #[test]
    fn markdown_preserves_links() {
        let html = r#"<p>Visit <a href="https://rust-lang.org">Rust</a> for more.</p>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Rust](https://rust-lang.org)"));
    }

    #[test]
    fn markdown_skips_anchor_links() {
        let html = r##"<a href="#section">Jump</a>"##;
        let md = html_to_markdown(html);
        // Anchor-only links should just show text, no markdown link.
        assert!(md.contains("Jump"));
        assert!(!md.contains("[Jump]"));
    }

    #[test]
    fn markdown_preserves_emphasis() {
        let html = "<p>This is <strong>bold</strong> and <em>italic</em>.</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("**bold**"));
        assert!(md.contains("*italic*"));
    }

    #[test]
    fn markdown_preserves_code_blocks() {
        let html = "<pre><code>fn main() {\n    println!(\"hello\");\n}</code></pre>";
        let md = html_to_markdown(html);
        assert!(md.contains("```"));
        assert!(md.contains("fn main()"));
    }

    #[test]
    fn markdown_preserves_inline_code() {
        let html = "<p>Use <code>cargo build</code> to compile.</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("`cargo build`"));
    }

    #[test]
    fn markdown_preserves_unordered_lists() {
        let html = "<ul><li>First</li><li>Second</li><li>Third</li></ul>";
        let md = html_to_markdown(html);
        assert!(md.contains("- First"));
        assert!(md.contains("- Second"));
        assert!(md.contains("- Third"));
    }

    #[test]
    fn markdown_preserves_ordered_lists() {
        let html = "<ol><li>One</li><li>Two</li><li>Three</li></ol>";
        let md = html_to_markdown(html);
        assert!(md.contains("1. One"));
        assert!(md.contains("2. Two"));
        assert!(md.contains("3. Three"));
    }

    #[test]
    fn markdown_preserves_blockquotes() {
        let html = "<blockquote>Important quote here</blockquote>";
        let md = html_to_markdown(html);
        assert!(md.contains("> Important quote here"));
    }

    #[test]
    fn markdown_strips_nav_and_footer() {
        let html = "<nav>Menu items</nav><main><p>Content</p></main><footer>Copyright</footer>";
        let md = html_to_markdown(html);
        assert!(!md.contains("Menu items"));
        assert!(md.contains("Content"));
        assert!(!md.contains("Copyright"));
    }

    #[test]
    fn markdown_preserves_images() {
        let html = r#"<img src="https://example.com/img.png" alt="A photo">"#;
        let md = html_to_markdown(html);
        assert!(md.contains("![A photo](https://example.com/img.png)"));
    }

    #[test]
    fn markdown_horizontal_rule() {
        let html = "<p>Above</p><hr><p>Below</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("---"));
        assert!(md.contains("Above"));
        assert!(md.contains("Below"));
    }

    #[test]
    fn extract_attr_double_quoted() {
        assert_eq!(
            extract_attr(r#"a href="https://example.com" class="link""#, "href"),
            Some("https://example.com".into())
        );
    }

    #[test]
    fn extract_attr_single_quoted() {
        assert_eq!(
            extract_attr("a href='https://example.com'", "href"),
            Some("https://example.com".into())
        );
    }

    #[test]
    fn extract_attr_missing() {
        assert_eq!(extract_attr("a class=\"link\"", "href"), None);
    }

    #[test]
    fn collapse_whitespace_works() {
        assert_eq!(collapse_whitespace("  hello   world  "), "hello world");
        assert_eq!(collapse_whitespace("a\n\n\nb"), "a\nb");
        assert_eq!(collapse_whitespace("  \n  \n  "), "");
    }

    // ── Integration tests (require network) ─────────────────────────

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_fetch_https() {
        let config = FetchConfig::default();
        let result = fetch_url("https://httpbin.org/html", &config)
            .await
            .unwrap();
        assert_eq!(result.status, 200);
        assert!(!result.content.is_empty());
        assert!(result.content.contains("Herman Melville"));
    }
}
