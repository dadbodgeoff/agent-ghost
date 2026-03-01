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
  - Add all subcommand groups: `Init`, `Login`, `Logout`, `Doctor`, `Completions`, `Logs`, `Agent(AgentCommands)`, `Safety(SafetyCommands)`, `Config(ConfigCommands)`, `Db(DbCommands)`, `Audit(AuditCommands)`, `Convergence(ConvergenceCommands)`, `Session(SessionCommands)`, `Identity(IdentityCommands)`, `Secret(SecretCommands)`, `Policy(PolicyCommands)`, `Mesh(MeshCommands)`, `Skill(SkillCommands)`, `Channel(ChannelCommands)`
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
  - Add stubs for Phase 1+ modules: `pub mod init`, `pub mod doctor`, `pub mod logs`, `pub mod agent`, `pub mod safety`, `pub mod config_cmd`, `pub mod db`, `pub mod audit_cmd`, `pub mod convergence`, `pub mod session`, `pub mod identity`, `pub mod secret`, `pub mod policy`, `pub mod mesh`, `pub mod skill`, `pub mod completions`, `pub mod channel`
  - Each stub: empty file with `//! ghost <group> — <description>.` doc comment + empty `Commands` enum + `pub async fn run() -> Result<(), CliError> { todo!() }`
  - File: `ghost-gateway/src/cli/mod.rs`, 18 new stub files

### 0.12 Integration Tests

- [ ] **T-0.12.1** Create CLI integration test file `§12, E.7`
  - Create `ghost-gateway/tests/cli_tests.rs`
  - Use `assert_cmd::Command::cargo_bin("ghost")`
  - Tests: `--help` works, `--version` works, unknown command fails with exit 2, `status` returns 0 or 69
  - Dependency: T-0.2.3, T-0.9.3
  - File: `ghost-gateway/tests/cli_tests.rs` (new)

### 0.13 Path Foundation & Early Completions

> Promoted from Cross-Cutting (T-X.2) and Phase 1 (T-1.8.1). Both must land before
> Phase 1 begins: init builds the entire `~/.ghost/` tree, and completions stub must
> compile before the stubs in T-0.11.1 are registered.

- [ ] **T-0.13.1** Add `GHOST_HOME` env var to path resolution `E.11`
  - Check `GHOST_HOME` before defaulting to `~/.ghost/` everywhere paths are constructed
  - Apply inside `shellexpand_tilde()` in `bootstrap.rs` and all callers that hardcode `~/.ghost/`
  - Precedence: `GHOST_HOME` → `~/.ghost/`
  - Document: XDG non-compliance is intentional; `GHOST_XDG=1` deferred
  - ⚠️ Must land before T-1.1.1 (`ghost init`) to avoid baking in `~/.ghost/` unconditionally
  - File: `ghost-gateway/src/bootstrap.rs`

- [ ] **T-0.13.2** Implement `ghost completions` `§4.1`
  - Moved from T-1.8.1 — no backend requirement, only depends on T-0.2.1 which is already Phase 0
  - Create `ghost-gateway/src/cli/completions.rs`
  - Accepts `<bash|zsh|fish|powershell|elvish>` positional arg (via `clap_complete::Shell`)
  - Generates completions to stdout via `clap_complete::generate()`
  - Backend: DirectOnly (no network, no DB)
  - Dependency: T-0.2.1
  - File: `ghost-gateway/src/cli/completions.rs`

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
| `ghost completions bash` | Valid bash completions output |
| `GHOST_HOME` | All path construction respects env override |

---

## Phase 1: Essential Commands

> Day-to-day platform operation commands.
> Ref: CLI_DESIGN.md §4, §5, §11, §13.2

### 1.1 Init & First-Run

- [ ] **T-1.1.1** Implement `ghost init` `§4.1, Appendix A, E.9`
  - Dependency: T-0.13.1 — all paths must go through `GHOST_HOME`-aware resolution, not hardcoded `~/.ghost/`
  - Create `ghost-gateway/src/cli/init.rs`
  - Create directory structure: `~/.ghost/{config,data,backups,agents,skills}/` (via resolved base path)
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
    - ⚠️ Env var checks are existence-only — a present but expired/invalid key still passes. Output must read "Key found (not validated)", never "✓ API key valid". Add `--probe` flag (deferred to Phase 2) that makes a minimal test API call to each configured provider.
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
  - Additionally, for each channel in `config.channels`, overlay channel credential env vars by resolving the `{key}_key` option values:
    - `GHOST_TELEGRAM_BOT_TOKEN` (if telegram channel configured)
    - `GHOST_WHATSAPP_ACCESS_TOKEN` (if whatsapp cloud-api configured)
    - `GHOST_SLACK_BOT_TOKEN`, `GHOST_SLACK_APP_TOKEN` (if slack configured)
    - `GHOST_DISCORD_BOT_TOKEN` (if discord configured)
    - Only show env vars for channels actually present in `config.channels` — do not show all possible channel vars unconditionally
  - Redact all secret values (show `****` suffix); `phone_number_id` for WhatsApp is not secret — show literal value
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

- [ ] **T-1.5.6** Implement `ghost agent update` (backend + CLI)
  - **Step A — Backend prerequisite**: `PATCH /api/agents/:id` does not exist in `bootstrap.rs` (only GET list, POST create, DELETE exist). Add:
    - Handler `update_agent()` in `ghost-gateway/src/api/agents.rs`
    - Request body: `UpdateAgentRequest { model: Option<String>, spending_cap: Option<f64>, capabilities: Option<Vec<String>> }` (all optional, patch semantics)
    - Route: `.route("/api/agents/:id", patch(crate::api::agents::update_agent))` in `bootstrap.rs` (append to existing agents block)
  - **Step B — CLI**: Add `ghost agent update <id|name>` subcommand
    - Args: `--model <model>`, `--spending-cap <f64>`, `--capabilities <comma-list>`
    - At least one flag required (clap `required_unless_present_any` or runtime check)
    - HTTP: `PATCH /api/agents/:id` with `UpdateAgentRequest` body
    - Print updated agent summary on success
  - Backend: HttpOnly
  - File: `ghost-gateway/src/api/agents.rs`, `ghost-gateway/src/bootstrap.rs`, `ghost-gateway/src/cli/agent.rs`

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

- **T-1.8.1** *(Moved to T-0.13.2 in Phase 0)* — `ghost completions` has no backend requirement and depends only on T-0.2.1 (`clap_complete`), which already lands in Phase 0. Implementing it in Phase 0 means the exit criteria binary is fully functional before Phase 1 starts.

### 1.9 Channel Commands

> Ghost agents communicate with end users through messaging adapters (WhatsApp, Telegram,
> Slack, Discord). The CLI is the operator's window into that layer — not for sending messages,
> but for verifying bindings and credential health. The `ghost-channels` crate already has
> working adapters for all four. These commands read from config (DirectOnly) and make
> lightweight external probe calls. No gateway dependency.
>
> **Credential resolution convention** (used by T-1.9.2 and T-1.9.4): for each option key
> (e.g., `bot_token`), check in order:
> 1. Env var named by `{KEY}_key` value in options (e.g., options `bot_token_key: GHOST_TELEGRAM_BOT_TOKEN` → `std::env::var("GHOST_TELEGRAM_BOT_TOKEN")`)
> 2. SecretProvider lookup using that same key name via `provider.get_secret("GHOST_TELEGRAM_BOT_TOKEN")`
> 3. Literal value in options under the plain key name (e.g., `bot_token: "xoxb-..."`) — warn operator that inline credentials in config are not recommended
> 4. `Err(CliError::Config("credential not found: bot_token for channel whatsapp"))` → exit 78

- [ ] **T-1.9.1** Implement `ghost channel list`
  - Create `ghost-gateway/src/cli/channel.rs`
  - Load `GhostConfig::channels` (Vec of `ChannelConfig { channel_type, agent, options }`)
  - For each entry, derive display fields from the known adapter implementations:

    | channel_type | mode derivation | streaming | editing |
    |---|---|---|---|
    | `telegram` | always "bot" | yes | yes |
    | `whatsapp` | options has `phone_number_id` → "cloud-api"; else "sidecar" | no | no |
    | `slack` | always "socket-mode" | no | yes |
    | `discord` | always "gateway" | no | yes |
    | `cli` | always "stdio" | yes | no |

  - `ChannelListEntry` struct with `#[derive(Serialize)]` + `TableDisplay`:
    - columns: `CHANNEL`, `AGENT`, `MODE`, `STREAMING`, `EDITING`, `CREDENTIALS`
    - `CREDENTIALS` column: list option keys present (names only, never values), e.g., "bot_token_key, ✓"
  - If `config.channels` is empty: print "No channels configured. Add a `channels:` block to ghost.yml or run `ghost init`." to stderr → exit 0
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/channel.rs`

- [ ] **T-1.9.2** Implement `ghost channel test [<type>]`
  - Accepts optional positional `<type>` arg. If omitted, test all channels in `config.channels`
  - For each matching channel, resolve credentials (see convention above), then execute the adapter-specific probe:

    **telegram** — `GET https://api.telegram.org/bot{token}/getMe` (5s timeout)
    - 200 + `{"ok": true}` → `✓ telegram → {agent}: @{result.username} (id={result.id})`
    - 200 + `{"ok": false, "description": "..."}` → `✗ telegram → {agent}: {description}` → exit 1
    - network error / timeout → `✗ telegram → {agent}: connection failed ({err})` → exit 69

    **whatsapp (cloud-api)** — `GET https://graph.facebook.com/v18.0/{phone_number_id}?fields=display_phone_number,verified_name&access_token={token}` (5s timeout)
    - 200 → `✓ whatsapp → {agent}: {display_phone_number} ({verified_name})`
    - 400/401 → parse `{"error": {"message": "..."}}` → `✗ whatsapp → {agent}: {message}` → exit 1
    - network error → exit 69

    **whatsapp (sidecar)** — cannot probe a local Baileys process via HTTP
    - Print: `~ whatsapp → {agent}: sidecar mode — cannot probe remotely. Ensure Node.js/Baileys process is running.` → exit 0 (not a failure)

    **slack** — `POST https://slack.com/api/auth.test` with `Authorization: Bearer {bot_token}`, no body (5s timeout)
    - 200 + `{"ok": true}` → `✓ slack → {agent}: {user} @ {team}`
    - 200 + `{"ok": false, "error": "..."}` → `✗ slack → {agent}: {error}` → exit 1
    - network error → exit 69

    **discord** — `GET https://discord.com/api/v10/users/@me` with `Authorization: Bot {token}` (5s timeout)
    - 200 → `✓ discord → {agent}: @{username}#{discriminator}`
    - 401 → `✗ discord → {agent}: invalid token` → exit 1
    - network error → exit 69

    **cli** — always `✓ cli → {agent}: ready (no external dependency)`

  - Use a single `reqwest::Client` with 5s timeout for all probes (do not create per-request clients)
  - `pub async fn probe_channel(entry: &ChannelConfig, provider: &dyn SecretProvider) -> ProbeResult` — export this function; T-1.9.3 and T-1.9.4 call it directly
  - Args: `--output` respects GlobalOpts format; JSON output wraps results in `[{"channel": ..., "agent": ..., "status": "ok"|"error"|"warning", "detail": ...}]`
  - Backend: DirectOnly (config) + external HTTPS (probe) — no gateway needed
  - Dependency: T-0.6.1 (SecretProvider resolution)
  - File: `ghost-gateway/src/cli/channel.rs`

- [ ] **T-1.9.3** Extend `ghost doctor` with channel health section
  - After the "Gateway" check block, add a "Channels" section
  - Call `channel::probe_channel(entry, &provider)` for each configured channel — reuse T-1.9.2's exported function exactly, no duplication
  - Output format (matching existing doctor style):
    ```
    Channels (2 configured)
      ✓ telegram → alice: @ghostbot (id=123456789)
      ✗ whatsapp → bob: (#190) invalid OAuth 2.0 access token
    ```
  - If `config.channels` is empty: `Channels  ✗ none configured — agents cannot receive messages`
  - Count channel failures toward the summary error count
  - Dependency: T-1.9.2 (`channel::probe_channel`), T-1.3.1
  - File: `ghost-gateway/src/cli/doctor.rs`

- [ ] **T-1.9.4** Extend `ghost init` with channel setup step
  - After LLM provider selection, prompt: `Configure a messaging channel? [y/N]`
  - If yes, present menu: `[1] Telegram  [2] WhatsApp (Cloud API)  [3] WhatsApp (Sidecar)  [4] Slack  [5] Discord  [0] Skip`
  - Per-type credential collection and config generation:

    **Telegram**
    - Check `GHOST_TELEGRAM_BOT_TOKEN` env var first; if set, skip prompt and print "✓ Found GHOST_TELEGRAM_BOT_TOKEN in environment"
    - Otherwise: `print!("Bot token (from @BotFather): ")` + raw stdin read (no echo — `std::io::stdin().read_line()` with terminal echo suppressed via platform API, or print a warning that the token will be visible)
    - Store via `provider.set_secret("GHOST_TELEGRAM_BOT_TOKEN", &token)?`
    - Write to `ghost.yml`: `options: { bot_token_key: "GHOST_TELEGRAM_BOT_TOKEN" }`

    **WhatsApp (Cloud API)**
    - Check `GHOST_WHATSAPP_ACCESS_TOKEN` + `GHOST_WHATSAPP_PHONE_NUMBER_ID` env vars
    - If not set: prompt for each separately (access token is secret, phone number ID is not)
    - Store access token via `provider.set_secret("GHOST_WHATSAPP_ACCESS_TOKEN", &token)?`
    - Write to `ghost.yml`: `options: { mode: "cloud_api", access_token_key: "GHOST_WHATSAPP_ACCESS_TOKEN", phone_number_id: "{literal_id}" }`
    - ⚠️ Print: "You must register a webhook at https://developers.facebook.com/apps/ pointing to {gateway_url}/api/channels/whatsapp/webhook"

    **WhatsApp (Sidecar)**
    - Write to `ghost.yml`: `options: { mode: "sidecar" }`
    - ⚠️ Print: "Sidecar mode requires Node.js and the Baileys package. See docs/channels/whatsapp-sidecar.md"

    **Slack**
    - Check `GHOST_SLACK_BOT_TOKEN` + `GHOST_SLACK_APP_TOKEN` env vars
    - Prompt for each if not present; store via SecretProvider
    - Write to `ghost.yml`: `options: { bot_token_key: "GHOST_SLACK_BOT_TOKEN", app_token_key: "GHOST_SLACK_APP_TOKEN" }`

    **Discord**
    - Check `GHOST_DISCORD_BOT_TOKEN` env var
    - Prompt if not present; store via SecretProvider
    - Write to `ghost.yml`: `options: { bot_token_key: "GHOST_DISCORD_BOT_TOKEN" }`

  - After collecting credentials: call `channel::probe_channel()` to validate before writing. If probe fails:
    - Print error + "Write config anyway? [y/N]". If N, abort channel setup (do not write channel block to ghost.yml). LLM and DB setup already done — do not roll back.
  - Prompt for which agent to bind: list registered agents from config (default: "default" if it exists)
  - Append `ChannelConfig` entry to `ghost.yml` channels list using `serde_yaml`
  - Skip all prompts with `--defaults` (no channel configured, print "Run `ghost init` again or edit ghost.yml to add channels")
  - Dependency: T-1.1.1 (init flow), T-1.9.2 (`probe_channel`)
  - File: `ghost-gateway/src/cli/init.rs`, `ghost-gateway/src/cli/channel.rs`

### Phase 1 Exit Criteria

| Metric | Target |
|---|---|
| `ghost init` | Creates `~/.ghost/` (or `$GHOST_HOME/`) with config, SOUL.md, keypair, DB, optional channel setup |
| `ghost login/logout` | Token stored/cleared via SecretProvider |
| `ghost doctor` | Reports 10+ health checks including channel health; API key output says "found (not validated)" |
| `ghost agent list/create/inspect/delete/update` | Full CRUD via HTTP, including PATCH |
| `ghost safety status/kill-all/clear` | Kill switch management works |
| `ghost db migrate/status` | Migration reporting works |
| `ghost channel list` | Shows all configured channel↔agent bindings |
| `ghost channel test` | Probes each channel's external API with correct credentials |
| All commands | Respect `--output json`, `--quiet`, `--color` |
| (`ghost completions` — in Phase 0 exit criteria) | |

---

## Phase 2: Observability Commands

> Debugging, monitoring, and live event streaming.
> Ref: CLI_DESIGN.md §13.3, §15.5

### 2.1 Live Event Streaming

- [ ] **T-2.1.1** Implement `ghost logs` `§4.1, E.3, E.8, R12`
  - Create `ghost-gateway/src/cli/logs.rs`
  - Connect to `ws://{gateway}/api/ws?token={token}` via `tokio-tungstenite`
  - Parse incoming `WsEvent` JSON (already `#[serde(tag = "type")]`)
  - Args: `--agent <id>` (filter), `--type <event_type>` (filter), `--json` (NDJSON output), `--idle-timeout <seconds>` (default: 1800)
  - Table mode: formatted event lines with timestamp, type, agent, summary
  - JSON mode: one JSON object per line (NDJSON)
  - Signal handling: `tokio::select!` with `ctrl_c()`, send WS Close frame on Ctrl+C (E.8)
  - Idle timeout: use `tokio::time::timeout` wrapping the WS read future; if no message arrives within `--idle-timeout` seconds, close the connection cleanly and print "Connection idle for {n}s. Re-run to reconnect." to stderr (R12)
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

- [ ] **T-2.4.2** Implement `ghost session inspect` `§4.1`
  - Accepts `<session_id>` positional arg
  - ~~F.5 (no endpoint) is stale~~ — `GET /api/sessions/:id/events` was added and is registered in `bootstrap.rs:405`
  - HTTP: `GET /api/sessions/:id/events` → parse event list
  - Direct fallback: query `itp_events WHERE session_id = ?` via `cortex_storage::queries::itp_event_queries`
  - Show: event timeline, gate states, agent, timestamps
  - Backend: PreferHttp
  - File: `ghost-gateway/src/cli/session.rs`

### 2.5 Database Commands (Continued)

- [ ] **T-2.5.1** Implement `ghost db verify` `§4.1`
  - Walk `itp_events` hash chain: verify each `event_hash` matches `blake3(content_hash + previous_hash)`
  - Args: `--full` (walk entire chain vs spot-check 100 random entries)
  - Report: chain length, breaks found, verification time
  - Backend: DirectOnly
  - File: `ghost-gateway/src/cli/db.rs`

- [ ] **T-2.5.2** Implement `ghost db compact` `§4.1, §8.3`
  - Args: `--yes`, `--dry-run`, `--force`
  - **Pre-flight gateway check** (R19): probe `{gateway_url}/api/health` with 2s timeout.
    - If 200 → abort: print "Gateway is running. VACUUM requires exclusive DB access and will conflict with active connections. Stop the gateway first, then compact. Pass `--force` to skip this check at your own risk." → exit `EX_UNAVAILABLE (69)`
    - If timeout/error → proceed (gateway not running)
    - `--force` skips the probe but prints warning: "⚠️ Skipping gateway check. Ensure no other process has the DB open."
  - Confirmation prompt: "Compact database? This may take a moment. [y/N]"
  - Execute: `PRAGMA wal_checkpoint(TRUNCATE); VACUUM;`
  - Dry-run: show current WAL size + estimated savings (skip probe in dry-run)
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

- [ ] **T-3.1.3** Update `ghost init` to use `SoulManager::create_template()` `F.3 follow-up`
  - T-1.1.1 writes SOUL.md from a hardcoded inline string because `SoulManager::create_template()` doesn't exist yet (F.3)
  - Once T-3.1.1 lands, replace the inline template in `init.rs` with `SoulManager::create_template(&soul_path)?`
  - Delete the hardcoded template string; do not keep both paths
  - Dependency: T-3.1.1, T-1.1.1
  - File: `ghost-gateway/src/cli/init.rs`

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
  - Fetch `/.well-known/agent.json` from remote URL to read the peer's `AgentCard`
  - Registration path (in priority order):
    1. If gateway is running: `POST /api/a2a/discover` does not exist — use `POST /api/mesh/peers` if added by ADE 4.1, otherwise fall back to option 2
    2. Config file: append peer entry to `~/.ghost/config/peers.yml` (create if absent)
  - ⚠️ Verify which registration endpoint ADE Phase 4.1 exposes before implementing. Add `--local` flag to force config-file storage regardless of gateway state.
  - Backend: HttpOnly (fetch) + DirectOnly fallback (config write)
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

### 4.6 Channel Injection (Operator Debug Tool)

> Operator-only escape hatch: inject a synthetic inbound message into a running agent without
> touching the real channel. Requires the gateway to be running — the agent must be live to
> receive and respond. Requires a new backend endpoint; the ghost-channels adapters have
> `push_inbound()` on WhatsApp but no general injection path accessible from outside the process.

- [ ] **T-4.6.1** Add `POST /api/channels/:type/inject` endpoint to ghost-gateway (backend prerequisite)
  - New handler in `ghost-gateway/src/api/channels.rs` (new file)
  - Route: `.route("/api/channels/:type/inject", post(crate::api::channels::inject_message))` in `bootstrap.rs`
  - Request body:
    ```rust
    struct InjectMessageRequest {
        content: String,
        sender: String,         // e.g. "ghost-operator" — shown in audit log as synthetic sender
        agent_id: Option<Uuid>, // if None: use first agent bound to this channel_type in config
    }
    ```
  - Handler logic:
    1. Find target agent: `agent_id` arg → `state.agent_registry.lookup(id)`, or scan `config.channels` for first entry with matching `channel_type` → `registry.lookup_by_name(entry.agent)`
    2. If no agent found for channel_type → 404 `{"error": "no agent bound to channel '{type}'"}`
    3. Construct `ghost_channels::types::InboundMessage::new(type, &request.sender, &request.content)` with a fresh `Uuid::now_v7()` as `id`
    4. Route to agent via existing session/lane machinery — use `state.session_manager` or `state.itp_router` (whichever owns the inbound message path; verify against `session/router.rs`)
    5. Return `202 Accepted` with `{"message_id": "{uuid}", "agent_id": "{uuid}", "routed": true}` — do not wait for the agent's response (async)
  - Auth: requires valid Bearer token (same middleware as all other routes)
  - ⚠️ `ghost-channels` must be added to `ghost-gateway/Cargo.toml [dependencies]` if not already present — verify before implementing
  - File: `ghost-gateway/src/api/channels.rs` (new), `ghost-gateway/src/bootstrap.rs`

- [ ] **T-4.6.2** Implement `ghost channel send <type> <message>`
  - Args: `<type>` (telegram/whatsapp/slack/discord), `<message>` (positional string), `--agent <id|name>` (optional), `--sender <name>` (default: `"ghost-operator"`)
  - HTTP: `POST /api/channels/:type/inject` with `InjectMessageRequest` body
  - On 202: print `Injected → agent {agent_id} (message_id={message_id})`. Then stream incoming events filtered to that agent via WS for 10s, printing any response the agent sends (so the operator can see if the pipeline end-to-end works). Ctrl+C exits early.
  - On 404: print "No agent is bound to channel '{type}'. Check `ghost channel list`." → exit 1
  - Backend: HttpOnly
  - Dependency: T-4.6.1, T-0.2.2 (tokio-tungstenite for response streaming)
  - File: `ghost-gateway/src/cli/channel.rs`

### Phase 4 Exit Criteria

| Metric | Target |
|---|---|
| `ghost mesh peers/trust/discover/ping` | A2A peer management works |
| `ghost skill list/install/inspect` | WASM skill lifecycle works |
| `ghost session replay` | Text-based replay renders conversation |
| `ghost convergence history` | Score history with deltas |
| `ghost channel send` | Injects test message, streams agent response for 10s |

---

## Cross-Cutting Concerns (All Phases)

### JSON Output Stability `E.10`

- [ ] **T-X.1** Document JSON output stability contract
  - Fields are append-only within a major version
  - New fields may be added; existing fields never removed or renamed
  - Add `--format-version` flag (default: latest) for future breaking changes
  - Document in `docs/API_CONTRACT.md` (or create `docs/CLI_CONTRACT.md`)

### XDG & Home Directory `E.11`

- **T-X.2** *(Promoted to T-0.13.1 in Phase 0)* — `GHOST_HOME` must be in place before `ghost init` constructs the `~/.ghost/` tree. Keeping it in Cross-Cutting created a dependency inversion: Phase 1 code would bake in `~/.ghost/` before the override ever landed. See T-0.13.1.

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
| R19 | `ghost db compact` runs `VACUUM` while gateway is active | 2 | Pre-flight HTTP health probe in T-2.5.2; abort if gateway reachable unless `--force` passed |
| R20 | F.x audit findings may be stale against actual source | 0 | Confirmed: F.5 (no sessions/:id/events endpoint) is already stale — endpoint exists at `bootstrap.rs:405`. Before implementing any task that relies on an F.x finding, verify the finding against current source. |
| R21 | WhatsApp sidecar mode cannot be probed remotely | 1 | `ghost channel test whatsapp` prints a warning for sidecar mode rather than failing. `ghost doctor` treats sidecar as `~` (warning, not `✗`) so it doesn't block a passing health check. Operator must verify sidecar health manually. |
| R22 | Inline credentials in `ChannelConfig.options` (e.g., `bot_token: "xoxb-..."`) | 1 | T-1.9.2 credential resolution warns the operator when a literal value is used instead of a secret key reference. T-1.9.4 (`ghost init`) always stores via SecretProvider and writes `_key` references — never writes literal credentials to ghost.yml. |

---

## Task Count Summary

| Phase | Tasks | Effort | Notes |
|---|---|---|---|
| Phase 0: Infrastructure | 22 | ~45h | +T-0.13.1 (GHOST_HOME), +T-0.13.2 (completions, promoted from Phase 1) |
| Phase 1: Essential | 21 | ~65h | +T-1.5.6 (agent update + backend), +T-1.9.1–T-1.9.4 (channel list/test/doctor/init) |
| Phase 2: Observability | 10 | ~35h | T-2.4.2 now PreferHttp (F.5 was stale); idle timeout added to T-2.1.1 |
| Phase 3: Identity/Secrets/Policy | 12 | ~28h | +T-3.1.3 (wire SoulManager back into init.rs) |
| Phase 4: Mesh/Skills/Advanced | 12 | ~35h | +T-4.6.1 (inject backend endpoint), +T-4.6.2 (ghost channel send) |
| Cross-Cutting | 5 | ~10h | T-X.2 promoted to Phase 0; T-X.6/7 unchanged |
| **Total** | **82** | **~218h** | |
