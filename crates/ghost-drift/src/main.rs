//! Ghost Drift MCP server entry point.
//!
//! Serves drift tools over stdio transport for MCP clients.
//!
//! Usage:
//!   ghost-drift [--workspace <path>]

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use rmcp::{transport::stdio, ServiceExt};

use ghost_drift::storage::DriftDb;
use ghost_drift::DriftService;

#[derive(Parser)]
#[command(name = "ghost-drift", about = "Ghost Drift MCP server — codebase intelligence")]
struct Cli {
    /// Workspace directory (default: current directory).
    #[arg(long, default_value = ".")]
    workspace: String,

    /// Database path (default: <workspace>/.ghost/drift/drift.db).
    #[arg(long)]
    db_path: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();

    let workspace = PathBuf::from(&cli.workspace)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&cli.workspace));

    let db_path = match &cli.db_path {
        Some(p) => PathBuf::from(p),
        None => workspace.join(".ghost").join("drift").join("drift.db"),
    };

    tracing::info!(workspace = %workspace.display(), db = %db_path.display(), "Starting Ghost Drift MCP server");

    let db = Arc::new(DriftDb::open(&db_path)?);
    let service = DriftService::new(db, workspace);

    let running = service
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serve error: {e:?}"))?;

    running.waiting().await?;

    Ok(())
}
