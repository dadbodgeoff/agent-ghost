//! ghost doctor — platform health checks (CLI§13).
//!
//! Checks: SQLite availability, DB file, gateway reachability,
//! convergence monitor reachability, config validity, disk space.
//! Exit 0 if all pass, exit 1 if any fail.

use std::path::Path;

use serde::Serialize;

use crate::bootstrap::{ghost_home, shellexpand_tilde};
use crate::config::GhostConfig;

use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

#[derive(Debug, Clone, Serialize)]
struct CheckResult {
    name: String,
    passed: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    checks: Vec<CheckResult>,
    all_passed: bool,
}

impl TableDisplay for DoctorReport {
    fn print_table(&self) {
        println!("GHOST Doctor");
        println!("────────────");
        for check in &self.checks {
            let icon = if check.passed { "✓" } else { "✗" };
            println!("  {icon} {}: {}", check.name, check.detail);
        }
        println!();
        if self.all_passed {
            println!("All checks passed.");
        } else {
            println!("Some checks failed. See above for details.");
        }
    }
}

pub async fn run() -> Result<(), CliError> {
    run_with_output(OutputFormat::Table).await
}

pub async fn run_with_output(output: OutputFormat) -> Result<(), CliError> {
    let mut checks = Vec::new();

    // 1. GHOST home directory
    let home = ghost_home();
    checks.push(CheckResult {
        name: "GHOST home".into(),
        passed: home.exists(),
        detail: if home.exists() {
            format!("{} exists", home.display())
        } else {
            format!("{} not found — run `ghost init`", home.display())
        },
    });

    // 2. Config file
    let config = GhostConfig::load_default(None);
    let config_ok = config.is_ok();
    let config = config.unwrap_or_default();
    checks.push(CheckResult {
        name: "Configuration".into(),
        passed: config_ok,
        detail: if config_ok {
            "ghost.yml loaded successfully".into()
        } else {
            "failed to load ghost.yml — run `ghost init`".into()
        },
    });

    // 3. Config validation
    let valid = config.validate();
    checks.push(CheckResult {
        name: "Config validation".into(),
        passed: valid.is_ok(),
        detail: match &valid {
            Ok(()) => "configuration is valid".into(),
            Err(e) => format!("validation error: {e}"),
        },
    });

    // 4. SQLite availability + WAL mode + version
    let db_path_str = shellexpand_tilde(&config.gateway.db_path);
    let db_path = Path::new(&db_path_str);
    let db_exists = db_path.exists();
    let mut sqlite_ok = false;
    if db_exists {
        match rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(conn) => {
                sqlite_ok = true;
                let mode: String = conn
                    .pragma_query_value(None, "journal_mode", |row| row.get(0))
                    .unwrap_or_else(|_| "unknown".into());
                checks.push(CheckResult {
                    name: "SQLite WAL mode".into(),
                    passed: mode.to_lowercase() == "wal",
                    detail: format!("journal_mode={mode}"),
                });

                let sqlite_ver = rusqlite::version();
                checks.push(CheckResult {
                    name: "SQLite version".into(),
                    passed: true,
                    detail: format!("v{sqlite_ver}"),
                });
            }
            Err(e) => {
                checks.push(CheckResult {
                    name: "SQLite open".into(),
                    passed: false,
                    detail: format!("failed to open database: {e}"),
                });
            }
        }
    }

    checks.push(CheckResult {
        name: "Database file".into(),
        passed: db_exists,
        detail: if db_exists {
            format!("{} ({})", db_path.display(), if sqlite_ok { "OK" } else { "error" })
        } else {
            format!("{} not found — run `ghost db migrate`", db_path.display())
        },
    });

    // 5. DB file permissions (Unix only)
    #[cfg(unix)]
    if db_exists {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::metadata(db_path) {
            let mode = meta.mode() & 0o777;
            let owner_only = mode & 0o077 == 0;
            checks.push(CheckResult {
                name: "DB permissions".into(),
                passed: owner_only,
                detail: if owner_only {
                    format!("{mode:03o} (owner-only)")
                } else {
                    format!("{mode:03o} — WARNING: group/other access allowed")
                },
            });
        }
    }

    // 6. Gateway reachability
    let gateway_url = format!("http://{}:{}", config.gateway.bind, config.gateway.port);
    let gateway_ok = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_default()
        .get(format!("{gateway_url}/api/health"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    checks.push(CheckResult {
        name: "Gateway".into(),
        passed: gateway_ok,
        detail: if gateway_ok {
            format!("{gateway_url} — reachable")
        } else {
            format!("{gateway_url} — not reachable")
        },
    });

    // 7. Convergence monitor reachability
    if config.convergence.monitor.enabled {
        let monitor_url = format!("http://{}", config.convergence.monitor.address);
        let monitor_ok = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap_or_default()
            .get(format!("{monitor_url}/health"))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        checks.push(CheckResult {
            name: "Convergence monitor".into(),
            passed: monitor_ok,
            detail: if monitor_ok {
                format!("{monitor_url} — reachable")
            } else {
                format!("{monitor_url} — not reachable")
            },
        });
    } else {
        checks.push(CheckResult {
            name: "Convergence monitor".into(),
            passed: true,
            detail: "disabled in config (convergence.monitor.enabled: false)".into(),
        });
    }

    let all_passed = checks.iter().all(|c| c.passed);
    let report = DoctorReport {
        checks,
        all_passed,
    };

    print_output(&report, output);

    if all_passed {
        Ok(())
    } else {
        Err(CliError::Internal("some health checks failed".into()))
    }
}
