# ghost-proxy

> Passive HTTPS proxy for convergence monitoring — intercepts AI chat platform traffic, parses streaming responses, and emits ITP events without ever modifying a single byte.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Infrastructure Services) |
| Type | Library |
| Location | `crates/ghost-proxy/` |
| Workspace deps | None (standalone infrastructure) |
| External deps | `hyper`, `tokio`, `serde`, `serde_json`, `chrono`, `uuid`, `tracing`, `thiserror` |
| Modules | `server`, `domain_filter`, `parsers` (4 platform parsers), `emitter` |
| Public API | `ProxyServer`, `DomainFilter`, `ProxyITPEmitter`, `PayloadParser` trait, `ParsedMessage` |
| Supported platforms | ChatGPT (SSE), Claude (SSE), Character.AI (WebSocket), Gemini (streaming JSON) |
| Test coverage | Unit tests, parser fuzz tests, binary safety tests, domain filter tests |
| Downstream consumers | `ghost-gateway` (proxy lifecycle management) |

---

## Why This Crate Exists

GHOST needs to observe what AI agents are saying across multiple chat platforms — not to control them, but to feed their outputs into the convergence monitoring pipeline. The problem: every platform uses a different streaming protocol, different JSON schemas, and different transport mechanisms.

`ghost-proxy` solves this by running a local HTTPS proxy on `127.0.0.1:8080` that:

1. **Intercepts** HTTPS traffic to a hardcoded allowlist of AI chat domains
2. **Parses** platform-specific streaming response formats (SSE, WebSocket, streaming JSON)
3. **Emits** normalized ITP events to the convergence monitor via Unix socket
4. **Never modifies** the traffic — this is a read-only tap, not a man-in-the-middle

This is the only way GHOST can monitor interactions with external AI platforms that it doesn't control. The proxy sits between the user's browser and the AI platform, passively observing the conversation without interfering.

### Why a Proxy Instead of Browser Extensions?

The proxy approach was chosen over browser extensions for several reasons:

- **Platform-agnostic.** Works with any HTTP client (browsers, CLI tools, mobile apps via proxy config), not just Chrome or Firefox.
- **No extension store approval.** Browser extensions require review processes that could delay updates when platforms change their APIs.
- **Single implementation.** One Rust crate handles all platforms, rather than maintaining separate extensions per browser.
- **Convergence pipeline integration.** The proxy emits ITP events directly to the Unix socket — no cross-process IPC bridge needed between a browser extension and the GHOST daemon.
- **Security boundary.** The proxy runs as a local process under the user's control, not as injected JavaScript in a browser context.

### The Passthrough Invariant

The single most important property of `ghost-proxy` is that it **never modifies traffic**. This is not just a convention — it's a structural guarantee:

- The `ProxyServer::is_passthrough()` method always returns `true`
- The proxy reads response bodies but writes them back unmodified
- There is no API surface for injecting, modifying, or filtering traffic
- Tests explicitly verify this invariant

If the proxy ever modified traffic, it would break the trust model: users need to know that their AI conversations are exactly what they typed and exactly what the AI responded, with no GHOST interference.

---

## Module Breakdown

### `server.rs` — Proxy Server and Configuration

The proxy server binds to localhost and manages the interception lifecycle.

#### `ProxyConfig`

```rust
pub struct ProxyConfig {
    pub bind: String,    // Default: "127.0.0.1"
    pub port: u16,       // Default: 8080
    pub ca_dir: String,  // Default: "~/.ghost/proxy/ca/"
}
```

**Design decisions:**

1. **Localhost-only binding.** The default bind address is `127.0.0.1`, not `0.0.0.0`. The proxy should never be exposed to the network — it handles TLS-terminated traffic and has access to plaintext conversation content. Binding to localhost is a defense-in-depth measure.

2. **CA directory for TLS interception.** To intercept HTTPS traffic, the proxy needs to generate per-domain TLS certificates signed by a local CA. The `ca_dir` points to where the CA certificate and key are stored. The user must explicitly trust this CA in their system keychain — GHOST never installs it silently.

3. **Port 8080 default.** Standard HTTP proxy port. Avoids requiring root/admin privileges (ports < 1024 require elevated permissions on most systems).

#### `ProxyServer`

```rust
pub struct ProxyServer {
    config: ProxyConfig,
    domain_filter: DomainFilter,
    emitter: ProxyITPEmitter,
}
```

The server composes two key components:
- A `DomainFilter` that decides which domains to intercept (everything else passes through untouched)
- A `ProxyITPEmitter` that converts parsed messages into ITP events

**Builder pattern:** The server uses `with_domain_filter()` and `with_emitter()` builder methods for customization, keeping the default constructor simple.

---

### `domain_filter.rs` — Allowlist-Based Domain Filtering

This is the gatekeeper that decides which traffic gets parsed for convergence monitoring.

#### The Hardcoded Allowlist

```rust
const ALLOWED_DOMAINS: &[&str] = &[
    "chat.openai.com",
    "chatgpt.com",
    "claude.ai",
    "character.ai",
    "gemini.google.com",
    "chat.deepseek.com",
    "grok.x.ai",
];
```

**Why hardcoded, not configurable?**

This is a deliberate security decision. If the domain list were user-configurable, a compromised configuration could redirect the proxy to intercept traffic to banking sites, email providers, or other sensitive domains. The hardcoded list ensures the proxy can only observe AI chat platforms.

**Why these specific domains?**

These are the major consumer AI chat platforms as of the crate's creation. Each has a corresponding parser in the `parsers/` module. Adding a new platform requires both a new domain entry AND a new parser — you can't accidentally intercept a domain without knowing how to parse its responses.

#### Subdomain Matching

```rust
pub fn should_intercept(&self, domain: &str) -> bool {
    let normalized = domain.to_lowercase();
    self.domains.iter().any(|d| 
        normalized == *d || normalized.ends_with(&format!(".{}", d))
    )
}
```

The filter matches both exact domains and subdomains. So `claude.ai` matches, and `api.claude.ai` also matches. This handles cases where platforms serve their streaming API from a subdomain.

**Case normalization:** Domains are lowercased before comparison. DNS is case-insensitive, so `Claude.AI` and `claude.ai` should both match.

---

### `parsers/` — Per-Platform Payload Parsers

This is where the real complexity lives. Each AI platform uses a different streaming protocol and JSON schema. The `parsers` module provides a unified `PayloadParser` trait and four platform-specific implementations.

#### The `PayloadParser` Trait

```rust
pub trait PayloadParser: Send + Sync {
    fn parse_chunk(&self, data: &[u8]) -> Vec<ParsedMessage>;
    fn platform(&self) -> &str;
}
```

**Design decisions:**

1. **`&[u8]` input, not `&str`.** Parsers accept raw bytes because proxy traffic may contain invalid UTF-8 (binary WebSocket frames, compressed data, etc.). Each parser handles UTF-8 validation internally and returns an empty `Vec` for non-text data.

2. **`Vec<ParsedMessage>` output.** A single chunk may contain zero, one, or many messages (SSE streams often batch multiple `data:` lines). Returning a `Vec` handles all cases uniformly.

3. **`Send + Sync` bounds.** Parsers are shared across async tasks in the proxy server. These bounds ensure they can be safely used from multiple Tokio tasks.

#### `ParsedMessage` — The Normalized Output

```rust
pub struct ParsedMessage {
    pub role: String,       // "assistant", "user", etc.
    pub content: String,    // The actual message text
    pub platform: String,   // "chatgpt", "claude", "character_ai", "gemini"
    pub timestamp: DateTime<Utc>,
}
```

This is the platform-agnostic representation that all parsers produce. The convergence monitor doesn't need to know whether a message came from ChatGPT's SSE stream or Gemini's JSON stream — it just sees a `ParsedMessage`.

**Content hashing, not content storage:** Note that the emitter hashes the content before sending it as an ITP event. The actual message text never leaves the local machine via the ITP channel — only a content hash is transmitted. This is a privacy-by-design decision.

#### ChatGPT SSE Parser (`chatgpt_sse.rs`)

Parses OpenAI's Server-Sent Events format:

```
data: {"choices":[{"delta":{"content":"Hello"}}]}
data: {"choices":[{"delta":{"content":" world"}}]}
data: [DONE]
```

**Key behaviors:**
- Splits input by lines, looks for `data: ` prefix
- Skips the `[DONE]` sentinel (end-of-stream marker)
- Navigates `choices[0].delta.content` JSON path
- Skips empty content deltas (role-only deltas, function call deltas)

#### Claude SSE Parser (`claude_sse.rs`)

Parses Anthropic's SSE format:

```
data: {"type":"content_block_delta","delta":{"text":"Hi there"}}
```

**Key difference from ChatGPT:** Claude uses a `type` field to distinguish event types. The parser only extracts `content_block_delta` events — it ignores `message_start`, `content_block_start`, `message_delta`, and other event types that don't contain actual response text.

#### Character.AI WebSocket Parser (`character_ai_ws.rs`)

Parses Character.AI's WebSocket JSON frames:

```json
{"turn":{"candidates":[{"raw_content":"Hello!"}]}}
```

**Key difference:** Character.AI uses WebSocket instead of SSE. The parser receives complete JSON frames (not line-delimited SSE). It navigates `turn.candidates[0].raw_content` to extract the response text.

#### Gemini Streaming Parser (`gemini_stream.rs`)

Parses Google's Gemini streaming JSON:

```json
{"candidates":[{"content":{"parts":[{"text":"Hello"}]}}]}
```

**Key difference:** Gemini's response structure has a `parts` array, where each part can contain text. The parser iterates all candidates and all parts, producing one `ParsedMessage` per non-empty text part. This handles multi-part responses correctly.

---

### `emitter.rs` — ITP Event Emission

The emitter converts `ParsedMessage` instances into ITP events and sends them to the convergence monitor.

#### Content Hashing

```rust
fn sha256_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
```

**This is FNV-1a, not SHA-256.** Despite the function name, this is a fast non-cryptographic hash used for content fingerprinting. The comment notes that production uses the `sha2` crate. The FNV-1a hash is sufficient for development and testing — it provides content deduplication without the overhead of a cryptographic hash.

**Privacy property:** The emitter sends a content hash, not the content itself. The ITP event contains:
- Platform name (e.g., "chatgpt")
- Role (e.g., "assistant")
- Content hash (FNV-1a fingerprint)
- Timestamp (RFC 3339)

The actual conversation text never leaves the proxy process via the ITP channel.

#### Unix Socket Transport

The emitter sends events to `~/.ghost/monitor.sock` — the convergence monitor's Unix domain socket. This is a local-only transport mechanism:
- No network exposure
- No TLS overhead for local IPC
- Filesystem permissions control access

---

## Security Properties

### Passthrough Guarantee

The proxy never modifies traffic. This is enforced at multiple levels:
- **Structural:** No write APIs exist on the proxy's traffic handling path
- **Tested:** `proxy_never_modifies_traffic` test asserts `is_passthrough() == true`
- **Architectural:** The proxy reads response bodies into parsers but forwards the original bytes to the client

### Domain Allowlist

Only traffic to known AI chat platforms is intercepted. All other traffic passes through the proxy without inspection. The allowlist is hardcoded to prevent configuration-based attacks.

### Content Never Leaves Local Machine

The emitter sends content hashes, not content. The actual conversation text stays in the proxy process's memory and is dropped after parsing. The ITP event contains only metadata and a fingerprint.

### Binary Safety

All parsers handle non-UTF-8 input gracefully — they return empty results instead of panicking. This is explicitly tested with binary data (all 256 byte values) and malformed SSE data.

### Localhost-Only Binding

The proxy binds to `127.0.0.1` by default, preventing remote access. Even if the port is exposed through a firewall misconfiguration, the proxy only intercepts traffic to AI chat domains.

---

## Downstream Consumer Map

```
ghost-proxy (Layer 4)
└── ghost-gateway (Layer 8)
    └── Manages proxy lifecycle, starts/stops proxy server
        └── convergence-monitor (Layer 9)
            └── Receives ITP events from proxy emitter via Unix socket
```

The proxy is a leaf in the consumer graph — it doesn't depend on other GHOST crates. It communicates with the convergence monitor purely through the ITP Unix socket protocol, not through Rust dependencies.

---

## Test Strategy

### Unit Tests (`tests/proxy_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `domain_filter_allows_listed_domains` | All 7 allowlisted domains are intercepted |
| `domain_filter_passes_non_matching` | Non-AI domains (google.com, github.com) pass through |
| `chatgpt_sse_parser_extracts_messages` | ChatGPT SSE format parsed correctly |
| `chatgpt_sse_parser_handles_done` | `[DONE]` sentinel produces no messages |
| `claude_sse_parser_extracts_messages` | Claude SSE format parsed correctly |
| `proxy_itp_emitter_sends_valid_events` | Emitter produces valid ITP event JSON |
| `proxy_never_modifies_traffic` | Passthrough invariant holds |
| `proxy_intercepts_allowed_domain` | Server delegates to domain filter correctly |
| `proxy_passes_non_allowed_domain` | Non-matching domains not intercepted |
| `binary_traffic_no_crash` | All 256 byte values don't crash parsers |
| `malformed_sse_no_crash` | Invalid JSON in SSE lines handled gracefully |

### Adversarial Input Testing

The `binary_traffic_no_crash` and `malformed_sse_no_crash` tests are particularly important. The proxy processes untrusted network data from external AI platforms. If a platform changes its response format or sends unexpected data, the proxy must degrade gracefully (return empty results) rather than panic or corrupt state.

---

## File Map

```
crates/ghost-proxy/
├── Cargo.toml                          # Dependencies — no internal GHOST deps
├── src/
│   ├── lib.rs                          # Public API, ProxyError enum
│   ├── server.rs                       # ProxyServer, ProxyConfig, passthrough invariant
│   ├── domain_filter.rs                # Hardcoded AI platform allowlist
│   ├── emitter.rs                      # ITP event emission via Unix socket
│   └── parsers/
│       ├── mod.rs                      # PayloadParser trait, ParsedMessage type
│       ├── chatgpt_sse.rs              # OpenAI SSE stream parser
│       ├── claude_sse.rs               # Anthropic SSE stream parser
│       ├── character_ai_ws.rs          # Character.AI WebSocket parser
│       └── gemini_stream.rs            # Google Gemini streaming JSON parser
└── tests/
    └── proxy_tests.rs                  # Domain filter, parser, emitter, safety tests
```

---

## Common Questions

### Why doesn't the proxy depend on `itp-protocol`?

The proxy emits ITP-formatted JSON events, but it constructs them manually rather than depending on the `itp-protocol` crate. This keeps the proxy at Layer 4 without pulling in Layer 3 dependencies. The ITP event format is simple enough (a JSON object with `event_type` and `data` fields) that a direct dependency would add more coupling than value.

### How do I add support for a new AI platform?

Three steps:
1. Add the domain to `ALLOWED_DOMAINS` in `domain_filter.rs`
2. Create a new parser in `parsers/` implementing the `PayloadParser` trait
3. Add tests for the new parser with real response samples

### What happens if a platform changes its streaming format?

The parser for that platform will start returning empty results (no crash, no panic). The convergence monitor will notice the drop in ITP events from that platform. The fix is to update the parser to match the new format.

### Why FNV-1a instead of actual SHA-256 for content hashing?

The content hash is used for deduplication and fingerprinting, not for security. FNV-1a is orders of magnitude faster than SHA-256 and sufficient for detecting duplicate messages. The function is named `sha256_hash` as a placeholder — production builds use the `sha2` crate for cryptographic hashing when the hash needs to be tamper-resistant.

### Can the proxy intercept non-browser traffic?

Yes. Any HTTP client that supports proxy configuration (curl, Python requests, Node.js fetch) can route through the proxy. The proxy doesn't care about the client — it only cares about the destination domain.
