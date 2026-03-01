//! ghost-gateway binary entry point (Task 6.6 — CLI subcommands).

use clap::Parser;
use ghost_gateway::bootstrap::GatewayBootstrap;
use ghost_gateway::cli;

#[derive(Parser)]
#[command(name = "ghost", about = "GHOST Platform Gateway", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to ghost.yml configuration file.
    #[arg(long, short, global = true)]
    config: Option<String>,
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
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli_args = Cli::parse();

    match cli_args.command.unwrap_or(Commands::Serve) {
        Commands::Serve => {
            let result = GatewayBootstrap::run(cli_args.config.as_deref()).await;
            match result {
                Ok((gateway, config)) => {
                    let router = GatewayBootstrap::build_router(&config);
                    if let Err(e) = gateway.run_with_router(Some(router), None).await {
                        tracing::error!(error = %e, "Gateway exited with error");
                        std::process::exit(70); // EX_SOFTWARE
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Bootstrap failed");
                    std::process::exit(e.exit_code());
                }
            }
        }
        Commands::Chat => {
            cli::chat::run_interactive_chat().await;
        }
        Commands::Status => {
            cli::status::show_status(cli_args.config.as_deref()).await;
        }
        Commands::Backup { output } => {
            cli::commands::run_backup(output.as_deref());
        }
        Commands::Export { path } => {
            cli::commands::run_export(&path);
        }
        Commands::Migrate { source } => {
            cli::commands::run_migrate(&source);
        }
    }
}
