//! ghost config — configuration management (CLI§10).
//!
//! Subcommands: show (print resolved config, redacting secrets), validate.

use serde::Serialize;

use crate::config::GhostConfig;

use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

/// Regex-free secret field detection.
fn is_secret_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("key")
}

/// Redact secret values in a serde_json::Value tree.
fn redact_secrets(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if is_secret_key(key) {
                    if val.is_string() {
                        *val = serde_json::Value::String("********".into());
                    }
                } else {
                    redact_secrets(val);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_secrets(item);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Serialize)]
struct ConfigShowResponse {
    config: serde_json::Value,
    path: Option<String>,
}

impl TableDisplay for ConfigShowResponse {
    fn print_table(&self) {
        if let Some(ref p) = self.path {
            println!("Config path: {p}");
            println!();
        }
        if let Ok(yaml) = serde_yaml::to_string(&self.config) {
            print!("{yaml}");
        }
    }
}

#[derive(Debug, Serialize)]
struct ConfigValidateResponse {
    valid: bool,
    error: Option<String>,
}

impl TableDisplay for ConfigValidateResponse {
    fn print_table(&self) {
        if self.valid {
            println!("Configuration is valid.");
        } else if let Some(ref e) = self.error {
            println!("Configuration is invalid: {e}");
        }
    }
}

pub async fn run_show(config_path: Option<&str>, output: OutputFormat) -> Result<(), CliError> {
    let config = GhostConfig::load_default(config_path)
        .map_err(|e| CliError::Config(format!("failed to load config: {e}")))?;

    let mut json = serde_json::to_value(&config)
        .map_err(|e| CliError::Internal(format!("failed to serialize config: {e}")))?;
    redact_secrets(&mut json);

    // Resolve config path for display
    let resolved_path = config_path
        .map(String::from)
        .or_else(|| std::env::var("GHOST_CONFIG").ok())
        .or_else(|| {
            let home = crate::bootstrap::ghost_home();
            let p = home.join("config/ghost.yml");
            if p.exists() {
                Some(p.display().to_string())
            } else {
                None
            }
        });

    let resp = ConfigShowResponse {
        config: json,
        path: resolved_path,
    };
    print_output(&resp, output);
    Ok(())
}

pub async fn run_validate(config_path: Option<&str>, output: OutputFormat) -> Result<(), CliError> {
    let config = GhostConfig::load_default(config_path)
        .map_err(|e| CliError::Config(format!("failed to load config: {e}")))?;

    let result = config.validate();
    let resp = ConfigValidateResponse {
        valid: result.is_ok(),
        error: result.err().map(|e| e.to_string()),
    };
    print_output(&resp, output);

    if resp.valid {
        Ok(())
    } else {
        Err(CliError::Config(resp.error.unwrap_or_default()))
    }
}
