# ghost-oauth

> Self-hosted OAuth 2.0 PKCE broker — the agent never sees raw tokens, only opaque `OAuthRefId` references. Tokens are encrypted at rest via `ghost-secrets`. Kill switch integration: `revoke_all()` instantly invalidates every connection.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Infrastructure Services) |
| Type | Library |
| Location | `crates/ghost-oauth/` |
| Workspace deps | `ghost-secrets` (Layer 0) |
| External deps | `reqwest`, `secrecy`, `zeroize`, `sha2`, `rand`, `base64`, `chrono`, `uuid`, `serde`, `tokio`, `tracing`, `thiserror` |
| Modules | `broker`, `provider`, `storage`, `types`, `error`, `providers/` (4 backends) |
| Public API | `OAuthBroker`, `OAuthProvider` trait, `TokenStore`, `OAuthRefId`, `PkceChallenge`, `TokenSet`, `ApiRequest`/`ApiResponse` |
| Supported providers | Google, GitHub, Slack, Microsoft (Azure AD) |
| Test coverage | Unit tests, storage integration tests, concurrent access tests, adversarial input tests, layer separation tests |
| Downstream consumers | `ghost-gateway` (owns the broker), `ghost-agent-loop` (executes API calls via ref_id) |

---

## Why This Crate Exists

AI agents need to interact with third-party APIs on behalf of users — reading Gmail, posting to Slack, creating GitHub issues. This requires OAuth tokens. The fundamental problem: **agents should never see raw tokens.**

If an agent has direct access to an OAuth access token, a compromised or misbehaving agent could:
- Exfiltrate the token to an external server
- Use the token for unauthorized operations beyond its intended scope
- Store the token in its memory/context where it persists across sessions

`ghost-oauth` solves this with a broker pattern:

1. The user authorizes via standard OAuth 2.0 PKCE flow (browser redirect)
2. The broker receives the authorization code, exchanges it for tokens, and encrypts them at rest
3. The agent receives only an opaque `OAuthRefId` (a UUID) — it cannot derive the token from this
4. When the agent needs to make an API call, it passes the `OAuthRefId` + request to the broker
5. The broker decrypts the token, injects it as a Bearer header, executes the request, and returns the response
6. The token is zeroized from memory after use

The agent never touches the token. The broker is the only component that handles raw credentials.

### Kill Switch Integration

When the kill switch fires (`QUARANTINE` or `KILL_ALL`), the gateway calls `OAuthBroker::revoke_all()`. This:
1. Iterates all stored connections across all providers
2. Attempts provider-side revocation (best-effort — some providers don't support it)
3. Deletes all encrypted token files from disk
4. Clears the in-memory connection map

After `revoke_all()`, every `OAuthRefId` becomes non-functional. The agent cannot make any more API calls through the broker.

---

## Module Breakdown

### `types.rs` — Core OAuth Types

This module defines every type that flows through the OAuth system. The design is driven by one principle: **secrets are wrapped, non-secrets are plain.**

#### `OAuthRefId` — The Agent's Handle

```rust
pub struct OAuthRefId(Uuid);
```

This is what the agent sees instead of tokens. It's a UUIDv7 (time-ordered) that maps to an encrypted token file on disk.

**Design decisions:**

1. **UUIDv7, not v4.** Time-ordered UUIDs allow chronological listing of connections without additional metadata. The timestamp component also helps with debugging ("this connection was created at roughly this time").

2. **Implements `Serialize`/`Deserialize`.** The ref_id needs to be stored in agent context, passed through tool calls, and persisted in session state. It contains no secret material.

3. **Implements `Display`.** The string representation is the UUID itself — no prefix, no encoding. This keeps it simple for logging and debugging.

#### `PkceChallenge` — PKCE S256 Implementation

```rust
pub struct PkceChallenge {
    pub code_verifier: SecretString,  // 128-char random URL-safe string
    pub code_challenge: String,        // BASE64URL(SHA256(verifier))
    pub method: String,                // Always "S256"
}
```

**Design decisions:**

1. **128-character verifier.** RFC 7636 allows 43-128 characters. GHOST uses the maximum length for maximum entropy. The verifier is generated from a 66-character URL-safe alphabet (`A-Z`, `a-z`, `0-9`, `-._~`).

2. **`SecretString` for verifier.** The code verifier is secret material — it proves that the entity requesting the token is the same entity that initiated the flow. It's wrapped in `SecretString` (zeroized on drop) and redacted in `Debug` output.

3. **S256 only, no plain.** The `method` field is always `"S256"` (SHA-256). The `plain` method (where challenge = verifier) is deliberately not supported because it provides no security benefit — an attacker who intercepts the authorization request gets the verifier directly.

4. **Debug redaction.** `Debug` shows `code_verifier: "[REDACTED]"` — the verifier never appears in logs.

#### `TokenSet` — Encrypted Token Container

```rust
pub struct TokenSet {
    pub access_token: SecretString,
    pub refresh_token: Option<SecretString>,
    pub expires_at: DateTime<Utc>,
    pub scopes: Vec<String>,
}
```

**Design decisions:**

1. **No `Serialize`/`Deserialize` on `TokenSet` itself.** This is intentional — `TokenSet` contains `SecretString` fields that should never be accidentally serialized. A separate `TokenSetSerde` type (crate-internal) handles the conversion for encrypted storage.

2. **`Debug` redaction.** Both `access_token` and `refresh_token` show `[REDACTED]` in debug output. Only `expires_at` and `scopes` are visible.

3. **`is_expired()` method.** Compares `expires_at` against `Utc::now()`. The `TokenStore` uses this to return `TokenExpired` errors, triggering the broker's auto-refresh flow.

4. **`Option<SecretString>` for refresh token.** Not all providers issue refresh tokens. GitHub uses long-lived tokens (no refresh). Slack's bot tokens don't expire by default. The `Option` handles this cleanly.

#### `ApiRequest` / `ApiResponse` — Broker-Mediated HTTP

```rust
pub struct ApiRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}
```

**Why `BTreeMap` for headers?** Deterministic ordering. When requests are logged or signed, the header order must be consistent. `HashMap` has non-deterministic iteration order, which would make logs non-reproducible and signatures unstable.

---

### `provider.rs` — The `OAuthProvider` Trait

```rust
pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &str;
    fn authorization_url(&self, scopes: &[String], state: &str, redirect_uri: &str) 
        -> Result<(String, PkceChallenge), OAuthError>;
    fn exchange_code(&self, code: &str, pkce_verifier: &str, redirect_uri: &str) 
        -> Result<TokenSet, OAuthError>;
    fn refresh_token(&self, refresh_token: &str) -> Result<TokenSet, OAuthError>;
    fn revoke_token(&self, token: &str) -> Result<(), OAuthError>;
    fn execute_api_call(&self, access_token: &str, request: &ApiRequest) 
        -> Result<ApiResponse, OAuthError>;
}
```

This trait abstracts away provider-specific quirks. Each of the 4 providers has different:
- Authorization URL formats
- Token exchange request formats (GitHub requires `Accept: application/json`)
- Refresh token support (GitHub has none)
- Revocation endpoints (Microsoft has none, Slack is a no-op)
- Error response formats (Slack wraps errors in `{"ok": false, "error": "..."}`)

The `Send + Sync` bounds allow the broker to be shared across async tasks via `Arc`.

---

### `providers/` — Four Provider Implementations

#### Google (`google.rs`)

- Auth: `https://accounts.google.com/o/oauth2/v2/auth`
- Token: `https://oauth2.googleapis.com/token`
- Revoke: `https://oauth2.googleapis.com/revoke`
- Supports refresh tokens (`access_type=offline&prompt=consent`)
- Default scopes: `gmail.readonly`, `calendar`, `drive.readonly`
- Revocation quirk: Returns 400 for already-revoked tokens (treated as success)

#### GitHub (`github.rs`)

- Auth: `https://github.com/login/oauth/authorize`
- Token: `https://github.com/login/oauth/access_token`
- Revoke: `https://api.github.com/applications/{client_id}/token` (DELETE with basic auth)
- **No refresh tokens** — GitHub tokens are long-lived (set to 365-day expiry)
- Exchange quirk: Requires `Accept: application/json` header (GitHub defaults to URL-encoded)
- Error quirk: May return errors in JSON body even with HTTP 200

#### Slack (`slack.rs`)

- Auth: `https://slack.com/oauth/v2/authorize`
- Token: `https://slack.com/api/oauth.v2.access`
- Uses `xoxb-` bot tokens and `xoxp-` user tokens
- Scopes are comma-separated (not space-separated like other providers)
- Error format: `{"ok": false, "error": "invalid_code"}` (not standard OAuth error format)
- Revocation: No programmatic single-token revoke — treated as no-op
- Token validation: `validate_token_prefix()` checks for `xoxb-` or `xoxp-` prefix

#### Microsoft (`microsoft.rs`)

- Auth: `https://login.microsoftonline.com/{tenant}/oauth2/v2/authorize`
- Token: `https://login.microsoftonline.com/{tenant}/oauth2/v2/token`
- Multi-tenant support via configurable tenant ID
- Default scopes: `Mail.Read`, `Calendars.Read`, `User.Read`
- Revocation: No standard v2.0 revocation endpoint — tokens expire naturally
- Uses `response_mode=query` for authorization code delivery

#### Shared Helpers

All providers share `execute_bearer_request()` for API call execution and `parse_token_response()` for standard OAuth token response parsing. These live in `google.rs` and are imported by other providers via `super::google::`.

---

### `storage.rs` — Encrypted Token Persistence

Tokens are stored as encrypted files on disk, organized by provider:

```
~/.ghost/oauth/tokens/
├── google/
│   ├── {ref_id_1}.age
│   └── {ref_id_2}.age
├── github/
│   └── {ref_id_3}.age
└── slack/
    └── {ref_id_4}.age
```

#### Vault Key Management

The encryption key is stored in `ghost-secrets` under the name `ghost-oauth-vault-key`. On first use, if no key exists, the store auto-generates a 256-bit random key and persists it. If the secret provider is read-only, the key is valid for the current session only.

#### Encryption Scheme

The current implementation uses a SHA-256-derived XOR stream cipher:
1. Generate 16-byte random salt
2. Derive stream key: `SHA-256(passphrase || salt)` → 32 bytes
3. XOR each data byte with the stream key (cycling every 32 bytes)
4. Store as `[16-byte salt][ciphertext]`

**Important caveat:** This is a placeholder. The production implementation should use the `age` crate for authenticated encryption. The current XOR stream provides confidentiality but not authentication — corrupted ciphertext decrypts to garbage rather than returning an error.

#### Atomic Writes

Token files are written atomically: write to `{ref_id}.tmp`, then `rename()` to `{ref_id}.age`. This prevents corruption if the process crashes mid-write — you either get the old file or the new file, never a partial write.

#### Path Traversal Defense

The `sanitize_path_component()` function strips `..`, `/`, `\`, and null bytes from provider names before using them as directory names. This prevents a malicious provider name from escaping the token directory.

---

### `broker.rs` — The Orchestrator

The `OAuthBroker` is the central component that ties everything together. It's owned by the gateway and shared with the agent loop via `Arc`.

#### Connect Flow

```
User clicks "Connect Google" in UI
  → broker.connect("google", scopes, redirect_uri)
    → Provider generates authorization URL with PKCE
    → Broker stores PendingFlow (state → ref_id + pkce + metadata)
    → Returns (authorization_url, ref_id)
  → UI redirects user to authorization_url
  → User authorizes at Google
  → Google redirects to callback with code + state
  → broker.callback(state, code)
    → Broker looks up PendingFlow by state
    → Rejects if flow is >10 minutes old (CSRF protection)
    → Provider exchanges code for tokens using PKCE verifier
    → TokenStore encrypts and stores tokens
    → Returns ref_id
```

#### Execute Flow

```
Agent wants to call Gmail API
  → broker.execute(ref_id, ApiRequest { method: "GET", url: "..." })
    → Broker loads encrypted token from TokenStore
    → If expired: auto-refresh transparently
    → Provider injects Bearer token and executes HTTP request
    → Returns ApiResponse { status, headers, body }
    → Token zeroized from memory on drop
```

#### State Parameter Security

The state parameter format is `{ref_id}:{random_uuid}`. This serves two purposes:
1. The `ref_id` links the callback to the pending flow
2. The random UUID provides CSRF protection — an attacker can't predict the state value

Pending flows expire after 10 minutes. This limits the window for CSRF attacks and prevents stale flows from accumulating.

#### Disconnect and Revoke

`disconnect(ref_id)` does three things:
1. Best-effort provider-side revocation (calls `provider.revoke_token()`)
2. Deletes the encrypted token file
3. Removes the connection from the in-memory map

`revoke_all()` does the same for every connection. It's designed for kill switch integration — when called, all OAuth connections become non-functional immediately.

---

## Security Properties

### Agent Never Sees Tokens

The agent only has `OAuthRefId` (a UUID). There is no API to convert a ref_id back to a token. The broker decrypts tokens internally, uses them for a single HTTP request, and drops them (triggering zeroize).

### Tokens Encrypted at Rest

All tokens are encrypted before writing to disk. The encryption key is stored in `ghost-secrets` (which uses the OS keychain on macOS/Windows, or encrypted files on Linux). Even if an attacker reads the token files, they get ciphertext.

### PKCE S256 Everywhere

All four providers use PKCE with S256 challenge method. This prevents authorization code interception attacks — even if an attacker captures the authorization code, they can't exchange it without the code verifier.

### Zeroize on Drop

`SecretString` (from the `secrecy` crate) overwrites memory with zeros when dropped. This applies to:
- Access tokens
- Refresh tokens
- PKCE code verifiers
- Client secrets

### Debug Redaction

`TokenSet` and `PkceChallenge` both implement custom `Debug` that shows `[REDACTED]` instead of secret values. This prevents accidental token leakage in log output.

### 10-Minute Flow Expiry

Pending OAuth flows expire after 10 minutes. This limits the CSRF attack window and prevents memory leaks from abandoned flows.

---

## Downstream Consumer Map

```
ghost-oauth (Layer 4)
├── ghost-gateway (Layer 8)
│   └── Owns the OAuthBroker, manages provider registration
│   └── Exposes /api/oauth/connect and /api/oauth/callback endpoints
│   └── Calls revoke_all() on kill switch activation
└── ghost-agent-loop (Layer 7)
    └── Calls broker.execute(ref_id, request) for API calls
    └── Only sees OAuthRefId, never raw tokens
```

---

## Test Strategy

### Unit Tests (`tests/oauth_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `oauth_ref_id_is_valid_uuid` | RefId wraps a valid UUID |
| `oauth_ref_id_serde_roundtrip` | JSON serialize/deserialize round-trip |
| `pkce_challenge_generates_valid_code_verifier_length` | Verifier is 43-128 chars |
| `pkce_challenge_code_verifier_is_url_safe` | Only URL-safe characters |
| `pkce_challenge_code_challenge_is_sha256_base64url` | Challenge = BASE64URL(SHA256(verifier)) |
| `pkce_challenge_debug_redacts_verifier` | Debug shows [REDACTED] |
| `token_set_debug_redacts_tokens` | No secrets in Debug output |
| `token_set_is_expired_when_past_expiry` | Expiry detection works |
| `encrypted_file_does_not_contain_plaintext_token` | Grep for token in file → not found |
| `no_tmp_files_left_after_store` | Atomic write cleanup |
| `corrupted_encrypted_file_returns_graceful_error` | No panic on corruption |
| `concurrent_store_load_same_ref_id_no_corruption` | 5 threads, no data corruption |
| `google_generates_correct_authorization_url` | URL contains all required params |
| `github_refresh_returns_unsupported_error` | Explicit "long-lived" message |
| `slack_revoke_is_noop` | Returns Ok (no revocation endpoint) |
| `microsoft_generates_correct_authorization_url_with_tenant` | Tenant ID in URL |

### E2E Tests (`tests/oauth_e2e.rs`)

| Test | What It Verifies |
|------|-----------------|
| `ghost_oauth_layer_separation` | Cargo.toml has `secrecy` but not `ghost-gateway` |
| `api_request_deterministic_headers` | BTreeMap produces consistent JSON |
| `connection_status_all_variants` | All 4 status variants serialize/deserialize |

---

## File Map

```
crates/ghost-oauth/
├── Cargo.toml                          # Deps: ghost-secrets, reqwest, secrecy, sha2
├── src/
│   ├── lib.rs                          # Public API re-exports
│   ├── types.rs                        # OAuthRefId, PkceChallenge, TokenSet, ApiRequest/Response
│   ├── error.rs                        # OAuthError enum (8 variants)
│   ├── provider.rs                     # OAuthProvider trait definition
│   ├── storage.rs                      # TokenStore — encrypted file persistence
│   ├── broker.rs                       # OAuthBroker — connect/callback/execute/disconnect/revoke_all
│   └── providers/
│       ├── mod.rs                      # Provider re-exports
│       ├── google.rs                   # Google OAuth + shared helpers
│       ├── github.rs                   # GitHub OAuth (no refresh tokens)
│       ├── slack.rs                    # Slack OAuth (xoxb- bot tokens)
│       └── microsoft.rs               # Microsoft/Azure AD (multi-tenant)
└── tests/
    ├── oauth_tests.rs                  # Comprehensive unit + integration tests
    └── oauth_e2e.rs                    # End-to-end pipeline tests
```

---

## Common Questions

### Why a broker pattern instead of giving the agent scoped tokens?

Even scoped tokens are dangerous in agent hands. A `gmail.readonly` token still lets the agent read all emails. The broker pattern adds a second layer: the agent can only make API calls through the broker, which means the gateway can log, rate-limit, and audit every API call. With direct tokens, the agent could make calls that bypass all monitoring.

### Why not use an existing OAuth library like `oauth2-rs`?

`oauth2-rs` is excellent for standard OAuth flows, but GHOST needs:
- Encrypted token storage integrated with `ghost-secrets`
- Kill switch integration (`revoke_all()`)
- Broker-mediated API execution (agent never sees tokens)
- Provider-specific quirk handling (Slack's non-standard error format, GitHub's Accept header)

These requirements are specific enough that wrapping `oauth2-rs` would add more complexity than building the focused subset GHOST needs.

### What happens if the vault key is lost?

All encrypted tokens become unreadable. The user would need to re-authorize all connections. This is by design — there's no "master recovery key" that could be compromised. The vault key is stored in the OS keychain (via `ghost-secrets`), which has its own backup/recovery mechanisms.

### Why does GitHub get a 365-day expiry instead of infinity?

GitHub tokens don't expire, but setting an infinite expiry would mean `is_expired()` never triggers, which could mask bugs in the refresh flow. The 365-day expiry acts as a safety net — if the token is somehow invalidated, the broker will attempt a refresh (which will fail with a clear error) rather than silently using a dead token forever.
