//! Output formatting and color helpers (Task 6.6 — §6, E.4, E.5, F.18).

use serde::Serialize;
use std::io::IsTerminal;

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table (default).
    Table,
    /// Pretty-printed JSON.
    Json,
    /// One JSON object per line (NDJSON).
    #[value(alias = "jsonl", alias = "ndjson")]
    JsonLines,
    /// YAML output.
    Yaml,
}

/// Trait for types that can render themselves as a human-readable table.
pub trait TableDisplay {
    fn print_table(&self);
}

/// Print a value in the requested output format.
pub fn print_output<T: Serialize + TableDisplay>(value: &T, format: OutputFormat) {
    match format {
        OutputFormat::Table => value.print_table(),
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(value) {
                println!("{json}");
            }
        }
        OutputFormat::JsonLines => {
            if let Ok(json) = serde_json::to_string(value) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(value) {
                print!("{yaml}");
            }
        }
    }
}

/// Color output preference.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ColorChoice {
    /// Auto-detect (color if terminal, respect NO_COLOR/FORCE_COLOR).
    Auto,
    /// Always emit ANSI color codes.
    Always,
    /// Never emit ANSI color codes.
    Never,
}

/// Determine whether to emit ANSI color codes.
pub fn should_colorize(choice: ColorChoice) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            if std::env::var("NO_COLOR").is_ok() {
                return false;
            }
            if std::env::var("FORCE_COLOR").is_ok() {
                return true;
            }
            std::io::stdout().is_terminal()
        }
    }
}

/// Wrap a string in red ANSI escape codes.
pub fn red(s: &str) -> String {
    format!("\x1b[31m{s}\x1b[0m")
}

/// Wrap a string in yellow ANSI escape codes.
pub fn yellow(s: &str) -> String {
    format!("\x1b[33m{s}\x1b[0m")
}

/// Wrap a string in green ANSI escape codes.
pub fn green(s: &str) -> String {
    format!("\x1b[32m{s}\x1b[0m")
}
