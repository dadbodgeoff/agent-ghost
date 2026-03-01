# ghost-channels

> Unified channel adapter framework — one trait, six platforms. CLI, WebSocket, Telegram, Discord, Slack, and WhatsApp all normalize to the same `InboundMessage`/`OutboundMessage` types.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 5 (Agent Services) |
| Type | Library |
| Location | `crates/ghost-channels/` |
| Workspace deps | None (standalone) |
| External deps | `reqwest`, `async-trait`, `tokio`, `serde`, `chrono`, `uuid`, `tracing`, `thiserror` |
| Modules | `adapter` (trait), `types`, `streaming`, `adapters/` (6 implementations) |
| Public API | `ChannelAdapter` trait, `InboundMessage`, `OutboundMessage`, `StreamingFormatter`, 6 adapter structs |
| Supported channels | CLI, WebSocket, Telegram, Discord, Slack, WhatsApp |
| Test coverage | Object safety tests, connect/disconnect tests, streaming formatter tests, sidecar restart tests |
| Downstream consumers | `ghost-gateway` (channel lifecycle), `ghost-agent-loop` (message I/O) |

---

## Why This Crate Exists

GHOST agents need to communicate with users across multiple platforms. A user might interact via a CLI during development, a WebSocket in a web dashboard, Telegram on mobile, or Slack in a team workspace. Without a unified abstraction, the agent loop would need platform-specific code for each channel — a maintenance nightmare.

`ghost-channels` solves this with a single `ChannelAdapter` trait that all platforms implement. The agent loop sees only `InboundMessage` and `OutboundMessage` — it doesn't know or care whether the message came from Discord or a terminal.

### Design Principles

1. **Object-safe trait.** `ChannelAdapter` can be used as `Box<dyn ChannelAdapter>`, enabling runtime channel selection without generics.
2. **Normalized messages.** Every platform's message format is converted to `InboundMessage` (with sender, content, timestamp, attachments) and `OutboundMessage` (with content, reply_to, attachments).
3. **Streaming support.** Channels that support message editing (Slack, Discord, Telegram, WebSocket) can show streaming LLM responses by editing the message in place. The `StreamingFormatter` handles chunk buffering and edit throttling.
4. **Graceful degradation.** If a channel doesn't support a feature (e.g., WhatsApp doesn't support editing), the adapter reports this via `supports_streaming()` and `supports_editing()`, and the agent loop falls back to sending complete messages.

---

## Module Breakdown

### `adapter.rs` — The `ChannelAdapter` Trait

```rust
#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    async fn connect(&mut self) -> Result<(), String>;
    async fn disconnect(&mut self) -> Result<(), String>;
    async fn send(&self, message: OutboundMessage) -> Result<(), String>;
    async fn receive(&mut self) -> Result<InboundMessage, String>;
    fn supports_streaming(&self) -> bool;
    fn supports_editing(&self) -> bool;
    fn channel_type(&self) -> &str;
}
```

**Why `async_trait`?** Channel operations are inherently async — network I/O for Telegram, WebSocket connections, Slack API calls. The `async_trait` crate enables async methods in traits (which Rust doesn't natively support in object-safe traits yet).

**Why `String` errors instead of a typed error?** Simplicity. Each adapter has different failure modes (HTTP errors, WebSocket disconnects, stdin EOF). A unified error enum would be large and rarely matched on — the agent loop just logs the error and retries. `String` keeps the trait minimal.

### `types.rs` — Normalized Messages

```rust
pub struct InboundMessage {
    pub id: Uuid,              // UUIDv7 (time-ordered)
    pub channel: String,       // "cli", "telegram", "discord", etc.
    pub sender: String,        // Username or phone number
    pub content: String,       // Message text
    pub timestamp: DateTime<Utc>,
    pub attachments: Vec<Attachment>,
}
```

**UUIDv7 for message IDs:** Time-ordered UUIDs allow chronological sorting without additional metadata. This is useful for conversation history display.

**Attachments:** Each attachment has a filename, content type, and raw bytes. This handles images, files, and other media that platforms support.

### `streaming.rs` — Chunk Buffering and Edit Throttle

```rust
pub struct StreamingFormatter {
    buffer: String,
    last_flush: Instant,
    throttle: Duration,    // Default: 100ms
}
```

LLM responses arrive as a stream of small chunks ("Hello", " world", "!"). Sending each chunk as a separate message would flood the channel. The `StreamingFormatter` buffers chunks and flushes at a throttled rate:

1. `push_chunk("Hello")` — adds to buffer
2. `should_flush()` — returns true if throttle duration has elapsed
3. `flush()` — returns accumulated content and resets the buffer

The default throttle is 100ms — fast enough to feel responsive, slow enough to avoid API rate limits on platforms like Slack and Discord.

---

### `adapters/` — Six Platform Implementations

#### CLI (`cli.rs`)

The simplest adapter. Reads from stdin, writes to stdout. No authentication, no network. Used for development and testing.

| Feature | Support |
|---------|---------|
| Streaming | No |
| Editing | No |
| Auth | None |

#### WebSocket (`websocket.rs`)

Bridges WebSocket connections to the agent loop. Used by the SvelteKit dashboard. Inbound messages are pushed to a `VecDeque` by the WebSocket handler; `receive()` pops from the queue.

| Feature | Support |
|---------|---------|
| Streaming | Yes |
| Editing | Yes |
| Default bind | `127.0.0.1:18789` |

#### Telegram (`telegram.rs`)

Uses the Telegram Bot API with long polling for inbound and REST for outbound.

- **Inbound:** `GET /getUpdates?offset={last_update_id+1}&timeout=30` — 30-second long poll
- **Outbound:** `POST /sendMessage` with `chat_id` and `text`
- **Threading:** Supports `reply_to_message_id` for threaded conversations

| Feature | Support |
|---------|---------|
| Streaming | Yes (via message editing) |
| Editing | Yes |
| Auth | Bot token |

#### Discord (`discord.rs`)

Uses the Discord Gateway WebSocket for inbound and REST API for outbound.

- **Inbound:** Connect to `wss://gateway.discord.gg`, handle HELLO → IDENTIFY → READY handshake, listen for `MESSAGE_CREATE` events
- **Outbound:** `POST /channels/{id}/messages`
- **Activation:** Mention-based — only responds when the bot is @mentioned

| Feature | Support |
|---------|---------|
| Streaming | No |
| Editing | Yes |
| Auth | Bot token |

#### Slack (`slack.rs`)

Uses Socket Mode (WebSocket) for inbound and Web API for outbound.

- **Inbound:** Call `apps.connections.open` with app token to get WebSocket URL, listen for `event_callback` with message type
- **Outbound:** `POST chat.postMessage` with bot token

| Feature | Support |
|---------|---------|
| Streaming | No |
| Editing | Yes |
| Auth | Bot token + App token |

#### WhatsApp (`whatsapp.rs`)

Two modes: Cloud API (official Meta Business API) and Baileys sidecar (self-hosted Node.js).

**Cloud API mode:**
- **Inbound:** Webhook receiver (messages pushed to `inbound_queue`)
- **Outbound:** `POST https://graph.facebook.com/v18.0/{phone_number_id}/messages`

**Sidecar mode:**
- Spawns a Node.js Baileys process
- Communicates via stdin/stdout JSON-RPC
- Restarts up to 3 times on crash, then degrades gracefully

| Feature | Support |
|---------|---------|
| Streaming | No |
| Editing | No |
| Auth | Access token + Phone number ID (Cloud API) |
| Sidecar restarts | Max 3 |

---

## Security Properties

### No Token Storage

Channel adapters receive tokens as constructor parameters. They don't persist tokens to disk. Token management is handled by `ghost-oauth` (for OAuth-based platforms) or `ghost-secrets` (for API keys).

### Platform-Specific Auth

Each adapter uses the platform's native authentication mechanism. No custom auth schemes that could introduce vulnerabilities.

### Graceful Degradation

If a channel disconnects or an API call fails, the adapter returns an error. The agent loop can retry or switch to a different channel. No panics, no data loss.

---

## Downstream Consumer Map

```
ghost-channels (Layer 5)
├── ghost-gateway (Layer 8)
│   └── Creates and manages channel adapters
│   └── Routes inbound messages to agent loop
│   └── Routes outbound messages from agent loop to channels
└── ghost-agent-loop (Layer 7)
    └── Calls adapter.receive() for user input
    └── Calls adapter.send() for agent responses
    └── Uses StreamingFormatter for streaming responses
```

---

## Test Strategy

### Adapter Tests (`tests/channel_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `trait_is_object_safe` | All 6 adapters work as `Box<dyn ChannelAdapter>` |
| `channel_types` | Each adapter returns correct type string |
| `streaming_support` | Correct streaming capability reporting |
| `editing_support` | Correct editing capability reporting |
| `connect_disconnect` | CLI adapter lifecycle |
| `send_message` | CLI adapter stdout output |
| `sidecar_restart_within_limit` | WhatsApp restarts 3 times |
| `sidecar_restart_exceeds_limit` | 4th restart returns false |
| `inbound_message_normalizes` | Message construction |
| `buffers_chunks` | StreamingFormatter accumulation |
| `flush_returns_content` | Flush empties buffer |
| `throttle_respected` | 10s throttle prevents immediate flush |

---

## File Map

```
crates/ghost-channels/
├── Cargo.toml                          # Deps: reqwest, async-trait, tokio
├── src/
│   ├── lib.rs                          # Module declarations
│   ├── adapter.rs                      # ChannelAdapter trait (object-safe)
│   ├── types.rs                        # InboundMessage, OutboundMessage, Attachment
│   ├── streaming.rs                    # StreamingFormatter with chunk buffering
│   └── adapters/
│       ├── mod.rs                      # Adapter re-exports
│       ├── cli.rs                      # stdin/stdout adapter
│       ├── websocket.rs               # WebSocket adapter (dashboard)
│       ├── telegram.rs                # Telegram Bot API (long polling)
│       ├── discord.rs                 # Discord Gateway + REST
│       ├── slack.rs                   # Slack Socket Mode + Web API
│       └── whatsapp.rs               # WhatsApp Cloud API / Baileys sidecar
└── tests/
    └── channel_tests.rs               # Object safety, lifecycle, streaming tests
```

---

## Common Questions

### Why not use a message broker (RabbitMQ, Kafka) between channels and the agent loop?

Overkill for the use case. GHOST runs as a single process (or small cluster). The channel adapters are in-process — no serialization overhead, no network hop. A message broker would add latency, operational complexity, and a new failure mode for no benefit.

### How do I add a new channel?

Implement the `ChannelAdapter` trait for your platform. The trait has 7 methods — `connect`, `disconnect`, `send`, `receive`, `supports_streaming`, `supports_editing`, and `channel_type`. Add the adapter to `adapters/mod.rs` and register it in the gateway's channel configuration.

### Why does WhatsApp have two modes?

The Cloud API is the official Meta Business API — reliable but requires a Meta Business account and phone number verification. Baileys is an open-source library that connects via the WhatsApp Web protocol — no business account needed, but less stable (hence the 3-restart limit).
