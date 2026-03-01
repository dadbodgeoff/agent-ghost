//! SSRF prevention for user-configurable URLs (T-5.5.1, T-5.5.2).
//!
//! Validates URLs against a blocklist of private/internal IP ranges to prevent
//! Server-Side Request Forgery attacks via webhooks, A2A targets, and
//! custom safety check URLs.
//!
//! Blocked ranges: 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16,
//! 169.254.0.0/16, ::1, fc00::/7, and unspecified addresses.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

/// Check if an IP address is in a private/internal range.
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()              // 127.0.0.0/8
                || v4.is_private()        // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local()     // 169.254.0.0/16
                || v4.is_unspecified()    // 0.0.0.0
                || is_v4_shared(v4)       // 100.64.0.0/10 (CGNAT)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()              // ::1
                || v6.is_unspecified()    // ::
                || is_v6_unique_local(v6) // fc00::/7
                || is_v6_link_local(v6)   // fe80::/10
        }
    }
}

/// Check IPv4 100.64.0.0/10 (Shared/CGNAT).
fn is_v4_shared(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0xC0) == 64
}

/// Check IPv6 unique-local (fc00::/7).
fn is_v6_unique_local(ip: &Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xFE00) == 0xFC00
}

/// Check IPv6 link-local (fe80::/10).
fn is_v6_link_local(ip: &Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xFFC0) == 0xFE80
}

/// Validate a URL against the SSRF blocklist.
///
/// Returns `Ok(())` if the URL is safe to request, or an error string describing
/// the violation.
///
/// Checks:
/// 1. Only `http://` and `https://` schemes allowed
/// 2. URL must have a non-empty host
/// 3. Resolved IP addresses must not be in private/internal ranges
/// 4. DNS rebinding protection: ALL resolved IPs are checked
pub fn validate_url(url_str: &str) -> Result<(), String> {
    // 1. Parse URL.
    let parsed = url::Url::parse(url_str)
        .map_err(|e| format!("Invalid URL: {e}"))?;

    // 2. Only allow http/https schemes.
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("URL scheme '{scheme}' not allowed — only http and https")),
    }

    // 3. Must have a host.
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL must have a host".to_string())?;

    if host.is_empty() {
        return Err("URL host is empty".to_string());
    }

    // 4. Check allowed hosts override.
    if let Ok(allowed) = std::env::var("GHOST_WEBHOOK_ALLOWED_HOSTS") {
        let allowed_hosts: Vec<&str> = allowed.split(',').map(|h| h.trim()).collect();
        if allowed_hosts.contains(&host) {
            return Ok(());
        }
    }

    // 5. Resolve hostname and check all IPs against blocklist.
    // Use port 443 as default for resolution (the actual port doesn't matter for IP check).
    let port = parsed.port().unwrap_or(443);
    let addr_str = format!("{host}:{port}");

    match addr_str.to_socket_addrs() {
        Ok(addrs) => {
            for addr in addrs {
                if is_private_ip(&addr.ip()) {
                    return Err(format!(
                        "URL resolves to private/internal IP {} — SSRF blocked",
                        addr.ip()
                    ));
                }
            }
        }
        Err(e) => {
            // If DNS resolution fails, allow the URL through — the HTTP client
            // will fail at connection time. We don't want to block URLs that
            // resolve to IPs not yet available (e.g., during deployment).
            tracing::debug!(url = url_str, error = %e, "SSRF check: DNS resolution failed, allowing URL");
        }
    }

    Ok(())
}
