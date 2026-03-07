//! `parse_ast` — parse source code into a list of symbol definitions.
//!
//! Read-only skill. Always available (convergence max = Level 4).
//!
//! Uses regex-based heuristics for symbol extraction. Designed for
//! upgrade to tree-sitter when grammar dependencies are integrated.
//!
//! ## Input
//!
//! | Field         | Type   | Required                     | Default     |
//! |---------------|--------|------------------------------|-------------|
//! | `file_path`   | string | yes (or `source_code`)       | —           |
//! | `source_code` | string | yes (or `file_path`)         | —           |
//! | `language`    | string | no                           | auto-detect |
//!
//! ## Output
//!
//! ```json
//! {
//!   "symbols": [{ "name": "main", "kind": "function", "line": 1, "signature": "fn main()" }],
//!   "language": "rust",
//!   "lines": 42
//! }
//! ```

use regex::Regex;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillResult};

use super::{patterns_for_language, read_source};

/// Skill that parses source code into a list of symbol definitions.
pub struct ParseAstSkill;

impl Skill for ParseAstSkill {
    fn name(&self) -> &str {
        "parse_ast"
    }

    fn description(&self) -> &str {
        "Parse source code into a list of symbol definitions (functions, structs, enums, etc.)"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let (source, language) = read_source(input)?;
        let lines: Vec<&str> = source.lines().collect();
        let line_count = lines.len();

        let patterns = patterns_for_language(&language);
        let mut symbols = Vec::new();

        // Extract functions.
        extract_symbols(&source, patterns.function, "function", &mut symbols);

        // Extract structs/classes.
        extract_symbols(&source, patterns.struct_or_class, "struct", &mut symbols);

        // Extract enums/interfaces/traits.
        extract_symbols(&source, patterns.enum_or_interface, "enum", &mut symbols);

        // Extract constants.
        extract_symbols(&source, patterns.constant, "constant", &mut symbols);

        // Extract imports.
        extract_symbols(&source, patterns.import, "import", &mut symbols);

        // Extract modules/type aliases.
        extract_symbols(&source, patterns.module_or_type, "type", &mut symbols);

        // Sort by line number.
        symbols.sort_by_key(|s| s["line"].as_u64().unwrap_or(0));

        Ok(serde_json::json!({
            "symbols": symbols,
            "language": language,
            "lines": line_count,
        }))
    }
}

/// Extract symbols matching the given regex pattern and add them to the output.
fn extract_symbols(source: &str, pattern: &str, kind: &str, symbols: &mut Vec<serde_json::Value>) {
    let re = match Regex::new(pattern) {
        Ok(re) => re,
        Err(_) => return, // Skip invalid patterns silently.
    };

    for cap in re.captures_iter(source) {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }

        let full_match = cap.get(0).unwrap();
        let byte_offset = full_match.start();

        // Compute line number (1-based) from byte offset.
        let line = source[..byte_offset].matches('\n').count() + 1;

        // Extract the full line as the signature.
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
    fn parses_rust_functions() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ParseAstSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "fn main() {\n    println!(\"hello\");\n}\n\npub fn helper(x: i32) -> i32 {\n    x + 1\n}\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let symbols = val["symbols"].as_array().unwrap();

        let functions: Vec<_> = symbols.iter().filter(|s| s["kind"] == "function").collect();
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0]["name"], "main");
        assert_eq!(functions[1]["name"], "helper");
    }

    #[test]
    fn parses_rust_structs_and_enums() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = concat!(
            "pub struct Config {\n",
            "    name: String,\n",
            "}\n\n",
            "pub enum Status {\n",
            "    Active,\n",
            "    Inactive,\n",
            "}\n\n",
            "trait Printable {\n",
            "    fn print(&self);\n",
            "}\n",
        );

        let result = ParseAstSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "language": "rust",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let symbols = val["symbols"].as_array().unwrap();

        let structs: Vec<_> = symbols.iter().filter(|s| s["kind"] == "struct").collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0]["name"], "Config");

        let enums: Vec<_> = symbols.iter().filter(|s| s["kind"] == "enum").collect();
        assert_eq!(enums.len(), 2); // Status + Printable (both match enum/trait pattern)
    }

    #[test]
    fn parses_python_code() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = concat!(
            "class MyClass:\n",
            "    def __init__(self):\n",
            "        pass\n\n",
            "def standalone_function(x):\n",
            "    return x * 2\n\n",
            "MAX_SIZE = 100\n",
        );

        let result = ParseAstSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "language": "python",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let symbols = val["symbols"].as_array().unwrap();

        assert!(symbols.iter().any(|s| s["name"] == "MyClass"));
        assert!(symbols.iter().any(|s| s["name"] == "__init__"));
        assert!(symbols.iter().any(|s| s["name"] == "standalone_function"));
        assert!(symbols.iter().any(|s| s["name"] == "MAX_SIZE"));
    }

    #[test]
    fn parses_typescript_code() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let source = concat!(
            "export interface Config {\n",
            "  name: string;\n",
            "}\n\n",
            "export function processData(input: Config): void {\n",
            "  console.log(input);\n",
            "}\n\n",
            "export type Result = Config | null;\n",
        );

        let result = ParseAstSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": source,
                "language": "typescript",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let symbols = val["symbols"].as_array().unwrap();

        assert!(symbols
            .iter()
            .any(|s| s["name"] == "Config" && s["kind"] == "enum"));
        assert!(symbols
            .iter()
            .any(|s| s["name"] == "processData" && s["kind"] == "function"));
        assert!(symbols.iter().any(|s| s["name"] == "Result"));
    }

    #[test]
    fn reports_line_count() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ParseAstSkill.execute(
            &ctx,
            &serde_json::json!({
                "source_code": "line1\nline2\nline3\n",
                "language": "rust",
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["lines"], 3);
    }

    #[test]
    fn requires_source_input() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ParseAstSkill.execute(&ctx, &serde_json::json!({ "language": "rust" }));
        assert!(result.is_err());
    }

    #[test]
    fn auto_detects_language() {
        let db = test_db();
        let ctx = test_ctx(&db);

        // Write a temp file.
        let path = std::env::temp_dir().join(format!("ghost-parse-{}.rs", Uuid::now_v7()));
        std::fs::write(&path, "fn hello() {}\n").unwrap();

        let result = ParseAstSkill.execute(
            &ctx,
            &serde_json::json!({ "file_path": path.to_str().unwrap() }),
        );
        let _ = std::fs::remove_file(&path);

        assert!(result.is_ok());
        assert_eq!(result.unwrap()["language"], "rust");
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(ParseAstSkill.name(), "parse_ast");
        assert!(ParseAstSkill.removable());
    }
}
