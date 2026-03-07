//! Live integration tests for all web tools.
//! Run with: cargo run -p ghost-agent-loop --example live_tool_tests

use ghost_agent_loop::tools::builtin::http_request::{http_request, HttpRequestConfig};
use ghost_agent_loop::tools::builtin::web_fetch::{fetch_url, FetchConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let mut passed = 0u32;
    let mut failed = 0u32;

    // ── web_fetch tests ─────────────────────────────────────────────

    // 1. Markdown conversion on a real docs page
    print_test("web_fetch: rust-lang.org markdown headings");
    let cfg = FetchConfig {
        max_text_chars: 8000,
        ..Default::default()
    };
    match fetch_url("https://www.rust-lang.org", &cfg).await {
        Ok(r) if r.status == 200 && r.content.contains("# ") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(
                &mut failed,
                &format!(
                    "status={}, has heading={}",
                    r.status,
                    r.content.contains("# ")
                ),
            );
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 2. Links preserved as markdown
    print_test("web_fetch: example.com preserves links");
    match fetch_url("https://example.com", &FetchConfig::default()).await {
        Ok(r) if r.content.contains("](") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(
                &mut failed,
                &format!(
                    "no markdown links found in: {}",
                    &r.content[..200.min(r.content.len())]
                ),
            );
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 3. JSON passthrough (not converted to markdown)
    print_test("web_fetch: JSON content returned as-is");
    match fetch_url("https://httpbin.org/json", &FetchConfig::default()).await {
        Ok(r) if r.content.contains("\"slideshow\"") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(
                &mut failed,
                &format!(
                    "JSON not preserved: {}",
                    &r.content[..200.min(r.content.len())]
                ),
            );
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 4. SSRF protection blocks localhost
    print_test("web_fetch: SSRF blocks localhost");
    match fetch_url("https://localhost/evil", &FetchConfig::default()).await {
        Err(_) => {
            ok(&mut passed);
        }
        Ok(_) => {
            fail(&mut failed, "should have been blocked");
        }
    }

    // 5. SSRF protection blocks cloud metadata
    print_test("web_fetch: SSRF blocks metadata endpoint");
    let cfg_http = FetchConfig {
        allow_http: true,
        ..Default::default()
    };
    match fetch_url("http://169.254.169.254/latest/meta-data", &cfg_http).await {
        Err(_) => {
            ok(&mut passed);
        }
        Ok(_) => {
            fail(&mut failed, "should have been blocked");
        }
    }

    // 6. HTTP rejected by default
    print_test("web_fetch: HTTP rejected by default");
    match fetch_url("http://example.com", &FetchConfig::default()).await {
        Err(_) => {
            ok(&mut passed);
        }
        Ok(_) => {
            fail(&mut failed, "HTTP should be rejected");
        }
    }

    // 7. Truncation works on large pages
    print_test("web_fetch: truncation on large content");
    let small_cfg = FetchConfig {
        max_text_chars: 500,
        ..Default::default()
    };
    match fetch_url("https://www.rust-lang.org", &small_cfg).await {
        Ok(r) if r.truncated && r.content.contains("[Content truncated]") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(
                &mut failed,
                &format!("truncated={}, len={}", r.truncated, r.content_length),
            );
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // ── http_request tests ──────────────────────────────────────────

    // 8. GET with response inspection
    print_test("http_request: GET returns JSON");
    let hcfg = HttpRequestConfig::default();
    let headers = HashMap::new();
    match http_request("https://httpbin.org/get", "GET", &headers, None, &hcfg).await {
        Ok(r) if r.status == 200 && r.body.contains("httpbin.org") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(&mut failed, &format!("status={}", r.status));
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 9. POST with JSON body
    print_test("http_request: POST JSON echoed back");
    let mut h = HashMap::new();
    h.insert("Content-Type".into(), "application/json".into());
    let body = r#"{"agent":"ghost","version":1}"#;
    match http_request("https://httpbin.org/post", "POST", &h, Some(body), &hcfg).await {
        Ok(r) if r.status == 200 && r.body.contains("ghost") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(
                &mut failed,
                &format!("status={}, body missing ghost", r.status),
            );
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 10. Custom headers sent correctly
    print_test("http_request: custom headers received by server");
    let mut h = HashMap::new();
    h.insert("X-Ghost-Agent".into(), "test-run-42".into());
    match http_request("https://httpbin.org/headers", "GET", &h, None, &hcfg).await {
        Ok(r) if r.body.contains("test-run-42") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(
                &mut failed,
                &format!("header not echoed: {}", &r.body[..300.min(r.body.len())]),
            );
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 11. PUT request
    print_test("http_request: PUT works");
    match http_request(
        "https://httpbin.org/put",
        "PUT",
        &HashMap::new(),
        Some("update"),
        &hcfg,
    )
    .await
    {
        Ok(r) if r.status == 200 => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(&mut failed, &format!("status={}", r.status));
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 12. DELETE request
    print_test("http_request: DELETE works");
    match http_request(
        "https://httpbin.org/delete",
        "DELETE",
        &HashMap::new(),
        None,
        &hcfg,
    )
    .await
    {
        Ok(r) if r.status == 200 => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(&mut failed, &format!("status={}", r.status));
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 13. PATCH request
    print_test("http_request: PATCH works");
    match http_request(
        "https://httpbin.org/patch",
        "PATCH",
        &HashMap::new(),
        Some("{}"),
        &hcfg,
    )
    .await
    {
        Ok(r) if r.status == 200 => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(&mut failed, &format!("status={}", r.status));
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 14. Domain allowlist blocks unauthorized domains
    print_test("http_request: domain allowlist blocks");
    let restricted = HttpRequestConfig {
        allowed_domains: vec!["api.github.com".into()],
        ..Default::default()
    };
    match http_request(
        "https://evil.com/steal",
        "GET",
        &HashMap::new(),
        None,
        &restricted,
    )
    .await
    {
        Err(_) => {
            ok(&mut passed);
        }
        Ok(_) => {
            fail(&mut failed, "should have been blocked by allowlist");
        }
    }

    // 15. Method restriction works
    print_test("http_request: disallowed method blocked");
    let get_only = HttpRequestConfig {
        allowed_methods: vec!["GET".into()],
        ..Default::default()
    };
    match http_request(
        "https://httpbin.org/post",
        "POST",
        &HashMap::new(),
        None,
        &get_only,
    )
    .await
    {
        Err(_) => {
            ok(&mut passed);
        }
        Ok(_) => {
            fail(&mut failed, "POST should be blocked");
        }
    }

    // 16. SSRF protection in http_request
    print_test("http_request: SSRF blocks private IPs");
    let cfg_http = HttpRequestConfig {
        allow_http: true,
        ..Default::default()
    };
    match http_request(
        "http://10.0.0.1/internal",
        "GET",
        &HashMap::new(),
        None,
        &cfg_http,
    )
    .await
    {
        Err(_) => {
            ok(&mut passed);
        }
        Ok(_) => {
            fail(&mut failed, "should have been blocked");
        }
    }

    // 17. Host header override blocked (SSRF vector)
    print_test("http_request: Host header silently stripped");
    let mut h = HashMap::new();
    h.insert("Host".into(), "evil.com".into());
    // Should still reach httpbin.org, not evil.com
    match http_request("https://httpbin.org/get", "GET", &h, None, &hcfg).await {
        Ok(r) if r.status == 200 && r.body.contains("httpbin.org") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(&mut failed, &format!("unexpected: status={}", r.status));
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 18. Response headers captured
    print_test("http_request: safe response headers captured");
    match http_request(
        "https://httpbin.org/get",
        "GET",
        &HashMap::new(),
        None,
        &hcfg,
    )
    .await
    {
        Ok(r) if r.headers.contains_key("content-type") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(&mut failed, &format!("headers: {:?}", r.headers));
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // 19. Oversized request body rejected
    print_test("http_request: oversized body rejected");
    let tiny = HttpRequestConfig {
        max_request_bytes: 10,
        ..Default::default()
    };
    match http_request(
        "https://httpbin.org/post",
        "POST",
        &HashMap::new(),
        Some(&"x".repeat(100)),
        &tiny,
    )
    .await
    {
        Err(_) => {
            ok(&mut passed);
        }
        Ok(_) => {
            fail(&mut failed, "should reject oversized body");
        }
    }

    // 20. Auto content-type detection for JSON body
    print_test("http_request: auto-detects JSON content-type");
    match http_request(
        "https://httpbin.org/post",
        "POST",
        &HashMap::new(),
        Some(r#"{"auto":"detect"}"#),
        &hcfg,
    )
    .await
    {
        Ok(r) if r.body.contains("application/json") => {
            ok(&mut passed);
        }
        Ok(r) => {
            fail(
                &mut failed,
                &format!(
                    "content-type not auto-detected in: {}",
                    &r.body[..400.min(r.body.len())]
                ),
            );
        }
        Err(e) => {
            fail(&mut failed, &e.to_string());
        }
    }

    // ── Summary ─────────────────────────────────────────────────────
    println!("\n{}", "=".repeat(50));
    println!(
        "RESULTS: {} passed, {} failed, {} total",
        passed,
        failed,
        passed + failed
    );
    if failed > 0 {
        std::process::exit(1);
    } else {
        println!("All live tests passed.");
    }
}

fn print_test(name: &str) {
    print!("  {:.<55} ", name);
}
fn ok(passed: &mut u32) {
    *passed += 1;
    println!("OK");
}
fn fail(failed: &mut u32, msg: &str) {
    *failed += 1;
    println!("FAIL: {}", msg);
}
