# GHOST CLI Reference

> Complete command-line interface documentation for the GHOST Platform.
>
> **Version**: 0.1.0  
> **Last Updated**: March 2026

---

## Table of Contents

1. [Installation](#installation)
2. [Global Options](#global-options)
3. [Core Commands](#core-commands)
4. [Agent Management](#agent-management)
5. [Safety & Kill Switch](#safety--kill-switch)
6. [Database Management](#database-management)
7. [Audit & Observability](#audit--observability)
8. [Convergence Monitoring](#convergence-monitoring)
9. [Session Management](#session-management)
10. [Identity & Signing](#identity--signing)
11. [Secret Management](#secret-management)
12. [Policy Management](#policy-management)
13. [Mesh Networking](#mesh-networking)
14. [Skills & Channels](#skills--channels)
15. [Utilities](#utilities)

---

## Installation

```bash
# Build from source
cargo build --release -p ghost-gateway

# Binary location
./target/release/ghost

# Add to PATH (optional)
export PATH="$PATH:$(pwd)/target/release"
```

---

## Global Options

Available for all commands:

| Flag | Short | Description | Default |
|------|-------|-------------|---------|
| `--config <PATH>` | `-c` | Path to ghost.yml configuration file | `~/.ghost/config/ghost.yml` |
| `--output <FORMAT>` | | Output format: `table`, `json`, `yaml` | `table` |
| `--gateway-url <URL>` | | Gateway URL (overrides config) | From config |
| `--verbose` | `-v` | Enable verbose output | `false` |
| `--quiet` | `-q` | Suppress non-essential output | `false` |
| `--color <CHOICE>` | | Color output: `auto`, `always`, `never` | `auto` |
| `--format-version <VER>` | | Pin structured output format version | `latest` |

**Example:**
```bash
ghost --config ~/my-config.yml --output json status
```

---

## Core Commands

### `ghost serve`

Start the gateway server (default command if none specified).

```bash
ghost serve
# or just
ghost
```

**What it does:**
- Loads configuration from `ghost.yml`
- Runs database migrations
- Checks convergence monitor health (degrades gracefully if unavailable)
- Initializes agents and channels
- Starts REST API server on configured bind/port
- Starts WebSocket server for real-time events

**Exit codes:**
- `0` - Clean shutdown
- `69` - Service unavailable (bootstrap failed)
- `70` - Internal software error
- `76` - Protocol error (database/network)
- `78` - Configuration error

---

### `ghost chat`

Interactive chat session with an agent.

```bash
ghost chat
```

**Features:**
- REPL-style interface
- Streaming responses
- Tool call visualization
- Convergence score display
- Ctrl+C to exit

**Environment variables:**
- `ANTHROPIC_API_KEY` - Anthropic API key
- `OPENAI_API_KEY` - OpenAI API key
- `GEMINI_API_KEY` - Google Gemini API key
- `OLLAMA_BASE_URL` - Ollama server URL

---

### `ghost status`

Show gateway and agent status.

```bash
ghost status
ghost status --output json
```

**Output includes:**
- Gateway health state
- Agent list with convergence scores
- Intervention levels
- Kill switch state
- Convergence monitor connection status

---

### `ghost backup`

Create an encrypted backup of platform state.

```bash
ghost backup --output backup.tar.gz
```

**What's backed up:**
- SQLite database (all tables)
- Agent identities (keypairs)
- Configuration files
- SOUL.md documents

**Encryption:**
- Uses `GHOST_BACKUP_KEY` environment variable
- AES-256-GCM encryption
- Includes integrity hash

---

### `ghost export`

Analyze a data export from an external AI platform.

```bash
ghost export path/to/chatgpt-export.json
```

**Supported formats:**
- ChatGPT export (JSON)
- Claude export (JSON)
- Character.AI export (JSON)
- Gemini export (JSON)

**Output:**
- Conversation count
- Message count
- Date range
- Import preview

---

### `ghost migrate`

Migrate from an OpenClaw installation.

```bash
ghost migrate --source ~/.openclaw
```

**What's migrated:**
- Agent configurations
- Conversation history
- Memory snapshots
- Identity keypairs (if compatible)

---

## Agent Management

### `ghost agent list`

List all agents.

```bash
ghost agent list
ghost agent list --output json
```

**Output columns:**
- Agent ID
- Name
- Status (active, paused, quarantined)
- Convergence score
- Intervention level
- Spending cap

---

### `ghost agent create`

Create a new agent.

```bash
ghost agent create
```

**Interactive prompts:**
- Agent name
- Model selection
- Channel selection
- Spending cap
- Capabilities

**Generates:**
- Ed25519 keypair
- Agent ID (UUID v7)
- SOUL.md template

---

### `ghost agent inspect <id>`

Inspect an agent's details.

```bash
ghost agent inspect agent-123
```

**Output:**
- Full configuration
- Current state
- Convergence history
- Memory statistics
- Session count

---

### `ghost agent delete <id>`

Delete an agent.

```bash
ghost agent delete agent-123
```

**Confirmation required** unless `--yes` flag is used.

**What's deleted:**
- Agent configuration
- Keypair
- Memory snapshots (moved to archive)
- Session history (preserved in audit log)

---

### `ghost agent update <id>`

Update agent settings.

```bash
ghost agent update agent-123
```

**Interactive prompts for:**
- Model
- Spending cap
- Capabilities
- Channel

---

### `ghost agent pause <id>`

Pause an agent (stops processing new messages).

```bash
ghost agent pause agent-123
```

---

### `ghost agent resume <id>`

Resume a paused agent.

```bash
ghost agent resume agent-123
```

---

### `ghost agent quarantine <id>`

Quarantine an agent (requires forensic review to resume).

```bash
ghost agent quarantine agent-123
```

**Quarantine triggers:**
- Manual quarantine via this command
- Automatic quarantine on L3 intervention
- Boundary violation detection
- Credential exfiltration attempt

---

## Safety & Kill Switch

### `ghost safety status`

Show safety status.

```bash
ghost safety status
```

**Output:**
- Platform kill state
- Per-agent intervention levels
- Gate check status
- Kill gate quorum state (if mesh enabled)

---

### `ghost safety kill-all`

Activate platform-wide kill switch.

```bash
ghost safety kill-all
```

**Effects:**
- All agents immediately halted
- `kill_state.json` written to disk
- Broadcast to mesh peers (if enabled)
- Requires explicit `clear` to resume

**Use cases:**
- Emergency stop
- Security incident
- Runaway agent behavior

---

### `ghost safety clear`

Clear kill state and resume normal operation.

```bash
ghost safety clear
```

**Confirmation required.**

**Checks before clearing:**
- No active boundary violations
- No unresolved quarantines
- Convergence monitor healthy
- Kill gate quorum agreement (if mesh enabled)

---

## Database Management

### `ghost db migrate`

Run pending database migrations.

```bash
ghost db migrate
```

**Safe to run multiple times** (idempotent).

**Migrations are append-only** - never destructive.

---

### `ghost db status`

Show database status.

```bash
ghost db status
ghost db status --output json
```

**Output:**
- Current schema version
- Pending migrations
- Database size
- Table row counts
- WAL mode status

---

### `ghost db verify`

Verify hash chain integrity.

```bash
ghost db verify
ghost db verify --full
```

**Default:** Spot-checks 100 random events  
**`--full`:** Walks entire chain (slow for large databases)

**Checks:**
- Event hash correctness
- Chain continuity (previous_hash links)
- Genesis block presence
- Snapshot state_hash integrity

---

### `ghost db compact`

Compact database (WAL checkpoint + VACUUM + memory event compaction).

```bash
ghost db compact
ghost db compact --yes --dry-run
ghost db compact --vacuum-only
```

**Flags:**
- `--yes` - Skip confirmation prompt
- `--dry-run` - Show what would happen without making changes
- `--force` - Skip gateway health probe (dangerous)
- `--vacuum-only` - Only run SQLite VACUUM, skip memory event compaction

**What it does:**
1. Checks gateway is not running (unless `--force`)
2. Runs WAL checkpoint
3. Runs VACUUM to reclaim space
4. Compacts memory event log (creates snapshots, prunes old events)

**Recommended:** Run weekly or when database exceeds 1GB.

---

## Audit & Observability

### `ghost audit query`

Query audit log.

```bash
ghost audit query
ghost audit query --agent agent-123 --severity high
ghost audit query --since 2026-03-01 --until 2026-03-02
ghost audit query --search "credential" --limit 100
```

**Filters:**
- `--agent <ID>` - Filter to specific agent
- `--severity <LEVEL>` - Filter by severity (low, medium, high, critical)
- `--event-type <TYPE>` - Filter by event type
- `--since <ISO8601>` - Start time filter
- `--until <ISO8601>` - End time filter
- `--search <TEXT>` - Full-text search across details
- `--limit <N>` - Maximum entries to return (default: 50)

---

### `ghost audit export`

Export audit log.

```bash
ghost audit export --format json --output audit.json
ghost audit export --format csv --output audit.csv
ghost audit export --format jsonl | gzip > audit.jsonl.gz
```

**Formats:**
- `json` - Single JSON array
- `csv` - CSV with headers
- `jsonl` - Newline-delimited JSON (streaming-friendly)

**Output:**
- `--output <PATH>` - Write to file
- Omit `--output` - Write to stdout

---

### `ghost audit tail`

Tail audit log (live stream).

```bash
ghost audit tail
```

**Connects via WebSocket** to receive real-time audit events.

**Ctrl+C to exit.**

---

### `ghost logs`

Stream live events from the gateway.

```bash
ghost logs
ghost logs --agent agent-123
ghost logs --type ScoreUpdate
ghost logs --json
```

**Filters:**
- `--agent <ID>` - Filter to specific agent
- `--type <TYPE>` - Filter to specific event type
- `--json` - Output as NDJSON instead of table
- `--idle-timeout <SECS>` - Idle timeout before closing (default: 1800)

**Event types:**
- `ScoreUpdate` - Convergence score changed
- `InterventionChange` - Intervention level changed
- `AgentStateChange` - Agent lifecycle state changed
- `KillSwitchActivation` - Kill switch activated
- `ProposalDecision` - Proposal approved/rejected
- `SessionEvent` - New ITP event in a session

---

## Convergence Monitoring

### `ghost convergence scores`

Show convergence scores for all agents.

```bash
ghost convergence scores
ghost convergence scores --output json
```

**Output:**
- Agent ID
- Composite score (0.0-1.0)
- Intervention level (L0-L4)
- 7 signal scores
- Last updated timestamp

---

### `ghost convergence history <agent_id>`

Show convergence history for an agent.

```bash
ghost convergence history agent-123
ghost convergence history agent-123 --since 2026-03-01
```

**Output:**
- Timestamp
- Composite score
- Intervention level
- Signal breakdown
- Trigger events

---

## Session Management

### `ghost session list`

List sessions.

```bash
ghost session list
ghost session list --agent agent-123 --limit 50
```

**Output:**
- Session ID
- Agent ID(s)
- Started at
- Last event at
- Event count

---

### `ghost session inspect <session_id>`

Inspect a session's events.

```bash
ghost session inspect session-456
```

**Output:**
- Full event log
- Tool calls
- Proposals
- Convergence score changes
- Intervention triggers

---

### `ghost session replay <session_id>`

Replay a session (step-by-step visualization).

```bash
ghost session replay session-456
```

**Interactive replay:**
- Step through events
- View agent state at each step
- See tool call results
- Inspect convergence signals

---

## Identity & Signing

### `ghost identity init`

Initialize identity (keypair + SOUL.md).

```bash
ghost identity init
```

**Creates:**
- Ed25519 keypair in `~/.ghost/identity/`
- SOUL.md template in `~/.ghost/identity/SOUL.md`

**Prompts for:**
- Agent name
- Core values
- Behavioral guidelines

---

### `ghost identity show`

Show identity information.

```bash
ghost identity show
ghost identity show --output json
```

**Output:**
- Public key (hex)
- SOUL.md path
- SOUL.md hash
- Created at

---

### `ghost identity drift`

Check for identity drift.

```bash
ghost identity drift
```

**Compares:**
- Current SOUL.md hash
- Last known hash from database
- Drift threshold from config

**Exit codes:**
- `0` - No drift
- `1` - Drift detected (within threshold)
- `2` - Drift exceeds threshold

---

### `ghost identity sign <file>`

Sign a file with agent identity.

```bash
ghost identity sign document.txt
```

**Creates:**
- `document.txt.sig` - Ed25519 signature (hex)

---

## Secret Management

### `ghost secret set <key>`

Set a secret value (reads value from stdin).

```bash
echo "my-secret-value" | ghost secret set MY_SECRET
ghost secret set API_KEY < api_key.txt
```

**Stored in:**
- macOS: Keychain
- Linux: Secret Service API (if available), else encrypted file
- Vault: HashiCorp Vault (if configured)

---

### `ghost secret list`

List known secret keys.

```bash
ghost secret list
```

**Output:**
- Secret key names (values never shown)
- Provider
- Created at

---

### `ghost secret delete <key>`

Delete a secret.

```bash
ghost secret delete MY_SECRET
ghost secret delete MY_SECRET --yes
```

**Confirmation required** unless `--yes` flag is used.

---

### `ghost secret provider`

Show active secret provider.

```bash
ghost secret provider
```

**Output:**
- Provider type (env, keychain, vault)
- Configuration
- Health status

---

## Policy Management

### `ghost policy show`

Show corporate policy.

```bash
ghost policy show
ghost policy show --output json
```

**Output:**
- Full CORP_POLICY.md content
- Parsed rules
- Enforcement level

---

### `ghost policy check <tool_name>`

Check a tool call against policy.

```bash
ghost policy check shell
ghost policy check shell --agent agent-123
```

**Output:**
- Allowed: yes/no
- Reason
- Applicable rules

---

### `ghost policy lint`

Lint corporate policy document.

```bash
ghost policy lint
```

**Checks:**
- Syntax errors
- Conflicting rules
- Unreachable rules
- Missing required sections

---

## Mesh Networking

### `ghost mesh peers`

List mesh peers.

```bash
ghost mesh peers
ghost mesh peers --output json
```

**Output:**
- Peer ID
- URL
- Trust score
- Last seen
- Status

---

### `ghost mesh trust`

Show trust scores.

```bash
ghost mesh trust
```

**Output:**
- Peer ID
- EigenTrust score
- Successful interactions
- Failed interactions
- Reputation

---

### `ghost mesh discover <url>`

Discover a remote peer.

```bash
ghost mesh discover https://peer.example.com
```

**Fetches:**
- `/.well-known/agent.json` - Peer agent card
- Verifies Ed25519 signature
- Adds to peer list

---

### `ghost mesh ping <peer_id>`

Ping a peer.

```bash
ghost mesh ping peer-789
```

**Output:**
- Round-trip time
- Peer status
- Last successful interaction

---

## Skills & Channels

### `ghost skill list`

List installed skills.

```bash
ghost skill list
```

**Output:**
- Skill name
- Version
- Source (bundled, user, workspace)
- State (loaded, quarantined)
- Capabilities

---

### `ghost skill install <path>`

Install a skill.

```bash
ghost skill install ./my-skill.wasm
ghost skill install https://skills.example.com/skill.wasm
```

**Verification:**
- Checks Ed25519 signature
- Validates manifest
- Quarantines if signature invalid

---

### `ghost skill inspect <name>`

Inspect a skill.

```bash
ghost skill inspect my-skill
```

**Output:**
- Full manifest
- Signature verification status
- Capabilities
- Timeout settings

---

### `ghost channel list`

List configured channels.

```bash
ghost channel list
```

**Output:**
- Channel type (telegram, whatsapp, slack, discord, cli, websocket)
- Status (active, inactive, error)
- Configuration

---

### `ghost channel test [type]`

Test channel connectivity.

```bash
ghost channel test
ghost channel test telegram
```

**Tests:**
- Authentication
- API connectivity
- Message send/receive

---

### `ghost channel send <type> <message>`

Send a test message to a channel.

```bash
ghost channel send telegram "Hello from CLI" --agent agent-123
```

**Flags:**
- `--agent <ID>` - Target agent
- `--sender <NAME>` - Sender name (default: "ghost-operator")

---

## Utilities

### `ghost init`

First-run platform setup.

```bash
ghost init
```

**Interactive setup:**
- Creates `~/.ghost/` directory structure
- Generates default `ghost.yml`
- Initializes identity
- Runs database migrations

---

### `ghost login`

Authenticate with a running gateway.

```bash
ghost login
```

**Prompts for:**
- Gateway URL
- Token or credentials

**Stores token** in `~/.ghost/auth/token`

---

### `ghost logout`

Remove stored authentication.

```bash
ghost logout
```

---

### `ghost doctor`

Run platform health checks.

```bash
ghost doctor
```

**Checks:**
- Configuration validity
- Database connectivity
- Convergence monitor health
- Secret provider availability
- Network connectivity
- Disk space

---

### `ghost completions <shell>`

Generate shell completions.

```bash
ghost completions bash > /etc/bash_completion.d/ghost
ghost completions zsh > ~/.zfunc/_ghost
ghost completions fish > ~/.config/fish/completions/ghost.fish
```

**Supported shells:**
- bash
- zsh
- fish
- powershell
- elvish

---

### `ghost man [dir]`

Generate man pages.

```bash
ghost man /usr/local/share/man/man1
```

**Generates:**
- `ghost.1` - Main command
- `ghost-agent.1` - Agent subcommand
- `ghost-safety.1` - Safety subcommand
- ... (one per subcommand)

---

### `ghost heartbeat status`

Show heartbeat engine status.

```bash
ghost heartbeat status
```

**Output:**
- Heartbeat tier (0-3)
- Last heartbeat timestamp
- Next heartbeat scheduled
- Convergence-aware tier selection

---

### `ghost cron list`

List registered cron jobs.

```bash
ghost cron list
```

**Output:**
- Job ID
- Schedule (cron expression)
- Last run
- Next run
- Status

---

### `ghost cron history`

Show cron execution history.

```bash
ghost cron history --limit 50
```

**Output:**
- Job ID
- Execution timestamp
- Duration
- Status (success, failure)
- Error message (if failed)

---

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `GHOST_TOKEN` | Authentication token | None |
| `GHOST_CONFIG` | Path to ghost.yml | `~/.ghost/config/ghost.yml` |
| `GHOST_BACKUP_KEY` | Backup encryption passphrase | None |
| `ANTHROPIC_API_KEY` | Anthropic API key | None |
| `OPENAI_API_KEY` | OpenAI API key | None |
| `GEMINI_API_KEY` | Google Gemini API key | None |
| `OLLAMA_BASE_URL` | Ollama server URL | `http://localhost:11434` |
| `GHOST_JWT_SECRET` | JWT signing secret (multi-user mode) | None |
| `GHOST_CORS_ORIGINS` | Allowed CORS origins (comma-separated) | `http://localhost:5173,http://localhost:18789` |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `69` | Service unavailable |
| `70` | Internal software error |
| `76` | Protocol error |
| `78` | Configuration error |

---

## Configuration File

Default location: `~/.ghost/config/ghost.yml`

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

security:
  soul_drift_threshold: 0.15
```

---

## Examples

### Daily Operations

```bash
# Start the gateway
ghost serve

# Check status
ghost status

# View live logs
ghost logs --agent my-agent

# Check convergence scores
ghost convergence scores

# Query recent audit events
ghost audit query --since 2026-03-01 --severity high
```

### Agent Management

```bash
# Create a new agent
ghost agent create

# List all agents
ghost agent list

# Inspect an agent
ghost agent inspect agent-123

# Pause an agent
ghost agent pause agent-123

# Resume an agent
ghost agent resume agent-123
```

### Safety & Debugging

```bash
# Check safety status
ghost safety status

# Emergency stop
ghost safety kill-all

# Verify database integrity
ghost db verify --full

# Replay a session
ghost session replay session-456
```

### Maintenance

```bash
# Run database migrations
ghost db migrate

# Compact database
ghost db compact --yes

# Create backup
ghost backup --output backup-$(date +%Y%m%d).tar.gz

# Export audit log
ghost audit export --format jsonl --output audit.jsonl
```

---

## Getting Help

```bash
# General help
ghost --help

# Subcommand help
ghost agent --help
ghost db compact --help

# Version
ghost --version
```

---

## See Also

- [API Contract](API_CONTRACT.md) - REST API documentation
- [Design System](DESIGN_SYSTEM.md) - Dashboard design tokens
- [ADE Design Plan](ADE_DESIGN_PLAN.md) - Full platform architecture
- [CLI Design](CLI_DESIGN.md) - CLI architecture and patterns
