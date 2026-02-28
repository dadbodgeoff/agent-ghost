# Configuration Reference

GHOST Platform is configured via `ghost.yml`. The gateway searches for config in this order:

1. CLI argument: `ghost serve --config /path/to/ghost.yml`
2. Environment variable: `GHOST_CONFIG=/path/to/ghost.yml`
3. User config: `~/.ghost/config/ghost.yml`
4. Local config: `./ghost.yml`

## Environment Variable Substitution

Use `${VAR_NAME}` syntax in ghost.yml to reference environment variables:

```yaml
security:
  token: "${GHOST_TOKEN}"
models:
  providers:
    - name: anthropic
      api_key_env: "ANTHROPIC_API_KEY"
```

## Full Schema Reference

### gateway

```yaml
gateway:
  bind: "127.0.0.1"     # Bind address (default: 127.0.0.1)
  port: 18789            # Port (default: 18789, matches OpenClaw)
  db_path: "~/.ghost/data/ghost.db"  # SQLite database path
```

### agents

```yaml
agents:
  - name: "ghost"              # Agent name (unique identifier)
    spending_cap: 5.0          # Daily spending cap in USD (default: 5.0)
    capabilities:              # Explicit capability grants (deny-by-default)
      - "web_search"
      - "file_read"
      - "file_write"
      - "memory_write"
      - "shell_sandboxed"
    isolation: inprocess        # inprocess | process | container
    template: null              # Optional: personal | developer | researcher
```

### channels

```yaml
channels:
  - channel_type: cli          # cli | websocket | telegram | discord | slack | whatsapp
    agent: "ghost"             # Agent name to bind to
    options: {}                # Channel-specific options
```

Channel-specific options:

```yaml
# Telegram
- channel_type: telegram
  agent: "ghost"
  options:
    bot_token: "${TELEGRAM_BOT_TOKEN}"

# Discord
- channel_type: discord
  agent: "ghost"
  options:
    bot_token: "${DISCORD_BOT_TOKEN}"
    guild_id: "123456789"

# Slack
- channel_type: slack
  agent: "ghost"
  options:
    bot_token: "${SLACK_BOT_TOKEN}"
    app_token: "${SLACK_APP_TOKEN}"

# WhatsApp (requires Node.js 18+ and baileys-bridge)
- channel_type: whatsapp
  agent: "ghost"
  options:
    bridge_path: "extension/bridges/baileys-bridge"

# WebSocket
- channel_type: websocket
  agent: "ghost"
  options:
    bind: "127.0.0.1"
    port: 18791
```

### convergence

```yaml
convergence:
  profile: "standard"          # standard | research | companion | productivity
  monitor:
    address: "127.0.0.1:18790" # Convergence monitor address
  contacts:                    # Emergency contacts for Level 3+ interventions
    - contact_type: email
      target: "[email]"
    - contact_type: webhook
      target: "https://hooks.example.com/ghost"
```

### Convergence Profiles

| Profile | Description | Threshold Adjustments |
|---------|-------------|----------------------|
| standard | Default balanced profile | Default thresholds |
| research | Relaxed for research use | Higher thresholds, longer sessions allowed |
| companion | Stricter for companion use | Lower thresholds, more sensitive |
| productivity | Task-focused | Reduced emotional signal weight |

### security

```yaml
security:
  soul_drift_threshold: 0.15   # Alert threshold for identity drift (default: 0.15)
```

### models

```yaml
models:
  providers:
    - name: anthropic
      api_key_env: "ANTHROPIC_API_KEY"
    - name: openai
      api_key_env: "OPENAI_API_KEY"
    - name: ollama
      api_key_env: null         # Ollama doesn't need an API key
```

### backup

```yaml
backup:
  enabled: false               # Enable automatic backups
  interval: "daily"            # daily | weekly
  retention_count: 7           # Number of backups to keep
```

## Hot Reload

Non-critical settings can be changed without restarting the gateway:
- Convergence profile
- Spending caps
- Channel options
- Backup settings

Critical settings require a restart:
- Gateway bind/port
- Database path
- Agent isolation mode

## Validation

Validate your config against the JSON schema:

```bash
# The schema is at schemas/ghost-config.schema.json
ghost serve --config ghost.yml  # Validates on startup
```
