//! `get_diagnostics` — detect syntax issues in source code.
//!
//! Read-only skill. Always available (convergence max = Level 4).
//!
//! Performs basic syntax checking: unmatched brackets, trailing whitespace,
//! long lines, TODO/FIXME detection. Uses state-tracking bracket matching
//! that respects strings and comments.
//!
//! ## Input
//!
//! | Field         | Type   | Required                     | Default     |
//! |---------------|--------|------------------------------|-------------|
//! | `file_path`   | string | yes (or `source_code`)       | —           |
//! | `source_code` | string | yes (or `file_path`)         | —           |
//! | `language`    | string | no                           | auto-detect |
//! | `max_line_length` | u64 | no                          | 120         |
//!
//! ## Output
//!
//! ```json
//! {
//!   "diagnostics": [{ "line": 5, "column": 1, "severity": "error", "message": "..." }],
//!   "error_count": 1,
//!   "warning_count": 2,
//!   "info_count": 1
//! }
//! ```

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillResult};

use super::read_source;

/// Skill that detects syntax issues in source code.
pub struct GetDiagnosticsSkill;

impl Skill for GetDiagnosticsSkill {
    fn name(&self) -> &str {
        "get_diagnostics"
    }

    fn description(&self) -> &str {
        "Detect syntax issues in source code (unmatched brackets, long lines, TODOs)"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let (source, language) = read_source(input)?;

        let max_line_length = input
            .get("max_line_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(120) as usize;

        let mut diagnostics = Vec::new();

        // Check bracket matching.
        check_brackets(&source, &language, &mut diagnostics);

        // Check line-level diagnostics.
        for (line_idx, line) in source.lines().enumerate() {
            let line_num = line_idx + 1;

            // Trailing whitespace.
            if line.ends_with(' ') || line.ends_with('\t') {
                diagnostics.push(serde_json::json!({
                    "line": line_num,
                    "column": line.len(),
                    "severity": "warning",
                    "message": "trailing whitespace",
                }));
            }

            // Long lines.
            if line.len() > max_line_length {
                diagnostics.push(serde_json::json!({
                    "line": line_num,
                    "column": max_line_length + 1,
                    "severity": "warning",
                    "message": format!("line exceeds {} characters ({})", max_line_length, line.len()),
                }));
            }

            // TODO/FIXME markers.
            let trimmed = line.trim();
            if trimmed.contains("TODO") || trimmed.contains("FIXME") || trimmed.contains("HACK") {
                let marker = if trimmed.contains("FIXME") {
                    "FIXME"
                } else if trimmed.contains("HACK") {
                    "HACK"
                } else {
                    "TODO"
                };
                diagnostics.push(serde_json::json!({
                    "line": line_num,
                    "column": line.find(marker).unwrap_or(0) + 1,
                    "severity": "info",
                    "message": format!("{marker} marker found"),
                }));
            }
        }

        // Count by severity.
        let error_count = diagnostics
            .iter()
            .filter(|d| d["severity"] == "error")
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d["severity"] == "warning")
            .count();
        let info_count = diagnostics
            .iter()
            .filter(|d| d["severity"] == "info")
            .count();

        // Sort by line, then severity (errors first).
        diagnostics.sort_by(|a, b| {
            let line_a = a["line"].as_u64().unwrap_or(0);
            let line_b = b["line"].as_u64().unwrap_or(0);
            line_a.cmp(&line_b).then_with(|| {
                severity_order(a["severity"].as_str().unwrap_or(""))
                    .cmp(&severity_order(b["severity"].as_str().unwrap_or("")))
            })
        });

        Ok(serde_json::json!({
            "diagnostics": diagnostics,
            "error_count": error_count,
            "warning_count": warning_count,
            "info_count": info_count,
            "language": language,
        }))
    }
}

fn severity_order(severity: &str) -> u8 {
    match severity {
        "error" => 0,
        "warning" => 1,
        "info" => 2,
        _ => 3,
    }
}

/// Check for unmatched brackets: `{`, `}`, `(`, `)`, `[`, `]`.
///
/// Tracks state to skip brackets inside string literals, character literals,
/// and single-line comments. Multi-line comments and raw strings are
/// handled on a best-effort basis.
fn check_brackets(source: &str, language: &str, diagnostics: &mut Vec<serde_json::Value>) {
    let single_line_comment = match language {
        "python" => "#",
        _ => "//",
    };

    // Stack of (char, line_num, col).
    let mut stack: Vec<(char, usize, usize)> = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = line_idx + 1;
        let mut chars = line.char_indices().peekable();
        let mut in_string = false;
        let mut string_delim = '"';

        while let Some((col, ch)) = chars.next() {
            // Check for single-line comment start.
            if !in_string {
                let remaining = &line[col..];
                if remaining.starts_with(single_line_comment) {
                    break; // Rest of line is a comment.
                }
            }

            // String state tracking.
            if !in_string && (ch == '"' || ch == '\'') {
                // In Rust, single quotes are char literals, not strings.
                // But for bracket matching, skipping their contents is fine.
                in_string = true;
                string_delim = ch;
                continue;
            }
            if in_string {
                if ch == '\\' {
                    // Skip escaped character.
                    chars.next();
                    continue;
                }
                if ch == string_delim {
                    in_string = false;
                }
                continue;
            }

            // Bracket matching.
            match ch {
                '{' | '(' | '[' => {
                    stack.push((ch, line_num, col + 1));
                }
                '}' | ')' | ']' => {
                    let expected = match ch {
                        '}' => '{',
                        ')' => '(',
                        ']' => '[',
                        _ => unreachable!(),
                    };
                    match stack.last() {
                        Some(&(open, _, _)) if open == expected => {
                            stack.pop();
                        }
                        Some(&(open, open_line, open_col)) => {
                            diagnostics.push(serde_json::json!({
                                "line": line_num,
                                "column": col + 1,
                                "severity": "error",
                                "message": format!(
                                    "mismatched bracket: found '{ch}' but expected closing for \
                                     '{open}' opened at line {open_line}:{open_col}"
                                ),
                            }));
                            stack.pop(); // Pop the mismatched open bracket.
                        }
                        None => {
                            diagnostics.push(serde_json::json!({
                                "line": line_num,
                                "column": col + 1,
                                "severity": "error",
                                "message": format!("unmatched closing bracket '{ch}'"),
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Report any unclosed brackets.
    for (ch, line, col) in stack {
        diagnostics.push(serde_json::json!({
            "line": line,
            "column": col,
            "severity": "error",
            "message": format!("unclosed bracket '{ch}'"),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillContext;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    #[test]
    fn clean_code_no_diagnostics() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {\n    println!(\"hello\");\n}\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["error_count"], 0);
    }

    #[test]
    fn detects_unclosed_brace() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {\n    println!(\"hello\");\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(val["error_count"].as_u64().unwrap() > 0);

        let diags = val["diagnostics"].as_array().unwrap();
        let errors: Vec<_> = diags.iter().filter(|d| d["severity"] == "error").collect();
        assert!(!errors.is_empty());
        assert!(errors[0]["message"].as_str().unwrap().contains("unclosed"));
    }

    #[test]
    fn detects_unmatched_closing() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() }\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(val["error_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn detects_trailing_whitespace() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {  \n}\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(val["warning_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn detects_long_lines() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let long_line = format!("let x = \"{}\";", "a".repeat(200));
        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": long_line,
                "language": "rust",
                "max_line_length": 120,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(val["warning_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn detects_todo_markers() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "// TODO: implement this\n// FIXME: broken\nfn main() {}\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["info_count"], 2);
    }

    #[test]
    fn ignores_brackets_in_strings() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "let s = \"{ not a bracket }\";\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["error_count"], 0);
    }

    #[test]
    fn ignores_brackets_in_comments() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GetDiagnosticsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "// { not counted\nfn main() {}\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["error_count"], 0);
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GetDiagnosticsSkill.name(), "get_diagnostics");
        assert!(GetDiagnosticsSkill.removable());
    }
}
