//! `format_code` — apply basic formatting to source code.
//!
//! Write skill. Gated to convergence Level 3, confirmation required.
//!
//! Applies in-process formatting: normalize line endings, remove trailing
//! whitespace, normalize indentation, ensure final newline. Does NOT shell
//! out to external formatters (rustfmt, prettier, etc.) — the skill executes
//! synchronously within the database lock scope.
//!
//! ## Input
//!
//! | Field          | Type   | Required | Default  |
//! |----------------|--------|----------|----------|
//! | `source_code`  | string | yes      | —        |
//! | `language`     | string | no       | unknown  |
//! | `indent_size`  | u64    | no       | 4        |
//! | `indent_style` | string | no       | spaces   |
//! | `trim_trailing`| bool   | no       | true     |
//! | `final_newline`| bool   | no       | true     |
//!
//! ## Output
//!
//! ```json
//! {
//!   "formatted": "...",
//!   "changes_made": true,
//!   "changes": ["removed trailing whitespace on 3 lines", ...]
//! }
//! ```

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Skill that applies basic formatting to source code.
pub struct FormatCodeSkill;

impl Skill for FormatCodeSkill {
    fn name(&self) -> &str {
        "format_code"
    }

    fn description(&self) -> &str {
        "Apply basic formatting to source code (whitespace, indentation, line endings)"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let source = input
            .get("source_code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'source_code' (string)".into())
            })?;

        let indent_size = input
            .get("indent_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(4) as usize;

        let indent_style = input
            .get("indent_style")
            .and_then(|v| v.as_str())
            .unwrap_or("spaces");

        let trim_trailing = input
            .get("trim_trailing")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let final_newline = input
            .get("final_newline")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut changes: Vec<String> = Vec::new();
        let mut formatted = String::with_capacity(source.len());

        // Normalize line endings (CRLF → LF).
        let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
        if normalized != source {
            changes.push("normalized line endings to LF".into());
        }

        // Process each line.
        let lines: Vec<&str> = normalized.lines().collect();
        let mut trailing_ws_count = 0;
        let mut indent_convert_count = 0;

        for line in &lines {
            let mut processed = (*line).to_string();

            // Trim trailing whitespace.
            if trim_trailing {
                let trimmed = processed.trim_end();
                if trimmed.len() < processed.len() {
                    trailing_ws_count += 1;
                    processed = trimmed.to_string();
                }
            }

            // Convert indentation style.
            let leading_ws: String = processed
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            let content = &processed[leading_ws.len()..];

            if !leading_ws.is_empty() && !content.is_empty() {
                let new_leading = match indent_style {
                    "tabs" => {
                        // Convert spaces to tabs.
                        let space_count = leading_ws.chars().filter(|c| *c == ' ').count();
                        let tab_count = leading_ws.chars().filter(|c| *c == '\t').count();
                        let total_tabs = tab_count + space_count / indent_size;
                        let remaining_spaces = space_count % indent_size;
                        if space_count > 0 {
                            indent_convert_count += 1;
                        }
                        format!(
                            "{}{}",
                            "\t".repeat(total_tabs),
                            " ".repeat(remaining_spaces)
                        )
                    }
                    _ => {
                        // "spaces" — convert tabs to spaces.
                        let has_tabs = leading_ws.contains('\t');
                        let new_ws: String = leading_ws
                            .chars()
                            .map(|c| {
                                if c == '\t' {
                                    " ".repeat(indent_size)
                                } else {
                                    c.to_string()
                                }
                            })
                            .collect();
                        if has_tabs {
                            indent_convert_count += 1;
                        }
                        new_ws
                    }
                };

                processed = format!("{new_leading}{content}");
            }

            formatted.push_str(&processed);
            formatted.push('\n');
        }

        if trailing_ws_count > 0 {
            changes.push(format!(
                "removed trailing whitespace on {trailing_ws_count} line(s)"
            ));
        }
        if indent_convert_count > 0 {
            changes.push(format!(
                "converted indentation on {indent_convert_count} line(s) to {indent_style}"
            ));
        }

        // Handle final newline.
        if final_newline {
            // Ensure exactly one trailing newline.
            let trimmed = formatted.trim_end_matches('\n');
            formatted = format!("{trimmed}\n");
        } else {
            // Remove trailing newlines.
            while formatted.ends_with('\n') {
                formatted.pop();
            }
        }

        let changes_made = !changes.is_empty() || formatted != source;

        Ok(serde_json::json!({
            "formatted": formatted,
            "changes_made": changes_made,
            "changes": changes,
        }))
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let style = input
            .get("indent_style")
            .and_then(|v| v.as_str())
            .unwrap_or("spaces");
        let size = input
            .get("indent_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(4);
        Some(format!(
            "Format code: {style} (indent={size}), trim trailing whitespace"
        ))
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
    fn removes_trailing_whitespace() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = FormatCodeSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {  \n    hello();  \n}\n",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["changes_made"], true);
        assert_eq!(val["formatted"], "fn main() {\n    hello();\n}\n");
    }

    #[test]
    fn converts_tabs_to_spaces() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = FormatCodeSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {\n\thello();\n}\n",
                "indent_style": "spaces",
                "indent_size": 4,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["formatted"], "fn main() {\n    hello();\n}\n");
    }

    #[test]
    fn normalizes_crlf() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = FormatCodeSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "line1\r\nline2\r\n",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(!val["formatted"].as_str().unwrap().contains('\r'));
    }

    #[test]
    fn ensures_final_newline() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = FormatCodeSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {}",
                "final_newline": true,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(val["formatted"].as_str().unwrap().ends_with('\n'));
    }

    #[test]
    fn no_changes_on_clean_code() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let clean = "fn main() {\n    hello();\n}\n";
        let result = FormatCodeSkill.execute(&ctx, &serde_json::json!({ "source_code": clean }));
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["formatted"], clean);
    }

    #[test]
    fn missing_source_returns_error() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = FormatCodeSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("source_code"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn preview_describes_formatting() {
        let preview = FormatCodeSkill.preview(&serde_json::json!({
            "indent_style": "tabs",
            "indent_size": 2,
        }));
        assert!(preview.unwrap().contains("tabs"));
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(FormatCodeSkill.name(), "format_code");
        assert!(FormatCodeSkill.removable());
    }
}
