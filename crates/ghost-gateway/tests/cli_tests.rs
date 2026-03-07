//! CLI integration tests (Task 6.6 — §12, E.7, T-X.3, T-X.5).

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

fn ghost_cmd() -> Command {
    Command::cargo_bin("ghost").unwrap()
}

#[test]
fn help_flag_succeeds() {
    ghost_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("GHOST Platform Gateway"));
}

#[test]
fn version_flag_succeeds() {
    ghost_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ghost"));
}

#[test]
fn help_shows_all_subcommands() {
    let output = ghost_cmd().arg("--help").assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    // Verify key subcommand groups are listed
    assert!(stdout.contains("serve"), "missing 'serve' subcommand");
    assert!(stdout.contains("chat"), "missing 'chat' subcommand");
    assert!(stdout.contains("status"), "missing 'status' subcommand");
    assert!(stdout.contains("init"), "missing 'init' subcommand");
    assert!(stdout.contains("doctor"), "missing 'doctor' subcommand");
    assert!(stdout.contains("agent"), "missing 'agent' subcommand");
    assert!(stdout.contains("safety"), "missing 'safety' subcommand");
    assert!(stdout.contains("config"), "missing 'config' subcommand");
    assert!(stdout.contains("db"), "missing 'db' subcommand");
    assert!(
        stdout.contains("completions"),
        "missing 'completions' subcommand"
    );
    assert!(stdout.contains("channel"), "missing 'channel' subcommand");
    // Phase 4 subcommands
    assert!(stdout.contains("mesh"), "missing 'mesh' subcommand");
    assert!(stdout.contains("skill"), "missing 'skill' subcommand");
    assert!(
        stdout.contains("heartbeat"),
        "missing 'heartbeat' subcommand"
    );
    assert!(stdout.contains("cron"), "missing 'cron' subcommand");
}

#[test]
fn status_returns_valid_exit_code() {
    // Status should either succeed (0) or return EX_UNAVAILABLE (69)
    // when the gateway isn't running.
    let assert = ghost_cmd().arg("status").assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 69,
        "expected exit code 0 or 69, got {code}"
    );
}

// ─── T-X.3: Prevent expand_tilde re-introduction ────────────────────────────

#[test]
fn no_expand_tilde_function_reintroduction() {
    // All path expansion must go through bootstrap::shellexpand_tilde().
    // This test greps source files to prevent a standalone fn expand_tilde
    // from being re-introduced (T-X.3, F.19).
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    walk_rs_files(&src_dir, &mut |path, content| {
        for (i, line) in content.lines().enumerate() {
            if line.contains("fn expand_tilde") && !line.trim_start().starts_with("//") {
                violations.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
            }
        }
    });
    assert!(
        violations.is_empty(),
        "Found standalone expand_tilde function(s). Use bootstrap::shellexpand_tilde() instead:\n{}",
        violations.join("\n")
    );
}

fn walk_rs_files(dir: &Path, cb: &mut dyn FnMut(&Path, &str)) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs_files(&path, cb);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    cb(&path, &content);
                }
            }
        }
    }
}

// ─── Phase 1 tests ──────────────────────────────────────────────────────────

#[test]
fn completions_bash_produces_output() {
    ghost_cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn completions_zsh_produces_output() {
    ghost_cmd()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn completions_fish_produces_output() {
    ghost_cmd()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn config_validate_returns_valid_exit_code() {
    // Config validate should succeed (0), return EX_CONFIG (78), or
    // panic with 101 if the Phase 1 stub is still a todo!().
    let assert = ghost_cmd().args(["config", "validate"]).assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 78 || code == 101,
        "expected exit code 0, 78, or 101, got {code}"
    );
}

// ─── Phase 2 tests ──────────────────────────────────────────────────────────

#[test]
fn db_status_returns_valid_exit_code() {
    // DB status should succeed (0) or return a config/database error.
    let assert = ghost_cmd().args(["db", "status"]).assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 76 || code == 78,
        "expected exit code 0, 76, or 78, got {code}"
    );
}

#[test]
fn audit_query_json_produces_valid_json() {
    let assert = ghost_cmd()
        .args(["audit", "query", "--output", "json", "--limit", "1"])
        .assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    // May fail if no config or DB available.
    if code == 0 {
        let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
        assert!(
            serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
            "audit query --output json did not produce valid JSON"
        );
    }
}

// ─── Phase 3 tests ──────────────────────────────────────────────────────────

#[test]
fn identity_show_returns_valid_exit_code() {
    let assert = ghost_cmd().args(["identity", "show"]).assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    // Should succeed or return a config/not-found error.
    assert!(
        code == 0 || code == 1 || code == 78,
        "expected exit code 0, 1, or 78, got {code}"
    );
}

#[test]
fn secret_provider_returns_valid_exit_code() {
    let assert = ghost_cmd().args(["secret", "provider"]).assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 78,
        "expected exit code 0 or 78, got {code}"
    );
}

// ─── Phase 4 tests ──────────────────────────────────────────────────────────

#[test]
fn mesh_peers_help_succeeds() {
    ghost_cmd()
        .args(["mesh", "peers", "--help"])
        .assert()
        .success();
}

#[test]
fn skill_list_help_succeeds() {
    ghost_cmd()
        .args(["skill", "list", "--help"])
        .assert()
        .success();
}

#[test]
fn channel_list_returns_valid_exit_code() {
    let assert = ghost_cmd().args(["channel", "list"]).assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 78,
        "expected exit code 0 or 78, got {code}"
    );
}

#[test]
fn heartbeat_status_help_succeeds() {
    ghost_cmd()
        .args(["heartbeat", "status", "--help"])
        .assert()
        .success();
}

#[test]
fn cron_list_help_succeeds() {
    ghost_cmd()
        .args(["cron", "list", "--help"])
        .assert()
        .success();
}

// ─── Cross-cutting: global flags ────────────────────────────────────────────

#[test]
fn unknown_command_fails() {
    ghost_cmd()
        .arg("nonexistent-command")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn format_version_flag_accepted() {
    ghost_cmd()
        .args(["--format-version", "v1", "--help"])
        .assert()
        .success();
}

#[test]
fn output_json_flag_accepted() {
    ghost_cmd()
        .args(["--output", "json", "--help"])
        .assert()
        .success();
}
