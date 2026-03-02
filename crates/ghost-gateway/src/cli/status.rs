//! Status query command (Task 6.6).

use serde::Serialize;

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

/// Gateway and platform status.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub gateway: String,
    pub health: Option<String>,
    pub monitor: String,
    pub config_path: Option<String>,
}

impl TableDisplay for StatusResponse {
    fn print_table(&self) {
        println!("GHOST Platform Status");
        println!("─────────────────────");
        println!("Gateway:  {}", self.gateway);
        if let Some(ref h) = self.health {
            println!("Health:   {h}");
        }
        println!("Monitor:  {}", self.monitor);
        if let Some(ref p) = self.config_path {
            println!("Config:   {p}");
        }
    }
}

/// Query and display gateway status.
pub async fn show_status(
    base_url: &str,
    config_path: Option<&str>,
    output: OutputFormat,
) -> Result<(), CliError> {
    let gateway;
    let mut health = None;

    match reqwest::get(format!("{base_url}/api/health")).await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                gateway = "RUNNING".to_string();
                health = resp.text().await.ok();
            } else {
                gateway = format!("ERROR (HTTP {status})");
            }
        }
        Err(_) => {
            gateway = "NOT RUNNING".to_string();
        }
    }

    // Try monitor health. Derive monitor URL from base by replacing port.
    let monitor_url = base_url
        .replace(":18789", ":18790")
        .replace("/api", "");
    let monitor = match reqwest::get(format!("{monitor_url}/health")).await {
        Ok(resp) if resp.status().is_success() => "CONNECTED".to_string(),
        _ => "NOT AVAILABLE".to_string(),
    };

    let resp = StatusResponse {
        gateway,
        health,
        monitor,
        config_path: config_path.map(String::from),
    };

    print_output(&resp, output);
    Ok(())
}
