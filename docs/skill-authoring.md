# Skill Authoring Guide

Skills extend agent capabilities. They can be native Rust functions or WASM modules.

## Skill Structure

Each skill is a YAML file with frontmatter and optional WASM binary:

```yaml
---
name: web-search
version: 1.0.0
description: Search the web for information
author: ghost-platform
capabilities:
  - network:read
  - network:allowlist:api.search.example.com
timeout: 30s
signature: <ed25519-signature>
---
```

## Capability Scoping

Skills declare required capabilities. The sandbox enforces these at runtime:

| Capability | Description |
|-----------|-------------|
| `filesystem:read` | Read files within workspace |
| `filesystem:write` | Write files within workspace |
| `network:read` | HTTP GET to allowlisted domains |
| `network:write` | HTTP POST to allowlisted domains |
| `shell:execute` | Run shell commands (restricted) |

## WASM Skills

WASM skills run in a wasmtime sandbox with:

- Memory limit (configurable, default 256MB)
- Execution timeout (configurable, default 30s)
- Capability-scoped imports only
- No direct filesystem, network, or process access

```rust
// Example WASM skill entry point
#[no_mangle]
pub extern "C" fn execute(input_ptr: *const u8, input_len: usize) -> i32 {
    // Process input, return result
    0
}
```

## Signing Skills

All skills must be Ed25519 signed. Unsigned skills are quarantined:

```bash
ghost skill sign my-skill.yml --key ~/.ghost/keys/signing.key
```

## Discovery Priority

Skills are discovered in order:
1. Workspace skills (`./skills/`)
2. User skills (`~/.ghost/skills/`)
3. Bundled skills (compiled into binary)

Later discoveries override earlier ones by name.

## Credential Broker

Skills that need API keys use the CredentialBroker pattern:

```yaml
credentials:
  - name: SEARCH_API_KEY
    max_uses: 1  # Single-use opaque token
```

The actual credential is never exposed to the skill. An opaque token is reified at execution time inside the sandbox.
