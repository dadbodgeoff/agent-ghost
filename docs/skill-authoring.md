# Skill Authoring Guide

Skills extend agent capabilities in the GHOST platform. They run in a WASM sandbox with capability-scoped permissions.

## Skill Discovery Order

1. Workspace skills: `./skills/`
2. User skills: `~/.ghost/skills/`
3. Bundled skills: compiled into the binary

Higher priority overrides lower (workspace > user > bundled).

## Skill Structure

Each skill is a directory containing:

```
my-skill/
├── skill.yml          # Manifest with metadata and permissions
├── skill.wasm         # Compiled WASM module (or skill.js for native)
└── README.md          # Optional documentation
```

## Manifest (skill.yml)

```yaml
name: my-skill
version: 0.1.0
description: "A custom skill that does something useful"
author: "Your Name"

# Tool schema exposed to the LLM
tools:
  - name: my_tool
    description: "Does the thing"
    parameters:
      type: object
      properties:
        input:
          type: string
          description: "The input to process"
      required: [input]

# Capability requirements (must be granted in ghost.yml)
capabilities:
  - file_read        # Read files in workspace
  # - file_write     # Write files (requires explicit grant)
  # - network        # Network access (requires explicit grant + allowlist)
  # - shell          # Shell execution (requires explicit grant)

# Resource limits
limits:
  timeout_seconds: 30
  memory_mb: 64
```

## WASM Skills

WASM skills run in a wasmtime sandbox with strict isolation:

- No filesystem access unless `file_read`/`file_write` capability granted
- No network access unless `network` capability granted
- No process spawning
- Memory limited per manifest
- Timeout enforced

### Building a WASM Skill (Rust)

```rust
// lib.rs
#[no_mangle]
pub extern "C" fn my_tool(input_ptr: *const u8, input_len: usize) -> *const u8 {
    // Read input
    let input = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    let input_str = std::str::from_utf8(input).unwrap_or("");

    // Do work
    let result = format!("Processed: {}", input_str);

    // Return result (platform handles memory)
    result.as_ptr()
}
```

```bash
cargo build --target wasm32-wasi --release
cp target/wasm32-wasi/release/my_skill.wasm skills/my-skill/skill.wasm
```

## Signing Skills

All skills must be Ed25519 signed. Unsigned skills are quarantined.

```bash
# Generate a signing key (if you don't have one)
ghost keygen --output ~/.ghost/keys/skill-signing.key

# Sign a skill
ghost sign-skill --key ~/.ghost/keys/skill-signing.key --skill skills/my-skill/

# This adds a signature file: skills/my-skill/skill.sig
```

The platform verifies signatures on every skill load. If verification fails, the skill is quarantined and a `TriggerEvent::SandboxEscape` may be emitted.

## Credential Access

Skills can request credentials via the CredentialBroker:

```yaml
# In skill.yml
credentials:
  - name: API_KEY
    description: "API key for the external service"
    max_uses: 1    # Credential is single-use by default
```

Credentials are opaque tokens reified only at execution time inside the sandbox. They are never exposed to the skill as plaintext outside the sandbox boundary.

## Sandbox Escape Detection

The following actions trigger immediate skill termination and a `TriggerEvent::SandboxEscape`:

- Filesystem write without `file_write` capability
- Network access to non-allowlisted domain
- Process spawning
- Environment variable read
- Memory limit exceeded
- Timeout exceeded

Forensic data is captured: skill name, skill hash, escape type, timestamp, and sandbox state.
