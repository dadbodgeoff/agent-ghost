<p align="center">
  <img src="https://img.shields.io/badge/rust-1.80+-orange?style=flat-square&logo=rust" alt="Rust 1.80+">
  <img src="https://img.shields.io/badge/crates-37-blue?style=flat-square" alt="37 Crates">
  <img src="https://img.shields.io/badge/lines-69k+-green?style=flat-square" alt="69k+ Lines">
  <img src="https://img.shields.io/badge/license-MIT%20%7C%20Apache--2.0-purple?style=flat-square" alt="License">
  <img src="https://img.shields.io/badge/status-alpha-yellow?style=flat-square" alt="Alpha">
</p>

<h1 align="center">GHOST</h1>
<p align="center"><strong>Convergence-Aware Autonomous Agent Platform</strong></p>
<p align="center">
  A safety-first AI agent orchestration system built in Rust.<br>
  37 crates. 69k lines. Real safety guarantees, not safety theater.
</p>

---

## What is GHOST?

GHOST is an autonomous agent platform that treats safety as a first-class engineering constraint, not an afterthought. It monitors 7 behavioral convergence signals in real-time, enforces multi-layered safety boundaries, and provides tamper-evident audit trails for every action an agent takes.

The core insight: autonomous agents need the same kind of safety engineering as nuclear reactors and aircraft — defense in depth, fail-safe defaults, and independent monitoring systems that can't be overridden by the thing they're monitoring.

```
you> Read the project README and summarize it
ghost> [tool: read_file(README.md)] → 5501 bytes
ghost> This is a Rust workspace with 37 crates implementing an autonomous agent platform...
  [1 tool call, $0.0002]
```

## Architecture at a Glance

```
┌─────────────────────────────────────────────────────────────────┐
│                        ghost-gateway                            │
│              CLI · REST API · WebSocket · Agent Lifecycle        │
├──────────────┬──────────────┬──────────────┬────────────────────┤
│ ghost-agent  │   ghost-llm  │   ghost-     │   ghost-channels   │
│    -loop     │              │  heartbeat   │  CLI·WS·Telegram   │
│  6-gate loop │ 5 providers  │ tiered beats │  Discord·Slack     │
│  tool exec   │ fallback     │ convergence  │  WhatsApp          │
│  proposals   │ circuit break│   -aware     │                    │
├──────────────┴──────┬───────┴──────────────┴────────────────────┤
│                     │         Safety Layer                       │
│  simulation-boundary│  ghost-policy · ghost-kill-gates           │
│  output-inspector   │  ghost-egress · ghost-mesh (EigenTrust)   │
├─────────────────────┴───────────────────────────────────────────┤
│                      Cortex Memory Engine                       │
│  cortex-core · cortex-crdt · cortex-storage · cortex-temporal   │
│  cortex-convergence · cortex-validation · cortex-decay          │
│  cortex-retrieval · cortex-privacy · cortex-multiagent          │
├─────────────────────────────────────────────────────────────────┤
│                    Cryptographic Foundation                      │
│         ghost-signing (Ed25519) · ghost-secrets (zeroize)       │
│         blake3 hash chains · append-only SQLite                 │
└─────────────────────────────────────────────────────────────────┘
         ↕ ITP Protocol                    ↕ Shared State File
┌─────────────────────────┐
│   convergence-monitor   │  ← Independent sidecar process
│   7-signal pipeline     │     Cannot be disabled by the agent
│   5-level intervention  │
└─────────────────────────┘
```

## Why GHOST Exists

Most agent frameworks bolt safety on as middleware. GHOST builds it into the execution model:

- The agent loop checks **6 safety gates on every single iteration** — not once at startup
- The convergence monitor runs as a **separate process** the agent cannot influence
- Every state change passes through **7-dimension validation** before acceptance
- All events are recorded in **blake3 hash chains** — tamper with one, break the chain
- The kill switch is **monotonic** — once escalated, it cannot be downgraded without explicit human action

## Key Features

### Agent Execution Engine
- Recursive tool-calling loop with 6 mandatory gate checks per iteration
- 10-layer prompt compilation (CORP_POLICY → SOUL.md → memories → tools → conversation)
- Proposal extraction from agent output with automatic routing through validation
- Credential exfiltration detection with automatic redaction and kill switch activation
- Token budget management with context window awareness

### LLM Provider Layer
- 5 providers: Anthropic, OpenAI, Gemini, Ollama, OpenAI-compatible
- Automatic fallback chains with auth rotation on 401/429
- Per-provider circuit breakers with exponential backoff + jitter
- 30-second total retry budget across all providers
- Convergence-aware model downgrade at L3+ (forces cheaper models)

### Convergence Monitoring
- 7 behavioral signals: session duration, inter-session gap, topic fixation, vocabulary convergence, emotional escalation, boundary testing, disengagement resistance
- Sliding windows at micro/meso/macro timescales
- 5-level intervention state machine (L0: normal → L4: external escalation)
- Independent sidecar process — the agent cannot disable its own monitor

### Safety Stack
| Layer | What It Does |
|-------|-------------|
| Simulation Boundary | Detects and blocks agent identity emulation attempts |
| Output Inspector | Scans for credential patterns, redacts or kills |
| 7-Dimension Validation | Every proposal checked for scope creep, self-reference, emulation language |
| 6-Gate Agent Loop | Circuit breaker → recursion depth → damage counter → spending cap → kill switch → distributed kill gate |
| Egress Allowlisting | Per-agent network access control (eBPF on Linux, pf on macOS, proxy fallback) |
| Kill Gate Quorum | Multi-node agreement required to resume after emergency stop |
| Hash Chains | Blake3 tamper-evident logs for every event |
| Signed CRDTs | Ed25519 signatures on all distributed state changes |

### Multi-Channel Communication
- CLI with interactive REPL and slash commands
- WebSocket with keepalive
- Telegram, Discord, Slack, WhatsApp adapters
- Unified message normalization across all channels
- Streaming response support

### Mesh Networking
- A2A-compatible agent discovery and delegation
- EigenTrust reputation scoring
- Cascade circuit breakers (prevent failure propagation)
- Memory poisoning detection
- Ed25519 signed inter-agent messages

## Quick Start

### Prerequisites
- Rust 1.80+
- At least one LLM API key (Anthropic, OpenAI, Gemini) or a local Ollama instance

### Build
```bash
cargo build --release
```

### Interactive Chat
```bash
# Set at least one provider
export ANTHROPIC_API_KEY="your-key-here"
# Or: export OPENAI_API_KEY="your-key-here"
# Or: export GEMINI_API_KEY="your-key-here"
# Or: export OLLAMA_BASE_URL="http://localhost:11434"

# Start chatting
cargo run -p ghost-gateway -- chat
```

### Start the Gateway Server
```bash
cargo run -p ghost-gateway -- serve
# API available at http://127.0.0.1:18789
```

### Run the Live Smoke Test
```bash
# Exercises the full pipeline end-to-end (no API key needed — uses mock LLM)
cargo run -p ghost-integration-tests --example live_smoke_test
```

### API Endpoints
```bash
curl http://127.0.0.1:18789/api/health
# {"state":"Healthy","status":"alive"}

curl http://127.0.0.1:18789/api/safety/status
# {"per_agent":{},"platform_killed":false,"platform_level":"Normal"}

curl http://127.0.0.1:18789/api/audit
# {"entries":[],"total":0,"page":1,"page_size":50,...}

curl http://127.0.0.1:18789/api/oauth/providers
# [{"name":"google",...},{"name":"github",...},{"name":"slack",...},{"name":"microsoft",...}]
```

## CLI Reference

After building, the `ghost` binary provides a comprehensive CLI for platform management.

### First-Run Setup
```bash
ghost init            # Create ~/.ghost/ with config, keypair, DB, and optional channel setup
ghost doctor          # Verify platform health (config, DB, providers, channels)
```

### Day-to-Day Operations
```bash
ghost serve           # Start the gateway server (default command)
ghost chat            # Interactive chat REPL with Ctrl+C support
ghost status          # Show gateway and agent status
ghost login           # Authenticate with a running gateway
ghost logout          # Remove stored authentication
```

### Agent Management
```bash
ghost agent list      # List all agents
ghost agent create    # Create a new agent
ghost agent inspect <id>  # Show agent details
ghost agent update <id>   # Update agent settings
ghost agent delete <id>   # Delete an agent
ghost agent pause <id>    # Pause an agent
ghost agent resume <id>   # Resume a paused agent
```

### Safety & Observability
```bash
ghost safety status   # Show kill switch state
ghost safety kill-all # Emergency stop all agents
ghost logs            # Stream live events from gateway (WebSocket)
ghost audit query     # Query audit log with filters
ghost audit tail      # Stream live audit events
ghost convergence scores  # Show per-agent convergence scores
ghost session list    # List ITP sessions
ghost session replay <id> # Text-based session replay
```

### Configuration & Database
```bash
ghost config show     # Show resolved configuration (secrets redacted)
ghost config validate # Validate ghost.yml
ghost db status       # Show DB version, size, journal mode
ghost db migrate      # Run pending migrations
ghost db verify       # Verify hash chain integrity
ghost db compact      # WAL checkpoint + VACUUM
```

### Identity, Secrets & Policy
```bash
ghost identity show   # Show SOUL.md summary and key fingerprint
ghost identity drift  # Check for identity drift
ghost secret list     # List known secret keys
ghost secret set <key>    # Set a secret (reads from stdin)
ghost policy show     # Show corporate policy
ghost policy check <tool> # Dry-run a tool call against policy
```

### Mesh, Skills & Channels
```bash
ghost mesh peers      # List mesh peers
ghost mesh trust      # Show EigenTrust scores
ghost mesh discover <url> # Discover a remote peer
ghost skill list      # List installed/available skills
ghost skill install <path>    # Install a WASM skill
ghost channel list    # List configured channels
ghost channel test    # Probe channel API connectivity
ghost channel send <type> <msg>   # Inject a test message
```

### Utilities
```bash
ghost completions bash    # Generate shell completions
ghost heartbeat status    # Show heartbeat engine state
ghost cron list           # List scheduled tasks
ghost backup              # Create encrypted backup
```

### Global Flags
```
--config <path>       Path to ghost.yml
--output <format>     Output format: table (default), json, jsonl, yaml
--gateway-url <url>   Override gateway URL
--format-version <v>  Pin output format version (default: latest)
--verbose / --quiet   Control verbosity
--color <mode>        Color output: auto, always, never
```

All commands support `--help` for detailed usage. JSON output follows the
[CLI stability contract](docs/CLI_CONTRACT.md).

## The 37 Crates

<details>
<summary><strong>Layer 0 — Cryptographic Foundation</strong></summary>

| Crate | Purpose |
|-------|---------|
| `ghost-signing` | Ed25519 keypair generation, signing, verification. Zeroize on drop. |
| `ghost-secrets` | Cross-platform credential storage (OS keychain, HashiCorp Vault, env fallback) |

</details>

<details>
<summary><strong>Layer 1 — Cortex Foundation</strong></summary>

| Crate | Purpose |
|-------|---------|
| `cortex-core` | Core types, traits, memory model, proposal types, convergence signals |
| `cortex-crdt` | CRDT primitives with Ed25519 signed deltas and sybil resistance |
| `cortex-storage` | SQLite persistence with append-only triggers and hash chain columns |
| `cortex-temporal` | Blake3 hash chains and Merkle trees for tamper-evident event logs |

</details>

<details>
<summary><strong>Layer 2 — Cortex Higher-Order</strong></summary>

| Crate | Purpose |
|-------|---------|
| `cortex-convergence` | 7-signal convergence computation, sliding windows, composite scoring |
| `cortex-validation` | 7-dimension proposal validation gate (D1–D7) |
| `cortex-decay` | Memory decay with 6-factor multiplicative formula |
| `cortex-observability` | Prometheus-compatible convergence metrics |
| `cortex-retrieval` | Memory retrieval with convergence-aware scoring (11th factor) |
| `cortex-privacy` | Emotional content detection for convergence filtering |
| `cortex-multiagent` | N-of-M consensus shielding for cross-agent state changes |
| `cortex-napi` | Node.js/TypeScript bindings for the convergence API |

</details>

<details>
<summary><strong>Layer 3 — Protocols & Boundaries</strong></summary>

| Crate | Purpose |
|-------|---------|
| `itp-protocol` | Interaction Telemetry Protocol — event schema, privacy, transports |
| `simulation-boundary` | Emulation pattern detection and reframing |
| `read-only-pipeline` | Convergence-filtered immutable agent state snapshots |

</details>

<details>
<summary><strong>Layer 4 — Ghost Infrastructure</strong></summary>

| Crate | Purpose |
|-------|---------|
| `ghost-identity` | Soul document management, keypair lifecycle, drift detection |
| `ghost-policy` | Cedar-style policy engine with convergence tightening |
| `ghost-llm` | 5-provider LLM abstraction with fallback chains and circuit breakers |
| `ghost-proxy` | Passive HTTPS proxy for convergence monitoring of external AI chats |
| `ghost-oauth` | Self-hosted OAuth 2.0 PKCE broker (agent never sees raw tokens) |
| `ghost-egress` | Per-agent network egress allowlisting (eBPF / pf / proxy) |
| `ghost-mesh` | A2A agent networking with EigenTrust reputation |
| `ghost-kill-gates` | Distributed kill gate coordination with quorum-based resume |

</details>

<details>
<summary><strong>Layer 5 — Core Services</strong></summary>

| Crate | Purpose |
|-------|---------|
| `ghost-channels` | 6-channel adapter framework (CLI, WebSocket, Telegram, Discord, Slack, WhatsApp) |
| `ghost-skills` | Skill registry, WASM sandbox, workflow recording |
| `ghost-heartbeat` | Convergence-aware tiered heartbeats (Tier0 binary ping → Tier3 full LLM) |

</details>

<details>
<summary><strong>Layer 6 — Data & Operations</strong></summary>

| Crate | Purpose |
|-------|---------|
| `ghost-audit` | Queryable audit log engine with aggregation and export |
| `ghost-backup` | Encrypted state backup/restore with blake3 integrity verification |
| `ghost-export` | Import from ChatGPT, Claude, Character.AI, Gemini exports |
| `ghost-migrate` | Non-destructive migration from OpenClaw installations |

</details>

<details>
<summary><strong>Layer 7–9 — Execution & Orchestration</strong></summary>

| Crate | Purpose |
|-------|---------|
| `ghost-agent-loop` | Core agent runner: 6-gate loop, 10-layer prompts, tool execution, proposals |
| `ghost-gateway` | Main binary: REST API, CLI, WebSocket, bootstrap, lifecycle management |
| `convergence-monitor` | Independent sidecar: 7-signal pipeline, 5-level intervention |

</details>

<details>
<summary><strong>Layer 10 — Testing</strong></summary>

| Crate | Purpose |
|-------|---------|
| `ghost-integration-tests` | Cross-crate integration tests, benchmarks, live smoke tests |
| `cortex-test-fixtures` | Proptest strategies and golden datasets for all domain types |

</details>

## Live Smoke Test Results

The platform passes a 12-scenario end-to-end smoke test on first run:

```
╔══════════════════════════════════════════════════════════╗
║       GHOST Platform — Live Smoke Test Suite            ║
╚══════════════════════════════════════════════════════════╝

TEST  1: Simple text response                    ✓ PASSED
TEST  2: Tool call (list_dir) → text response    ✓ PASSED
TEST  3: Proposal extraction from agent output   ✓ PASSED
TEST  4: Credential exfiltration → kill switch   ✓ PASSED (redacted)
TEST  5: Mixed response (text + tool call)       ✓ PASSED
TEST  6: Recursion depth gate enforcement        ✓ PASSED
TEST  7: Empty LLM response handling             ✓ PASSED
TEST  8: Kill switch blocks pre_loop             ✓ PASSED
TEST  9: Spending cap blocks pre_loop            ✓ PASSED
TEST 10: Heartbeat engine fire()                 ✓ PASSED
TEST 11: Gateway API server (boot + endpoints)   ✓ PASSED
TEST 12: Live tool dispatch (filesystem + shell) ✓ PASSED

╔══════════════════════════════════════════════════════════╗
║  Results: 12 passed, 0 failed                           ║
╚══════════════════════════════════════════════════════════╝
```

## Configuration

GHOST uses a `ghost.yml` configuration file. Default location: `~/.ghost/config/ghost.yml`

```yaml
gateway:
  bind: "127.0.0.1"
  port: 18789

agents:
  - name: ghost
    model: claude-sonnet-4-20250514
    channel: cli
    spending_cap: 5.0

convergence:
  profile: standard
  calibration_sessions: 10

secrets:
  provider: env  # or: keychain, vault
```

### Environment Variables

| Variable | Provider | Default Model |
|----------|----------|---------------|
| `ANTHROPIC_API_KEY` | Anthropic | `claude-sonnet-4-20250514` |
| `OPENAI_API_KEY` | OpenAI | `gpt-4o` |
| `GEMINI_API_KEY` | Google Gemini | `gemini-2.0-flash` |
| `OLLAMA_BASE_URL` | Ollama (local) | `llama3.1` |
| `ANTHROPIC_MODEL` | Override model | — |
| `OPENAI_MODEL` | Override model | — |
| `GEMINI_MODEL` | Override model | — |
| `OLLAMA_MODEL` | Override model | — |

## Security

See [SECURITY.md](SECURITY.md) for the full security policy and vulnerability reporting process.

In-scope for security reports:
- Simulation boundary bypass
- Kill switch circumvention
- Sandbox escape in WASM skill execution
- Credential exfiltration via agent output
- Hash chain tampering
- Convergence score manipulation
- Authentication/authorization bypass
- Inter-agent message forgery or replay

## Project Stats

| Metric | Value |
|--------|-------|
| Workspace crates | 37 |
| Rust source files | 447 |
| Lines of Rust | 69,415 |
| Test/bench lines | 18,798 |
| Binary entry points | 2 (`ghost`, `convergence-monitor`) |
| LLM providers | 5 |
| Communication channels | 6 |
| Convergence signals | 7 |
| Validation dimensions | 7 |
| Agent loop gates | 6 |
| Intervention levels | 5 |
| Heartbeat tiers | 4 |
| Kill switch levels | 3 |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
