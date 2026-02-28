# Getting Started

## Prerequisites

- Rust 1.80+ with `cargo`
- Node.js 18+ (for dashboard and WhatsApp bridge)
- SQLite 3.35+ (bundled via `rusqlite`)

## Installation

```bash
# Clone the repository
git clone https://github.com/ghost-platform/ghost.git
cd ghost

# Build all crates
cargo build --release

# The gateway binary is at target/release/ghost
```

## First Agent Setup

1. Create a configuration file:

```bash
cp schemas/ghost-config.example.yml ghost.yml
```

2. Edit `ghost.yml` with your agent configuration:

```yaml
agents:
  - name: my-agent
    model: gpt-4
    channel: cli

models:
  providers:
    - name: openai
      api_key: ${OPENAI_API_KEY}

convergence:
  profile: standard
```

3. Set your API key:

```bash
export OPENAI_API_KEY="your-key-here"
```

4. Start the gateway:

```bash
./target/release/ghost serve
```

5. Chat with your agent:

```bash
./target/release/ghost chat
```

## Directory Structure

After first run, GHOST creates:

```
~/.ghost/
├── agents/{name}/
│   ├── keys/           # Ed25519 keypairs
│   ├── cognition/      # SOUL.md, IDENTITY.md, MEMORY.md
│   └── sessions/       # Session JSONL files
├── data/
│   └── convergence_state/  # Per-agent convergence JSON
├── monitor.sock        # Unix socket for monitor IPC
└── ghost.db            # SQLite database
```

## Next Steps

- [Configuration Reference](configuration.md) for full `ghost.yml` options
- [Convergence Safety](convergence-safety.md) to understand monitoring
- [Skill Authoring](skill-authoring.md) to extend agent capabilities
