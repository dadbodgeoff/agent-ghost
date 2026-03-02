//! ghost identity — identity and signing (T-3.2.1–T-3.2.4).

use std::path::PathBuf;

use serde::Serialize;

use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Resolve the platform keys directory: `~/.ghost/agents/platform/keys/`.
fn keys_dir() -> PathBuf {
    crate::bootstrap::ghost_home()
        .join("agents")
        .join("platform")
        .join("keys")
}

/// Resolve the SOUL.md path: `~/.ghost/config/SOUL.md`.
fn soul_path() -> PathBuf {
    crate::bootstrap::ghost_home().join("config").join("SOUL.md")
}

/// Resolve the baseline hash file: `~/.ghost/config/.soul_baseline`.
fn baseline_path() -> PathBuf {
    crate::bootstrap::ghost_home()
        .join("config")
        .join(".soul_baseline")
}

/// Compute a hex-encoded blake3 fingerprint of a verifying key.
fn key_fingerprint(vk: &ghost_signing::VerifyingKey) -> String {
    let hash = blake3::hash(&vk.to_bytes());
    hex_encode(hash.as_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ─── ghost identity init (T-3.2.1) ──────────────────────────────────────────

pub struct IdentityInitArgs {}

pub fn run_init(_args: IdentityInitArgs) -> Result<(), CliError> {
    let soul = soul_path();
    let kdir = keys_dir();

    // Create SOUL.md if it doesn't exist.
    if soul.exists() {
        eprintln!("SOUL.md already exists at {}", soul.display());
    } else {
        if let Some(parent) = soul.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CliError::Config(format!("failed to create config dir: {e}")))?;
        }
        ghost_identity::soul_manager::SoulManager::create_template(&soul)
            .map_err(|e| CliError::Config(format!("failed to create SOUL.md: {e}")))?;
        println!("✓ Created SOUL.md at {}", soul.display());
    }

    // Generate platform keypair.
    std::fs::create_dir_all(&kdir)
        .map_err(|e| CliError::Config(format!("failed to create keys dir: {e}")))?;

    let mut keymgr = ghost_identity::keypair_manager::AgentKeypairManager::new(kdir.clone());
    let vk = keymgr
        .generate()
        .map_err(|e| CliError::Internal(format!("keypair generation failed: {e}")))?;

    let fp = key_fingerprint(vk);
    println!("✓ Generated platform keypair in {}", kdir.display());
    println!("  Fingerprint: {fp}");

    // Store baseline hash for drift detection.
    let mut soul_mgr = ghost_identity::soul_manager::SoulManager::new();
    if let Ok(doc) = soul_mgr.load(&soul) {
        std::fs::write(baseline_path(), doc.hash)
            .map_err(|e| CliError::Config(format!("failed to write baseline: {e}")))?;
        println!("✓ Stored SOUL.md baseline hash for drift detection.");
    }

    Ok(())
}

// ─── ghost identity show (T-3.2.2) ──────────────────────────────────────────

pub struct IdentityShowArgs {
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct IdentityShowResult {
    soul_summary: Vec<String>,
    soul_path: String,
    key_fingerprint: Option<String>,
    keys_dir: String,
}

impl TableDisplay for IdentityShowResult {
    fn print_table(&self) {
        println!("SOUL.md ({})", self.soul_path);
        for line in &self.soul_summary {
            println!("  {line}");
        }
        println!();
        if let Some(fp) = &self.key_fingerprint {
            println!("Platform key fingerprint: {fp}");
        } else {
            println!("Platform key: not found (run `ghost identity init`)");
        }
    }
}

pub fn run_show(args: IdentityShowArgs) -> Result<(), CliError> {
    let soul = soul_path();
    let kdir = keys_dir();

    // Load SOUL.md.
    let mut soul_mgr = ghost_identity::soul_manager::SoulManager::new();
    let doc = soul_mgr
        .load(&soul)
        .map_err(|e| CliError::Config(format!("failed to load SOUL.md: {e}")))?;

    let summary: Vec<String> = doc.content.lines().take(5).map(String::from).collect();

    // Load public key.
    let keymgr = ghost_identity::keypair_manager::AgentKeypairManager::new(kdir.clone());
    let fp = keymgr.load_verifying_key().ok().map(|vk| key_fingerprint(&vk));

    let result = IdentityShowResult {
        soul_summary: summary,
        soul_path: soul.display().to_string(),
        key_fingerprint: fp,
        keys_dir: kdir.display().to_string(),
    };

    print_output(&result, args.output);
    Ok(())
}

// ─── ghost identity drift (T-3.2.3) ─────────────────────────────────────────

pub struct IdentityDriftArgs {}

pub fn run_drift(_args: IdentityDriftArgs) -> Result<(), CliError> {
    let soul = soul_path();
    let baseline = baseline_path();

    // Load baseline hash.
    if !baseline.exists() {
        eprintln!("No baseline found. Run `ghost identity init` first.");
        return Err(CliError::Config("no SOUL.md baseline hash stored".into()));
    }

    let stored_hash: [u8; 32] = std::fs::read(&baseline)
        .map_err(|e| CliError::Config(format!("failed to read baseline: {e}")))?
        .try_into()
        .map_err(|_| CliError::Config("baseline file is corrupt (expected 32 bytes)".into()))?;

    // Load current SOUL.md and compute hash.
    let mut soul_mgr = ghost_identity::soul_manager::SoulManager::new();
    let doc = soul_mgr
        .load(&soul)
        .map_err(|e| CliError::Config(format!("failed to load SOUL.md: {e}")))?;

    if doc.hash == stored_hash {
        println!("✓ No drift detected — SOUL.md matches baseline.");
    } else {
        eprintln!("✗ SOUL.md has changed (hash mismatch).");
        eprintln!(
            "  Baseline: {}",
            hex_encode(&stored_hash)
        );
        eprintln!(
            "  Current:  {}",
            hex_encode(&doc.hash)
        );
        eprintln!("  Run `ghost identity init` to update the baseline.");
    }

    Ok(())
}

// ─── ghost identity sign (T-3.2.4) ──────────────────────────────────────────

pub struct IdentitySignArgs {
    pub file: String,
}

pub fn run_sign(args: IdentitySignArgs) -> Result<(), CliError> {
    let kdir = keys_dir();

    // Load signing key.
    let mut keymgr = ghost_identity::keypair_manager::AgentKeypairManager::new(kdir);
    // Attempt to generate (loads existing if present, or generates new).
    keymgr
        .generate()
        .map_err(|e| CliError::Internal(format!("failed to load keypair: {e}")))?;

    let signing_key = keymgr
        .signing_key()
        .ok_or_else(|| CliError::Internal("no signing key available".into()))?;

    // Read file.
    let data = std::fs::read(&args.file)
        .map_err(|e| CliError::Config(format!("failed to read '{}': {e}", args.file)))?;

    // Sign.
    let signature = ghost_signing::sign(&data, signing_key);

    // Output base64-encoded signature.
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
    println!("{encoded}");

    Ok(())
}
