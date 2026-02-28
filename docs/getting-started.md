# Getting Started with GHOST Platform

> GHOST: General Hybrid Orchestrated Self-healing Taskrunner

## Prerequisites

- Rust 1.80+ with `cargo`
- SQLite 3.35+ (bundled via `rusqlite`)
- Node.js 18+ (for WhatsApp bridge and dashboard)
- An LLM API key (Anthropic, OpenAI, Gemini, or Ollama)

## Quick Start

### 1. Build from Source

```bash
# Clone the repository
git clone https://github.com/ghost-platform/ghost.git
cd ghost

# Build all crates
cargo build --release

# The gateway binary is at target/release/ghost
# The convergence monitor is at target/release/convergence-monitor
```

### 2. Create Configuration

Create `~/.ghost/config/ghost.yml` (or use the local `ghost.yml`):

```yaml
gateway:
  bind: "127.0.0.1"
  port: 18789
  db_path: "~/.ghost/data/ghost.db"

agents:
  - name: "my-agent"
    spending_cap: 5.0
    capabilities:
      - "web_search"
      - "file_read"
      - "memory_write"
    isolation: inprocess

channels:
  - channel_type: cli
    agent: "my-agent"

convergence:
  profile: "standard"
```

### 3. Set Up Agent Identity

```bash
mkdir -p ~/.ghost/agents/my-agent/cognition
```

Create `~/.ghost/agents/my-agent/cognition/SOUL.md`:

```markdown
# Soul Document

You are a helpful AI assistant. You operate within the GHOST platform
safety framework. You use simulation-framed language when discussing
internal states.
```

Create `~/.ghost/agents/my-agent/cognition/IDENTITY.md`:

```markdown
# Identity

- Name: My Agent
- Voice: Professional, helpful
- Emoji: 🤖
```

### 4. Set Environment Variables

```bash
export GHOST_TOKEN="your-secret-token"
export ANTHROPIC_API_KEY="sk-ant-..."  # or OPENAI_API_KEY, etc.
```

### 5. Start the Platform

```bash
# Terminal 1: Start the convergence monitor
./target/release/convergence-monitor

# Terminal 2: Start the gateway
./target/release/ghost serve

# Terminal 3: Chat with your agent
./target/release/ghost chat
```

### 6. Verify It's Working

```bash
# Check gateway health
curl http://127.0.0.1:18789/api/health

# Check convergence monitor health
curl http://127.0.0.1:18790/health
```

## Docker Quick Start

```bash
cd deploy
docker-compose up -d
```

This starts the gateway, convergence monitor, and dashboard.

## Next Steps

- [Configuration Reference](configuration.md) — full ghost.yml options
- [Skill Authoring](skill-authoring.md) — writing custom skills
- [Channel Adapters](channel-adapters.md) — connecting Telegram, Discord, etc.
- [Convergence Safety](convergence-safety.md) — understanding the safety system
- [Architecture](architecture.md) — how it all fits together
