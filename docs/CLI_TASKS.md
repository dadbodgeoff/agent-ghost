# GHOST CLI — Implementation Tasks

> Derived from `docs/CLI_DESIGN.md` (§1–§16) and audit appendices (E, F).
> Each task is atomic, ordered by dependency, and tagged with its phase.
>
> **Legend**: ⬜ Not started · 🟡 In progress · ✅ Done · 🚫 Blocked
>
> **Cross-references**: `§3.2` = CLI_DESIGN.md section 3.2, `F.1` = Appendix F finding 1
>
> **Self-Audit (2026-03-01)**: Every task verified against actual source.
> Phantom APIs flagged. Method signatures confirmed. See Appendix F in CLI_DESIGN.md.

---

## Phase 0: Infrastructure (Do First)

> No new user-facing commands. Build the plumbing every command depends on.
> Ref: CLI_DESIGN.md §3, §5, §6, §7, §8, §9, §13.1

### 0.1 Pre-Requisite: Library Fixes

> One-line fixes in upstream crates that unblock CLI work.

- [ ] **T-0.1.1** Make `GhostConfig::validate()` public `F.20`
  - Change `fn validate` → `pub fn validate` in `config.rs`
  - Unblocks `ghost config validate` calling it directly
  - File: `ghost-gateway/src/config.rs`

- [ ] **T-0.1.2** Add `current_version()` to cortex-storage `F.11`
  - Add `pub fn current_version(conn: &Connection) -> CortexResult<u32>` to `migrations/mod.rs`
  - Body: `SELECT COALESCE(MAX(version), 0) FROM schema_version`
  - Re-export from `cortex-storage/src/lib.rs`
  - Unblocks `ghost db status`
  - File: `cortex-storage/src/migrations/mod.rs`, `cortex-storage/src/lib.rs`

### 0.2 Dependencies

- [ ] **T-0.2.1** Add `clap_complete` to workspace `§10.2`
  - Root `Cargo.toml` `[workspace.dependencies]`: `clap_complete = "4"`
  - `ghost-gateway/Cargo.toml` `[dependencies]`: `clap_complete = { workspace = true }`
  - Files: `Cargo.toml`, `ghost-gateway/Cargo.toml`

- [ ] **T-0.2.2** Add `tokio-tungstenite` to workspace `E.3`
  - Root `Cargo.toml`: `tokio-tungstenite = { version = "0.24", features = ["native-tls"] }`
  - `ghost-gateway/Cargo.toml`: `tokio-tungstenite = { workspace = true }`
  - ⚠️ Verify compatibility with axum 0.7's hyper/tokio versions: `cargo check -p ghost-gateway`
  - Files: `Cargo.toml`, `ghost-gateway/Cargo.toml`

- [ ] **T-0.2.3** Add `assert_cmd` + `predicates` to dev-dependencies `E.7`
  - `ghost-gateway/Cargo.toml` `[dev-dependencies]`: `assert_cmd = "2"`, `predicates = "3"`
  - File: `ghost-gateway/Cargo.toml`

### 0.3 Error Handling

- [ ] **T-0.3.1** Create `CliError` type `§7.1`
  - Create `ghost-gateway/src/cli/error.rs`
  - Variants: `Config`, `Database`, `Http`, `AuthRequired`, `Auth`, `GatewayRequired`, `NoBackend`, `NotFound`, `Conflict`, `Cancelled`, `Internal`, `Usage`
  - `thiserror` derive with `#[error("...")]` on each variant
  - `exit_code()` method mapping to sysexits.h codes (78, 76, 69, 77, 70, 64, 1)
  - File: `ghost-gateway/src/cli/error.rs` (new)

### 0.4 Output Formatting

- [ ] **T-0.4.1** Create `OutputFormat` enum + `TableDisplay` trait `§6, E.4, F.18`
  - Create `ghost-gateway/src/cli/output.rs`
  - `OutputFormat` enum: `Table`, `Json`, `JsonLines`, `Yaml` with `clap::ValueEnum` derive
  - Add `#[value(alias = "jsonl", alias = "ndjson")]` on `JsonLines`
  - `TableDisplay` trait with `fn print_table(&self)`
  - `pub fn print_output<T: Serialize + TableDisplay>(value: &T, format: OutputFormat)`
  - JSON → `serde_json::to_string_pretty`, YAML → `serde_yaml::to_string`, JsonLines → `serde_json::to_string` (one line)
  - File: `ghost-gateway/src/cli/output.rs` (new)

- [ ] **T-0.4.2** Create `ColorChoice` enum + color helpers `E.5`
  - Add to `output.rs`: `ColorChoice` enum (`Auto`, `Always`, `Never`) with `clap::ValueEnum`
  - Helper: `fn should_colorize(choice: ColorChoice) -> bool` — checks `NO_COLOR`, `FORCE_COLOR`, `stdout.is_terminal()`
  - ANSI helpers: `fn red(s: &str)`, `fn yellow(s: &str)`, `fn green(s: &str)` — raw escape codes, no external crate
  - Disable color when `OutputFormat` is `Json`/`Yaml`/`JsonLines`
  - File: `ghost-gateway/src/cli/output.rs`

### 0.5 Confirmation & Dry-Run

- [ ] **T-0.5.1** Create `confirm()` function `§8`
  - Create `ghost-gateway/src/cli/confirm.rs`
  - `pub fn confirm(prompt: &str, yes_flag: bool) -> bool` — returns true if `--yes` or user types `y`/`yes`
  - Prompt to stderr, read from stdin
  - File: `ghost-gateway/src/cli/confirm.rs` (new)

### 0.6 Authentication

- [ ] **T-0.6.1** Create token storage/loading module `§5.2, F.1`
  - Create `ghost-gateway/src/cli/auth.rs`
  - `const CLI_TOKEN_KEY: &str = "ghost_cli_token";`
  - `pub fn store_token(provider: &dyn SecretProvider, token: &str) -> Result<(), CliError>` — calls `provider.set_secret()`
  - `pub fn load_token(provider: &dyn SecretProvider) -> Option<String>` — calls `provider.get_secret()`, then `.expose_secret().to_string()`
  - `pub fn clear_token(provider: &dyn SecretProvider) -> Result<(), CliError>` — calls `provider.delete_secret()`
  - `pub fn resolve_token(provider: &dyn SecretProvider) -> Option<String>` — checks `GHOST_TOKEN` env var first, then stored token
  - ⚠️ Method names are `get_secret`/`set_secret`/`delete_secret`, NOT `get`/`set`/`delete` (F.1)
  - ⚠️ `get_secret` returns `SecretString` — must call `.expose_secret()` to get `&str`
  - File: `ghost-gateway/src/cli/auth.rs` (new)

### 0.7 HTTP Client

- [ ] **T-0.7.1** Create `GhostHttpClient` `§5.4, E.12, E.14`
  - Create `ghost-gateway/src/cli/http_client.rs`
  - Struct: `GhostHttpClient { client: reqwest::Client, base_url: String, token: Option<String> }`
  - Single `reqwest::Client` instance, reused across all requests (E.14)
  - Methods: `get(path)`, `post(path, body)`, `delete(path)` — all inject `Authorization: Bearer` header if token present
  - Retry logic: exponential backoff for 429/502/503/504, max 3 retries, respect `Retry-After` header (E.12)
  - Map HTTP status codes to `CliError` variants (401→`AuthRequired`, 404→`NotFound`, 409→`Conflict`, 429→`Http("rate limited")`, 500→`Internal`)
  - Include `X-Request-ID` from response in error messages
  - File: `ghost-gateway/src/cli/http_client.rs` (new)

### 0.8 Backend Abstraction

- [ ] **T-0.8.1** Create `CliBackend` enum `§3.2, F.12`
  - Create `ghost-gateway/src/cli/backend.rs`
  - `CliBackend::Http { client: GhostHttpClient }` — owns the HTTP client (E.14)
  - `CliBackend::Direct { config: GhostConfig, db: Arc<Mutex<Connection>> }`
  - `BackendRequirement` enum: `HttpOnly`, `PreferHttp`, `DirectOnly`
  - `pub async fn detect(config: &GhostConfig, token: Option<String>) -> Result<Self, CliError>` — accepts token as param, not loaded internally (F.12)
  - Detection: HTTP health probe (2s timeout) → Direct DB fallback → `Err(NoBackend)`
  - `pub fn require(&self, req: BackendRequirement) -> Result<(), CliError>`
  - DB open uses `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` (matches existing pattern)
  - Dependency: T-0.6.1, T-0.7.1
  - File: `ghost-gateway/src/cli/backend.rs` (new)

### 0.9 Refactor main.rs

- [ ] **T-0.9.1** Extract `GlobalOpts` struct `E.1`
  - Create `GlobalOpts` struct with `#[derive(Debug, clap::Args)]`
  - Fields: `config`, `output` (OutputFormat), `gateway_url`, `verbose`, `quiet`, `color` (ColorChoice)
  - All fields `#[arg(global = true)]`
  - Use `#[command(flatten)]` on `Cli` struct
  - File: `ghost-gateway/src/main.rs`

- [ ] **T-0.9.2** Switch to `current_thread` tokio runtime `E.2`
  - Change `#[tokio::main]` → `#[tokio::main(flavor = "current_thread")]`
  - `ghost serve` builds its own multi-thread runtime internally via `tokio::runtime::Builder::new_multi_thread()`
  - File: `ghost-gateway/src/main.rs`

- [ ] **T-0.9.3** Refactor dispatch to `Result<(), CliError>` pattern `§7.2`
  - Wrap all command dispatch in `run_command()` → `Result<(), CliError>`
  - `main()` handles `Ok(())` → exit 0, `Err(e)` → `eprintln!` to stderr + `exit(e.exit_code())`
  - Errors always to stderr, data always to stdout
  - Dependency: T-0.3.1
  - File: `ghost-gateway/src/main.rs`

- [ ] **T-0.9.4** Add new `Commands` enum variants `§4.3`
  - Add all subcommand groups: `Init`, `Login`, `Logout`, `Doctor`, `Completions`, `Logs`, `Agent(AgentCommands)`, `Safety(SafetyCommands)`, `Config(ConfigCommands)`, `Db(DbCommands)`, `Audit(AuditCommands)`, `Convergence(ConvergenceCommands)`, `Session(SessionCommands)`, `Identity(IdentityCommands)`, `Secret(SecretCommands)`, `Policy(PolicyCommands)`, `Mesh(MeshCommands)`, `Skill(SkillCommands)`
  - Preserve existing `Serve`, `Chat`, `Backup`, `Export`, `Migrate` for backward compat
  - Wire dispatch to `cli::{module}::run()` for each group
  - Dependency: T-0.9.1, T-0.9.3
  - File: `ghost-gateway/src/main.rs`

### 0.10 Refactor Existing Commands

- [ ] **T-0.10.1** Refactor `commands.rs` to return `Result<(), CliError>` `F.8`
  - Change `run_backup()`, `run_export()`, `run_migrate()` return types
  - Replace all `std::process::exit(1)` with `Err(CliError::...)`
  - Replace `eprintln!` with error propagation
  - Replace local `expand_tilde()` with `crate::bootstrap::shellexpand_tilde()` (F.19)
  - Delete local `expand_tilde()` and `dirs_home()` functions
  - File: `ghost-gateway/src/cli/commands.rs`

- [ ] **T-0.10.2** Refactor `status.rs` to use `CliBackend` `F.9`
  - Change `show_status(config_path)` → `show_status(backend: &CliBackend, output: OutputFormat)`
  - Remove hardcoded `http://127.0.0.1:18789`
  - Use `CliBackend` for HTTP calls or direct health check
  - Add `StatusResponse` struct with `Serialize` + `TableDisplay`
  - File: `ghost-gateway/src/cli/status.rs`

- [ ] **T-0.10.3** Wrap `chat.rs` error handling `§15.10`
  - Wrap `run_interactive_chat()` to return `Result<(), CliError>`
  - No other changes — chat works correctly as-is
  - File: `ghost-gateway/src/cli/chat.rs`

### 0.11 Module Registration

- [ ] **T-0.11.1** Update `cli/mod.rs` with all new modules `§9.4`
  - Add: `pub mod error`, `pub mod output`, `pub mod confirm`, `pub mod backend`, `pub mod http_client`, `pub mod auth`
  - Add stubs for Phase 1+ modules: `pub mod init`, `pub mod doctor`, `pub mod logs`, `pub mod agent`, `pub mod safety`, `pub mod config_cmd`, `pub mod db`, `pub mod audit_cmd`, `pub mod convergence`, `pub mod session`, `pub mod identity`, `pub mod secret`, `pub mod policy`, `pub mod mesh`, `pub mod skill`, `pub mod completions`
  - Each stub: empty file with `//! ghost <group> — <description>.` doc comment + empty `Commands` enum + `pub async fn run() -> Result<(), CliError> { todo!() }`
  - File: `ghost-gateway/src/cli/mod.rs`, 17 new stub files

### 0.12 Integration Tests

- [ ] **T-0.12.1** Create CLI integration test file `§12, E.7`
  - Create `ghost-gateway/tests/cli_tests.rs`
  - Use `assert_cmd::Command::cargo_bin("ghost")`
  - Tests: `--help` works, `--version` works, unknown command fails with exit 2, `status` returns 0 or 69
  - Dependency: T-0.2.3, T-0.9.3
  - File: `ghost-gateway/tests/cli_tests.rs` (new)

### Phase 0 Exit Criteria

| Metric | Target |
|---|---|
| `cargo check -p ghost-gateway` | Compiles clean |
| `ghost --help` | Shows all subcommand groups |
| `ghost --version` | Prints version |
| `ghost status` | Uses CliBackend, respects `--gateway-url` |
| `ghost backup/export/migrate` | Return `CliError` instead of `process::exit()` |
| CLI integration tests | 4+ tests pass via `assert_cmd` |
| Error output | All errors to stderr, data to stdout |

---

## Phase 1: Essential Commands

> Day-to-day platform operation commands.
> Ref: CLI_DESIGN.md §4, §5, §11, §13.2

### 1.1 Init & First-Run

- [ ] **T-1.1.1** Implement `ghost init` `§4.1, Appendix A, E.9`
  - Create `ghost-gateway/src/cli/init.rs`
  - Create directory structure: `~/.ghost/{config,data,backups,agents,skills}/`
  - Generate platform Ed25519 keypair via `ghost_identity::keypair_manager::AgentKeypairManager::generate()`
  - Write default `ghost.yml` via `GhostConfig::default()` → `serde_yaml::to_string()`
  - Write SOUL.md template (hardcoded string — `SoulManager::create_template()` doesn't exist, F.3)
  - Write CORP_POLICY.md template (hardcoded string — `CorpPolicy::load()` doesn't exist, F.2)
  - Run DB migrations via `cortex_storage::run_all_migrations()`
  - Interactive LLM provider selection (stdin prompt, skip with `--defaults`)
  - ⚠️ Atomicity: use staging dir `~/.ghost/.init-tmp-{uuid}/`, rename on success, delete on failure (E.9)
  - ⚠️ Refuse to overwrite existing `~/.ghost/` unless `--force` passed
  - File: `ghost-gateway/src/cli/init.rs`

### 1.2 Authentication

- [ ] **T-1.2.1** Implement `ghost login` `§5.3`
  - Add to `ghost-gateway/src/cli/auth.rs`
  - Resolution: `--token` flag → `GHOST_TOKEN` env var → interactive stdin prompt
  - Validate token: `GET /api/health` with `Authorization: Bearer` header
  - On 200: store via `store_token()`, print success
  - On 401: print error, do not store
  - File: `ghost-gateway/src/cli/auth.rs`

- [ ] **T-1.2.2** Implement `ghost logout` `§5.3`
  - Call `clear_token()` via SecretProvider
  - Print confirmation
  - File: `ghost-gateway/src/cli/auth.rs`

### 1.3 Diagnostics

- [ ] **T-1.3.1** Implement `ghost doctor` `Appendix B`
  - Create `ghost-gateway/src/cli/doctor.rs`
  - Checks: `~/.ghost/` exists, `ghost.yml` valid, SOUL.md present, CORP_POLICY.md present, platform keypair present
  - LLM providers: check `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `OLLAMA_BASE_URL`
  - Database: exists, migrations current (compare `current_version()` vs `LATEST_VERSION`), WAL mode, hash chain spot-check
  - Gateway: HTTP health probe
  - Convergence monitor: HTTP health probe at configured address
  - Kill state: check `kill_state.json` existence
  - Disk space: `~/.ghost/` total size, backup count + size
  - Output: `✓`/`✗` per check, summary line
  - Dependency: T-0.1.2 (current_version)
  - File: `ghost-gateway/src/cli/doctor.rs`

### 1.4 Config Commands

- [ ] **T-1.4.1** Implement `ghost config show` `§11.2, F.13`
  - Create `ghost-gateway/src/cli/config_cmd.rs`
  - Load config via `GhostConfig::load_default()`
  - Overlay all env var overrides: `GHOST_TOKEN`, `GHOST_JWT_SECRET`, `GHOST_CORS_ORIGINS`, `GHOST_BACKUP_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `OLLAMA_BASE_URL`, vault token env
  - Redact all secret values (show `****` suffix)
  - `ResolvedConfig` struct with `Serialize` + `TableDisplay`
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/config_cmd.rs`

- [ ] **T-1.4.2** Implement `ghost config validate` `§11.3, F.20`
  - Call `GhostConfig::load_default()` then `config.validate()` (now public via T-0.1.1)
  - Report each check as `✓`/`✗` with description
  - Exit code 0 if valid, 78 (EX_CONFIG) if invalid
  - Backend: DirectOnly
  - Dependency: T-0.1.1
  - File: `ghost-gateway/src/cli/config_cmd.rs`

### 1.5 Agent Commands

- [ ] **T-1.5.1** Implement `ghost agent list` `§4.1, Appendix C`
  - Create `ghost-gateway/src/cli/agent.rs`
  - HTTP: `GET /api/agents` → parse `Vec<AgentInfo>`
  - Direct: query agents from config (no `agents` table in DB — agents are in-memory registry + config)
  - `AgentListResponse` with `Serialize` + `TableDisplay` (table: ID, Name, Status, Spend Cap)
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/agent.rs`

- [ ] **T-1.5.2** Implement `ghost agent create` `§4.1, Appendix C`
  - Args struct: `name`, `--model`, `--spending-cap` (default 5.0), `--capabilities` (comma-delimited), `--no-keypair`
  - HTTP: `POST /api/agents` with `CreateAgentRequest` body
  - Backend: HttpOnly (needs registry + WS broadcast)
  - File: `ghost-gateway/src/cli/agent.rs`

- [ ] **T-1.5.3** Implement `ghost agent inspect` `§4.1, F.6`
  - Accepts `<id|name>` positional arg
  - HTTP: `GET /api/agents` → filter by id or name client-side (no single-agent endpoint, F.6)
  - Direct: read from config + DB
  - Show: id, name, status, spending cap, capabilities, has_keypair, channel bindings
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/agent.rs`

- [ ] **T-1.5.4** Implement `ghost agent delete` `§4.1, §8.1`
  - Accepts `<id|name>` + `--yes` flag
  - Confirmation prompt: "Delete agent '{name}'? This is irreversible. [y/N]"
  - HTTP: `DELETE /api/agents/:id`
  - Handle 409 Conflict (quarantined agent)
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/agent.rs`

- [ ] **T-1.5.5** Implement `ghost agent pause/resume/quarantine` `§4.1, F.7`
  - HTTP: `POST /api/safety/pause/:agent_id`, `POST /api/safety/resume/:agent_id`, `POST /api/safety/quarantine/:agent_id`
  - ⚠️ Endpoints are under `/api/safety/`, not `/api/agents/` (F.7) — CLI groups under `agent` for UX
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/agent.rs`

### 1.6 Safety Commands

- [ ] **T-1.6.1** Implement `ghost safety status` `§4.1, Appendix C`
  - Create `ghost-gateway/src/cli/safety.rs`
  - HTTP: `GET /api/safety/status`
  - Direct: read `~/.ghost/data/kill_state.json` if exists
  - Show: platform level, per-agent states, activated_at, reason
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/safety.rs`

- [ ] **T-1.6.2** Implement `ghost safety kill-all` `§4.1, §8.1`
  - Args: `--reason <text>`, `--yes`, `--dry-run`
  - Confirmation prompt (RED): "KILL ALL AGENTS? This stops all agent execution. [y/N]"
  - HTTP: `POST /api/safety/kill-all` with `KillAllRequest` body
  - Dry-run: show what would happen without executing
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/safety.rs`

- [ ] **T-1.6.3** Implement `ghost safety clear` `§4.1, F.14`
  - Args: `--yes`
  - Confirmation prompt: "Clear kill state? Agents will be able to resume. [y/N]"
  - Direct: delete `~/.ghost/data/kill_state.json`
  - ⚠️ Print warning: "Gateway restart required for agents to resume. This only clears the persisted state." (F.14)
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/safety.rs`

### 1.7 Database Commands

- [ ] **T-1.7.1** Implement `ghost db migrate` `§4.1, F.10`
  - Create `ghost-gateway/src/cli/db.rs`
  - Query `current_version()` before, run `cortex_storage::run_all_migrations()`, query after
  - Report: "Applied N migrations (vX → vY)" or "Already up to date (vY)"
  - Backend: DirectOnly
  - Dependency: T-0.1.2
  - File: `ghost-gateway/src/cli/db.rs`

- [ ] **T-1.7.2** Implement `ghost db status` `§4.1, F.11`
  - Show: current version vs `LATEST_VERSION`, table counts, DB file size, WAL size, journal mode
  - Backend: DirectOnly
  - Dependency: T-0.1.2
  - File: `ghost-gateway/src/cli/db.rs`

### 1.8 Shell Completions

- [ ] **T-1.8.1** Implement `ghost completions` `§4.1`
  - Create `ghost-gateway/src/cli/completions.rs`
  - Accepts `<bash|zsh|fish|powershell|elvish>` positional arg (via `clap_complete::Shell`)
  - Generates completions to stdout via `clap_complete::generate()`
  - Backend: DirectOnly (no network, no DB)
  - Dependency: T-0.2.1
  - File: `ghost-gateway/src/cli/completions.rs`

### Phase 1 Exit Criteria

| Metric | Target |
|---|---|
| `ghost init` | Creates `~/.ghost/` with config, SOUL.md, keypair, DB |
| `ghost login/logout` | Token stored/cleared via SecretProvider |
| `ghost doctor` | Reports 10+ health checks |
| `ghost agent list/create/inspect/delete` | Full CRUD via HTTP |
| `ghost safety status/kill-all/clear` | Kill switch management works |
| `ghost db migrate/status` | Migration reporting works |
| `ghost completions bash` | Valid bash completions output |
| All commands | Respect `--output json`, `--quiet`, `--color` |

---

## Phase 2: Observability Commands

> Debugging, monitoring, and live event streaming.
> Ref: CLI_DESIGN.md §13.3, §15.5

### 2.1 Live Event Streaming

- [ ] **T-2.1.1** Implement `ghost logs` `§4.1, E.3, E.8`
  - Create `ghost-gateway/src/cli/logs.rs`
  - Connect to `ws://{gateway}/api/ws?token={token}` via `tokio-tungstenite`
  - Parse incoming `WsEvent` JSON (already `#[serde(tag = "type")]`)
  - Args: `--agent <id>` (filter), `--type <event_type>` (filter), `--json` (NDJSON output)
  - Table mode: formatted event lines with timestamp, type, agent, summary
  - JSON mode: one JSON object per line (NDJSON)
  - Signal handling: `tokio::select!` with `ctrl_c()`, send WS Close frame on Ctrl+C (E.8)
  - Backend: HttpOnly (WS)
  - Dependency: T-0.2.2
  - File: `ghost-gateway/src/cli/logs.rs`

### 2.2 Audit Commands

- [ ] **T-2.2.1** Implement `ghost audit query` `§4.1, Appendix C`
  - Create `ghost-gateway/src/cli/audit_cmd.rs`
  - Args: `--agent`, `--severity`, `--event-type`, `--since`, `--until`, `--search`, `--limit`
  - HTTP: `GET /api/audit` with query params mapped to `AuditFilter`
  - Direct: construct `AuditQueryEngine` with DB connection, call `query()`
  - `AuditQueryResponse` with `Serialize` + `TableDisplay`
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/audit_cmd.rs`

- [ ] **T-2.2.2** Implement `ghost audit export` `§4.1, Appendix C`
  - Args: `--format json|csv|jsonl`, `--output <path>`
  - HTTP: `GET /api/audit/export?format=...`
  - Direct: use `ghost_audit::export::AuditExporter::export()` with entries from query
  - Write to file or stdout
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/audit_cmd.rs`

- [ ] **T-2.2.3** Implement `ghost audit tail` `§4.1, E.8`
  - Connect to WS, filter to audit-type events only
  - Same signal handling as `ghost logs` (Ctrl+C → Close frame)
  - Backend: HttpOnly (WS)
  - Dependency: T-0.2.2
  - File: `ghost-gateway/src/cli/audit_cmd.rs`

### 2.3 Convergence Commands

- [ ] **T-2.3.1** Implement `ghost convergence scores` `§4.1, Appendix C`
  - Create `ghost-gateway/src/cli/convergence.rs`
  - HTTP: `GET /api/convergence/scores`
  - Direct: read `~/.ghost/data/convergence_state/*.json` files (same as health endpoint)
  - Show: agent_id, score, level, signal breakdown, updated_at
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/convergence.rs`

### 2.4 Session Commands

- [ ] **T-2.4.1** Implement `ghost session list` `§4.1, Appendix C`
  - Create `ghost-gateway/src/cli/session.rs`
  - Args: `--agent <id>`, `--limit <n>`
  - HTTP: `GET /api/sessions` with query params
  - Direct: query `itp_events` grouped by `session_id`
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/session.rs`

- [ ] **T-2.4.2** Implement `ghost session inspect` `§4.1, F.5`
  - Accepts `<session_id>` positional arg
  - ⚠️ No `GET /api/sessions/:id/events` endpoint exists (F.5) — must use Direct mode
  - Direct: query `itp_events WHERE session_id = ?` via `cortex_storage::queries::itp_event_queries`
  - Show: event timeline, gate states, agent, timestamps
  - Backend: DirectOnly (until endpoint is added)
  - File: `ghost-gateway/src/cli/session.rs`

### 2.5 Database Commands (Continued)

- [ ] **T-2.5.1** Implement `ghost db verify` `§4.1`
  - Walk `itp_events` hash chain: verify each `event_hash` matches `blake3(content_hash + previous_hash)`
  - Args: `--full` (walk entire chain vs spot-check 100 random entries)
  - Report: chain length, breaks found, verification time
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/db.rs`

- [ ] **T-2.5.2** Implement `ghost db compact` `§4.1, §8.3`
  - Args: `--yes`, `--dry-run`
  - Confirmation prompt: "Compact database? This may take a moment. [y/N]"
  - Execute: `PRAGMA wal_checkpoint(TRUNCATE); VACUUM;`
  - Dry-run: show current WAL size + estimated savings
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/db.rs`

### Phase 2 Exit Criteria

| Metric | Target |
|---|---|
| `ghost logs` | Streams WS events, Ctrl+C closes cleanly |
| `ghost audit query` | Filters work, JSON output valid |
| `ghost audit tail` | Live audit stream works |
| `ghost convergence scores` | Shows per-agent scores |
| `ghost session list/inspect` | Session browsing works |
| `ghost db verify` | Hash chain verification reports results |
| `ghost db compact` | WAL checkpoint + VACUUM completes |

---

## Phase 3: Identity, Secrets, Policy

> Security and identity management commands.
> Ref: CLI_DESIGN.md §13.4, §15.7, §15.8, §15.9

### 3.1 Library Prerequisites

> APIs that don't exist yet and must be added before CLI commands can use them.

- [ ] **T-3.1.1** Add `SoulManager::create_template()` to ghost-identity `F.3`
  - `pub fn create_template(path: &Path) -> Result<(), SoulError>` — writes default SOUL.md
  - Replaces hardcoded template in `init.rs` (T-1.1.1)
  - File: `ghost-identity/src/soul_manager.rs`

- [ ] **T-3.1.2** Add `CorpPolicy::load(path)` to ghost-policy `F.2`
  - `pub fn load(path: &Path) -> Result<Self, PolicyError>` — parse CORP_POLICY.md markdown deny-list
  - Unblocks `ghost policy show` and `ghost policy lint`
  - File: `ghost-policy/src/corp_policy.rs`

### 3.2 Identity Commands

- [ ] **T-3.2.1** Implement `ghost identity init` `§4.1, F.3`
  - Create `ghost-gateway/src/cli/identity.rs`
  - Create SOUL.md via `SoulManager::create_template()` (T-3.1.1) or inline template
  - Generate platform keypair via `AgentKeypairManager::generate()`
  - Show fingerprint of generated key
  - Backend: DirectOnly
  - Dependency: T-3.1.1 (or use inline template)
  - File: `ghost-gateway/src/cli/identity.rs`

- [ ] **T-3.2.2** Implement `ghost identity show` `§4.1`
  - Load SOUL.md via `SoulManager::load()`
  - Load public key via `AgentKeypairManager::load_verifying_key()`
  - Show: soul document summary (first 5 lines), key fingerprint (blake3 of public key bytes)
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/identity.rs`

- [ ] **T-3.2.3** Implement `ghost identity drift` `§4.1, F.4`
  - ⚠️ Type is `IdentityDriftDetector`, not `DriftDetector` (F.4)
  - ⚠️ Requires pre-computed embeddings — full embedding-based drift needs LLM provider
  - Scope down to hash-based drift: compare current SOUL.md blake3 hash against stored baseline
  - Load SOUL.md → compute `blake3::hash()` → compare against `SoulDocument.hash` from last `identity init`
  - Report: "No drift detected" or "SOUL.md has changed (hash mismatch)"
  - Future: add `--embedding` flag for LLM-based drift when provider is available (R16)
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/identity.rs`

- [ ] **T-3.2.4** Implement `ghost identity sign` `§4.1`
  - Accepts `<file>` positional arg
  - Load signing key from `AgentKeypairManager`
  - Sign file contents with Ed25519 via `ghost_signing::sign()`
  - Output signature (base64-encoded) to stdout
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/identity.rs`

### 3.3 Secret Commands

- [ ] **T-3.3.1** Implement `ghost secret set/list/delete/provider` `§4.1, F.1`
  - Create `ghost-gateway/src/cli/secret.rs`
  - `set <key>`: read value from stdin (pipe-friendly), call `provider.set_secret()`
  - `list`: call `provider.has_secret()` for known keys, display key names only (never values)
  - `delete <key>`: call `provider.delete_secret()`, confirmation prompt
  - `provider`: show active backend name (env/keychain/vault) from config
  - ⚠️ Use `set_secret`/`get_secret`/`delete_secret` method names (F.1)
  - ⚠️ EnvProvider is read-only — `set` and `delete` will return `SecretsError::StorageUnavailable`
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/secret.rs`

### 3.4 Policy Commands

- [ ] **T-3.4.1** Implement `ghost policy show` `§4.1, F.2`
  - Create `ghost-gateway/src/cli/policy.rs`
  - Load CORP_POLICY.md from `~/.ghost/config/CORP_POLICY.md`
  - If T-3.1.2 done: parse via `CorpPolicy::load()`, show structured deny-list
  - If not: read raw markdown, display as-is
  - Backend: DirectOnly
  - Dependency: T-3.1.2 (optional — degrades gracefully)
  - File: `ghost-gateway/src/cli/policy.rs`

- [ ] **T-3.4.2** Implement `ghost policy check` `§4.1`
  - Args: `<tool_name>`, `--agent <id>`
  - Construct `PolicyEngine` with loaded `CorpPolicy`
  - Build synthetic `ToolCall` + `PolicyContext`
  - Call `engine.evaluate()` → show `Permit`/`Deny`/`Escalate` with feedback
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/policy.rs`

- [ ] **T-3.4.3** Implement `ghost policy lint` `§4.1, F.2`
  - Validate CORP_POLICY.md structure: check markdown headings, deny-list format
  - Report: `✓`/`✗` per check
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/policy.rs`

### Phase 3 Exit Criteria

| Metric | Target |
|---|---|
| `ghost identity init/show` | Keypair + SOUL.md management works |
| `ghost identity drift` | Hash-based drift detection reports results |
| `ghost secret set/list/delete` | Keychain/vault/env backends work |
| `ghost policy show/check/lint` | Policy inspection and dry-run evaluation works |

---

## Phase 4: Mesh, Skills, Advanced

> Multi-agent networking, extensibility, and advanced features.
> Ref: CLI_DESIGN.md §13.5, §15.7

### 4.1 Mesh Commands

- [ ] **T-4.1.1** Implement `ghost mesh peers` `§4.1`
  - Create `ghost-gateway/src/cli/mesh.rs`
  - HTTP: `GET /api/mesh/trust-graph` (if endpoint exists from ADE Phase 3) or list from config
  - Show: peer_id, name, trust score, last seen
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/mesh.rs`

- [ ] **T-4.1.2** Implement `ghost mesh trust` `§4.1`
  - HTTP: `GET /api/mesh/trust-graph`
  - Show: EigenTrust scores between agents as a matrix or edge list
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/mesh.rs`

- [ ] **T-4.1.3** Implement `ghost mesh discover` `§4.1`
  - Accepts `<url>` positional arg
  - Fetch `/.well-known/agent.json` from remote URL
  - Register as known peer
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/mesh.rs`

- [ ] **T-4.1.4** Implement `ghost mesh ping` `§4.1`
  - Accepts `<peer_id>` positional arg
  - Send health probe to peer, report latency
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/mesh.rs`

### 4.2 Skill Commands

- [ ] **T-4.2.1** Implement `ghost skill list` `§4.1`
  - Create `ghost-gateway/src/cli/skill.rs`
  - HTTP: `GET /api/skills` (if endpoint exists from ADE Phase 4)
  - Direct: read from `~/.ghost/skills/` directory
  - Show: name, version, capabilities, status
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/skill.rs`

- [ ] **T-4.2.2** Implement `ghost skill install` `§4.1`
  - Accepts `<path|url>` positional arg
  - Install WASM skill to `~/.ghost/skills/`
  - Validate WASM module via `ghost_skills::sandbox`
  - Backend: HttpOnly (if via API) or DirectOnly (if local path)
  - File: `ghost-gateway/src/cli/skill.rs`

- [ ] **T-4.2.3** Implement `ghost skill inspect` `§4.1`
  - Accepts `<name>` positional arg
  - Show: metadata, permissions, capability requirements
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/skill.rs`

### 4.3 Session Replay

- [ ] **T-4.3.1** Implement `ghost session replay` `§4.1`
  - Accepts `<session_id>` positional arg
  - Text-based session replay: print events sequentially with timestamps
  - Reconstruct conversation from ITP events
  - Show gate state transitions inline
  - Backend: PreferHttp (or DirectOnly per F.5)
  - File: `ghost-gateway/src/cli/session.rs`

### 4.4 Convergence History

- [ ] **T-4.4.1** Implement `ghost convergence history` `§4.1`
  - Accepts `<agent_id>` positional arg
  - Args: `--since <time>`
  - Query convergence score history from DB
  - Show: timestamp, score, level, delta from previous
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/convergence.rs`

### 4.5 Heartbeat & Cron (Backlog) `F.15`

- [ ] **T-4.5.1** Implement `ghost heartbeat status` `F.15`
  - Show: engine state, current frequency, convergence-aware tier, last beat timestamp
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/heartbeat.rs` (new)

- [ ] **T-4.5.2** Implement `ghost cron list/history` `F.15`
  - `list`: show registered cron jobs with schedule, last run, next run
  - `history --limit <n>`: show recent executions with cost
  - Backend: HttpOnly
  - File: `ghost-gateway/src/cli/cron.rs` (new)

### Phase 4 Exit Criteria

| Metric | Target |
|---|---|
| `ghost mesh peers/trust/discover/ping` | A2A peer management works |
| `ghost skill list/install/inspect` | WASM skill lifecycle works |
| `ghost session replay` | Text-based replay renders conversation |
| `ghost convergence history` | Score history with deltas |

---

## Cross-Cutting Concerns (All Phases)

### JSON Output Stability `E.10`

- [ ] **T-X.1** Document JSON output stability contract
  - Fields are append-only within a major version
  - New fields may be added; existing fields never removed or renamed
  - Add `--format-version` flag (default: latest) for future breaking changes
  - Document in `docs/API_CONTRACT.md` (or create `docs/CLI_CONTRACT.md`)

### XDG & Home Directory `E.11`

- [ ] **T-X.2** Add `GHOST_HOME` env var override
  - Check `GHOST_HOME` before defaulting to `~/.ghost/`
  - Apply in `shellexpand_tilde()` and all path resolution
  - Document XDG non-compliance decision: `~/.ghost/` is canonical, `GHOST_XDG=1` deferred
  - File: `ghost-gateway/src/bootstrap.rs`

### Path Resolution Consolidation `F.19`

- [ ] **T-X.3** Consolidate `expand_tilde()` implementations
  - Delete `commands.rs::expand_tilde()` and `commands.rs::dirs_home()`
  - Replace all calls with `crate::bootstrap::shellexpand_tilde()`
  - Verify `config.rs::dirs_path()` and `dirs_home()` use same logic
  - Add test: grep for `fn expand_tilde` to prevent re-introduction
  - Files: `ghost-gateway/src/cli/commands.rs`, `ghost-gateway/src/config.rs`

### Signal Handling `E.8`

- [ ] **T-X.4** Implement Ctrl+C handling for all streaming commands
  - `ghost logs`, `ghost audit tail`: `tokio::select!` with `ctrl_c()` + WS Close frame
  - `ghost chat`: already handles `/quit` — add Ctrl+C for consistency
  - Pattern: shared helper in `cli/` that wraps WS read loop with signal handling

### Testing `§12, E.7`

- [ ] **T-X.5** Add CLI integration tests for each phase
  - Phase 0: `--help`, `--version`, unknown command, `status` exit codes
  - Phase 1: `config validate`, `agent list --output json` (valid JSON), `completions bash` (non-empty)
  - Phase 2: `audit query --output json` (valid JSON), `db status` (reports version)
  - Phase 3: `identity show` (reports key or error), `secret provider` (reports backend)
  - Use `assert_cmd` + `predicates` (T-0.2.3)
  - File: `ghost-gateway/tests/cli_tests.rs`

### Documentation

- [ ] **T-X.6** Add CLI usage to README.md
  - Quick start: `ghost init`, `ghost serve`, `ghost chat`
  - Command reference: link to `ghost --help`
  - Shell completions setup instructions

- [ ] **T-X.7** Add man page generation (deferred)
  - Use `clap_mangen` to generate man pages from clap definitions
  - Low priority — `--help` covers most use cases

---

## Dependency Summary

### New Workspace Dependencies

| Crate | Version | Task | Required? |
|---|---|---|---|
| `clap_complete` | `4` | T-0.2.1 | Yes |
| `tokio-tungstenite` | `0.24` | T-0.2.2 | Yes (Phase 2) |

### New Dev Dependencies (ghost-gateway)

| Crate | Version | Task | Required? |
|---|---|---|---|
| `assert_cmd` | `2` | T-0.2.3 | Yes |
| `predicates` | `3` | T-0.2.3 | Yes |

### Deferred Dependencies

| Crate | Version | Purpose | When |
|---|---|---|---|
| `indicatif` | `0.17` | Progress bars | If hand-rolled becomes unmaintainable |
| `comfy-table` | `7` | Table formatting | If hand-rolled becomes unmaintainable |
| `dialoguer` | `0.11` | Interactive prompts | If raw stdin becomes unmaintainable |
| `clap_mangen` | `0.2` | Man page generation | T-X.7 |

---

## New Files Summary

### Phase 0 (17 new files)

| File | Task | Purpose |
|---|---|---|
| `cli/error.rs` | T-0.3.1 | `CliError` type |
| `cli/output.rs` | T-0.4.1 | `OutputFormat`, `TableDisplay`, color helpers |
| `cli/confirm.rs` | T-0.5.1 | Confirmation prompts |
| `cli/auth.rs` | T-0.6.1 | Token storage/loading |
| `cli/http_client.rs` | T-0.7.1 | `GhostHttpClient` with retry |
| `cli/backend.rs` | T-0.8.1 | `CliBackend` enum |
| `cli/init.rs` | stub | Empty stub |
| `cli/doctor.rs` | stub | Empty stub |
| `cli/logs.rs` | stub | Empty stub |
| `cli/agent.rs` | stub | Empty stub |
| `cli/safety.rs` | stub | Empty stub |
| `cli/config_cmd.rs` | stub | Empty stub |
| `cli/db.rs` | stub | Empty stub |
| `cli/audit_cmd.rs` | stub | Empty stub |
| `cli/convergence.rs` | stub | Empty stub |
| `cli/session.rs` | stub | Empty stub |
| `cli/completions.rs` | stub | Empty stub |

### Phase 1 (7 new files, fill stubs)

| File | Task | Purpose |
|---|---|---|
| `cli/init.rs` | T-1.1.1 | `ghost init` |
| `cli/doctor.rs` | T-1.3.1 | `ghost doctor` |
| `cli/config_cmd.rs` | T-1.4.1 | `ghost config show/validate` |
| `cli/agent.rs` | T-1.5.1 | `ghost agent list/create/inspect/delete/pause/resume/quarantine` |
| `cli/safety.rs` | T-1.6.1 | `ghost safety status/kill-all/clear` |
| `cli/db.rs` | T-1.7.1 | `ghost db migrate/status` |
| `cli/completions.rs` | T-1.8.1 | `ghost completions` |

### Phase 2 (fill stubs)

| File | Task | Purpose |
|---|---|---|
| `cli/logs.rs` | T-2.1.1 | `ghost logs` (WS streaming) |
| `cli/audit_cmd.rs` | T-2.2.1 | `ghost audit query/export/tail` |
| `cli/convergence.rs` | T-2.3.1 | `ghost convergence scores` |
| `cli/session.rs` | T-2.4.1 | `ghost session list/inspect` |

### Phase 3 (5 new files)

| File | Task | Purpose |
|---|---|---|
| `cli/identity.rs` | T-3.2.1 | `ghost identity init/show/drift/sign` |
| `cli/secret.rs` | T-3.3.1 | `ghost secret set/list/delete/provider` |
| `cli/policy.rs` | T-3.4.1 | `ghost policy show/check/lint` |

### Phase 4 (4 new files)

| File | Task | Purpose |
|---|---|---|
| `cli/mesh.rs` | T-4.1.1 | `ghost mesh peers/trust/discover/ping` |
| `cli/skill.rs` | T-4.2.1 | `ghost skill list/install/inspect` |
| `cli/heartbeat.rs` | T-4.5.1 | `ghost heartbeat status` |
| `cli/cron.rs` | T-4.5.2 | `ghost cron list/history` |

---

## Risk Register (from CLI_DESIGN.md)

| # | Risk | Phase | Mitigation |
|---|---|---|---|
| R1 | Auth middleware lands before CLI auth | 0 | Build `ghost login` + token injection in Phase 0 |
| R3 | SQLite concurrent access CLI + gateway | 0 | `PRAGMA busy_timeout=5000` in both paths |
| R7 | Direct backend diverges from HTTP backend | 1 | Direct is read-only for data. All mutations via HTTP. |
| R8 | `ghost init` partial state on failure | 1 | Staging dir + atomic rename (E.9) |
| R9 | Token stored in plaintext without keychain | 1 | `ghost-secrets` falls back to env vars. Never write plaintext. |
| R10 | `ghost safety clear` while gateway running | 1 | Print restart warning (F.14) |
| R11 | `tokio-tungstenite` version conflict | 2 | `cargo check` before merging (E.3) |
| R12 | Streaming commands hold WS indefinitely | 2 | Idle timeout (30min). Document interactive use. |
| R13 | `ghost safety clear` race condition | 1 | Atomic file ops. Document gateway-stopped requirement. |
| R15 | `SecretString` leaked to logs | 0 | Never log/print `SecretString`. Use `expose_secret()` only at injection point. |
| R16 | `ghost identity drift` needs LLM embeddings | 3 | Fall back to hash-based drift. Document embedding requirement. |
| R17 | Three `expand_tilde()` implementations | 0 | Consolidate in Phase 0 (T-X.3) |
| R18 | `ghost safety clear` expectation mismatch | 1 | Print explicit restart warning (T-1.6.3) |

---

## Task Count Summary

| Phase | Tasks | Effort |
|---|---|---|
| Phase 0: Infrastructure | 20 | ~40h |
| Phase 1: Essential | 17 | ~50h |
| Phase 2: Observability | 10 | ~35h |
| Phase 3: Identity/Secrets/Policy | 11 | ~25h |
| Phase 4: Mesh/Skills/Advanced | 10 | ~30h |
| Cross-Cutting | 7 | ~15h |
| **Total** | **75** | **~195h** |
