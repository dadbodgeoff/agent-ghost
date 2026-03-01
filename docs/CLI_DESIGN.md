# GHOST CLI — Comprehensive Design Document

>    design for expanding the `ghost` CLI binary from 6 commands
> to a full platform management interface. Every decision is grounded in
> the existing codebase conventions, module structure, and dependency graph.
>
> **Date**: March 2026
> **Status**: Design
> **Scope**: `crates/ghost-gateway/src/cli/` expansion (no new crate)
> **Prerequisite**: Familiarity with `docs/ADE_DESIGN_PLAN.md` §4–§5

---

## Table of Contents

1. [Current State](#1-current-state)
2. [Architectural Decision: Where the CLI Lives](#2-architectural-decision-where-the-cli-lives)
3. [Backend Abstraction: HTTP vs Direct](#3-backend-abstraction-http-vs-direct)
4. [Command Tree](#4-command-tree)
5. [Authentication Flow](#5-authentication-flow)
6. [Output Formatting](#6-output-formatting)
7. [Error Handling & Exit Codes](#7-error-handling--exit-codes)
8. [Confirmation & Dry-Run Patterns](#8-confirmation--dry-run-patterns)
9. [File Layout & Module Structure](#9-file-layout--module-structure)
10. [Dependency Management](#10-dependency-management)
11. [Configuration Resolution](#11-configuration-resolution)
12. [Testing Strategy](#12-testing-strategy)
13. [Implementation Phases](#13-implementation-phases)
14. [Conventions & Style Guide](#14-conventions--style-guide)
15. [Integration with Existing Systems](#15-integration-with-existing-systems)
16. [Risk Register](#16-risk-register)

---

## 1. Current State

### 1.1 What Exists

The `ghost` binary lives in `crates/ghost-gateway/src/main.rs` as a `[[bin]]`
target. It uses clap 4 derive macros and dispatches to handler functions in
`crates/ghost-gateway/src/cli/`:

```
ghost-gateway/src/
├── main.rs          # Cli struct, Commands enum, tokio::main
├── cli/
│   ├── mod.rs       # pub mod chat; pub mod status; pub mod commands;
│   ├── chat.rs      # run_interactive_chat() — full AgentRunner REPL
│   ├── status.rs    # show_status() — HTTP client to /api/health
│   └── commands.rs  # run_backup(), run_export(), run_migrate()
```

**Existing commands**: `serve` (default), `chat`, `status`, `backup`, `export`, `migrate`.

### 1.2 Existing Patterns to Preserve

| Pattern | Location | Convention |
|---|---|---|
| Clap derive macros | `main.rs` | `#[derive(Parser)]` on root, `#[derive(clap::Subcommand)]` on enum |
| Global flags | `main.rs` | `--config` with `global = true` |
| Async entry | `main.rs` | `#[tokio::main]` with `tracing_subscriber` init before clap parse |
| Exit codes | `bootstrap.rs` | sysexits.h constants: `EX_CONFIG=78`, `EX_UNAVAILABLE=69`, `EX_SOFTWARE=70`, `EX_PROTOCOL=76` |
| Tilde expansion | `bootstrap.rs` | `shellexpand_tilde()` — `HOME`/`USERPROFILE` fallback |
| DB open pattern | `bootstrap.rs`, `chat.rs` | `Connection::open()` → `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` |
| Config loading | `config.rs` | `GhostConfig::load_default(cli_path)` — checks CLI arg, then `~/.ghost/config/ghost.yml`, then defaults |
| Error types | Per-crate | `thiserror` derive with `#[error("...")]` |
| Tracing | Everywhere | `tracing::info!`, `tracing::warn!`, `tracing::error!` with structured fields |
| JSON serialization | API handlers | `serde_json::json!({})` for ad-hoc responses, `#[derive(Serialize)]` for typed |
| UUID generation | `agents.rs` | `Uuid::now_v7()` for time-ordered IDs |
| Secret handling | `ghost-secrets` | `SecretProvider` trait with keychain/vault/env backends |

### 1.3 What's Missing

- No auth for HTTP-based CLI commands (collision course with T-1.1.1 REST auth)
- No structured output (JSON/table/YAML) — all commands print ad-hoc strings
- No backend abstraction (some commands use HTTP, some use filesystem directly)
- No `init` / first-run experience
- No diagnostics / doctor command
- No agent management, safety controls, audit queries, convergence inspection,
  session inspection, or live event streaming from CLI
- No shell completions
- No confirmation prompts on destructive operations
- No progress indicators for long operations

---

## 2. Architectural Decision: Where the CLI Lives

### 2.1 Option A: Keep Everything in `ghost-gateway` (Recommended)

The CLI stays in `crates/ghost-gateway/src/cli/`. The `ghost` binary remains
the single entry point for both the server (`ghost serve`) and all management
commands.

**Why this is correct for GHOST:**

1. **The binary already exists.** `ghost-gateway/Cargo.toml` declares
   `[[bin]] name = "ghost"`. Splitting would mean two binaries, two install
   targets, two things to version.

2. **Shared code.** CLI commands need `GhostConfig`, `shellexpand_tilde()`,
   `AppState` construction, DB opening, migration running — all of which live
   in `ghost-gateway`. A separate crate would need to depend on `ghost-gateway`
   as a library (it already exposes `lib.rs`), creating a circular feel even
   if technically acyclic.

3. **Precedent.** This is how `docker` (daemon + client in one binary),
   `kubectl`, and `cargo` work. The binary detects whether it needs to start
   a server or run a one-shot command based on the subcommand.

4. **The gateway already depends on every crate the CLI needs.** Looking at
   `ghost-gateway/Cargo.toml`: `ghost-audit`, `ghost-backup`, `ghost-export`,
   `ghost-migrate`, `ghost-identity`, `ghost-signing`, `ghost-secrets`,
   `ghost-oauth`, `ghost-egress`, `ghost-mesh`, `ghost-kill-gates`,
   `ghost-agent-loop`, `cortex-core`, `cortex-storage`, `ghost-llm`. There
   is nothing a separate CLI crate would import that the gateway doesn't
   already have.

### 2.2 Option B: Separate `ghost-cli` Crate (Rejected)

A new `crates/ghost-cli/` with its own `[[bin]]` target.

**Why this is wrong:**

- Duplicates the dependency tree (same 15+ crate imports).
- Requires `ghost-gateway` as a library dependency for config, bootstrap,
  and state types — creating a confusing dependency where the "CLI" depends
  on the "gateway."
- Two binaries to build, distribute, and version.
- No compile-time benefit — the gateway already compiles everything.

### 2.3 Decision

**All CLI code lives in `crates/ghost-gateway/src/cli/`.** The `ghost` binary
in `main.rs` is the single entry point. New subcommands are added as modules
under `cli/` and wired into the `Commands` enum in `main.rs`.

One exception: a thin `crates/ghost-gateway/src/cli/backend.rs` module that
abstracts HTTP vs direct-DB access (see §3).

---

## 3. Backend Abstraction: HTTP vs Direct

### 3.1 The Problem

Some CLI commands need the gateway running (agent CRUD via REST, safety
controls that broadcast WS events). Others can operate directly against
the filesystem and SQLite database (backup, config validation, DB migration,
hash chain verification). Some can do both (status, audit queries).

The CLI must be explicit about which mode it's using and fail clearly when
a command requires the gateway but it's not running.

### 3.2 The Design

```rust
// crates/ghost-gateway/src/cli/backend.rs

use crate::config::GhostConfig;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Backend for CLI command execution.
/// Determines whether commands talk to the gateway via HTTP
/// or operate directly against the DB/filesystem.
pub enum CliBackend {
    /// Gateway is running — use REST API.
    Http {
        base_url: String,
        token: Option<String>,
    },
    /// Gateway is not running — direct DB/filesystem access.
    /// Write operations that need the safety stack are refused.
    Direct {
        config: GhostConfig,
        db: Arc<Mutex<rusqlite::Connection>>,
    },
}

/// What a command requires from the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendRequirement {
    /// Must have HTTP (gateway running). Fail if not.
    HttpOnly,
    /// Prefer HTTP, fall back to Direct.
    PreferHttp,
    /// Direct only — never needs the gateway.
    DirectOnly,
}

impl CliBackend {
    /// Detect the best available backend.
    ///
    /// 1. Try HTTP health check at the configured gateway address.
    /// 2. If reachable, return Http backend.
    /// 3. If not, try opening the DB directly.
    /// 4. If DB opens, return Direct backend.
    /// 5. If neither works, return an error.
    pub async fn detect(config: &GhostConfig) -> Result<Self, CliError> {
        let base_url = format!(
            "http://{}:{}",
            config.gateway.bind, config.gateway.port
        );

        // Load stored credentials for HTTP mode.
        let token = load_stored_token();

        // Try HTTP first.
        let health_url = format!("{}/api/health", base_url);
        match reqwest::Client::new()
            .get(&health_url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                return Ok(Self::Http { base_url, token });
            }
            _ => {}
        }

        // Fall back to direct DB access.
        let db_path = crate::bootstrap::shellexpand_tilde(&config.gateway.db_path);
        if std::path::Path::new(&db_path).exists() {
            let conn = rusqlite::Connection::open(&db_path)
                .map_err(|e| CliError::Database(format!("open: {e}")))?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
                .map_err(|e| CliError::Database(format!("pragma: {e}")))?;
            return Ok(Self::Direct {
                config: config.clone(),
                db: Arc::new(Mutex::new(conn)),
            });
        }

        Err(CliError::NoBackend)
    }

    /// Require a specific backend type. Returns error with actionable message
    /// if the requirement isn't met.
    pub fn require(&self, req: BackendRequirement) -> Result<(), CliError> {
        match (req, self) {
            (BackendRequirement::HttpOnly, Self::Direct { .. }) => {
                Err(CliError::GatewayRequired)
            }
            (BackendRequirement::DirectOnly, Self::Http { .. }) => {
                // This shouldn't happen — DirectOnly commands don't call detect().
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
```

### 3.3 Command Classification

| Backend | Commands |
|---|---|
| `DirectOnly` | `config show/validate/init`, `db migrate/status/verify/compact`, `init`, `doctor`, `completions`, `identity init/show` |
| `PreferHttp` | `status`, `agent inspect`, `safety status`, `audit query`, `convergence scores`, `session list/inspect`, `db status` |
| `HttpOnly` | `agent create/delete`, `safety kill-all/pause/resume/quarantine`, `audit tail`, `logs` |

### 3.4 Error Messages

When a command requires the gateway and it's not running:

```
Error: Gateway not running at 127.0.0.1:18789

  Start the gateway:  ghost serve
  Or specify address:  ghost --gateway-url http://host:port <command>
```

When a command falls back to direct mode:

```
Note: Gateway not reachable — reading directly from database.
      Some data may be stale. Start the gateway for live data.
```

---

## 4. Command Tree

### 4.1 Complete Command Hierarchy

```
ghost [--config <path>] [--output json|table|yaml] [--gateway-url <url>] <command>

  serve                              Start the gateway server (default)
  chat                               Interactive chat session
  init [--defaults]                  Initialize ~/.ghost/ directory structure
  login [--token <token>]            Authenticate and store credentials
  logout                             Clear stored credentials
  doctor                             Check platform health and prerequisites
  completions <bash|zsh|fish>        Generate shell completions
  logs [--agent <id>] [--type <t>]   Stream live events from gateway WebSocket
       [--json]

  status                             Show gateway and monitor status

  agent
    list                             List registered agents
    create <name> [--model <m>]      Create a new agent
           [--spending-cap <cap>]
           [--capabilities <list>]
           [--no-keypair]
    inspect <id|name>                Show agent details
    delete <id|name> [--yes]         Soft-delete an agent
    pause <id|name>                  Pause an agent
    resume <id|name>                 Resume a paused/quarantined agent
    quarantine <id|name>             Quarantine an agent

  safety
    status                           Show kill switch state (platform + per-agent)
    kill-all [--reason <r>] [--yes]  Emergency stop all agents
    clear [--yes]                    Clear kill_state.json after human review

  config
    show                             Dump resolved configuration (secrets redacted)
    validate                         Validate ghost.yml syntax and semantics
    init                             Interactive config wizard (alias for `ghost init`)

  db
    migrate                          Run pending database migrations
    status                           Show migration version, table counts, DB size
    verify [--full]                  Verify hash chain integrity
    compact                          WAL checkpoint + VACUUM

  audit
    query [--agent <id>]             Query audit log
          [--severity <level>]
          [--event-type <type>]
          [--since <time>]
          [--until <time>]
          [--search <text>]
          [--limit <n>]
    export [--format json|csv|jsonl]  Export audit data
           [--output <path>]
    tail [--agent <id>]              Live-stream audit events

  convergence
    scores [--agent <id>]            Show current convergence scores
    history <agent_id>               Score history
            [--since <time>]

  session
    list [--agent <id>] [--limit <n>]  List recent sessions
    inspect <id>                       Show session events and gate states
    replay <id>                        Text-based session replay

  identity
    init                             Create SOUL.md + platform keypair
    show                             Display soul document summary + key fingerprint
    drift                            Run drift detection
    sign <file>                      Sign a file with the platform key

  secret
    set <key>                        Store a secret (reads from stdin)
    list                             List stored secret keys (not values)
    delete <key>                     Delete a stored secret
    provider                         Show active secret backend

  policy
    show                             Display active CORP_POLICY rules
    check <tool> [--agent <id>]      Dry-run a policy evaluation
    lint                             Validate CORP_POLICY.md syntax

  mesh
    peers                            List known A2A peers
    trust                            Show EigenTrust scores
    discover <url>                   Fetch and register a remote agent
    ping <peer_id>                   Test connectivity to a peer

  skill
    list                             List registered skills
    install <path|url>               Install a WASM skill
    inspect <name>                   Show skill metadata and permissions

  backup [--output <path>]           Create encrypted backup (existing)
  export <path>                      Analyze external AI export (existing)
  migrate [--source <path>]          Migrate from OpenClaw (existing)
```

### 4.2 Global Flags

| Flag | Short | Type | Default | Description |
|---|---|---|---|---|
| `--config` | `-c` | `String` | `~/.ghost/config/ghost.yml` | Path to config file |
| `--output` | `-o` | `OutputFormat` | `table` | Output format: `table`, `json`, `yaml` |
| `--gateway-url` | `-g` | `String` | From config | Override gateway base URL |
| `--verbose` | `-v` | `bool` | `false` | Enable debug-level tracing |
| `--quiet` | `-q` | `bool` | `false` | Suppress non-essential output |

### 4.3 Clap Structure in `main.rs`

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ghost", about = "GHOST Platform", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to ghost.yml configuration file.
    #[arg(long, short, global = true)]
    config: Option<String>,

    /// Output format: table, json, yaml.
    #[arg(long, short, global = true, default_value = "table")]
    output: OutputFormat,

    /// Override gateway base URL.
    #[arg(long, short = 'g', global = true)]
    gateway_url: Option<String>,

    /// Enable verbose (debug) logging.
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Suppress non-essential output.
    #[arg(long, short, global = true)]
    quiet: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}
```

Subcommand groups use nested enums:

```rust
#[derive(Subcommand)]
enum Commands {
    Serve,
    Chat,
    Init { #[arg(long)] defaults: bool },
    Login { #[arg(long)] token: Option<String> },
    Logout,
    Doctor,
    Completions { shell: clap_complete::Shell },
    Logs(LogsArgs),
    Status,
    #[command(subcommand)]
    Agent(AgentCommands),
    #[command(subcommand)]
    Safety(SafetyCommands),
    #[command(subcommand)]
    Config(ConfigCommands),
    #[command(subcommand)]
    Db(DbCommands),
    #[command(subcommand)]
    Audit(AuditCommands),
    #[command(subcommand)]
    Convergence(ConvergenceCommands),
    #[command(subcommand)]
    Session(SessionCommands),
    #[command(subcommand)]
    Identity(IdentityCommands),
    #[command(subcommand)]
    Secret(SecretCommands),
    #[command(subcommand)]
    Policy(PolicyCommands),
    #[command(subcommand)]
    Mesh(MeshCommands),
    #[command(subcommand)]
    Skill(SkillCommands),
    // Existing top-level commands preserved for backward compat
    Backup { #[arg(long, short)] output: Option<String> },
    Export { path: String },
    Migrate { #[arg(long, default_value = "~/.openclaw")] source: String },
}
```

Each subcommand group gets its own enum in its own file (see §9).

---

## 5. Authentication Flow

### 5.1 The Problem

REST auth middleware (T-1.1.1) is coming in Phase 1 Week 1. Once deployed,
every HTTP-based CLI command will get 401 unless the CLI sends a valid
`Authorization: Bearer <token>` header. The CLI must authenticate before
T-1.1.1 lands, or the two features will conflict.

### 5.2 Token Storage

Credentials are stored via the existing `ghost-secrets` `SecretProvider`
trait, which already supports OS keychain, HashiCorp Vault, and env var
fallback. The CLI stores one secret:

- Key: `ghost_cli_token`
- Value: The JWT access token (or legacy `GHOST_TOKEN` value)

```rust
// crates/ghost-gateway/src/cli/auth.rs

const CLI_TOKEN_KEY: &str = "ghost_cli_token";

/// Store a token using the configured SecretProvider.
pub fn store_token(provider: &dyn ghost_secrets::SecretProvider, token: &str) -> Result<(), CliError> {
    provider.set(CLI_TOKEN_KEY, token)
        .map_err(|e| CliError::Auth(format!("failed to store token: {e}")))
}

/// Load the stored token. Returns None if not set.
pub fn load_token(provider: &dyn ghost_secrets::SecretProvider) -> Option<String> {
    provider.get(CLI_TOKEN_KEY).ok()
}

/// Clear the stored token.
pub fn clear_token(provider: &dyn ghost_secrets::SecretProvider) -> Result<(), CliError> {
    provider.delete(CLI_TOKEN_KEY)
        .map_err(|e| CliError::Auth(format!("failed to clear token: {e}")))
}
```

### 5.3 Login Flow

```
ghost login
```

1. If `GHOST_TOKEN` env var is set → store it directly (legacy mode).
2. If `--token <token>` flag is provided → store it directly.
3. Otherwise → prompt for token interactively (stdin).
4. Validate the token by calling `GET /api/health` with the
   `Authorization: Bearer <token>` header.
5. If 200 → store in keychain, print success.
6. If 401 → print error, do not store.

When JWT auth (T-1.1.3) is implemented, `ghost login` will be extended to:
1. Prompt for username + password.
2. Call `POST /api/auth/login`.
3. Store the returned JWT access token.
4. Store the refresh token if returned.

### 5.4 Token Injection into HTTP Requests

Every HTTP request from the CLI goes through a shared `reqwest::Client`
that injects the stored token:

```rust
// crates/ghost-gateway/src/cli/http_client.rs

pub struct GhostHttpClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl GhostHttpClient {
    pub fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
            base_url,
            token,
        }
    }

    pub async fn get(&self, path: &str) -> Result<reqwest::Response, CliError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.get(&url);
        if let Some(ref token) = self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req.send().await.map_err(|e| CliError::Http(e.to_string()))
    }

    pub async fn post<T: serde::Serialize>(
        &self, path: &str, body: &T,
    ) -> Result<reqwest::Response, CliError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.post(&url).json(body);
        if let Some(ref token) = self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req.send().await.map_err(|e| CliError::Http(e.to_string()))
    }

    pub async fn delete(&self, path: &str) -> Result<reqwest::Response, CliError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.delete(&url);
        if let Some(ref token) = self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req.send().await.map_err(|e| CliError::Http(e.to_string()))
    }
}
```

### 5.5 Env Var Override

For CI/headless environments, `GHOST_TOKEN` env var always takes precedence
over stored credentials. This matches the server-side pattern where
`GHOST_TOKEN` is the legacy auth mechanism.

Resolution order:
1. `GHOST_TOKEN` env var (if set)
2. Stored token via `ghost-secrets` SecretProvider
3. No token (commands that don't require auth still work)

---

## 6. Output Formatting

### 6.1 The `OutputFormat` Enum

All commands that produce data output accept the global `--output` flag.
The formatting layer is a single trait:

```rust
// crates/ghost-gateway/src/cli/output.rs

use serde::Serialize;

/// Format and print a value according to the selected output format.
pub fn print_output<T: Serialize + TableDisplay>(
    value: &T,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(value).unwrap());
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(value).unwrap());
        }
        OutputFormat::Table => {
            value.print_table();
        }
    }
}

/// Trait for types that can render themselves as a human-readable table.
/// Implemented per-command on the response types.
pub trait TableDisplay {
    fn print_table(&self);
}
```

### 6.2 Table Formatting Convention

Tables use simple aligned columns with Unicode box-drawing for headers.
No external crate needed — the formatting is simple enough to hand-roll
with `format!` width specifiers:

```
ID                                   Name        Status    Spend Cap
─────────────────────────────────── ─────────── ───────── ──────────
019577a2-3f4e-7b8c-9d0e-1a2b3c4d5e  researcher  Starting  $5.00
019577a2-4a5b-7c8d-0e1f-2a3b4c5d6e  reviewer    Ready     $10.00
```

### 6.3 JSON Output for Scripting

When `--output json` is used, the output is valid JSON on stdout. Diagnostic
messages (warnings, progress) go to stderr. This allows:

```bash
ghost agent list --output json | jq '.[].name'
ghost audit query --output json --limit 100 > audit_dump.json
```

### 6.4 Quiet Mode

When `--quiet` is used, only the essential output is printed (no banners,
no progress, no notes). Combined with `--output json`, this gives clean
machine-readable output.

---

## 7. Error Handling & Exit Codes

### 7.1 The `CliError` Type

A single error type for all CLI operations, with structured exit codes:

```rust
// crates/ghost-gateway/src/cli/error.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("authentication required — run `ghost login` first")]
    AuthRequired,

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("gateway not running at {0} — start with `ghost serve`")]
    GatewayRequired,

    #[error("gateway and database both unavailable")]
    NoBackend,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("operation cancelled by user")]
    Cancelled,

    #[error("internal error: {0}")]
    Internal(String),

    #[error("{0}")]
    Usage(String),
}

impl CliError {
    /// Map to sysexits.h exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) => 78,          // EX_CONFIG
            Self::Database(_) => 76,        // EX_PROTOCOL
            Self::Http(_) => 69,            // EX_UNAVAILABLE
            Self::AuthRequired => 77,       // EX_NOPERM
            Self::Auth(_) => 77,            // EX_NOPERM
            Self::GatewayRequired => 69,    // EX_UNAVAILABLE
            Self::NoBackend => 69,          // EX_UNAVAILABLE
            Self::NotFound(_) => 1,         // General error
            Self::Conflict(_) => 1,         // General error
            Self::Cancelled => 1,           // General error
            Self::Internal(_) => 70,        // EX_SOFTWARE
            Self::Usage(_) => 64,           // EX_USAGE
        }
    }
}
```

### 7.2 Error Handling in `main.rs`

The dispatch in `main.rs` wraps every command in a `Result<(), CliError>`
and handles the exit code uniformly:

```rust
#[tokio::main]
async fn main() {
    // ... tracing init, clap parse ...

    let result = run_command(cli_args).await;
    match result {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            // Errors go to stderr, never stdout (stdout is for data).
            eprintln!("Error: {e}");
            std::process::exit(e.exit_code());
        }
    }
}
```

### 7.3 HTTP Error Mapping

When the CLI receives an HTTP error from the gateway, it maps status codes
to `CliError` variants:

| HTTP Status | CliError Variant | User Message |
|---|---|---|
| 401 | `AuthRequired` | "Authentication required — run `ghost login` first" |
| 403 | `Auth("insufficient permissions")` | "Insufficient permissions for this operation" |
| 404 | `NotFound(entity)` | "Agent 'xyz' not found" |
| 409 | `Conflict(reason)` | "Cannot delete quarantined agent — resume first" |
| 429 | `Http("rate limited")` | "Rate limited — retry after {n} seconds" |
| 500 | `Internal(body)` | "Gateway internal error: {details}" |
| Connection refused | `GatewayRequired` | "Gateway not running at {addr}" |

---

## 8. Confirmation & Dry-Run Patterns

### 8.1 Destructive Commands

Commands that modify state require confirmation unless `--yes` is passed:

| Command | Confirmation Prompt | Color |
|---|---|---|
| `ghost agent delete <id>` | "Delete agent '{name}'? This is irreversible. [y/N]" | Yellow |
| `ghost safety kill-all` | "KILL ALL AGENTS? This stops all agent execution. [y/N]" | Red |
| `ghost safety clear` | "Clear kill state? Agents will be able to resume. [y/N]" | Yellow |
| `ghost db compact` | "Compact database? This may take a moment. [y/N]" | Normal |
| `ghost secret delete <key>` | "Delete secret '{key}'? [y/N]" | Yellow |

### 8.2 Implementation

```rust
// crates/ghost-gateway/src/cli/confirm.rs

use std::io::{self, Write};

/// Prompt for confirmation. Returns true if user confirms.
/// Always returns true if `--yes` flag was passed.
pub fn confirm(prompt: &str, yes_flag: bool) -> bool {
    if yes_flag {
        return true;
    }
    eprint!("{} ", prompt);
    io::stderr().flush().ok();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}
```

### 8.3 Dry-Run

Commands that support `--dry-run`:

- `ghost safety kill-all --dry-run` — show what would happen without executing
- `ghost db compact --dry-run` — show current WAL size and estimated savings
- `ghost db verify --dry-run` — show chain length without walking it

Dry-run output is prefixed with `[dry-run]` and uses the same output format
as the real command.

---

## 9. File Layout & Module Structure

### 9.1 Directory Structure

```
crates/ghost-gateway/src/cli/
├── mod.rs                  # Re-exports all submodules
├── error.rs                # CliError type (§7)
├── output.rs               # OutputFormat, TableDisplay trait, print_output() (§6)
├── confirm.rs              # Confirmation prompts (§8)
├── backend.rs              # CliBackend enum, detect(), require() (§3)
├── http_client.rs          # GhostHttpClient with auth injection (§5)
├── auth.rs                 # Token storage/loading via ghost-secrets (§5)
├── chat.rs                 # [existing] Interactive REPL
├── commands.rs             # [existing] backup, export, migrate
├── status.rs               # [existing, refactored] show_status()
├── init.rs                 # ghost init — directory structure + config wizard
├── doctor.rs               # ghost doctor — health checks
├── logs.rs                 # ghost logs — WebSocket event streaming
├── agent.rs                # ghost agent {list,create,inspect,delete,pause,resume,quarantine}
├── safety.rs               # ghost safety {status,kill-all,clear}
├── config_cmd.rs           # ghost config {show,validate,init}  (not config.rs — that's taken)
├── db.rs                   # ghost db {migrate,status,verify,compact}
├── audit_cmd.rs            # ghost audit {query,export,tail}
├── convergence.rs          # ghost convergence {scores,history}
├── session.rs              # ghost session {list,inspect,replay}
├── identity.rs             # ghost identity {init,show,drift,sign}
├── secret.rs               # ghost secret {set,list,delete,provider}
├── policy.rs               # ghost policy {show,check,lint}
├── mesh.rs                 # ghost mesh {peers,trust,discover,ping}
├── skill.rs                # ghost skill {list,install,inspect}
└── completions.rs          # ghost completions — shell completion generation
```

### 9.2 Module Naming Convention

- Files that conflict with existing module names in the crate get a `_cmd`
  suffix: `config_cmd.rs` (because `config.rs` exists), `audit_cmd.rs`
  (because `ghost-audit` crate exists and could cause confusion).
- Each file contains:
  1. The clap `Subcommand` enum for that group
  2. The `pub async fn run(...)` dispatch function
  3. Private handler functions for each subcommand
  4. Response types with `Serialize` + `TableDisplay` impls

### 9.3 Module Template

Every command module follows this pattern:

```rust
//! ghost <group> — <description>.

use clap::Subcommand;
use crate::cli::backend::CliBackend;
use crate::cli::error::CliError;
use crate::cli::output::{OutputFormat, TableDisplay, print_output};

#[derive(Subcommand)]
pub enum GroupCommands {
    /// Brief description of subcommand.
    SubcommandName {
        /// Arg description.
        #[arg(long)]
        flag: Option<String>,
    },
}

/// Dispatch to the appropriate handler.
pub async fn run(
    cmd: GroupCommands,
    backend: &CliBackend,
    output: OutputFormat,
) -> Result<(), CliError> {
    match cmd {
        GroupCommands::SubcommandName { flag } => {
            handle_subcommand(backend, output, flag).await
        }
    }
}

// --- Handlers ---

async fn handle_subcommand(
    backend: &CliBackend,
    output: OutputFormat,
    flag: Option<String>,
) -> Result<(), CliError> {
    // Implementation
    Ok(())
}

// --- Response types ---

#[derive(serde::Serialize)]
struct SubcommandResponse {
    // fields
}

impl TableDisplay for SubcommandResponse {
    fn print_table(&self) {
        // Human-readable output
    }
}
```

### 9.4 `mod.rs` Update

```rust
//! CLI subcommand implementations.

pub mod error;
pub mod output;
pub mod confirm;
pub mod backend;
pub mod http_client;
pub mod auth;

// Existing
pub mod chat;
pub mod commands;
pub mod status;

// New command groups
pub mod init;
pub mod doctor;
pub mod logs;
pub mod agent;
pub mod safety;
pub mod config_cmd;
pub mod db;
pub mod audit_cmd;
pub mod convergence;
pub mod session;
pub mod identity;
pub mod policy;
pub mod secret;
pub mod mesh;
pub mod skill;
pub mod completions;
```

---

## 10. Dependency Management

### 10.1 New Workspace Dependencies

The CLI needs a few crates not currently in the workspace:

| Crate | Version | Purpose | Added To |
|---|---|---|---|
| `clap_complete` | `4` | Shell completion generation | `ghost-gateway/Cargo.toml` |
| `indicatif` | `0.17` | Progress bars for long operations | `ghost-gateway/Cargo.toml` |
| `comfy-table` | `7` | Table formatting (optional — can hand-roll) | `ghost-gateway/Cargo.toml` |
| `dialoguer` | `0.11` | Interactive prompts (optional — can use raw stdin) | `ghost-gateway/Cargo.toml` |

**Recommendation**: Start with zero new deps. Use raw `format!()` for tables,
raw `stdin` for prompts, and `eprintln!()` for progress. Add `indicatif` and
`comfy-table` only if the hand-rolled versions become unmaintainable. The only
hard requirement is `clap_complete` for shell completions.

### 10.2 Workspace Dependency Declaration

Add to the root `Cargo.toml` `[workspace.dependencies]` section:

```toml
# CLI
clap = { version = "4", features = ["derive"] }       # already exists
clap_complete = "4"                                     # new
```

Then in `ghost-gateway/Cargo.toml`:

```toml
clap_complete = { workspace = true }
```

### 10.3 No New Workspace Members

The CLI does not create a new crate. All code lives in `ghost-gateway`.
The workspace `members` list in the root `Cargo.toml` is unchanged.

### 10.4 Feature Flags

No new feature flags are needed. The existing `keychain`, `vault`, `ebpf`,
and `pf` features in `ghost-gateway/Cargo.toml` already gate the relevant
functionality. The CLI inherits these features transparently.

---

## 11. Configuration Resolution

### 11.1 The Problem

`GhostConfig::load_default()` loads from YAML and applies `#[serde(default)]`
for missing fields. But env var overrides happen ad-hoc throughout bootstrap
(e.g., `GHOST_TOKEN` is checked in `token_auth.rs`, `ANTHROPIC_API_KEY` in
`chat.rs`). There's no single function that shows the fully resolved config.

### 11.2 The Solution: `ghost config show`

The `config show` command loads the config via `GhostConfig::load_default()`,
then overlays known env var overrides, and prints the result with secrets
redacted:

```rust
// crates/ghost-gateway/src/cli/config_cmd.rs (partial)

async fn handle_show(config: &GhostConfig, output: OutputFormat) -> Result<(), CliError> {
    let mut resolved = ResolvedConfig::from(config);

    // Overlay env var overrides.
    if std::env::var("GHOST_TOKEN").is_ok() {
        resolved.auth_mode = "legacy_token (GHOST_TOKEN)".into();
    }
    if std::env::var("GHOST_JWT_SECRET").is_ok() {
        resolved.auth_mode = "jwt (GHOST_JWT_SECRET)".into();
    }
    // ... provider keys ...
    for provider in &["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"] {
        if std::env::var(provider).is_ok() {
            resolved.active_providers.push(provider.to_string());
        }
    }
    if std::env::var("OLLAMA_BASE_URL").is_ok() {
        resolved.active_providers.push("OLLAMA_BASE_URL".into());
    }

    print_output(&resolved, output);
    Ok(())
}
```

Secrets are always redacted in output:

```
Gateway
  Bind:     127.0.0.1
  Port:     18789
  DB Path:  ~/.ghost/data/ghost.db

Auth
  Mode:     legacy_token (GHOST_TOKEN=****)

Providers
  anthropic  ANTHROPIC_API_KEY=sk-ant-****
  openai     OPENAI_API_KEY=sk-****

Agents
  researcher  spending_cap=$5.00  capabilities=[read_file, web_search]
  reviewer    spending_cap=$10.00 capabilities=[read_file, write_file]

Convergence
  Profile:  standard
  Monitor:  127.0.0.1:18790

Secrets
  Provider: keychain
  Service:  ghost-platform
```

### 11.3 Config Validation: `ghost config validate`

Runs `GhostConfig::load_default()` + `validate()` and reports all issues:

```
✓ ghost.yml syntax valid
✓ Gateway bind address valid
✓ All agent names unique
✓ All spending caps positive
✗ Agent 'researcher' references unknown channel 'telegram'
✗ ANTHROPIC_API_KEY not set (agent 'researcher' uses anthropic model)
✓ Database path writable
✓ Convergence monitor address parseable

2 issues found.
```

---

## 12. Testing Strategy

### 12.1 Test Location

CLI tests live in `crates/ghost-gateway/tests/cli_tests.rs` (integration
tests) and inline `#[cfg(test)]` modules for unit tests in each CLI module.

### 12.2 Test Patterns

**Unit tests** (in each `cli/*.rs` file):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_exit_codes() {
        assert_eq!(CliError::Config("x".into()).exit_code(), 78);
        assert_eq!(CliError::AuthRequired.exit_code(), 77);
        assert_eq!(CliError::GatewayRequired.exit_code(), 69);
    }

    #[test]
    fn output_format_json() {
        // Test that JSON output is valid JSON.
    }
}
```

**Integration tests** (in `tests/cli_tests.rs`):

```rust
//! CLI integration tests.
//!
//! These tests invoke the CLI binary as a subprocess and verify
//! exit codes, stdout, and stderr.

use std::process::Command;

fn ghost_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ghost"))
}

#[test]
fn status_without_gateway_returns_info() {
    let output = ghost_cmd().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("NOT RUNNING") || stdout.contains("RUNNING"));
}

#[test]
fn config_validate_with_default_config() {
    let output = ghost_cmd()
        .args(["config", "validate"])
        .output()
        .unwrap();
    // Should not crash, exit code 0 or 78 depending on config presence.
    assert!(output.status.code().unwrap() == 0 || output.status.code().unwrap() == 78);
}

#[test]
fn unknown_command_returns_usage_error() {
    let output = ghost_cmd().arg("nonexistent").output().unwrap();
    assert_eq!(output.status.code().unwrap(), 2); // clap usage error
}

#[test]
fn json_output_is_valid_json() {
    let output = ghost_cmd()
        .args(["agent", "list", "--output", "json"])
        .output()
        .unwrap();
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(serde_json::from_str::<serde_json::Value>(&stdout).is_ok());
    }
}

#[test]
fn help_flag_works() {
    let output = ghost_cmd().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GHOST Platform"));
}

#[test]
fn version_flag_works() {
    let output = ghost_cmd().arg("--version").output().unwrap();
    assert!(output.status.success());
}
```

### 12.3 Testing the Backend Abstraction

The `CliBackend` is tested with a mock HTTP server (using `axum::Server`
in tests) and an in-memory SQLite database:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn direct_backend() -> CliBackend {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        CliBackend::Direct {
            config: GhostConfig::default(),
            db: Arc::new(Mutex::new(conn)),
        }
    }

    #[test]
    fn direct_backend_refuses_http_only() {
        let backend = direct_backend();
        assert!(backend.require(BackendRequirement::HttpOnly).is_err());
    }

    #[test]
    fn direct_backend_allows_prefer_http() {
        let backend = direct_backend();
        assert!(backend.require(BackendRequirement::PreferHttp).is_ok());
    }
}
```

### 12.4 Test Naming Convention

Following the existing pattern in `bootstrap_audit_tests.rs`:

- Snake_case function names
- Descriptive: `agent_list_returns_empty_when_no_agents`
- No `test_` prefix (Rust's `#[test]` attribute handles this)
- Group related tests with comments, not nested modules

---

## 13. Implementation Phases

### 13.1 Phase 0: Infrastructure (Do First)

Build the cross-cutting modules that every command depends on. No new
user-facing commands yet — just the plumbing.

| Task | File | Depends On | Effort |
|---|---|---|---|
| `CliError` type | `cli/error.rs` | Nothing | S |
| `OutputFormat` + `TableDisplay` | `cli/output.rs` | `serde`, `serde_json`, `serde_yaml` | S |
| `confirm()` function | `cli/confirm.rs` | Nothing | S |
| `CliBackend` enum + `detect()` | `cli/backend.rs` | `config.rs`, `reqwest` | M |
| `GhostHttpClient` | `cli/http_client.rs` | `reqwest`, `cli/auth.rs` | M |
| Token storage/loading | `cli/auth.rs` | `ghost-secrets` | S |
| Refactor `main.rs` dispatch | `main.rs` | All above | M |
| Add `clap_complete` dep | `Cargo.toml` (workspace + gateway) | Nothing | S |
| Add global flags | `main.rs` | `cli/output.rs` | S |

**Refactor `main.rs`**: Change the dispatch from direct function calls to
the `Result<(), CliError>` pattern. Existing commands (`chat`, `status`,
`backup`, `export`, `migrate`) are wrapped to return `CliError` instead
of calling `std::process::exit()` directly.

### 13.2 Phase 1: Essential Commands

The commands you need to operate the platform day-to-day.

| Task | File | Backend | Effort |
|---|---|---|---|
| `ghost init` | `cli/init.rs` | DirectOnly | L |
| `ghost login` / `ghost logout` | `cli/auth.rs` | HttpOnly (validate) | M |
| `ghost doctor` | `cli/doctor.rs` | DirectOnly | M |
| `ghost config show` | `cli/config_cmd.rs` | DirectOnly | M |
| `ghost config validate` | `cli/config_cmd.rs` | DirectOnly | M |
| `ghost agent list` | `cli/agent.rs` | PreferHttp | M |
| `ghost agent create` | `cli/agent.rs` | HttpOnly | M |
| `ghost agent inspect` | `cli/agent.rs` | PreferHttp | M |
| `ghost agent delete` | `cli/agent.rs` | HttpOnly | S |
| `ghost safety status` | `cli/safety.rs` | PreferHttp | M |
| `ghost safety kill-all` | `cli/safety.rs` | HttpOnly | M |
| `ghost safety clear` | `cli/safety.rs` | DirectOnly | S |
| `ghost db migrate` | `cli/db.rs` | DirectOnly | S |
| `ghost db status` | `cli/db.rs` | DirectOnly | M |
| `ghost completions` | `cli/completions.rs` | DirectOnly | S |

### 13.3 Phase 2: Observability Commands

Commands for debugging and monitoring.

| Task | File | Backend | Effort |
|---|---|---|---|
| `ghost logs` | `cli/logs.rs` | HttpOnly (WS) | L |
| `ghost audit query` | `cli/audit_cmd.rs` | PreferHttp | M |
| `ghost audit export` | `cli/audit_cmd.rs` | PreferHttp | M |
| `ghost audit tail` | `cli/audit_cmd.rs` | HttpOnly (WS) | M |
| `ghost convergence scores` | `cli/convergence.rs` | PreferHttp | M |
| `ghost session list` | `cli/session.rs` | PreferHttp | M |
| `ghost session inspect` | `cli/session.rs` | PreferHttp | M |
| `ghost db verify` | `cli/db.rs` | DirectOnly | L |
| `ghost db compact` | `cli/db.rs` | DirectOnly | S |

### 13.4 Phase 3: Identity, Secrets, Policy

Commands for security and identity management.

| Task | File | Backend | Effort |
|---|---|---|---|
| `ghost identity init` | `cli/identity.rs` | DirectOnly | M |
| `ghost identity show` | `cli/identity.rs` | DirectOnly | S |
| `ghost identity drift` | `cli/identity.rs` | DirectOnly | M |
| `ghost identity sign` | `cli/identity.rs` | DirectOnly | S |
| `ghost secret set/list/delete` | `cli/secret.rs` | DirectOnly | M |
| `ghost secret provider` | `cli/secret.rs` | DirectOnly | S |
| `ghost policy show` | `cli/policy.rs` | DirectOnly | M |
| `ghost policy check` | `cli/policy.rs` | DirectOnly | M |
| `ghost policy lint` | `cli/policy.rs` | DirectOnly | S |

### 13.5 Phase 4: Mesh, Skills, Advanced

Commands for multi-agent and extensibility features.

| Task | File | Backend | Effort |
|---|---|---|---|
| `ghost mesh peers` | `cli/mesh.rs` | HttpOnly | M |
| `ghost mesh trust` | `cli/mesh.rs` | HttpOnly | M |
| `ghost mesh discover` | `cli/mesh.rs` | HttpOnly | M |
| `ghost mesh ping` | `cli/mesh.rs` | HttpOnly | S |
| `ghost skill list` | `cli/skill.rs` | PreferHttp | M |
| `ghost skill install` | `cli/skill.rs` | HttpOnly | L |
| `ghost skill inspect` | `cli/skill.rs` | PreferHttp | S |
| `ghost session replay` | `cli/session.rs` | PreferHttp | L |
| `ghost convergence history` | `cli/convergence.rs` | PreferHttp | M |

### 13.6 Effort Key

- S = Small (< 50 lines, < 1 hour)
- M = Medium (50–200 lines, 1–4 hours)
- L = Large (200+ lines, 4+ hours)

---

## 14. Conventions & Style Guide

### 14.1 Rust Code Conventions

These conventions are derived from the existing codebase patterns observed
across all 37 crates:

| Convention | Example | Source |
|---|---|---|
| Error types | `#[derive(Debug, Error)]` with `thiserror` | Every crate |
| Result aliases | `pub type XResult<T> = Result<T, XError>;` | `ghost-audit`, `cortex-storage` |
| Module doc comments | `//! Description.` at top of every file | Every file |
| Function doc comments | `/// Description.` with `///` blank line before params | `runner.rs`, `bootstrap.rs` |
| Tracing structured fields | `tracing::info!(field = %value, "message")` | `bootstrap.rs` |
| UUID generation | `Uuid::now_v7()` for time-ordered, `Uuid::new_v4()` for random | `agents.rs` |
| Arc wrapping | `Arc::new()` at construction, `Arc::clone(&x)` for cloning | `bootstrap.rs` |
| Mutex pattern | `Arc<Mutex<T>>` for shared mutable state | `state.rs` |
| RwLock pattern | `Arc<RwLock<T>>` for read-heavy shared state | `state.rs` (agents) |
| Serde defaults | `#[serde(default)]` on struct fields, `fn default_x()` functions | `config.rs` |
| Feature gating | `#[cfg(all(target_os = "linux", feature = "ebpf"))]` | `ghost-egress` |
| Test organization | `#[cfg(test)] mod tests { ... }` inline, integration in `tests/` | Everywhere |

### 14.2 CLI-Specific Conventions

| Convention | Rule |
|---|---|
| Data output | Always to stdout |
| Diagnostic output | Always to stderr (`eprintln!`, `tracing::*`) |
| Progress indicators | To stderr only |
| Confirmation prompts | To stderr, read from stdin |
| Exit codes | sysexits.h (see §7) |
| Flag naming | `--long-name` with kebab-case, `-s` short form for common flags |
| Subcommand naming | Lowercase, no hyphens: `ghost agent list`, not `ghost agent-list` |
| Argument order | Positional args first, then flags |
| Default behavior | `ghost` with no args = `ghost serve` (preserved from current) |
| Help text | Every command and flag has a `///` doc comment (clap derives help from these) |

### 14.3 Naming Conventions for CLI Files

| Item | Convention | Example |
|---|---|---|
| Module file | Lowercase, matches subcommand group | `agent.rs`, `safety.rs` |
| Subcommand enum | `{Group}Commands` | `AgentCommands`, `SafetyCommands` |
| Dispatch function | `pub async fn run(cmd, backend, output)` | Same signature everywhere |
| Handler function | `async fn handle_{subcommand}(...)` | `handle_list()`, `handle_create()` |
| Response struct | `{Subcommand}Response` | `AgentListResponse`, `SafetyStatusResponse` |
| TableDisplay impl | On the response struct | `impl TableDisplay for AgentListResponse` |

### 14.4 Commit Message Convention

CLI commits follow the existing crate-scoped convention:

```
ghost-gateway/cli: add agent list/create/inspect commands

- AgentCommands enum with List, Create, Inspect, Delete subcommands
- HTTP backend calls GET/POST/DELETE /api/agents
- Direct backend reads from DB for list/inspect
- TableDisplay impl for agent list output
```

Prefix: `ghost-gateway/cli:` for all CLI changes.

---

## 15. Integration with Existing Systems

### 15.1 Integration with `GhostConfig`

The CLI loads config via the existing `GhostConfig::load_default(cli_path)`
method. The `--config` global flag maps directly to the `cli_path` parameter.
No changes to `config.rs` are needed for basic CLI operation.

**New requirement for `ghost config show`**: A `GhostConfig::resolve()`
method that returns a `ResolvedConfig` struct with env var overrides applied.
This is a new method on `GhostConfig`, not a new type — it returns a
display-oriented struct that includes the source of each value (yaml, env,
default).

### 15.2 Integration with Bootstrap

Commands that need the full `AppState` (none in the current design — all
HTTP-based commands talk to the running gateway) do NOT call
`GatewayBootstrap::run()`. Only `ghost serve` calls bootstrap.

Commands that need direct DB access open the connection themselves using
the same pattern as `chat.rs`:

```rust
let db_path = crate::bootstrap::shellexpand_tilde(&config.gateway.db_path);
let conn = rusqlite::Connection::open(&db_path)?;
conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
```

This is encapsulated in `CliBackend::detect()` so individual commands
don't repeat it.

### 15.3 Integration with Migrations

`ghost db migrate` calls `cortex_storage::migrations::run_migrations(&conn)`
directly — the same function that `GatewayBootstrap::run()` calls in step 2.
The migration system is idempotent (checks `schema_version` table), so
running it from the CLI while the gateway is also running is safe because
SQLite WAL mode + busy_timeout handles concurrent access.

`ghost db status` reads from `schema_version` to show the current version
and compares against `cortex_storage::migrations::LATEST_VERSION`.

### 15.4 Integration with Kill Switch

`ghost safety kill-all` via HTTP calls `POST /api/safety/kill-all`, which
uses the in-memory `KillSwitch` and broadcasts a `WsEvent::KillSwitchActivation`.

`ghost safety clear` operates directly on the filesystem: it deletes
`~/.ghost/data/kill_state.json`. This is the only way to exit safe mode
after a KILL_ALL — the gateway checks for this file on startup (see
`bootstrap.rs` pre-step). The clear command does NOT need the gateway
running; in fact, the gateway should typically be stopped when clearing
kill state.

### 15.5 Integration with WebSocket Events

`ghost logs` and `ghost audit tail` connect to the gateway's WebSocket
endpoint at `ws://{gateway_addr}/api/ws?token={token}`. They parse
incoming `WsEvent` JSON messages and display them.

The `WsEvent` enum is defined in `ghost-gateway/src/api/websocket.rs` and
is already `#[derive(Serialize, Deserialize)]` with `#[serde(tag = "type")]`.
The CLI deserializes these directly.

### 15.6 Integration with ghost-secrets

`ghost login`, `ghost logout`, and `ghost secret *` commands use the
`SecretProvider` trait from `ghost-secrets`. The provider is constructed
using the same `crate::config::build_secret_provider()` function that
bootstrap uses. This ensures the CLI and gateway use the same secret
backend.

### 15.7 Integration with ghost-identity

`ghost identity init` calls:
1. `ghost_identity::soul_manager::SoulManager::create_template()` — creates SOUL.md
2. `ghost_identity::keypair_manager::AgentKeypairManager::generate()` — creates Ed25519 keypair

`ghost identity drift` calls:
1. `ghost_identity::drift_detector::DriftDetector::detect()` — compares current behavior against soul document

These are direct library calls, no HTTP needed.

### 15.8 Integration with ghost-audit

`ghost audit query` via HTTP calls `GET /api/audit` with query parameters
that map to `AuditFilter` fields. Via direct backend, it constructs an
`AuditQueryEngine` with the DB connection and calls `query()` directly.

`ghost audit export` via HTTP calls `GET /api/audit/export?format=json`.
Via direct backend, it uses `ghost_audit::export::AuditExporter` directly.

### 15.9 Integration with ghost-policy

`ghost policy show` reads `CORP_POLICY.md` from `~/.ghost/config/CORP_POLICY.md`
and parses it using `ghost_policy::corp_policy::CorpPolicy::load()`.

`ghost policy check` constructs a `ghost_policy::engine::PolicyEngine` and
calls `evaluate()` with a synthetic tool call context.

### 15.10 Integration with the `chat` Command

The existing `chat.rs` is preserved as-is. It already works correctly —
it creates an `AgentRunner` directly without going through HTTP. The only
change is wrapping its error handling to return `CliError` instead of
printing and exiting directly.

---

## 16. Risk Register

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R1 | Auth middleware (T-1.1.1) lands before CLI auth | All HTTP CLI commands break with 401 | Build `ghost login` + token injection in Phase 0, before T-1.1.1 |
| R2 | Binary size bloat from pulling in all crate deps | Slow CI, large distribution | Already the case — gateway binary already links everything. CLI adds ~5KB of code. Profile with `cargo bloat` if concerned. |
| R3 | SQLite concurrent access from CLI + gateway | WAL contention, SQLITE_BUSY | Already mitigated: `PRAGMA busy_timeout=5000` in both paths. CLI direct mode is read-heavy. Write operations go through HTTP. |
| R4 | Config format changes break CLI | CLI reads stale config | CLI uses the same `GhostConfig` type as the gateway — changes propagate automatically. |
| R5 | Clap command tree becomes unwieldy | Slow `--help`, confusing UX | Group related commands under subcommand groups. Use `#[command(hide = true)]` for internal/debug commands. |
| R6 | Shell completions become stale | User frustration | `clap_complete` generates from the live `Command` tree — completions are always in sync with the binary. |
| R7 | Direct backend diverges from HTTP backend | Inconsistent behavior | Direct backend is read-only for data queries. All mutations go through HTTP. Test both paths in integration tests. |
| R8 | `ghost init` creates partial state on failure | Broken installation | Use a temp directory, build everything, then atomically rename to `~/.ghost/`. If any step fails, clean up the temp dir. |
| R9 | Token stored in plaintext on systems without keychain | Security risk | `ghost-secrets` already falls back to env vars when keychain is unavailable. Document that `GHOST_TOKEN` env var is the recommended approach for headless/CI. Never write tokens to disk in plaintext. |
| R10 | `ghost safety clear` run while gateway is running | Gateway doesn't pick up the change until restart | Document that `ghost safety clear` requires a gateway restart. Print a warning: "Restart the gateway for this to take effect." |

---

## Appendix A: `ghost init` Detailed Flow

```
$ ghost init

GHOST Platform Setup
────────────────────

Creating directory structure...
  ✓ ~/.ghost/
  ✓ ~/.ghost/config/
  ✓ ~/.ghost/data/
  ✓ ~/.ghost/backups/
  ✓ ~/.ghost/agents/
  ✓ ~/.ghost/skills/

Generating platform keypair...
  ✓ Ed25519 keypair generated
  ✓ Public key: ~/.ghost/config/platform.pub
  ✓ Fingerprint: SHA256:xYz...

Creating default configuration...
  LLM Provider:
    [1] Anthropic (ANTHROPIC_API_KEY)
    [2] OpenAI (OPENAI_API_KEY)
    [3] Google Gemini (GEMINI_API_KEY)
    [4] Ollama (local)
    [5] Skip for now
  Select [1-5]: 1

  ✓ ghost.yml written to ~/.ghost/config/ghost.yml

Creating default SOUL.md...
  ✓ SOUL.md template written to ~/.ghost/config/SOUL.md

Creating default CORP_POLICY.md...
  ✓ CORP_POLICY.md written to ~/.ghost/config/CORP_POLICY.md

Running database migrations...
  ✓ Database created at ~/.ghost/data/ghost.db
  ✓ Migrations applied (v16 → v19)

Create your first agent?
  Agent name: researcher
  Spending cap [$5.00]: 10
  ✓ Agent 'researcher' created with Ed25519 keypair

Setup complete. Start chatting:
  ghost chat

Or start the gateway server:
  ghost serve
```

With `--defaults` flag, all prompts are skipped and defaults are used.

---

## Appendix B: `ghost doctor` Detailed Flow

```
$ ghost doctor

GHOST Platform Health Check
───────────────────────────

Prerequisites
  ✓ Rust 1.80+ (1.82.0 installed)
  ✓ ~/.ghost/ directory exists
  ✓ ghost.yml valid
  ✓ SOUL.md present
  ✓ CORP_POLICY.md present
  ✓ Platform keypair present (fingerprint: SHA256:xYz...)

LLM Providers
  ✓ ANTHROPIC_API_KEY set
  ✗ OPENAI_API_KEY not set
  ✗ GEMINI_API_KEY not set
  ✗ OLLAMA_BASE_URL not set

Database
  ✓ ghost.db exists (2.4 MB)
  ✓ Migrations current (v19)
  ✓ WAL mode enabled
  ✓ Hash chains intact (spot check: 100 random entries)

Gateway
  ✓ Running at 127.0.0.1:18789 (state: Healthy)
  ✓ 2 agents registered

Convergence Monitor
  ✗ Not reachable at 127.0.0.1:18790
    → Gateway will run in Degraded mode without the monitor.
    → Start with: cargo run -p convergence-monitor

Kill State
  ✓ No kill_state.json (clean state)

Disk Space
  ✓ ~/.ghost/ using 12.8 MB
  ✓ Backups: 3 files, 8.2 MB, newest: 2h ago

Summary: 11 passed, 4 warnings, 0 errors
```

---

## Appendix C: Mapping CLI Commands to Existing API Endpoints

| CLI Command | HTTP Method | Endpoint | Direct DB Alternative |
|---|---|---|---|
| `ghost agent list` | `GET` | `/api/agents` | `SELECT * FROM agents` (if table exists) or read config |
| `ghost agent create` | `POST` | `/api/agents` | None (needs registry + WS broadcast) |
| `ghost agent inspect` | `GET` | `/api/agents` + filter | Read from config + DB |
| `ghost agent delete` | `DELETE` | `/api/agents/:id` | None (needs registry + WS broadcast) |
| `ghost agent pause` | `POST` | `/api/safety/pause/:id` | None (needs KillSwitch) |
| `ghost agent resume` | `POST` | `/api/safety/resume/:id` | None (needs KillSwitch) |
| `ghost agent quarantine` | `POST` | `/api/safety/quarantine/:id` | None (needs KillSwitch) |
| `ghost safety status` | `GET` | `/api/safety/status` | Read `kill_state.json` |
| `ghost safety kill-all` | `POST` | `/api/safety/kill-all` | None (needs KillSwitch + WS) |
| `ghost safety clear` | N/A | N/A | Delete `kill_state.json` |
| `ghost audit query` | `GET` | `/api/audit` | `AuditQueryEngine::query()` |
| `ghost audit export` | `GET` | `/api/audit/export` | `AuditExporter::export()` |
| `ghost convergence scores` | `GET` | `/api/convergence/scores` | Read state file |
| `ghost session list` | `GET` | `/api/sessions` | Query `itp_events` |
| `ghost session inspect` | `GET` | `/api/sessions/:id/events` | Query `itp_events` |
| `ghost db migrate` | N/A | N/A | `run_migrations()` |
| `ghost db status` | N/A | N/A | Query `schema_version` |
| `ghost db verify` | N/A | N/A | Walk hash chains in `itp_events` |
| `ghost db compact` | N/A | N/A | `PRAGMA wal_checkpoint(TRUNCATE); VACUUM;` |
| `ghost config show` | N/A | N/A | `GhostConfig::load_default()` |
| `ghost config validate` | N/A | N/A | `GhostConfig::load_default()` + `validate()` |
| `ghost status` | `GET` | `/api/health` + `/api/ready` | Check process + files |
| `ghost logs` | WS | `/api/ws` | None (needs live events) |
| `ghost audit tail` | WS | `/api/ws` (filter to audit) | None (needs live events) |

---

## Appendix D: Interaction with ADE Design Plan Tasks

| ADE Task | CLI Impact | Notes |
|---|---|---|
| T-1.1.1 (REST auth middleware) | CLI must send `Authorization` header | Phase 0 `ghost login` + `http_client.rs` |
| T-1.1.3 (JWT auth endpoints) | `ghost login` extended for username/password | Phase 1 update to `auth.rs` |
| T-1.1.5 (Rate limiting) | CLI must handle 429 responses | `http_client.rs` retry logic |
| T-1.1.6 (Request ID tracing) | CLI can display `X-Request-ID` in errors | `CliError::Http` includes request ID |
| T-1.2.1 (CostTracker wiring) | `ghost agent inspect` shows real costs | No CLI change needed — data comes from API |
| T-1.3.1 (OpenAPI spec) | Future: generate CLI from OpenAPI | Not in scope for this design |
| T-1.3.2 (Error response contract) | CLI error mapping uses standard envelope | `http_client.rs` parses `ErrorResponse` |
| T-2.1.8 (WS topic subscriptions) | `ghost logs --agent <id>` uses topic filter | Send `Subscribe` message after WS connect |
| T-3.4.1 (Backup endpoint) | `ghost backup` can use HTTP instead of direct | Add HTTP path to existing `run_backup()` |


---

## Appendix E: Design Audit — Gaps, Flaws, and Improvements

> **Date**: March 2026
> **Method**: Cross-referenced the design against Rain's Rust CLI
> recommendations ([source](https://rust-cli-recommendations.sunshowers.io)),
> clap 4 best practices, Cargo's own CLI patterns, the `assert_cmd` testing
> ecosystem, and community patterns for Rust CLI architecture.
>
> Content was rephrased for compliance with licensing restrictions.

### E.1 Structural Flaw: Global Options Should Use `#[command(flatten)]`

**Problem**: §4.3 puts global options as individual fields on the root `Cli`
struct. Rain's Rust CLI recommendations (the authoritative guide, written by
the nextest author) strongly recommends extracting global options into a
separate `GlobalOpts` struct and using `#[command(flatten)]`.

**Why it matters**: As the CLI grows, global options will be passed to every
command handler. Passing 5 individual fields is worse than passing one
`&GlobalOpts` reference. It also makes it harder to add new global options
later without touching every handler signature.

**Fix**: Change §4.3 from individual fields to:

```rust
#[derive(Debug, clap::Args)]
pub struct GlobalOpts {
    /// Path to ghost.yml configuration file.
    #[arg(long, short, global = true)]
    config: Option<String>,

    /// Output format: table, json, yaml.
    #[arg(long, short, global = true, default_value = "table")]
    output: OutputFormat,

    /// Override gateway base URL.
    #[arg(long, short = 'g', global = true)]
    gateway_url: Option<String>,

    /// Enable verbose (debug) logging.
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Suppress non-essential output.
    #[arg(long, short, global = true)]
    quiet: bool,
}

#[derive(Parser)]
#[command(name = "ghost", about = "GHOST Platform", version)]
struct Cli {
    #[command(flatten)]
    global: GlobalOpts,

    #[command(subcommand)]
    command: Option<Commands>,
}
```

Then every handler receives `&GlobalOpts` instead of 5 separate params.

### E.2 Flaw: Tokio Runtime Overhead for Sync Commands

**Problem**: §4.3 uses `#[tokio::main]` which spins up a multi-threaded
runtime for every command, including purely synchronous ones like
`ghost config show`, `ghost completions bash`, and `ghost db status`.
The multi-threaded runtime spawns worker threads even when no async work
is needed.

**Why it matters**: Startup latency. For a command like `ghost completions`
that should return in <10ms, spinning up a thread pool is wasteful. More
importantly, it affects shell completion responsiveness — completions run
on every tab press.

**Fix**: Use `#[tokio::main(flavor = "current_thread")]` instead of the
default multi-threaded runtime. The CLI is not a server — it makes at most
a few HTTP requests sequentially. The current-thread runtime avoids spawning
extra OS threads while still supporting async/await for HTTP calls.

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() {
    // ...
}
```

The `ghost serve` command, which starts the actual axum server, should
build its own multi-threaded runtime internally if needed. But since
`serve` calls `GatewayBootstrap::run()` which already uses tokio
internally, and the `#[tokio::main]` runtime is inherited, this works
fine — `current_thread` can still run axum (it just uses one thread).
For production `serve`, consider switching to multi-threaded only for
that subcommand:

```rust
Commands::Serve => {
    // Build a multi-threaded runtime for the server
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async { /* bootstrap + serve */ });
}
```

### E.3 Gap: No WebSocket Client Dependency for `ghost logs` / `ghost audit tail`

**Problem**: §15.5 says `ghost logs` connects to the gateway WebSocket, but
the workspace has no WebSocket *client* dependency. The gateway uses axum's
built-in WS server (`axum::extract::ws`), which is server-side only. The
`reqwest` crate does not support WebSocket connections.

**Why it matters**: `ghost logs` and `ghost audit tail` are two of the
highest-value CLI commands. Without a WS client, they can't be built.

**Fix**: Add `tokio-tungstenite` to the workspace dependencies:

```toml
# In root Cargo.toml [workspace.dependencies]
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }

# In ghost-gateway/Cargo.toml [dependencies]
tokio-tungstenite = { workspace = true }
```

`tokio-tungstenite` is the standard async WS client for the tokio ecosystem.
It's lightweight and well-maintained. The `native-tls` feature enables TLS
for `wss://` connections (needed when the gateway is behind a reverse proxy).

### E.4 Gap: No Streaming Output Format (NDJSON)

**Problem**: §6 defines three output formats: `table`, `json`, `yaml`. But
for streaming commands (`ghost logs`, `ghost audit tail`), pretty-printed
JSON is wrong — each event should be a single line of JSON (newline-delimited
JSON / NDJSON).

**Why it matters**: This is how `cargo build --message-format json` works.
Rain's CLI recommendations explicitly state: "If many lines of structured
data are incrementally printed out, prefer newline-delimited JSON." Without
NDJSON, piping `ghost logs --output json` to `jq` won't work because
pretty-printed JSON spans multiple lines per event.

**Fix**: Add a fourth output format variant:

```rust
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,       // Pretty-printed JSON (for single responses)
    JsonLines,  // NDJSON (for streaming commands)
    Yaml,
}
```

Streaming commands (`logs`, `audit tail`) should default to `table` for
humans and automatically switch to `json-lines` when `--output json` is
used in a streaming context. Or expose it explicitly as `--output json-lines`.

### E.5 Gap: No Color Support or `--color` Flag

**Problem**: The design has no mention of terminal color output. The
`ghost doctor` output (Appendix B) uses `✓` and `✗` symbols but no color.
Destructive command warnings (§8) mention "Red" and "Yellow" but there's
no mechanism to produce colored output or respect `NO_COLOR` / `FORCE_COLOR`
environment variables.

**Why it matters**: Color is a basic CLI UX expectation. More importantly,
the [NO_COLOR standard](https://no-color.org/) and `FORCE_COLOR` convention
must be respected for accessibility and CI environments.

**Fix**: Add a `--color` global option (following Rain's recommendation):

```rust
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}
```

Add to `GlobalOpts`:
```rust
/// Color output: auto, always, never. Respects NO_COLOR env var.
#[arg(long, global = true, default_value = "auto")]
color: ColorChoice,
```

For implementation, use raw ANSI codes (no external crate needed for basic
red/yellow/green). Auto-detect by checking:
1. `NO_COLOR` env var set → disable
2. `FORCE_COLOR` env var set → enable
3. `--color never` → disable
4. `--color always` → enable
5. `stdout.is_terminal()` → enable if true, disable if piped

Colors must be disabled when `--output json` or `--output yaml` is used
(per Rain's recommendation: "Colors must be disabled for machine-readable
output").

### E.6 Flaw: Error Handling Should Use `anyhow` Internally, `thiserror` at Boundaries

**Problem**: §7 defines `CliError` with `thiserror`. This is correct for
the error type definition, but the design doesn't address how errors from
the 15+ workspace crates (each with their own error types) get converted
to `CliError`.

**Why it matters**: Without `From` impls or `anyhow` context, every crate
call requires `.map_err(|e| CliError::Database(e.to_string()))` boilerplate.
The existing codebase already has `anyhow` in workspace dependencies.

**Fix**: Keep `CliError` as the public error type (with `thiserror` for
structured exit codes), but use `anyhow::Context` internally for error
propagation with context:

```rust
use anyhow::Context;

async fn handle_list(backend: &CliBackend, output: OutputFormat) -> Result<(), CliError> {
    let agents = backend.get("/api/agents").await
        .context("failed to fetch agent list")
        .map_err(|e| CliError::Http(format!("{e:#}")))?;
    // ...
}
```

The `{e:#}` format with anyhow gives the full error chain. This is much
better than losing context with `.to_string()`.

### E.7 Gap: Testing Should Use `assert_cmd` + `predicates`

**Problem**: §12 shows raw `std::process::Command` for integration tests.
This works but is verbose and doesn't provide good error messages on failure.

**Why it matters**: `assert_cmd` is the standard Rust crate for CLI
integration testing (used by ripgrep, bat, fd, and most major Rust CLIs).
It provides `Command::cargo_bin("ghost")` which automatically finds the
binary, and `predicates` provides composable assertions.

**Fix**: Add to dev-dependencies:

```toml
# ghost-gateway/Cargo.toml [dev-dependencies]
assert_cmd = "2"
predicates = "3"
```

Rewrite integration tests:

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn status_shows_gateway_state() {
    Command::cargo_bin("ghost")
        .unwrap()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Gateway:"));
}

#[test]
fn agent_list_json_is_valid() {
    Command::cargo_bin("ghost")
        .unwrap()
        .args(["agent", "list", "--output", "json"])
        .assert()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn unknown_command_fails() {
    Command::cargo_bin("ghost")
        .unwrap()
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}
```

### E.8 Gap: No Signal Handling for Streaming Commands

**Problem**: `ghost logs` and `ghost audit tail` are long-running streaming
commands. The design doesn't address how they handle Ctrl+C gracefully.

**Why it matters**: If the user presses Ctrl+C during `ghost logs`, the
WebSocket connection should be closed cleanly (send a Close frame) rather
than just being dropped. Dropped connections leave the server-side handler
in a broken state until the next keepalive timeout.

**Fix**: Use `tokio::signal::ctrl_c()` in a `select!` alongside the WS
message loop:

```rust
loop {
    tokio::select! {
        msg = ws_stream.next() => {
            match msg {
                Some(Ok(msg)) => { /* print event */ }
                _ => break,
            }
        }
        _ = tokio::signal::ctrl_c() => {
            // Send WS Close frame
            ws_stream.close(None).await.ok();
            break;
        }
    }
}
```

This is already the pattern used in `gateway.rs::run_with_router()` for
the server's graceful shutdown.

### E.9 Flaw: `ghost init` Atomicity is Under-Specified

**Problem**: §16 R8 mentions using a temp directory and atomic rename for
`ghost init`, but Appendix A shows a step-by-step flow that creates files
directly in `~/.ghost/`. If any step fails mid-way, the user is left with
a partial installation.

**Fix**: Specify the exact atomicity strategy:

1. Create `~/.ghost/.init-tmp-{uuid}/` as a staging directory.
2. Build all files (config, SOUL.md, CORP_POLICY.md, keypair) in the
   staging directory.
3. Run DB migrations against a DB file in the staging directory.
4. If all steps succeed, rename staging contents into `~/.ghost/`.
5. If any step fails, delete the staging directory and print the error.
6. If `~/.ghost/` already exists, refuse to overwrite (require `--force`).

### E.10 Gap: No `--format-version` for Machine-Readable Output Stability

**Problem**: §6 defines JSON output but doesn't address output stability.
Rain's CLI recommendations state: "Within a binary version series, output
must be kept stable and append-only. Breaking changes must be gated to an
argument (e.g., `--format-version 2`)."

**Why it matters**: If someone writes a script that parses `ghost agent list
--output json`, and a future version changes the JSON shape, their script
breaks silently.

**Fix**: Document the stability contract:

- JSON output fields are append-only within a major version.
- New fields may be added; existing fields are never removed or renamed.
- If a breaking change is needed, add `--format-version 2` flag.
- The default format version is always the latest.

This mirrors the `cargo --message-format json-v2` pattern.

### E.11 Gap: XDG Base Directory Compliance

**Problem**: The design hardcodes `~/.ghost/` as the data directory. This
violates the XDG Base Directory Specification on Linux, where config should
go to `$XDG_CONFIG_HOME/ghost/` (default `~/.config/ghost/`) and data to
`$XDG_DATA_HOME/ghost/` (default `~/.local/share/ghost/`).

**Why it matters**: Linux users and package maintainers expect XDG
compliance. The `dirs` crate (already used implicitly via `ghost-secrets`
which uses OS keychain paths) provides cross-platform directory resolution.

**Fix**: This is a larger change that affects the entire platform, not just
the CLI. For now, document the decision:

- `~/.ghost/` is the canonical directory for all platforms (macOS, Linux,
  Windows).
- A future version may add `GHOST_HOME` env var override.
- XDG compliance is deferred to a future release, gated behind a
  `GHOST_XDG=1` env var for opt-in migration.

The `ghost doctor` command should warn if `~/.ghost/` doesn't exist but
XDG directories do (indicating a user who expects XDG compliance).

### E.12 Gap: No Retry Logic for HTTP Requests

**Problem**: §5.4 defines `GhostHttpClient` with a 30-second timeout but
no retry logic. If the gateway returns 429 (rate limited) or 503 (temporarily
unavailable), the CLI fails immediately.

**Fix**: Add basic retry with exponential backoff for retryable status codes:

```rust
const RETRYABLE_STATUSES: &[u16] = &[429, 502, 503, 504];
const MAX_RETRIES: u32 = 3;

async fn request_with_retry(&self, req: reqwest::RequestBuilder) -> Result<Response, CliError> {
    let mut delay = Duration::from_millis(500);
    for attempt in 0..=MAX_RETRIES {
        let resp = req.try_clone().unwrap().send().await
            .map_err(|e| CliError::Http(e.to_string()))?;

        if !RETRYABLE_STATUSES.contains(&resp.status().as_u16()) || attempt == MAX_RETRIES {
            return Ok(resp);
        }

        // Respect Retry-After header if present (from T-1.1.5 rate limiting)
        if let Some(retry_after) = resp.headers().get("retry-after") {
            if let Ok(secs) = retry_after.to_str().unwrap_or("1").parse::<u64>() {
                delay = Duration::from_secs(secs);
            }
        }

        eprintln!("Retrying in {}ms (attempt {}/{})...", delay.as_millis(), attempt + 1, MAX_RETRIES);
        tokio::time::sleep(delay).await;
        delay *= 2;
    }
    unreachable!()
}
```

### E.13 Improvement: Subcommand Args Should Use Separate Structs

**Problem**: §4.3 shows some subcommands with inline args and some with
separate structs. The design should be consistent.

**Fix**: Every subcommand with more than 2 arguments should use a separate
`Args` struct with `#[derive(clap::Args)]`. This follows Rain's
recommendation of "liberal use of `#[clap(flatten)]`" and makes it easier
to pass args to handler functions:

```rust
#[derive(Subcommand)]
pub enum AgentCommands {
    List,
    Create(CreateAgentArgs),
    Inspect(InspectAgentArgs),
    Delete(DeleteAgentArgs),
    // ...
}

#[derive(clap::Args)]
pub struct CreateAgentArgs {
    /// Agent name.
    pub name: String,
    /// LLM model to use.
    #[arg(long)]
    pub model: Option<String>,
    /// Daily spending cap in dollars.
    #[arg(long, default_value = "5.0")]
    pub spending_cap: f64,
    /// Comma-separated list of capabilities.
    #[arg(long, value_delimiter = ',')]
    pub capabilities: Vec<String>,
    /// Skip Ed25519 keypair generation.
    #[arg(long)]
    pub no_keypair: bool,
}
```

### E.14 Improvement: `reqwest::Client` Should Be Reused

**Problem**: §5.4 creates a new `reqwest::Client` in `GhostHttpClient::new()`.
This is correct. But the design doesn't specify that `GhostHttpClient` should
be created once and passed to all commands, not recreated per-request.

**Why it matters**: `reqwest::Client` uses an internal `Arc` and connection
pool. Creating one per request defeats connection reuse. For a CLI that makes
1-3 requests, this is minor, but it's a correctness issue.

**Fix**: Create `GhostHttpClient` once in the `main()` dispatch, pass it
to all handlers via `&CliBackend`. The `CliBackend::Http` variant should
own the `GhostHttpClient`:

```rust
pub enum CliBackend {
    Http {
        client: GhostHttpClient,  // owns the reqwest::Client
    },
    Direct {
        config: GhostConfig,
        db: Arc<Mutex<rusqlite::Connection>>,
    },
}
```

### E.15 Summary of Required Changes to the Design

| # | Type | Section | Change |
|---|---|---|---|
| E.1 | Structural | §4.3 | Extract `GlobalOpts` struct with `#[command(flatten)]` |
| E.2 | Performance | §4.3 | Use `current_thread` tokio runtime, multi-thread only for `serve` |
| E.3 | Missing dep | §10 | Add `tokio-tungstenite` for WS client (`ghost logs`, `ghost audit tail`) |
| E.4 | Missing format | §6 | Add `JsonLines` (NDJSON) output format for streaming commands |
| E.5 | Missing feature | §4.2, §14 | Add `--color auto|always|never` global flag, respect `NO_COLOR` |
| E.6 | Error handling | §7 | Use `anyhow::Context` internally, keep `CliError` at boundaries |
| E.7 | Testing | §12 | Use `assert_cmd` + `predicates` instead of raw `std::process::Command` |
| E.8 | Missing feature | §15.5 | Add Ctrl+C signal handling for streaming commands |
| E.9 | Under-specified | Appendix A | Specify atomic staging directory for `ghost init` |
| E.10 | Missing contract | §6 | Document JSON output stability contract (append-only) |
| E.11 | Convention | §11 | Document XDG non-compliance decision, add `GHOST_HOME` env var |
| E.12 | Missing feature | §5.4 | Add retry with exponential backoff for 429/503 responses |
| E.13 | Consistency | §4.3 | Use separate `Args` structs for all subcommands with >2 args |
| E.14 | Correctness | §5.4 | Ensure `reqwest::Client` is created once and reused |

### E.16 Dependencies Audit Update

The original §10 listed `clap_complete` as the only hard new dependency.
After this audit, the full list is:

| Crate | Version | Required? | Purpose |
|---|---|---|---|
| `clap_complete` | `4` | Yes | Shell completions |
| `tokio-tungstenite` | `0.24` | Yes | WS client for `ghost logs` / `ghost audit tail` |
| `assert_cmd` | `2` | Dev only | CLI integration testing |
| `predicates` | `3` | Dev only | Assertion library for `assert_cmd` |
| `indicatif` | `0.17` | No (defer) | Progress bars — hand-roll first |
| `comfy-table` | `7` | No (defer) | Table formatting — hand-roll first |
| `dialoguer` | `0.11` | No (defer) | Interactive prompts — use raw stdin first |

### E.17 Risk Register Additions

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R11 | `tokio-tungstenite` version conflicts with axum's internal hyper/tokio versions | Build failures | Pin to version compatible with axum 0.7's tokio/hyper versions. Test with `cargo check` before merging. |
| R12 | Streaming commands (`ghost logs`) hold WS connection indefinitely | Resource leak if user backgrounds the process | Implement idle timeout (close after 30min of no events). Document that `ghost logs` is meant for interactive use. |
| R13 | `ghost safety clear` deletes `kill_state.json` while gateway is reading it | Race condition | Use atomic file operations: write empty file first, then delete. Or use `flock()` advisory locking. Document that gateway should be stopped first. |
| R14 | Shell completions for dynamic values (agent names, session IDs) | Completions show static options only | Use `clap_complete`'s custom completer to call `ghost agent list --output json` for dynamic completions. Defer to Phase 2. |

---

## Appendix F: Final Engineering Audit — Source-Level Verification

> **Date**: March 2026
> **Method**: Every integration claim, API reference, function signature,
> type name, and module path in this document was cross-referenced against
> the actual source code. Each finding below is a verified discrepancy
> between what the design says and what the code actually does.

### F.1 CRITICAL: `SecretProvider` Method Names Are Wrong

**Design says** (§5.2, `auth.rs` code block):
```rust
provider.set(CLI_TOKEN_KEY, token)
provider.get(CLI_TOKEN_KEY)
provider.delete(CLI_TOKEN_KEY)
```

**Actual trait** (`crates/ghost-secrets/src/provider.rs`):
```rust
fn get_secret(&self, key: &str) -> Result<SecretString, SecretsError>;
fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretsError>;
fn delete_secret(&self, key: &str) -> Result<(), SecretsError>;
```

The methods are `get_secret`, `set_secret`, `delete_secret` — not `get`,
`set`, `delete`. Additionally, `get_secret` returns `Result<SecretString, _>`,
not `Result<String, _>`. The CLI must call `.expose_secret()` (from the
`secrecy` crate re-exported by `ghost-secrets`) to get the inner `&str`.

**Impact**: Every line of `auth.rs` that touches `SecretProvider` will fail
to compile.

**Fix**: Update §5.2 code:
```rust
provider.set_secret(CLI_TOKEN_KEY, token)
    .map_err(|e| CliError::Auth(format!("failed to store token: {e}")))?;

provider.get_secret(CLI_TOKEN_KEY)
    .ok()
    .map(|s| s.expose_secret().to_string())

provider.delete_secret(CLI_TOKEN_KEY)
    .map_err(|e| CliError::Auth(format!("failed to clear token: {e}")))?;
```

### F.2 CRITICAL: `CorpPolicy::load()` Does Not Exist

**Design says** (§15.9):
> `ghost policy show` reads `CORP_POLICY.md` and parses it using
> `ghost_policy::corp_policy::CorpPolicy::load()`.

**Actual code** (`crates/ghost-policy/src/corp_policy.rs`):
`CorpPolicy` has `new()` and `with_denied_tools(HashSet<String>)`. There
is no `load()` method. There is no file parser. The struct is a simple
in-memory deny-list — it does not read `CORP_POLICY.md` from disk.

**Impact**: `ghost policy show` and `ghost policy lint` cannot delegate to
`ghost-policy` for parsing. The CLI must implement its own CORP_POLICY.md
parser, or the `ghost-policy` crate needs a `load_from_file()` method added.

**Fix**: Add to Phase 1 prerequisites: implement `CorpPolicy::load(path: &Path)`
in `ghost-policy/src/corp_policy.rs` that parses the markdown deny-list format.
Until then, `ghost policy show` should read the raw markdown and display it,
and `ghost policy lint` should validate the markdown structure directly.

### F.3 CRITICAL: `SoulManager::create_template()` Does Not Exist

**Design says** (§15.7):
> `ghost identity init` calls
> `ghost_identity::soul_manager::SoulManager::create_template()`

**Actual code** (`crates/ghost-identity/src/soul_manager.rs`):
`SoulManager` has `new()`, `load(path)`, `document()`,
`set_baseline_embedding()`, `baseline_embedding()`. There is no
`create_template()` method. The manager can only load an existing
SOUL.md — it cannot create one.

**Impact**: `ghost identity init` and `ghost init` cannot delegate soul
document creation to `ghost-identity`. The CLI must either:
1. Write a hardcoded SOUL.md template directly (simple, works now), or
2. Add `SoulManager::create_template(path: &Path)` to `ghost-identity`.

**Fix**: Option 1 for Phase 0 (write template inline in `init.rs`), then
backfill with option 2 in Phase 3 when `ghost identity` commands are built.

### F.4 CRITICAL: `DriftDetector::detect()` Does Not Exist

**Design says** (§15.7):
> `ghost identity drift` calls
> `ghost_identity::drift_detector::DriftDetector::detect()`

**Actual code** (`crates/ghost-identity/src/drift_detector.rs`):
The type is `IdentityDriftDetector` (not `DriftDetector`). It has
`compute_drift(baseline, current)` and `evaluate(drift_score)`. There is
no `detect()` method. The detector requires pre-computed embedding vectors
as input — it does not load files or compute embeddings itself.

**Impact**: `ghost identity drift` cannot be a simple one-liner. It needs to:
1. Load SOUL.md via `SoulManager::load()`
2. Obtain baseline embedding (stored where? Not on disk currently)
3. Compute current embedding (requires LLM embedding API call)
4. Call `compute_drift()` then `evaluate()`

This is significantly more complex than the design implies. Steps 2-3
require either a stored baseline (which `SoulManager` can hold in memory
but doesn't persist) or an LLM provider to generate embeddings.

**Fix**: Document this complexity in §15.7. `ghost identity drift` should
be moved from Phase 3 to Phase 4 (Advanced), or scoped down to a hash-based
drift check (compare SOUL.md blake3 hash against a stored baseline hash,
which `SoulDocument.hash` already provides).

### F.5 HIGH: No `GET /api/sessions/:id/events` Endpoint

**Design says** (Appendix C):
> `ghost session inspect` → `GET /api/sessions/:id/events`

**Actual routes** (`bootstrap.rs::build_router()`):
Only `/api/sessions` (list) exists. There is no `/api/sessions/:id/events`
route. The `sessions.rs` handler only has `list_sessions`.

**Impact**: `ghost session inspect <id>` has no HTTP backend. It can only
work in Direct mode by querying `itp_events` table directly.

**Fix**: Either:
1. Add the endpoint to the gateway (new task, not in CLI scope), or
2. Reclassify `ghost session inspect` as `DirectOnly` in §3.3 and
   implement it with direct DB queries using
   `cortex_storage::queries::itp_event_queries`.

### F.6 HIGH: No `GET /api/agents/:id` Endpoint (Single Agent)

**Design says** (Appendix C):
> `ghost agent inspect` → `GET /api/agents` + filter

**Actual routes**: Only `GET /api/agents` (list all) and
`DELETE /api/agents/:id`. There is no single-agent GET endpoint.

**Impact**: `ghost agent inspect <id>` via HTTP must fetch the full list
and filter client-side, which is what the design says ("+ filter"). This
works but is inefficient. More importantly, the design should note this
explicitly as a known limitation and suggest adding `GET /api/agents/:id`
as a prerequisite task.

**Fix**: Add a note to §15.1 that `GET /api/agents/:id` should be added
to the gateway. For Phase 1, the CLI can filter the list response.

### F.7 HIGH: No `POST /api/safety/pause/:id` or `quarantine/:id` in CLI Command Tree

**Design says** (§4.1):
> `ghost agent pause <id>`, `ghost agent resume <id>`,
> `ghost agent quarantine <id>`

**Actual routes** (`bootstrap.rs::build_router()`):
```
POST /api/safety/pause/:agent_id
POST /api/safety/resume/:agent_id
POST /api/safety/quarantine/:agent_id
```

These are under `/api/safety/`, not `/api/agents/`. The design puts
`pause`, `resume`, `quarantine` under the `agent` subcommand group but
the API puts them under `safety`.

**Impact**: UX confusion — the CLI groups these as agent operations but
the API treats them as safety operations. Not a compile error, but the
CLI's `agent.rs` module will need to call safety endpoints, creating a
cross-concern dependency.

**Fix**: Two options:
1. Keep CLI grouping as-is (user-friendly) but document that `agent.rs`
   calls `/api/safety/*` endpoints internally. This is fine.
2. Move `pause/resume/quarantine` to the `safety` subcommand group in
   the CLI to match the API. This is more consistent but less intuitive.

Recommend option 1 — the CLI should optimize for user mental model, not
API structure.

### F.8 HIGH: `commands.rs` Uses `std::process::exit()` Directly

**Design says** (§13.1):
> Existing commands (`backup`, `export`, `migrate`) are wrapped to return
> `CliError` instead of calling `std::process::exit()` directly.

**Actual code** (`crates/ghost-gateway/src/cli/commands.rs`):
All three functions (`run_backup`, `run_export`, `run_migrate`) call
`std::process::exit(1)` on error. They also use their own `expand_tilde()`
instead of `crate::bootstrap::shellexpand_tilde()`.

**Impact**: The Phase 0 refactor of `main.rs` must also refactor these
three functions to return `Result<(), CliError>`. This is more work than
the design implies — it's not just wrapping, it's rewriting the error paths.

**Fix**: Add explicit refactoring tasks for `commands.rs`:
- Change return types to `Result<(), CliError>`
- Replace `std::process::exit(1)` with `Err(CliError::...)`
- Replace local `expand_tilde()` with `crate::bootstrap::shellexpand_tilde()`
- Replace `eprintln!` error output with `CliError` propagation

### F.9 HIGH: `status.rs` Hardcodes Gateway URL

**Design says** (§4.2):
> `--gateway-url` global flag overrides the gateway base URL.

**Actual code** (`crates/ghost-gateway/src/cli/status.rs`):
```rust
let base_url = "http://127.0.0.1:18789";
```

Hardcoded. Ignores `--gateway-url`, ignores `GhostConfig`, ignores the
`config_path` parameter it receives.

**Impact**: `ghost status --gateway-url http://other:9999` would be silently
ignored. The Phase 0 refactor must pass the resolved base URL from
`GlobalOpts` or `CliBackend` into `show_status()`.

**Fix**: Refactor `show_status()` to accept `&CliBackend` or at minimum
a `base_url: &str` parameter. This is part of the Phase 0 `main.rs`
dispatch refactor.

### F.10 MEDIUM: `run_migrations()` Returns `CortexResult<()>`, Not Migration Count

**Design says** (§15.3):
> `ghost db migrate` calls `cortex_storage::migrations::run_migrations(&conn)`

**Actual code** (`crates/cortex/cortex-storage/src/migrations/mod.rs`):
The public API is `run_migrations(conn)` which returns `CortexResult<()>`.
But the internal implementation does track `applied` count. The public
`run_all_migrations(conn)` in `lib.rs` also returns `CortexResult<()>`.

**Impact**: `ghost db migrate` cannot report "Applied N migrations" without
either:
1. Querying `schema_version` before and after, or
2. Changing `run_migrations()` to return `CortexResult<u32>`.

**Fix**: Query `schema_version` before and after in the CLI. Don't change
the library API just for CLI reporting.

### F.11 MEDIUM: No `current_version()` Public Function

**Design says** (§15.3):
> `ghost db status` reads from `schema_version` to show the current version
> and compares against `cortex_storage::migrations::LATEST_VERSION`.

**Actual code**: `LATEST_VERSION` is public (`pub const LATEST_VERSION: u32 = 19`).
But there is no public `current_version(conn)` function in the main crate.
The explore/drift copies have one, but the workspace copy does not.

**Impact**: `ghost db status` must inline the query:
```rust
let current: u32 = conn.query_row(
    "SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0)
)?;
```

**Fix**: Either add `pub fn current_version(conn: &Connection) -> CortexResult<u32>`
to `cortex-storage`, or inline the query in `cli/db.rs`. Recommend adding
the public function — it's 5 lines and useful beyond the CLI.

### F.12 MEDIUM: `CliBackend::detect()` References Non-Existent `load_stored_token()`

**Design says** (§3.2, `backend.rs` code block):
```rust
let token = load_stored_token();
```

This function is referenced but never defined in the design. It's not in
`auth.rs` either — `auth.rs` defines `load_token(provider)` which requires
a `&dyn SecretProvider` parameter.

**Impact**: `CliBackend::detect()` needs access to a `SecretProvider` to
load the token, but the design constructs the backend before the secret
provider is available (chicken-and-egg: you need config to build the
provider, but you need the backend to load config in HTTP mode).

**Fix**: The detection sequence must be:
1. Load `GhostConfig` (always local, no backend needed)
2. Build `SecretProvider` from config
3. Load token from provider
4. Detect backend (HTTP probe with token, or direct DB)

Update §3.2 to accept `token: Option<String>` as a parameter to `detect()`
rather than loading it internally.

### F.13 MEDIUM: `ghost config show` Env Var Overlay Is Incomplete

**Design says** (§11.2):
> Overlays known env var overrides for `GHOST_TOKEN`, `GHOST_JWT_SECRET`,
> `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `OLLAMA_BASE_URL`.

**Actual config** (`crates/ghost-gateway/src/config.rs`):
The config also supports `GHOST_CORS_ORIGINS` (checked in bootstrap),
`GHOST_BACKUP_KEY` (checked in `commands.rs`), and the vault token env
var (configurable via `secrets.vault.token_env`, default `VAULT_TOKEN`).
The `models` section has per-provider configs with `api_key_env` fields.

**Impact**: `ghost config show` would miss several env var overrides,
giving an incomplete picture.

**Fix**: Expand the overlay list in §11.2 to include all env vars checked
across the codebase. Add a comment in the implementation noting that new
env vars must be added here when introduced.

### F.14 MEDIUM: `ghost safety clear` Behavior Mismatch

**Design says** (§15.4):
> `ghost safety clear` operates directly on the filesystem: it deletes
> `~/.ghost/data/kill_state.json`.

**Actual code** (`crates/ghost-gateway/src/api/safety.rs`, line 62-77):
The `kill_all` handler writes `kill_state.json` with a JSON body containing
`reason`, `timestamp`. The bootstrap pre-step reads this file and restores
kill state.

**The problem**: Simply deleting the file is correct for clearing the
persisted state, but the design says "This is the only way to exit safe
mode after a KILL_ALL." This is wrong — `resume_agent()` in `kill_switch.rs`
can resume individual agents, and the in-memory `KillSwitch` state is
separate from the file. Deleting the file only prevents the kill state
from being restored on next startup.

**Impact**: Users may think `ghost safety clear` immediately resumes all
agents. It doesn't — it only clears the persistence file. The in-memory
kill state remains until gateway restart.

**Fix**: Update §15.4 and the `ghost safety clear` help text to say:
"Clears the persisted kill state file. The gateway must be restarted for
agents to resume. This does not affect the in-memory kill state of a
running gateway."

### F.15 MEDIUM: `ghost-heartbeat` Not Represented in CLI

**Actual code**: `crates/ghost-heartbeat/` provides `HeartbeatEngine` and
`CronEngine` (Req 34) with configurable intervals, convergence-aware
frequency, and per-job cost tracking.

**Design gap**: No CLI commands for heartbeat/cron management. No way to:
- View heartbeat status or frequency
- List cron jobs
- Trigger a manual heartbeat
- View cron job history/costs

**Impact**: Low for Phase 1-2, but becomes relevant when operators need
to debug heartbeat-related issues.

**Fix**: Add to Phase 4 backlog:
```
ghost heartbeat
  status                Show heartbeat engine state and frequency
  trigger               Force an immediate heartbeat cycle

ghost cron
  list                  List registered cron jobs
  history [--limit <n>] Show recent cron executions
```

### F.16 MEDIUM: `ghost-proxy` Not Represented in CLI

**Actual code**: `crates/ghost-proxy/` exists in the workspace but is not
mentioned anywhere in the CLI design.

**Impact**: Unknown until the proxy crate's purpose is clarified. If it's
an HTTP proxy for agent egress, it may need CLI commands for configuration
and status.

**Fix**: Audit `ghost-proxy` and determine if CLI commands are needed.
Add to Phase 4 backlog if so.

### F.17 LOW: Design References `serde_yaml` but Workspace Has `serde_yaml = "0.9"`

`serde_yaml` 0.9 is the last version before the crate was deprecated in
favor of alternatives. The crate still works but is unmaintained. For the
CLI's `--output yaml` feature, this is fine — the output is simple
key-value structures. But worth noting for future dependency audits.

**Fix**: No action needed now. If `serde_yaml` causes issues later,
switch to `serde_yml` or drop YAML output support (JSON covers the
machine-readable use case).

### F.18 LOW: `OutputFormat` Enum Conflicts with Potential `clap::ValueEnum` Derivation

**Design says** (§4.3):
```rust
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}
```

After E.4 adds `JsonLines`, the enum becomes:
```rust
pub enum OutputFormat {
    Table,
    Json,
    JsonLines,
    Yaml,
}
```

`clap::ValueEnum` will generate the CLI value `json-lines` (kebab-case
by default). This is correct, but the design should explicitly note that
users type `--output json-lines` (not `--output jsonl` or `--output ndjson`).
Consider adding `#[value(alias = "jsonl")]` for convenience.

**Fix**: Add alias annotation:
```rust
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    #[value(alias = "jsonl", alias = "ndjson")]
    JsonLines,
    Yaml,
}
```

### F.19 LOW: `expand_tilde()` Duplication

`commands.rs` has its own `expand_tilde()` + `dirs_home()` functions that
duplicate `bootstrap::shellexpand_tilde()`. The bootstrap version is more
robust (handles `USERPROFILE` on Windows). The `config.rs` also has its
own `dirs_path()` and `dirs_home()`.

**Impact**: Three separate tilde expansion implementations. The CLI should
use exactly one.

**Fix**: Phase 0 should consolidate to `bootstrap::shellexpand_tilde()`
everywhere. Delete the local copies in `commands.rs` and ensure `config.rs`
uses the same function.

### F.20 LOW: `GhostConfig::validate()` Is Private

**Design says** (§11.3):
> `ghost config validate` runs `GhostConfig::load_default()` + `validate()`

**Actual code** (`crates/ghost-gateway/src/config.rs`):
```rust
fn validate(&self) -> Result<(), ConfigError> {
```

The method is `fn validate` (private), not `pub fn validate`. It's called
internally by `load()` and `load_default()`, but cannot be called
separately from the CLI.

**Impact**: `ghost config validate` cannot call `validate()` directly.
It would need to call `load_default()` which already validates internally
and returns `Err` on validation failure. This means the CLI can't
distinguish "file not found" from "validation error" from "parse error"
without matching on `ConfigError` variants.

**Fix**: Make `validate()` public: `pub fn validate(&self)`. This is a
one-word change in `config.rs`. Add to Phase 0 prerequisites.

### F.21 Summary of All Findings

| # | Severity | Section | Finding |
|---|---|---|---|
| F.1 | CRITICAL | §5.2 | `SecretProvider` methods are `get_secret`/`set_secret`/`delete_secret`, not `get`/`set`/`delete`. Returns `SecretString`, not `String`. |
| F.2 | CRITICAL | §15.9 | `CorpPolicy::load()` does not exist. No file parser in `ghost-policy`. |
| F.3 | CRITICAL | §15.7 | `SoulManager::create_template()` does not exist. Can only load, not create. |
| F.4 | CRITICAL | §15.7 | `DriftDetector::detect()` does not exist. Type is `IdentityDriftDetector`. Requires pre-computed embeddings. |
| F.5 | HIGH | Appendix C | No `GET /api/sessions/:id/events` endpoint. `session inspect` has no HTTP backend. |
| F.6 | HIGH | Appendix C | No `GET /api/agents/:id` endpoint. Must filter full list client-side. |
| F.7 | HIGH | §4.1 | `pause/resume/quarantine` are under `/api/safety/`, not `/api/agents/`. CLI groups them under `agent` but API has them under `safety`. |
| F.8 | HIGH | §13.1 | `commands.rs` calls `std::process::exit(1)` directly. Needs full rewrite to return `Result<(), CliError>`. |
| F.9 | HIGH | §4.2 | `status.rs` hardcodes `127.0.0.1:18789`. Ignores `--gateway-url` and config. |
| F.10 | MEDIUM | §15.3 | `run_migrations()` returns `()`, not migration count. CLI must query before/after. |
| F.11 | MEDIUM | §15.3 | No public `current_version()` function. Must inline query or add one. |
| F.12 | MEDIUM | §3.2 | `load_stored_token()` referenced but undefined. Detection sequence has chicken-and-egg with `SecretProvider`. |
| F.13 | MEDIUM | §11.2 | Env var overlay list is incomplete. Missing `GHOST_CORS_ORIGINS`, `GHOST_BACKUP_KEY`, vault token, per-provider `api_key_env`. |
| F.14 | MEDIUM | §15.4 | `ghost safety clear` only deletes file. In-memory kill state persists until restart. Design implies immediate effect. |
| F.15 | MEDIUM | — | `ghost-heartbeat` (HeartbeatEngine, CronEngine) has no CLI representation. |
| F.16 | MEDIUM | — | `ghost-proxy` crate exists but is not mentioned in the design. |
| F.17 | LOW | §10 | `serde_yaml` 0.9 is deprecated/unmaintained. Works but worth noting. |
| F.18 | LOW | §6, E.4 | `JsonLines` variant needs `#[value(alias = "jsonl")]` for usability. |
| F.19 | LOW | §9 | Three separate `expand_tilde()` implementations. Consolidate to one. |
| F.20 | LOW | §11.3 | `GhostConfig::validate()` is private. Must be made `pub` for CLI use. |

### F.22 Pre-Implementation Blockers

These must be resolved before Phase 0 can begin:

1. **Make `GhostConfig::validate()` public** — 1 word change in `config.rs`
2. **Fix `SecretProvider` method names in design** — update all code blocks in §5.2
3. **Define detection sequence** — config → provider → token → backend (§3.2 fix)
4. **Acknowledge missing library APIs** — `CorpPolicy::load()`,
   `SoulManager::create_template()`, `IdentityDriftDetector` naming.
   These don't block Phase 0-1 but block Phase 3.

### F.23 Risk Register Additions

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R15 | `SecretProvider` returns `SecretString` (zeroized) — CLI must handle `expose_secret()` correctly | Token leaked to logs if `Debug`/`Display` is used on `SecretString` | Never log or print `SecretString` directly. Always use `expose_secret()` only at the point of HTTP header injection. |
| R16 | `ghost identity drift` requires LLM embedding API call | Drift command fails without configured LLM provider | Fall back to hash-based drift (blake3 comparison) when no LLM provider is available. Document that embedding-based drift requires a provider. |
| R17 | Three `expand_tilde()` implementations may diverge | Inconsistent path resolution across CLI commands | Consolidate in Phase 0. Add a clippy lint or test that greps for `fn expand_tilde` to prevent re-introduction. |
| R18 | `ghost safety clear` user expectation mismatch | Users expect immediate agent resumption, but only file is deleted | Print explicit warning: "Gateway restart required for agents to resume." Add `--restart` flag that also sends SIGTERM to the gateway process (Phase 2). |
