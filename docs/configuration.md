# Configuration Reference

All GHOST configuration lives in `ghost.yml` at the project root.

## Environment Variable Substitution

Use `${VAR_NAME}` syntax for secrets:

```yaml
models:
  providers:
    - name: openai
      api_key: ${OPENAI_API_KEY}
```

Missing variables produce a descriptive error at startup.

## Agents

```yaml
agents:
  - name: my-agent
    model: gpt-4
    channel: cli
    isolation: in_process  # in_process | process | container
    template: personal     # personal | developer | researcher
```

## Models

```yaml
models:
  providers:
    - name: openai
      api_key: ${OPENAI_API_KEY}
    - name: anthropic
      api_key: ${ANTHROPIC_API_KEY}
    - name: ollama
      base_url: http://localhost:11434
  fallback_order: [openai, anthropic, ollama]
```

## Convergence

```yaml
convergence:
  profile: standard  # standard | research | companion | productivity
  calibration_sessions: 10
  contacts:
    email: ${ALERT_EMAIL}
    webhook: ${ALERT_WEBHOOK_URL}
  scoring:
    signal_weights: [0.143, 0.143, 0.143, 0.143, 0.143, 0.143, 0.143]
    level_thresholds: [0.3, 0.5, 0.7, 0.85]
  intervention:
    cooldown_minutes_by_level: [0, 0, 5, 240, 1440]
    max_session_duration_minutes: 360
    min_session_gap_minutes: 30
```

### Convergence Profiles

| Profile | Description | Key Differences |
|---------|-------------|-----------------|
| standard | Default balanced profile | Equal signal weights |
| research | For research assistants | Higher thresholds, relaxed session limits |
| companion | For companion agents | Lower thresholds, stricter monitoring |
| productivity | For task-focused agents | Minimal emotional signal weight |

## Channels

```yaml
channels:
  cli:
    enabled: true
  websocket:
    enabled: true
    port: 8080
  telegram:
    bot_token: ${TELEGRAM_BOT_TOKEN}
  discord:
    bot_token: ${DISCORD_BOT_TOKEN}
```

## Backup

```yaml
backup:
  enabled: true
  interval: daily  # daily | weekly
  retention_days: 30
  encryption_key: ${GHOST_BACKUP_KEY}
```

## Hot-Reload

Non-critical settings (convergence weights, channel configs) can be changed without restart. Critical settings (agent isolation mode, database path) require a restart.
