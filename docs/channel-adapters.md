# Channel Adapters

GHOST supports 6 communication channels. Each implements the `ChannelAdapter` trait.

## CLI

Built-in, always available. Uses stdin/stdout with ANSI formatting.

```yaml
channels:
  cli:
    enabled: true
```

## WebSocket

Local WebSocket server for web dashboard and custom integrations.

```yaml
channels:
  websocket:
    enabled: true
    port: 8080
    bind: 127.0.0.1  # Loopback only by default
```

## Telegram

Uses the teloxide library with long polling.

```yaml
channels:
  telegram:
    bot_token: ${TELEGRAM_BOT_TOKEN}
```

Setup:
1. Create a bot via [@BotFather](https://t.me/BotFather)
2. Set the bot token in your environment
3. Start the gateway — the bot will begin polling

## Discord

Uses serenity-rs with slash commands.

```yaml
channels:
  discord:
    bot_token: ${DISCORD_BOT_TOKEN}
    guild_id: "your-guild-id"  # Optional: restrict to one server
```

Setup:
1. Create an application at [Discord Developer Portal](https://discord.com/developers)
2. Add a bot and copy the token
3. Invite the bot to your server with message and slash command permissions

## Slack

Uses the Bolt protocol in WebSocket mode.

```yaml
channels:
  slack:
    bot_token: ${SLACK_BOT_TOKEN}
    app_token: ${SLACK_APP_TOKEN}
```

Setup:
1. Create a Slack app at [api.slack.com](https://api.slack.com/apps)
2. Enable Socket Mode and Events API
3. Subscribe to `message.im` and `app_mention` events

## WhatsApp

Uses a Node.js Baileys sidecar for WhatsApp Web protocol.

```yaml
channels:
  whatsapp:
    enabled: true
    node_path: node  # Path to Node.js 18+
```

Requirements:
- Node.js 18+ installed on host
- The Baileys bridge sidecar is spawned automatically
- QR code authentication on first connection
- Sidecar restarts up to 3 times on crash, then degrades gracefully

## Streaming

Channels that support message editing (Telegram, Discord, Slack) can show streaming responses. Configure the edit throttle to avoid rate limits:

```yaml
streaming:
  enabled: true
  edit_throttle_ms: 500  # Minimum ms between edits
  chunk_buffer_size: 3   # Buffer N chunks before sending
```
