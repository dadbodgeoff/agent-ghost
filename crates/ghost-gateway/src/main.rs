//! ghost-gateway binary entry point (Task 6.6 — CLI subcommands).

use clap::Parser;
use ghost_gateway::bootstrap::GatewayBootstrap;
use ghost_gateway::cli;
use ghost_gateway::cli::audit_cmd::{AuditExportArgs, AuditQueryArgs, AuditTailArgs};
use ghost_gateway::cli::backend::CliBackend;
use ghost_gateway::cli::channel::{ChannelListArgs, ChannelSendArgs, ChannelTestArgs};
use ghost_gateway::cli::convergence::{ConvergenceHistoryArgs, ConvergenceScoresArgs};
use ghost_gateway::cli::cron::{CronHistoryArgs, CronListArgs};
use ghost_gateway::cli::db::{
    DbCompactArgs, DbMigrateArgs, DbStatusArgs, DbVerifyArgs,
};
use ghost_gateway::cli::error::CliError;
use ghost_gateway::cli::heartbeat::HeartbeatStatusArgs;
use ghost_gateway::cli::logs::LogsArgs;
use ghost_gateway::cli::mesh::{MeshDiscoverArgs, MeshPeersArgs, MeshPingArgs, MeshTrustArgs};
use ghost_gateway::cli::output::{ColorChoice, OutputFormat};
use ghost_gateway::cli::identity::{
    IdentityDriftArgs, IdentityInitArgs, IdentityShowArgs, IdentitySignArgs,
};
use ghost_gateway::cli::policy::{PolicyCheckArgs, PolicyLintArgs, PolicyShowArgs};
use ghost_gateway::cli::secret::{
    SecretDeleteArgs, SecretListArgs, SecretProviderArgs, SecretSetArgs,
};
use ghost_gateway::cli::session::{SessionInspectArgs, SessionListArgs, SessionReplayArgs};
use ghost_gateway::cli::skill::{SkillInspectArgs, SkillInstallArgs, SkillListArgs};
use ghost_gateway::config::GhostConfig;

#[derive(Parser)]
#[command(name = "ghost", about = "GHOST Platform Gateway", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    global: GlobalOpts,
}

/// Global options available to all subcommands.
#[derive(Debug, clap::Args)]
struct GlobalOpts {
    /// Path to ghost.yml configuration file.
    #[arg(long, short, global = true)]
    config: Option<String>,

    /// Output format.
    #[arg(long, global = true, default_value = "table")]
    output: OutputFormat,

    /// Gateway URL (overrides config).
    #[arg(long, global = true)]
    gateway_url: Option<String>,

    /// Enable verbose output.
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Suppress non-essential output.
    #[arg(long, short, global = true)]
    quiet: bool,

    /// Color output preference.
    #[arg(long, global = true, default_value = "auto")]
    color: ColorChoice,

    /// Pin structured output to a specific format version (default: latest).
    #[arg(long, global = true, default_value = "latest")]
    format_version: String,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Start the gateway server (default).
    Serve,
    /// Interactive chat session.
    Chat,
    /// Show gateway and agent status.
    Status,
    /// Create an encrypted backup of platform state.
    Backup {
        /// Output path for the backup archive.
        #[arg(long, short)]
        output: Option<String>,
    },
    /// Analyze a data export from an external AI platform.
    Export {
        /// Path to the export file to analyze.
        path: String,
    },
    /// Migrate from an OpenClaw installation.
    Migrate {
        /// Path to the OpenClaw installation directory.
        #[arg(long, default_value = "~/.openclaw")]
        source: String,
    },

    // ─── Phase 0+ commands ───

    /// First-run platform setup.
    Init,
    /// Authenticate with a running gateway.
    Login,
    /// Remove stored authentication.
    Logout,
    /// Run platform health checks.
    Doctor,
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        shell: clap_complete::Shell,
    },
    /// Generate man pages to a directory (T-X.7).
    Man {
        /// Output directory for man pages.
        #[arg(default_value = ".")]
        dir: String,
    },

    // ─── Phase 2: Observability ───

    /// Stream live events from the gateway.
    Logs {
        /// Filter to a specific agent ID.
        #[arg(long)]
        agent: Option<String>,
        /// Filter to a specific event type.
        #[arg(long, value_name = "TYPE")]
        r#type: Option<String>,
        /// Output as NDJSON instead of a table.
        #[arg(long)]
        json: bool,
        /// Idle timeout in seconds before closing connection.
        #[arg(long, default_value = "1800")]
        idle_timeout: u64,
    },

    /// Manage agents.
    #[command(subcommand)]
    Agent(AgentCommands),
    /// Safety and kill switch management.
    #[command(subcommand)]
    Safety(SafetyCommands),
    /// Configuration management.
    #[command(name = "config", subcommand)]
    Config(ConfigCommands),
    /// Database management.
    #[command(subcommand)]
    Db(DbCommands),
    /// Audit log queries.
    #[command(subcommand)]
    Audit(AuditCommands),
    /// Convergence score queries.
    #[command(subcommand)]
    Convergence(ConvergenceCommands),
    /// Session management.
    #[command(subcommand)]
    Session(SessionCommands),
    /// Identity and signing management.
    #[command(subcommand)]
    Identity(IdentityCommands),
    /// Secret management.
    #[command(subcommand)]
    Secret(SecretCommands),
    /// Corporate policy management.
    #[command(subcommand)]
    Policy(PolicyCommands),
    /// Multi-agent mesh networking.
    #[command(subcommand)]
    Mesh(MeshCommands),
    /// WASM skill management.
    #[command(subcommand)]
    Skill(SkillCommands),
    /// Messaging channel management.
    #[command(subcommand)]
    Channel(ChannelCommands),
    /// Agent heartbeat monitoring.
    #[command(subcommand)]
    Heartbeat(HeartbeatCommands),
    /// Scheduled task management.
    #[command(subcommand)]
    Cron(CronCommands),
}

// ─── Subcommand enums ─────────────────────────────────────────────────────────

#[derive(clap::Subcommand)]
enum AgentCommands {
    /// List all agents.
    List,
    /// Create a new agent.
    Create,
    /// Inspect an agent.
    Inspect { id: String },
    /// Delete an agent.
    Delete { id: String },
    /// Update agent settings.
    Update { id: String },
    /// Pause an agent.
    Pause { id: String },
    /// Resume a paused agent.
    Resume { id: String },
    /// Quarantine an agent.
    Quarantine { id: String },
}

#[derive(clap::Subcommand)]
enum SafetyCommands {
    /// Show safety status.
    Status,
    /// Kill all agents.
    KillAll,
    /// Clear kill state.
    Clear,
}

#[derive(clap::Subcommand)]
enum ConfigCommands {
    /// Show resolved configuration.
    Show,
    /// Validate configuration.
    Validate,
}

#[derive(clap::Subcommand)]
enum DbCommands {
    /// Run pending database migrations.
    Migrate,
    /// Show database status.
    Status,
    /// Verify hash chain integrity.
    Verify {
        /// Walk the entire chain (default: spot-check 100 events).
        #[arg(long)]
        full: bool,
    },
    /// Compact database (WAL checkpoint + VACUUM).
    Compact {
        /// Skip confirmation prompt.
        #[arg(long, short)]
        yes: bool,
        /// Show what would happen without making changes.
        #[arg(long)]
        dry_run: bool,
        /// Skip gateway health probe (dangerous).
        #[arg(long)]
        force: bool,
    },
}

#[derive(clap::Subcommand)]
enum AuditCommands {
    /// Query audit log.
    Query {
        /// Filter to a specific agent ID.
        #[arg(long)]
        agent: Option<String>,
        /// Filter to a specific severity level.
        #[arg(long)]
        severity: Option<String>,
        /// Filter to a specific event type.
        #[arg(long, value_name = "TYPE")]
        event_type: Option<String>,
        /// Start time filter (ISO 8601).
        #[arg(long)]
        since: Option<String>,
        /// End time filter (ISO 8601).
        #[arg(long)]
        until: Option<String>,
        /// Full-text search across details.
        #[arg(long)]
        search: Option<String>,
        /// Maximum entries to return.
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// Export audit log.
    Export {
        /// Export format: json, csv, jsonl.
        #[arg(long, default_value = "json")]
        format: String,
        /// Output file path (default: stdout).
        #[arg(long, short)]
        output: Option<String>,
    },
    /// Tail audit log (live stream).
    Tail,
}

#[derive(clap::Subcommand)]
enum ConvergenceCommands {
    /// Show convergence scores.
    Scores,
    /// Show convergence history for an agent.
    History {
        /// Agent ID.
        agent_id: String,
        /// Start time filter (ISO 8601).
        #[arg(long)]
        since: Option<String>,
    },
}

#[derive(clap::Subcommand)]
enum SessionCommands {
    /// List sessions.
    List {
        /// Filter to a specific agent ID.
        #[arg(long)]
        agent: Option<String>,
        /// Maximum sessions to return.
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Inspect a session's events.
    Inspect {
        /// Session ID.
        session_id: String,
    },
    /// Replay a session.
    Replay {
        /// Session ID.
        session_id: String,
    },
}

#[derive(clap::Subcommand)]
enum IdentityCommands {
    /// Initialize identity (keypair + SOUL.md).
    Init,
    /// Show identity information.
    Show,
    /// Check for identity drift.
    Drift,
    /// Sign a file.
    Sign { file: String },
}

#[derive(clap::Subcommand)]
enum SecretCommands {
    /// Set a secret value (reads value from stdin).
    Set { key: String },
    /// List known secret keys.
    List,
    /// Delete a secret.
    Delete {
        key: String,
        /// Skip confirmation prompt.
        #[arg(long, short)]
        yes: bool,
    },
    /// Show active secret provider.
    Provider,
}

#[derive(clap::Subcommand)]
enum PolicyCommands {
    /// Show corporate policy.
    Show,
    /// Check a tool call against policy.
    Check {
        tool_name: String,
        /// Agent ID to evaluate against.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Lint corporate policy document.
    Lint,
}

#[derive(clap::Subcommand)]
enum MeshCommands {
    /// List mesh peers.
    Peers,
    /// Show trust scores.
    Trust,
    /// Discover a remote peer.
    Discover { url: String },
    /// Ping a peer.
    Ping { peer_id: String },
}

#[derive(clap::Subcommand)]
enum SkillCommands {
    /// List installed skills.
    List,
    /// Install a skill.
    Install { path: String },
    /// Inspect a skill.
    Inspect { name: String },
}

#[derive(clap::Subcommand)]
enum ChannelCommands {
    /// List configured channels.
    List,
    /// Test channel connectivity.
    Test {
        /// Channel type to test (omit to test all).
        channel_type: Option<String>,
    },
    /// Send a test message to a channel (inject into a running agent).
    Send {
        /// Channel type (telegram, whatsapp, slack, discord).
        channel_type: String,
        /// Message content.
        message: String,
        /// Agent ID or name to target.
        #[arg(long)]
        agent: Option<String>,
        /// Sender name for the synthetic message.
        #[arg(long, default_value = "ghost-operator")]
        sender: String,
    },
}

#[derive(clap::Subcommand)]
enum HeartbeatCommands {
    /// Show heartbeat engine status.
    Status,
}

#[derive(clap::Subcommand)]
enum CronCommands {
    /// List registered cron jobs.
    List,
    /// Show cron execution history.
    History {
        /// Maximum entries to show.
        #[arg(long, default_value = "20")]
        limit: u32,
    },
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli_args = Cli::parse();

    // Serve needs multi-thread for concurrent connection handling.
    // All other CLI subcommands use current_thread (lighter, sufficient for I/O).
    let is_serve = matches!(cli_args.command, None | Some(Commands::Serve));

    let rt = if is_serve {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build multi-thread runtime")
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build current-thread runtime")
    };

    match rt.block_on(run_command(cli_args)) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(e.exit_code());
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Load config from file or defaults.
fn load_config(config_path: Option<&str>) -> Result<GhostConfig, CliError> {
    GhostConfig::load_default(config_path).map_err(CliError::from)
}

/// Resolve the gateway URL from CLI flag or config.
fn resolve_gateway_url(override_url: Option<&str>, config: &GhostConfig) -> String {
    override_url
        .map(String::from)
        .unwrap_or_else(|| format!("http://{}:{}", config.gateway.bind, config.gateway.port))
}

/// Resolve bearer token from env var or stored credential.
fn resolve_token() -> Option<String> {
    std::env::var("GHOST_TOKEN").ok()
}

// ─── Command dispatch ─────────────────────────────────────────────────────────

async fn run_command(cli_args: Cli) -> Result<(), CliError> {
    match cli_args.command.unwrap_or(Commands::Serve) {
        Commands::Serve => {
            let config_path = cli_args.global.config.clone();
            let result = GatewayBootstrap::run(config_path.as_deref()).await;
            match result {
                Ok((gateway, config)) => {
                    let app_state = gateway
                        .app_state
                        .clone()
                        .expect("AppState must be set after bootstrap");
                    let router = GatewayBootstrap::build_router(
                        &config,
                        app_state,
                        gateway.mesh_router.clone(),
                    );
                    gateway
                        .run_with_router(Some(router), None)
                        .await
                        .map_err(|e| CliError::Internal(format!("gateway error: {e}")))
                }
                Err(e) => Err(CliError::Internal(format!("bootstrap: {e}"))),
            }
        }

        Commands::Chat => cli::chat::run_interactive_chat().await,

        Commands::Status => {
            let base_url = cli_args
                .global
                .gateway_url
                .as_deref()
                .unwrap_or("http://127.0.0.1:18789");
            cli::status::show_status(
                base_url,
                cli_args.global.config.as_deref(),
                cli_args.global.output,
            )
            .await
        }

        Commands::Backup { output } => cli::commands::run_backup(output.as_deref()),
        Commands::Export { path } => cli::commands::run_export(&path),
        Commands::Migrate { source } => cli::commands::run_migrate(&source),

        Commands::Completions { shell } => {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            cli::completions::run(shell, &mut cmd)
        }

        Commands::Man { dir } => {
            use clap::CommandFactory;
            let cmd = Cli::command();
            let out_dir = std::path::PathBuf::from(&dir);
            std::fs::create_dir_all(&out_dir)
                .map_err(|e| CliError::Internal(format!("create dir {dir}: {e}")))?;
            clap_mangen::generate_to(cmd, &out_dir)
                .map_err(|e| CliError::Internal(format!("generate man pages: {e}")))?;
            eprintln!("Man pages written to {dir}/");
            Ok(())
        }

        // Phase 0+ stubs (not yet implemented).
        Commands::Init => cli::init::run().await,
        Commands::Login => todo!("ghost login — Phase 1"),
        Commands::Logout => todo!("ghost logout — Phase 1"),
        Commands::Doctor => cli::doctor::run().await,

        // ─── Phase 2: ghost logs ───────────────────────────────────────────

        Commands::Logs { agent, r#type, json, idle_timeout } => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let gateway_url = resolve_gateway_url(cli_args.global.gateway_url.as_deref(), &config);
            let token = resolve_token();
            cli::logs::run(LogsArgs {
                agent,
                event_type: r#type,
                json,
                idle_timeout,
                gateway_url,
                token,
            })
            .await
        }

        // ─── Phase 1 stubs (Agent / Safety / Config) ─────────────────────

        Commands::Agent(_) => cli::agent::run().await,
        Commands::Safety(_) => cli::safety::run().await,
        Commands::Config(_) => cli::config_cmd::run().await,

        // ─── Phase 2: ghost db ────────────────────────────────────────────

        Commands::Db(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let gateway_url = resolve_gateway_url(cli_args.global.gateway_url.as_deref(), &config);
            match sub {
                DbCommands::Migrate => {
                    let backend = CliBackend::open_direct(&config)?;
                    cli::db::run_migrate(DbMigrateArgs {}, &backend)
                }
                DbCommands::Status => {
                    let backend = CliBackend::open_direct(&config)?;
                    cli::db::run_status(DbStatusArgs { output: cli_args.global.output }, &backend)
                }
                DbCommands::Verify { full } => {
                    let backend = CliBackend::open_direct(&config)?;
                    cli::db::run_verify(DbVerifyArgs { full }, &backend)
                }
                DbCommands::Compact { yes, dry_run, force } => {
                    let backend = CliBackend::open_direct(&config)?;
                    cli::db::run_compact(
                        DbCompactArgs { yes, dry_run, force, gateway_url },
                        &backend,
                    )
                    .await
                }
            }
        }

        // ─── Phase 2: ghost audit ─────────────────────────────────────────

        Commands::Audit(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let gateway_url = resolve_gateway_url(cli_args.global.gateway_url.as_deref(), &config);
            let token = resolve_token();
            match sub {
                AuditCommands::Query {
                    agent, severity, event_type, since, until, search, limit,
                } => {
                    let backend = CliBackend::detect(
                        &config,
                        cli_args.global.gateway_url.as_deref(),
                        token,
                    )
                    .await?;
                    cli::audit_cmd::run_query(
                        AuditQueryArgs {
                            agent,
                            severity,
                            event_type,
                            since,
                            until,
                            search,
                            limit,
                            output: cli_args.global.output,
                        },
                        &backend,
                    )
                    .await
                }
                AuditCommands::Export { format, output } => {
                    let backend = CliBackend::detect(
                        &config,
                        cli_args.global.gateway_url.as_deref(),
                        token,
                    )
                    .await?;
                    cli::audit_cmd::run_export(AuditExportArgs { format, output }, &backend).await
                }
                AuditCommands::Tail => {
                    cli::audit_cmd::run_tail(AuditTailArgs { gateway_url, token }).await
                }
            }
        }

        // ─── Phase 2: ghost convergence ───────────────────────────────────

        Commands::Convergence(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let token = resolve_token();
            match sub {
                ConvergenceCommands::Scores => {
                    let backend = CliBackend::detect(
                        &config,
                        cli_args.global.gateway_url.as_deref(),
                        token,
                    )
                    .await?;
                    cli::convergence::run_scores(
                        ConvergenceScoresArgs { output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                ConvergenceCommands::History { agent_id, since } => {
                    let backend = CliBackend::detect(
                        &config,
                        cli_args.global.gateway_url.as_deref(),
                        token,
                    )
                    .await?;
                    cli::convergence::run_history(
                        ConvergenceHistoryArgs {
                            agent_id,
                            since,
                            output: cli_args.global.output,
                        },
                        &backend,
                    )
                    .await
                }
            }
        }

        // ─── Phase 2+4: ghost session ─────────────────────────────────────

        Commands::Session(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let token = resolve_token();
            let backend = CliBackend::detect(
                &config,
                cli_args.global.gateway_url.as_deref(),
                token,
            )
            .await?;
            match sub {
                SessionCommands::List { agent, limit } => {
                    cli::session::run_list(
                        SessionListArgs { agent, limit, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                SessionCommands::Inspect { session_id } => {
                    cli::session::run_inspect(
                        SessionInspectArgs { session_id, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                SessionCommands::Replay { session_id } => {
                    cli::session::run_replay(
                        SessionReplayArgs { session_id, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
            }
        }

        // ─── Phase 3: ghost identity ──────────────────────────────────────

        Commands::Identity(sub) => match sub {
            IdentityCommands::Init => {
                cli::identity::run_init(IdentityInitArgs {})
            }
            IdentityCommands::Show => {
                cli::identity::run_show(IdentityShowArgs {
                    output: cli_args.global.output,
                })
            }
            IdentityCommands::Drift => {
                cli::identity::run_drift(IdentityDriftArgs {})
            }
            IdentityCommands::Sign { file } => {
                cli::identity::run_sign(IdentitySignArgs { file })
            }
        },

        // ─── Phase 3: ghost secret ───────────────────────────────────────

        Commands::Secret(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let provider = ghost_gateway::config::build_secret_provider(&config.secrets)
                .map_err(|e| CliError::Config(format!("secret provider: {e}")))?;
            match sub {
                SecretCommands::Set { key } => {
                    cli::secret::run_set(SecretSetArgs { key }, &*provider)
                }
                SecretCommands::List => {
                    cli::secret::run_list(
                        SecretListArgs { output: cli_args.global.output },
                        &*provider,
                        &config.secrets.provider,
                    )
                }
                SecretCommands::Delete { key, yes } => {
                    cli::secret::run_delete(SecretDeleteArgs { key, yes }, &*provider)
                }
                SecretCommands::Provider => {
                    cli::secret::run_provider(
                        SecretProviderArgs { output: cli_args.global.output },
                        &config.secrets.provider,
                    )
                }
            }
        }

        // ─── Phase 3: ghost policy ───────────────────────────────────────

        Commands::Policy(sub) => match sub {
            PolicyCommands::Show => {
                cli::policy::run_show(PolicyShowArgs {
                    output: cli_args.global.output,
                })
            }
            PolicyCommands::Check { tool_name, agent } => {
                cli::policy::run_check(PolicyCheckArgs {
                    tool_name,
                    agent_id: agent,
                    output: cli_args.global.output,
                })
            }
            PolicyCommands::Lint => {
                cli::policy::run_lint(PolicyLintArgs {})
            }
        },

        // ─── Phase 4: ghost mesh ─────────────────────────────────────────

        Commands::Mesh(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let token = resolve_token();
            let backend = CliBackend::detect(
                &config,
                cli_args.global.gateway_url.as_deref(),
                token,
            )
            .await?;
            match sub {
                MeshCommands::Peers => {
                    cli::mesh::run_peers(
                        MeshPeersArgs { output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                MeshCommands::Trust => {
                    cli::mesh::run_trust(
                        MeshTrustArgs { output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                MeshCommands::Discover { url } => {
                    cli::mesh::run_discover(
                        MeshDiscoverArgs { url, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                MeshCommands::Ping { peer_id } => {
                    cli::mesh::run_ping(
                        MeshPingArgs { peer_id, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
            }
        }

        // ─── Phase 4: ghost skill ────────────────────────────────────────

        Commands::Skill(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let token = resolve_token();
            let backend = CliBackend::detect(
                &config,
                cli_args.global.gateway_url.as_deref(),
                token,
            )
            .await?;
            match sub {
                SkillCommands::List => {
                    cli::skill::run_list(
                        SkillListArgs { output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                SkillCommands::Install { path } => {
                    cli::skill::run_install(
                        SkillInstallArgs { path, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                SkillCommands::Inspect { name } => {
                    cli::skill::run_inspect(
                        SkillInspectArgs { name, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
            }
        }

        // ─── Phase 4: ghost channel ──────────────────────────────────────

        Commands::Channel(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            match sub {
                ChannelCommands::List => {
                    cli::channel::run_list(
                        ChannelListArgs { output: cli_args.global.output },
                        &config,
                    )
                    .await
                }
                ChannelCommands::Test { channel_type } => {
                    cli::channel::run_test(
                        ChannelTestArgs { channel_type, output: cli_args.global.output },
                        &config,
                    )
                    .await
                }
                ChannelCommands::Send { channel_type, message, agent, sender } => {
                    let token = resolve_token();
                    let backend = CliBackend::detect(
                        &config,
                        cli_args.global.gateway_url.as_deref(),
                        token,
                    )
                    .await?;
                    cli::channel::run_send(
                        ChannelSendArgs {
                            channel_type,
                            message,
                            agent,
                            sender,
                            output: cli_args.global.output,
                        },
                        &backend,
                    )
                    .await
                }
            }
        }

        // ─── Phase 4: ghost heartbeat ────────────────────────────────────

        Commands::Heartbeat(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let token = resolve_token();
            let backend = CliBackend::detect(
                &config,
                cli_args.global.gateway_url.as_deref(),
                token,
            )
            .await?;
            match sub {
                HeartbeatCommands::Status => {
                    cli::heartbeat::run_status(
                        HeartbeatStatusArgs { output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
            }
        }

        // ─── Phase 4: ghost cron ─────────────────────────────────────────

        Commands::Cron(sub) => {
            let config = load_config(cli_args.global.config.as_deref())?;
            let token = resolve_token();
            let backend = CliBackend::detect(
                &config,
                cli_args.global.gateway_url.as_deref(),
                token,
            )
            .await?;
            match sub {
                CronCommands::List => {
                    cli::cron::run_list(
                        CronListArgs { output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
                CronCommands::History { limit } => {
                    cli::cron::run_history(
                        CronHistoryArgs { limit, output: cli_args.global.output },
                        &backend,
                    )
                    .await
                }
            }
        }
    }
}
