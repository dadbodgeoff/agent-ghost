# ghost-secrets

> Cross-platform credential storage with zeroize-on-drop semantics — the second cryptographic leaf of the GHOST platform.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 0 (Leaf) |
| Type | Library |
| Location | `crates/ghost-secrets/` |
| Workspace deps | **None** — zero `ghost-*` or `cortex-*` dependencies |
| External deps | `secrecy` 0.10, `zeroize` 1.x, `thiserror` 2.x, `serde` 1.x, `tracing` 0.1 |
| Feature-gated deps | `keyring` 3.x (feature `keychain`), `reqwest` 0.12 + `serde_json` + `tokio` (feature `vault`) |
| Modules | `provider`, `error`, `env_provider`, `keychain_provider` (feature-gated), `vault_provider` (feature-gated) |
| Public API | `SecretProvider` trait, `EnvProvider`, `KeychainProvider`, `VaultProvider`, `ProviderConfig`, `SecretString`, `ExposeSecret`, `SecretsError` |
| Downstream consumers | `ghost-llm`, `ghost-oauth`, `ghost-gateway`, `ghost-integration-tests` |

---

## Why This Crate Exists

Every secret in the GHOST platform — LLM API keys, OAuth tokens, Vault credentials, agent signing key material — flows through `ghost-secrets`. The crate exists to enforce three invariants:

1. **All secrets are wrapped in `SecretString`.** The `secrecy` crate's `SecretString` type zeroizes its contents on drop and redacts in `Debug`/`Display` output. No raw `String` holding a secret ever escapes this crate.

2. **Backend selection is compile-time.** Feature gates (`keychain`, `vault`) control which storage backends are compiled in. If you don't need Vault, you don't pay for `reqwest` and `tokio` in your binary. The workspace `Cargo.toml` sets `default-features = false` for `ghost-secrets`, meaning consumers must opt in.

3. **Leaf-crate guarantee.** Like `ghost-signing`, this crate has zero dependencies on any `ghost-*` or `cortex-*` crate. This is CI-verified via a test that parses `Cargo.toml` and asserts no internal dependencies exist in the `[dependencies]` section.

---

## The `SecretProvider` Trait

```rust
pub trait SecretProvider: Send + Sync {
    fn get_secret(&self, key: &str) -> Result<SecretString, SecretsError>;
    fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretsError>;
    fn delete_secret(&self, key: &str) -> Result<(), SecretsError>;
    fn has_secret(&self, key: &str) -> bool;
}
```

This is the core abstraction. Every backend implements it. Key design decisions:

**`Send + Sync` bound.** The trait requires thread safety. This is essential because `ghost-gateway` shares a single `SecretProvider` instance across multiple async tasks (agent loops, OAuth flows, LLM auth). Without this bound, you'd need `Arc<Mutex<dyn SecretProvider>>` everywhere.

**`get_secret` returns `SecretString`, not `String`.** This is the most important decision in the crate. By returning `SecretString`, the caller is forced to use `.expose_secret()` to access the raw value. This creates a natural audit point — every place in the codebase that calls `.expose_secret()` is a place where secret material is being used, making security reviews tractable.

**`set_secret` takes `&str`, not `SecretString`.** The input is `&str` because the caller already has the secret in memory (they're providing it). Wrapping it in `SecretString` just to unwrap it inside the provider would be ceremony without security benefit.

**`has_secret` returns `bool`, not `Result`.** Existence checks should never fail in a way the caller needs to handle differently. If the backend is down, `has_secret` returns `false` — the caller's response is the same either way.

---

## Module Breakdown

### `error.rs` — Error Types

```rust
pub enum SecretsError {
    NotFound(String),
    StorageUnavailable(String),
    ProviderError(String),
    InvalidKey(String),
}
```

Four variants, each with a `String` payload for context. The design is deliberately flat — no nested error types, no `source()` chains. This is intentional:

- **`NotFound`** — The key doesn't exist. Callers can match on this to distinguish "missing" from "broken."
- **`StorageUnavailable`** — The backend is down or read-only. Used by `EnvProvider` for write operations and by `VaultProvider` for network timeouts.
- **`ProviderError`** — Catch-all for backend-specific failures (keyring errors, Vault HTTP errors, malformed JSON).
- **`InvalidKey`** — The key itself is malformed (empty, contains null bytes, contains `=`). This is checked before any backend call.

The `thiserror` derive provides `Display` and `Error` implementations automatically.

---

### `env_provider.rs` — Environment Variable Backend

The simplest backend. Read-only at runtime.

**Key validation:**
```rust
fn validate_key(key: &str) -> Result<(), SecretsError> {
    if key.is_empty() { ... }
    if key.contains('\0') { ... }
    if key.contains('=') { ... }
    Ok(())
}
```

Three checks:
1. **Empty key** — `std::env::var("")` behavior is platform-dependent. Reject it.
2. **Null bytes** — Environment variable names cannot contain null bytes on any platform. Passing one to `std::env::var` would panic on some systems.
3. **Equals sign** — The `=` character is the key-value separator in the environment block. A key containing `=` is ambiguous and could be exploited for injection.

**Read-only enforcement:** `set_secret` and `delete_secret` always return `StorageUnavailable`. While `std::env::set_var` exists, modifying environment variables at runtime is unsafe in multi-threaded programs (it's UB in POSIX). The provider correctly refuses to do it.

**Non-UTF-8 handling:** If an env var contains non-UTF-8 data, `std::env::var` returns `VarError::NotUnicode`. The provider maps this to `ProviderError` with a descriptive message rather than silently dropping the value.

---

### `keychain_provider.rs` — OS Keychain Backend

Feature-gated behind `#[cfg(feature = "keychain")]`. Uses the `keyring` crate (v3) which abstracts over:
- macOS: Security Framework (Keychain Services)
- Windows: Credential Manager
- Linux: Secret Service D-Bus API or kernel keyutils

**Service name scoping:** Every secret is stored under a `(service_name, key)` tuple. The default service name is `"ghost-platform"`. This prevents collisions with other applications using the same keyring.

```rust
pub struct KeychainProvider {
    service_name: String,
}
```

**Synchronous API:** The `keyring` crate is synchronous. The source comments explicitly note: "wrap in `tokio::task::spawn_blocking()` when called from async context." This is the caller's responsibility (typically `ghost-gateway`), not the provider's. The provider stays synchronous to keep the API simple and avoid forcing a tokio dependency.

**Error mapping:** The provider maps `keyring::Error::NoEntry` to `SecretsError::NotFound` and all other keyring errors to `SecretsError::ProviderError`. This keeps the error surface clean — callers don't need to know about keyring internals.

**Delete semantics:** `delete_secret` calls `delete_credential()` (not `delete_password()`). This removes the entire credential entry, not just the password field. If the entry doesn't exist, it returns `NotFound` rather than silently succeeding.

---

### `vault_provider.rs` — HashiCorp Vault Backend

Feature-gated behind `#[cfg(feature = "vault")]`. The most complex backend.

**Architecture:**
```rust
pub struct VaultProvider {
    endpoint: String,     // Vault server URL
    mount: String,        // KV v2 mount path (default: "secret")
    token: SecretString,  // Vault auth token (zeroized on drop)
    client: reqwest::blocking::Client,  // HTTP client with 5s timeout
}
```

#### URL Construction

All secrets are stored under the path `ghost/` within the KV v2 mount:
```
GET  {endpoint}/v1/{mount}/data/ghost/{key}     # read
POST {endpoint}/v1/{mount}/data/ghost/{key}     # write
DELETE {endpoint}/v1/{mount}/metadata/ghost/{key} # delete (metadata endpoint)
```

**Path traversal defense:** The `sanitize_key` method strips `..`, `/`, and `\` from keys before constructing URLs:
```rust
fn sanitize_key(key: &str) -> String {
    key.replace("..", "")
       .replace('/', "")
       .replace('\\', "")
}
```

This prevents an attacker from crafting a key like `../../etc/passwd` to read arbitrary Vault paths. The test suite explicitly verifies this with a path traversal attempt.

**Trailing slash normalization:** The constructor trims trailing slashes from the endpoint URL to prevent double-slash issues in URL construction (`https://vault.example.com//v1/...`).

#### KV v2 JSON Parsing

Vault's KV v2 API returns a nested JSON structure:
```json
{
  "data": {
    "data": { "value": "the-secret" },
    "metadata": { "version": 1 }
  }
}
```

The `parse_kv2_response` method navigates `.data.data.value` and returns the string value. This method is `pub` (not `pub(crate)`) to enable direct unit testing without needing a running Vault instance.

**Adversarial response handling:** The parser handles:
- Malformed JSON → `ProviderError("malformed JSON from Vault")`
- Missing `.data.data.value` path → `ProviderError("Vault response missing .data.data.value field")`
- HTML responses (e.g., 503 pages from a load balancer) → `ProviderError`

#### Token Management

The Vault token is stored as `SecretString` — zeroized on drop. The `renew_token()` method sends `POST /v1/auth/token/renew-self` to extend the token lease.

**Token renewal is the caller's responsibility.** The provider does not automatically renew tokens. The source comments note: "in production, call `renew_token()` periodically before the lease expires (e.g. from a background `tokio::task` in ghost-gateway)." This keeps the provider stateless and avoids hidden background threads.

**HTTP status code handling:**
| Status | `get_secret` | `set_secret` | `delete_secret` | `renew_token` |
|--------|-------------|-------------|-----------------|---------------|
| 200 | Parse JSON | Success | Success | Success |
| 204 | — | Success | Success | — |
| 403 | `ProviderError` (auth failed) | `ProviderError` | `ProviderError` | `ProviderError` (token expired) |
| 404 | `NotFound` | — | `NotFound` | — |
| Other | `ProviderError` | `ProviderError` | `ProviderError` | `ProviderError` |
| Timeout | `StorageUnavailable` | `StorageUnavailable` | `StorageUnavailable` | `StorageUnavailable` |

The timeout/network distinction is important: timeouts map to `StorageUnavailable` (transient, retry-worthy), while HTTP errors map to `ProviderError` (likely permanent, needs investigation).

#### Delete Uses Metadata Endpoint

A subtle but important detail: `delete_secret` hits the **metadata** endpoint (`/v1/{mount}/metadata/ghost/{key}`), not the data endpoint. In Vault KV v2, deleting via the data endpoint only soft-deletes the latest version. Deleting via the metadata endpoint permanently removes all versions of the secret. This is the correct behavior for credential cleanup.

---

## `ProviderConfig` — Runtime Backend Selection

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum ProviderConfig {
    #[default]
    Env,
    Keychain { service_name: String },
    Vault { endpoint: String, mount: String, token_env: String },
}
```

**Tagged enum serialization:** The `#[serde(tag = "provider")]` attribute means the JSON representation uses a `"provider"` field as the discriminant:
```json
{"provider": "env"}
{"provider": "keychain", "service_name": "ghost-platform"}
{"provider": "vault", "endpoint": "https://vault.local", "mount": "secret", "token_env": "VAULT_TOKEN"}
```

**Default is `Env`.** The safest, most portable option. No external services required.

**Vault bootstrap problem:** The Vault config contains `token_env` — the name of an environment variable holding the Vault token. This is a deliberate bootstrap design: you need a secret (the Vault token) to access the secret store. The solution is to store the bootstrap token in an env var, which the `VaultProvider` reads at construction time. In production, this env var is typically injected by the deployment system (Kubernetes secret, systemd credential, etc.).

---

## Feature Gate Architecture

```toml
[features]
default = ["keychain"]
keychain = ["dep:keyring"]
vault = ["dep:reqwest", "dep:serde_json", "dep:tokio"]
```

**Default includes `keychain`.** Most desktop users want OS keychain integration. The `vault` feature is opt-in because it pulls in `reqwest` (with TLS) and `tokio`, which significantly increases compile time and binary size.

**Workspace-level `default-features = false`.** The root `Cargo.toml` declares:
```toml
ghost-secrets = { path = "crates/ghost-secrets", default-features = false }
```

This means every consumer must explicitly opt into features. `ghost-gateway` does this via its own feature flags:
```toml
[features]
keychain = ["ghost-secrets/keychain"]
vault = ["ghost-secrets/vault"]
```

This cascading feature gate design means you can build `ghost-gateway` without Vault support by disabling the `vault` feature, and the `reqwest`/`tokio` dependencies won't be compiled at all.

---

## Security Properties

### SecretString Zeroize-on-Drop

Every secret value returned by `get_secret()` is a `SecretString` from the `secrecy` crate. When dropped:
- The inner `String`'s bytes are overwritten with zeros via `zeroize`
- The `Debug` and `Display` impls show `[REDACTED]`, never the actual value
- Access requires explicit `.expose_secret()` — a grep-able audit point

### Vault Token Protection

The `VaultProvider`'s token field is `SecretString`. It's zeroized when the provider is dropped. The token is only exposed in the `X-Vault-Token` HTTP header during requests.

### Key Validation as Defense in Depth

All providers validate keys before use:
- Empty keys → `InvalidKey`
- Null bytes → `InvalidKey` (prevents C-string truncation attacks)
- Equals signs → `InvalidKey` (prevents env var injection)
- Path traversal sequences → sanitized (Vault provider)

---

## Downstream Consumer Map

```
ghost-secrets (Layer 0)
├── ghost-llm (Layer 4)
│   └── LLM API key retrieval for provider authentication
├── ghost-oauth (Layer 4)
│   └── OAuth token encryption at rest, client secret storage
├── ghost-gateway (Layer 8)
│   └── Feature-gate passthrough (keychain/vault), bootstrap config
└── ghost-integration-tests (Layer 10)
    └── Test infrastructure
```

---

## Test Strategy

### Unit Tests (`tests/secrets_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `env_provider_reads_existing_env_var` | Basic read round-trip |
| `env_provider_missing_var_returns_not_found` | Missing key → `NotFound` |
| `env_provider_set_secret_returns_storage_unavailable` | Write rejection (read-only) |
| `env_provider_delete_secret_returns_storage_unavailable` | Delete rejection (read-only) |
| `env_provider_has_secret_true_for_set_var` | Existence check (positive) |
| `env_provider_has_secret_false_for_unset_var` | Existence check (negative) |
| `secret_string_is_zeroize_on_drop` | Exercises the zeroize drop path |
| `cargo_toml_has_no_ghost_or_cortex_dependencies` | Leaf-crate invariant |
| `env_provider_empty_key_returns_invalid_key` | Empty key rejection |
| `env_provider_key_with_null_byte_returns_invalid_key` | Null byte rejection |
| `env_provider_key_with_equals_returns_invalid_key` | Equals sign rejection |
| `env_provider_key_with_spaces_works` | Spaces allowed (OS-dependent) |
| `env_provider_very_long_value_no_oom` | 1 MB value handling |
| `provider_config_*_serde_round_trip` | Config serialization for all variants |

### Vault Tests (`tests/vault_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `vault_provider_constructs_correct_data_url` | Constructor doesn't panic |
| `vault_provider_trims_trailing_slash` | URL normalization |
| `vault_provider_empty_key_returns_invalid_key` | Key validation |
| `vault_provider_null_key_returns_invalid_key` | Null byte defense |
| `vault_provider_path_traversal_key_sanitized` | Path traversal defense |
| `vault_provider_network_timeout_returns_storage_unavailable` | Timeout → correct error variant |
| `vault_provider_parses_kv2_json_response_correctly` | Happy path JSON parsing |
| `vault_provider_malformed_json_returns_provider_error` | Garbage input handling |
| `vault_provider_missing_data_field_returns_provider_error` | Incomplete JSON handling |
| `vault_provider_html_response_returns_provider_error` | Adversarial HTML response |

### Property Tests (`tests/property_tests.rs`)

500 cases per property:

| Property | Invariant |
|----------|-----------|
| `env_provider_round_trip_random_values` | ∀ valid key/value: set → get = original |
| `env_provider_get_never_panics` | ∀ arbitrary string: `get_secret` never panics |
| `env_provider_has_secret_never_panics` | ∀ arbitrary string: `has_secret` never panics |

### E2E Tests (`tests/secrets_e2e.rs`)

Full pipeline tests plus a `#[ignore]`-gated keychain round-trip test that requires a real OS keychain.

---

## File Map

```
crates/ghost-secrets/
├── Cargo.toml                  # Feature-gated deps, zero internal deps
├── src/
│   ├── lib.rs                  # Re-exports, conditional module inclusion
│   ├── provider.rs             # SecretProvider trait + ProviderConfig enum
│   ├── error.rs                # SecretsError enum (4 variants)
│   ├── env_provider.rs         # Environment variable backend (read-only)
│   ├── keychain_provider.rs    # OS keychain backend (feature: keychain)
│   └── vault_provider.rs       # HashiCorp Vault KV v2 (feature: vault)
└── tests/
    ├── secrets_tests.rs        # Unit tests + leaf-crate audit
    ├── property_tests.rs       # Proptest: 3 properties × 500 cases
    ├── secrets_e2e.rs          # E2E pipeline tests
    └── vault_tests.rs          # Vault URL/JSON/timeout tests
```

---

## Common Questions

### Why `secrecy` instead of just `zeroize` directly?

`zeroize` provides the low-level memory cleanup. `secrecy` builds on it to provide:
- `SecretString` — a `String` wrapper that zeroizes on drop
- `ExposeSecret` trait — forces explicit `.expose_secret()` calls (audit trail)
- `Debug`/`Display` redaction — prints `[REDACTED]` instead of the value

Using `zeroize` directly would require building all of this manually. `secrecy` is the standard Rust crate for this pattern.

### Why blocking HTTP for Vault instead of async?

The `VaultProvider` uses `reqwest::blocking::Client`, not the async client. This matches the `SecretProvider` trait, which is synchronous. The reasoning:
- Secret retrieval is typically done at startup or on-demand, not in hot paths
- Making the trait async would force `async-trait` on all backends, including `EnvProvider` (which has no async operations)
- The caller can wrap in `spawn_blocking()` if needed (documented in source comments)

### Why is `default-features = false` set at the workspace level?

To prevent accidental feature creep. If a new crate adds `ghost-secrets` as a dependency without thinking about features, it gets the minimal build (env provider only). This is the principle of least privilege applied to compile-time dependencies.

### Can the Vault token be rotated without restarting?

Not currently. The `VaultProvider` stores the token at construction time. Token rotation would require either:
- Reconstructing the provider with a new token
- Adding interior mutability (`RwLock<SecretString>`) to the token field

The source comments note this as a future enhancement, with the current recommendation being to use `renew_token()` to extend the existing lease.
