//! ghost secret — secret management (T-3.3.1).

use ghost_secrets::SecretProvider;
use serde::Serialize;

use super::confirm::confirm;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

/// Well-known secret keys checked by `ghost secret list`.
const KNOWN_KEYS: &[&str] = &[
    "ghost_cli_token",
    "GHOST_TOKEN",
    "GHOST_JWT_SECRET",
    "GHOST_BACKUP_PASSPHRASE",
    "GHOST_BACKUP_KEY",
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "GHOST_TELEGRAM_BOT_TOKEN",
    "GHOST_SLACK_BOT_TOKEN",
    "GHOST_SLACK_APP_TOKEN",
    "GHOST_DISCORD_BOT_TOKEN",
    "GHOST_WHATSAPP_ACCESS_TOKEN",
];

// ─── ghost secret set ────────────────────────────────────────────────────────

pub struct SecretSetArgs {
    pub key: String,
}

pub fn run_set(args: SecretSetArgs, provider: &dyn SecretProvider) -> Result<(), CliError> {
    eprintln!("Enter value for '{}' (then press Enter):", args.key);
    let mut value = String::new();
    std::io::stdin()
        .read_line(&mut value)
        .map_err(|e| CliError::Internal(format!("failed to read stdin: {e}")))?;
    let value = value.trim_end_matches('\n').trim_end_matches('\r');

    if value.is_empty() {
        return Err(CliError::Usage("empty value — aborting".into()));
    }

    provider
        .set_secret(&args.key, value)
        .map_err(|e| match &e {
            ghost_secrets::SecretsError::StorageUnavailable(_) => CliError::Config(
                "cannot write secrets with the current provider (env is read-only). \
                     Configure `secrets.provider: keychain` in ghost.yml."
                    .to_string(),
            ),
            _ => CliError::Internal(format!("failed to set secret: {e}")),
        })?;

    println!("✓ Secret '{}' stored.", args.key);
    Ok(())
}

// ─── ghost secret list ───────────────────────────────────────────────────────

pub struct SecretListArgs {
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct SecretEntry {
    key: String,
    present: bool,
}

#[derive(Serialize)]
struct SecretListResult {
    entries: Vec<SecretEntry>,
    provider: String,
}

impl TableDisplay for SecretListResult {
    fn print_table(&self) {
        println!("Secret provider: {}", self.provider);
        println!();
        for entry in &self.entries {
            let icon = if entry.present { "✓" } else { "✗" };
            println!("  {icon} {}", entry.key);
        }
    }
}

pub fn run_list(
    args: SecretListArgs,
    provider: &dyn SecretProvider,
    provider_name: &str,
) -> Result<(), CliError> {
    let entries: Vec<SecretEntry> = KNOWN_KEYS
        .iter()
        .map(|&key| SecretEntry {
            key: key.to_string(),
            present: provider.has_secret(key),
        })
        .collect();

    let result = SecretListResult {
        entries,
        provider: provider_name.to_string(),
    };

    print_output(&result, args.output);
    Ok(())
}

// ─── ghost secret delete ─────────────────────────────────────────────────────

pub struct SecretDeleteArgs {
    pub key: String,
    pub yes: bool,
}

pub fn run_delete(args: SecretDeleteArgs, provider: &dyn SecretProvider) -> Result<(), CliError> {
    if !provider.has_secret(&args.key) {
        return Err(CliError::NotFound(format!(
            "secret '{}' not found in current provider",
            args.key
        )));
    }

    if !confirm(&format!("Delete secret '{}'?", args.key), args.yes) {
        return Err(CliError::Cancelled);
    }

    provider.delete_secret(&args.key).map_err(|e| match &e {
        ghost_secrets::SecretsError::StorageUnavailable(_) => CliError::Config(
            "cannot delete secrets with the current provider (env is read-only)".into(),
        ),
        _ => CliError::Internal(format!("failed to delete secret: {e}")),
    })?;

    println!("✓ Secret '{}' deleted.", args.key);
    Ok(())
}

// ─── ghost secret provider ──────────────────────────────────────────────────

pub struct SecretProviderArgs {
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct ProviderInfo {
    provider: String,
}

impl TableDisplay for ProviderInfo {
    fn print_table(&self) {
        println!("Active secret provider: {}", self.provider);
    }
}

pub fn run_provider(args: SecretProviderArgs, provider_name: &str) -> Result<(), CliError> {
    let info = ProviderInfo {
        provider: provider_name.to_string(),
    };
    print_output(&info, args.output);
    Ok(())
}
