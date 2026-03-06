//! `find_references` — find all occurrences of a symbol in source code.
//!
//! Read-only skill. Always available (convergence max = Level 4).
//!
//! Uses word-boundary regex matching to find all occurrences of a symbol.
//! Optionally distinguishes definitions from references.
//!
//! ## Input
//!
//! | Field                 | Type   | Required                     | Default     |
//! |-----------------------|--------|------------------------------|-------------|
//! | `file_path`           | string | yes (or `source_code`)       | —           |
//! | `source_code`         | string | yes (or `file_path`)         | —           |
//! | `symbol`              | string | yes                          | —           |
//! | `language`            | string | no                           | auto-detect |
//! | `include_definitions` | bool   | no                           | true        |
//!
//! ## Output
//!
//! ```json
//! {
//!   "references": [{
//!     "line": 5,
//!     "column": 4,
//!     "context": "    process_data(input);",
//!     "is_definition": false
//!   }],
//!   "total": 3
//! }
//! ```

use regex::Regex;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::{patterns_for_language, read_source};

/// Skill that finds all occurrences of a symbol in source code.
pub struct FindReferencesSkill;

impl Skill for FindReferencesSkill {
    fn name(&self) -> &str {
        "find_references"
    }

    fn description(&self) -> &str {
        "Find all occurrences of a symbol in source code"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let (source, language) = read_source(input)?;

        let symbol = input
            .get("symbol")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'symbol' (string)".into())
            })?;

        if symbol.is_empty() {
            return Err(SkillError::InvalidInput(
                "'symbol' must not be empty".into(),
            ));
        }

        let include_definitions = input
            .get("include_definitions")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Build word-boundary regex for the symbol.
        let escaped = regex::escape(symbol);
        let pattern = format!(r"\b{escaped}\b");
        let re = Regex::new(&pattern).map_err(|e| {
            SkillError::Internal(format!("failed to compile regex: {e}"))
        })?;

        // Collect definition patterns for this language.
        let def_patterns = definition_patterns(&language, symbol);

        let mut references = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            for mat in re.find_iter(line) {
                let column = mat.start() + 1; // 1-based

                let is_definition = def_patterns
                    .iter()
                    .any(|def_re| def_re.is_match(line));

                if !include_definitions && is_definition {
                    continue;
                }

                references.push(serde_json::json!({
                    "line": line_num,
                    "column": column,
                    "context": line.trim(),
                    "is_definition": is_definition,
                }));
            }
        }

        let total = references.len();
        Ok(serde_json::json!({
            "references": references,
            "total": total,
            "symbol": symbol,
            "language": language,
        }))
    }
}

/// Build regex patterns that identify definitions of the given symbol.
fn definition_patterns(language: &str, symbol: &str) -> Vec<Regex> {
    let escaped = regex::escape(symbol);
    let patterns = patterns_for_language(language);

    // Replace the generic capture group with the specific symbol name.
    let definition_strs: Vec<String> = match language {
        "rust" => vec![
            format!(r"(?:pub(?:\([\w:]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?fn\s+{escaped}\b"),
            format!(r"(?:pub(?:\([\w:]+\))?\s+)?struct\s+{escaped}\b"),
            format!(r"(?:pub(?:\([\w:]+\))?\s+)?(?:enum|trait)\s+{escaped}\b"),
            format!(r"(?:pub(?:\([\w:]+\))?\s+)?(?:const|static)\s+{escaped}\b"),
            format!(r"(?:pub(?:\([\w:]+\))?\s+)?(?:mod|type)\s+{escaped}\b"),
            format!(r"let(?:\s+mut)?\s+{escaped}\b"),
        ],
        "python" => vec![
            format!(r"(?:async\s+)?def\s+{escaped}\b"),
            format!(r"class\s+{escaped}\b"),
            format!(r"^{escaped}\s*="),
        ],
        "javascript" | "jsx" | "typescript" | "tsx" => vec![
            format!(r"(?:export\s+)?(?:async\s+)?function\s+{escaped}\b"),
            format!(r"(?:export\s+)?class\s+{escaped}\b"),
            format!(r"(?:export\s+)?(?:const|let|var)\s+{escaped}\b"),
            format!(r"(?:export\s+)?(?:interface|type|enum)\s+{escaped}\b"),
        ],
        "go" => vec![
            format!(r"func\s+(?:\([^)]+\)\s+)?{escaped}\b"),
            format!(r"type\s+{escaped}\s+(?:struct|interface)\b"),
            format!(r"(?:const|var)\s+{escaped}\b"),
        ],
        _ => vec![
            format!(r"(?:fn|func|function|def)\s+{escaped}\b"),
            format!(r"(?:struct|class)\s+{escaped}\b"),
            format!(r"(?:enum|interface|trait)\s+{escaped}\b"),
            format!(r"(?:const|let|var|static)\s+{escaped}\b"),
        ],
    };

    // Compile each pattern, skipping any that fail.
    // The generic patterns from `patterns_for_language` are used as a fallback reference
    // but the actual definition detection uses the symbol-specific patterns above.
    let _ = patterns; // Used for the language detection logic, not needed here.
    definition_strs
        .into_iter()
        .filter_map(|p| Regex::new(&p).ok())
        .collect()
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
    fn finds_all_occurrences() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = concat!(
            "fn process(x: i32) -> i32 {\n",
            "    let result = process(x - 1);\n",
            "    result\n",
            "}\n",
        );

        let result = FindReferencesSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "symbol": "process",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["total"], 2); // Definition + usage
    }

    #[test]
    fn distinguishes_definitions_from_references() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = concat!(
            "fn helper(x: i32) -> i32 {\n",
            "    x + 1\n",
            "}\n\n",
            "fn main() {\n",
            "    let y = helper(5);\n",
            "}\n",
        );

        let result = FindReferencesSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "symbol": "helper",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let refs = val["references"].as_array().unwrap();
        assert_eq!(refs.len(), 2);

        // First occurrence should be a definition.
        assert_eq!(refs[0]["is_definition"], true);
        // Second should be a reference.
        assert_eq!(refs[1]["is_definition"], false);
    }

    #[test]
    fn excludes_definitions_when_requested() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = "fn foo() {}\nfn bar() { foo(); }\n";

        let result = FindReferencesSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "symbol": "foo",
                "language": "rust",
                "include_definitions": false,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let refs = val["references"].as_array().unwrap();
        // Only the call site, not the definition.
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["is_definition"], false);
    }

    #[test]
    fn does_not_match_substrings() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = "fn foo_bar() {}\nfn foo() {}\n";

        let result = FindReferencesSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "symbol": "foo",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        // Should NOT match "foo_bar", only "foo".
        assert_eq!(val["total"], 1);
    }

    #[test]
    fn missing_symbol_returns_error() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = FindReferencesSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {}",
                "language": "rust",
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("symbol"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(FindReferencesSkill.name(), "find_references");
        assert!(FindReferencesSkill.removable());
    }
}
