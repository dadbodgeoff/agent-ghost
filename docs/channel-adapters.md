# Channel Adapters

GHOST supports 6 communication channels. Each channel implements the `ChannelAdapter` trait providing unified message handling.

## CLI

The default channel. No configuration needed.

```yaml
channels:
  - channel_type: cli
    agent: "ghost"
```

Start an interactive session:
```bash
ghost chat
```

## WebSocket

For programmatic access and the web dashboard.

```yaml
channels:
  - channel_type: websocket
    agent: "ghost"
    options:
      bind: "127.0.0.1"
      port: 18791
```

Connect via any WebSocket client:
```javascript
const ws = new WebSocket("ws://127.0.0.1:18791");
ws.send(JSON.stringify({ type: "message", content: "Hello" }));
```

## Telegram

Uses [teloxide](https://github.com/teloxide/teloxide) with long polling.

```yaml
channels:
  - channel_type: telegram
    agent: "ghost"
    options:
      bot_token: "${TELEGRAM_BOT_TOKEN}"
```

Setup:
1. Create a bot via [@BotFather](https://t.me/BotFather)
2. Set `TELEGRAM_BOT_TOKEN` environment variable
3. Start the gateway — the bot will begin polling

Features: message editing for streaming responses, inline keyboards.

## Discord

Uses [serenity-rs](https://github.com/serenity-rs/serenity) with slash commands.

```yaml
channels:
  - channel_type: discord
    agent: "ghost"
    options:
      bot_token: "${DISCORD_BOT_TOKEN}"
      guild_id: "123456789"
```

Setup:
1. Create a Discord application at [discord.com/developers](https://discord.com/developers)
2. Create a bot and copy the token
3. Invite the bot to your server with message + slash command permissions
4. Set `DISCORD_BOT_TOKEN` and configure `guild_id`

## Slack

Uses the Bolt protocol in WebSocket mode.

```yaml
channels:
  - channel_type: slack
    agent: "ghost"
    options:
      bot_token: "${SLACK_BOT_TOKEN}"
      app_token: "${SLACK_APP_TOKEN}"
```

Setup:
1. Create a Slack app at [api.slack.com](https://api.slack.com)
2. Enable Socket Mode and create an app-level token
3. Add bot scopes: `chat:write`, `app_mentions:read`, `im:history`
4. Install to workspace

## WhatsApp

Uses a Node.js sidecar bridge via [@whiskeysockets/baileys](https://github.com/WhiskeySockets/Baileys).

```yaml
channels:
  - channel_type: whatsapp
    agent: "ghost"
    options:
      bridge_path: "extension/bridges/baileys-bridge"
```

Setup:
1. Ensure Node.js 18+ is installed
2. Install bridge dependencies: `cd extension/bridges/baileys-bridge && npm install`
3. Start the gateway — it will spawn the bridge process
4. Scan the QR code displayed in the terminal to link WhatsApp Web

The bridge communicates with the gateway via JSON-RPC over stdin/stdout. If the bridge crashes, the WhatsApp adapter restarts it up to 3 times before degrading gracefully.

## Streaming Support

Channels that support streaming (WebSocket, Telegram, Discord, Slack) receive chunked responses via the `StreamingFormatter`. The formatter:

- Buffers chunks to avoid excessive edits
- Respects per-channel edit throttle limits
- Runs `SimulationBoundaryEnforcer` on text at delivery time

## Adding Custom Channels

Implement the `ChannelAdapter` trait:

```rust
#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    async fn connect(&mut self) -> Result<(), ChannelError>;
    async fn disconnect(&mut self) -> Result<(), ChannelError>;
    async fn send(&self, msg: &OutboundMessage) -> Result<(), ChannelError>;
    async fn receive(&mut self) -> Result<InboundMessage, ChannelError>;
    fn supports_streaming(&self) -> bool;
    fn supports_editing(&self) -> bool;
}
```
