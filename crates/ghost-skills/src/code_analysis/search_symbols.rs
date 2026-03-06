//! `search_symbols` — search for symbol definitions in source code.
//!
//! Read-only skill. Always available (convergence max = Level 4).
//!
//! Searches for symbol definitions matching a query string, with optional
//! filtering by symbol kind (function, struct, enum, etc.).
//!
//! ## Input
//!
//! | Field         | Type   | Required                     | Default     |
//! |---------------|--------|------------------------------|-------------|
//! | `file_path`   | string | yes (or `source_code`)       | —           |
//! | `source_code` | string | yes (or `file_path`)         | —           |
//! | `query`       | string | yes                          | —           |
//! | `kind`        | string | no                           | all kinds   |
//! | `language`    | string | no                           | auto-detect |
//! | `case_sensitive` | bool | no                          | false       |
//!
//! ## Output
//!
//! ```json
//! {
//!   "symbols": [{
//!     "name": "process_data",
//!     "kind": "function",
//!     "line": 10,
//!     "signature": "pub fn process_data(input: &str) -> Result<(), Error>"
//!   }],
//!   "total": 1
//! }
//! ```

use regex::Regex;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::{patterns_for_language, read_source};

/// Skill that searches for symbol definitions in source code.
pub struct SearchSymbolsSkill;

impl Skill for SearchSymbolsSkill {
    fn name(&self) -> &str {
        "search_symbols"
    }

    fn description(&self) -> &str {
        "Search for symbol definitions matching a query (functions, structs, enums, etc.)"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let (source, language) = read_source(input)?;

        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'query' (string)".into())
            })?;

        if query.is_empty() {
            return Err(SkillError::InvalidInput(
                "'query' must not be empty".into(),
            ));
        }

        let kind_filter = input.get("kind").and_then(|v| v.as_str());

        let case_sensitive = input
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let patterns = patterns_for_language(&language);

        // Collect all symbols from all pattern categories.
        let pattern_entries: Vec<(&str, &str)> = vec![
            (patterns.function, "function"),
            (patterns.struct_or_class, "struct"),
            (patterns.enum_or_interface, "enum"),
            (patterns.constant, "constant"),
            (patterns.module_or_type, "type"),
        ];

        let mut symbols = Vec::new();

        for (pattern, kind) in &pattern_entries {
            // Apply kind filter if specified.
            if let Some(filter) = kind_filter {
                if *kind != filter {
                    continue;
                }
            }

            let re = match Regex::new(pattern) {
                Ok(re) => re,
                Err(_) => continue,
            };

            for cap in re.captures_iter(&source) {
                let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }

                // Apply query filter (substring match).
                let matches = if case_sensitive {
                    name.contains(query)
                } else {
                    name.to_lowercase().contains(&query.to_lowercase())
                };

                if !matches {
                    continue;
                }

                let full_match = cap.get(0).unwrap();
                let byte_offset = full_match.start();
                let line = source[..byte_offset].matches('\n').count() + 1;

                // Extract the full source line as the signature.
                let line_start = source[..byte_offset]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let line_end = source[byte_offset..]
                    .find('\n')
                    .map(|i| byte_offset + i)
                    .unwrap_or(source.len());
                let signature = source[line_start..line_end].trim().to_string();

                symbols.push(serde_json::json!({
                    "name": name,
                    "kind": kind,
                    "line": line,
                    "signature": signature,
                }));
            }
        }

        // Sort by line number.
        symbols.sort_by_key(|s| s["line"].as_u64().unwrap_or(0));

        let total = symbols.len();
        Ok(serde_json::json!({
            "symbols": symbols,
            "total": total,
            "query": query,
            "language": language,
        }))
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
    fn finds_functions_by_prefix() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = concat!(
            "fn process_data() {}\n",
            "fn process_request() {}\n",
            "fn handle_error() {}\n",
        );

        let result = SearchSymbolsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "query": "process",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["total"], 2);

        let symbols = val["symbols"].as_array().unwrap();
        assert!(symbols.iter().all(|s| s["name"]
            .as_str()
            .unwrap()
            .contains("process")));
    }

    #[test]
    fn filters_by_kind() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = concat!(
            "fn data_processor() {}\n",
            "struct DataStore {}\n",
            "const DATA_MAX: usize = 100;\n",
        );

        let result = SearchSymbolsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "query": "data",
                "kind": "function",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["total"], 1);
        assert_eq!(val["symbols"][0]["name"], "data_processor");
        assert_eq!(val["symbols"][0]["kind"], "function");
    }

    #[test]
    fn case_insensitive_by_default() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SearchSymbolsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn ProcessData() {}\n",
                "query": "processdata",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["total"], 1);
    }

    #[test]
    fn case_sensitive_when_requested() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SearchSymbolsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn ProcessData() {}\n",
                "query": "processdata",
                "language": "rust",
                "case_sensitive": true,
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["total"], 0);
    }

    #[test]
    fn no_results_for_nonexistent_query() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SearchSymbolsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {}\n",
                "query": "nonexistent",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["total"], 0);
    }

    #[test]
    fn missing_query_returns_error() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SearchSymbolsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {}",
                "language": "rust",
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("query"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn includes_signature() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SearchSymbolsSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "pub fn process_data(input: &str) -> Result<(), Error> {\n}\n",
                "query": "process_data",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let sig = val["symbols"][0]["signature"].as_str().unwrap();
        assert!(sig.contains("pub fn process_data"));
        assert!(sig.contains("Result<(), Error>"));
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(SearchSymbolsSkill.name(), "search_symbols");
        assert!(SearchSymbolsSkill.removable());
    }
}
