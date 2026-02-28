# Secrets Management

GHOST uses the `ghost-secrets` crate for cross-platform credential storage.
It's a leaf crate with zero `ghost-*`/`cortex-*` dependencies, ensuring it can
be used at any layer of the stack.

## Providers

### Environment Variables (default)

The simplest provider. Reads secrets from environment variables.

```yaml
# ghost.yml
secrets:
  provider: env
```

Set credentials via your shell:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
```

### OS Keychain (feature: `keychain`)

Uses the OS credential store (macOS Keychain, Windows Credential Manager, Linux Secret Service).

```yaml
secrets:
  provider: keychain
  keychain_service: "ghost-platform"
```

Store a credential:

```bash
ghost secrets set ANTHROPIC_API_KEY
# Prompts for value, stores in OS keychain
```

### HashiCorp Vault (feature: `vault`)

For production deployments with centralized secret management.

```yaml
secrets:
  provider: vault
  vault_addr: "https://vault.internal:8200"
  vault_mount: "secret"
  vault_path_prefix: "ghost/"
```

Authenticate via `VAULT_TOKEN` env var or AppRole.

## Migration from Environment Variables

If you're currently using env vars and want to switch to keychain:

1. Update `ghost.yml` to `provider: keychain`
2. Run `ghost secrets import-env` to copy existing env vars to keychain
3. Remove env vars from your shell profile

## Security Considerations

- All secret values are wrapped in `SecretString` (zeroized on drop via `secrecy` crate)
- Secrets are never logged — Debug impls show `[REDACTED]`
- The `ghost-secrets` crate has no network dependencies in `env` and `keychain` modes
- Vault provider uses TLS for all communication
- Per-agent credential isolation: each agent's secrets are namespaced by agent name
