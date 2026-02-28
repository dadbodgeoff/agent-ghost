//! Convergence monitor sidecar binary.
//!
//! Independent process that ingests ITP events, computes convergence scores,
//! and triggers interventions across 5 levels (0-4).

use tracing_subscriber::EnvFilter;

mod config;
mod intervention;
mod monitor;
mod pipeline;
mod session;
mod state_publisher;
mod transport;
mod validation;
mod verification;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    tracing::info!("convergence-monitor starting");

    let config = config::MonitorConfig::load()?;
    let mut monitor = monitor::ConvergenceMonitor::new(config)?;
    monitor.run().await
}
