//! ghost completions — shell completion generation (Task 6.6 — §4.1, §10.2).

use clap_complete::{generate, Shell};

use super::error::CliError;

/// Generate shell completions and print to stdout.
pub fn run(shell: Shell, cmd: &mut clap::Command) -> Result<(), CliError> {
    generate(shell, cmd, "ghost", &mut std::io::stdout());
    Ok(())
}
