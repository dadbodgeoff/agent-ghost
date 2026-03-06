//! Phase 7: Code analysis skills.
//!
//! Provides AST parsing, diagnostics, reference finding, code formatting,
//! and symbol search using regex-based heuristics. Designed for easy
//! upgrade to tree-sitter when grammar dependencies are integrated.
//!
//! | Skill              | Risk     | Autonomy Default       | Convergence Max |
//! |--------------------|----------|------------------------|-----------------|
//! | `parse_ast`        | Read     | Act Autonomously       | Level 4         |
//! | `get_diagnostics`  | Read     | Act Autonomously       | Level 4         |
//! | `find_references`  | Read     | Act Autonomously       | Level 4         |
//! | `format_code`      | Write    | Act with Confirmation  | Level 3         |
//! | `search_symbols`   | Read     | Act Autonomously       | Level 4         |

pub mod find_references;
pub mod format_code;
pub mod get_diagnostics;
pub mod parse_ast;
pub mod search_symbols;

use crate::autonomy::AutonomyLevel;
use crate::convergence_guard::{ConvergenceGuard, GuardConfig};
use crate::skill::Skill;

/// Returns all Phase 7 code analysis skills as boxed trait objects.
///
/// Read-only skills are wrapped with permissive `ConvergenceGuard` settings.
/// `format_code` (write) is wrapped with stricter settings.
pub fn all_code_analysis_skills() -> Vec<Box<dyn Skill>> {
    vec![
        // ── Read-only skills (always available) ─────────────────────
        Box::new(ConvergenceGuard::new(
            parse_ast::ParseAstSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            get_diagnostics::GetDiagnosticsSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            find_references::FindReferencesSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            search_symbols::SearchSymbolsSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        // ── Write skills (gated) ────────────────────────────────────
        Box::new(ConvergenceGuard::new(
            format_code::FormatCodeSkill,
            GuardConfig {
                max_convergence_level: 3,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(100),
                ..Default::default()
            },
        )),
    ]
}

/// Maximum file size (bytes) for code analysis operations.
/// Files larger than 1 MiB are rejected to prevent resource exhaustion.
pub(crate) const MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Read source code from either `file_path` or `source_code` in the input.
///
/// Returns `(source_code, language_id)`.
pub(crate) fn read_source(
    input: &serde_json::Value,
) -> Result<(String, String), crate::skill::SkillError> {
    let source = if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
        // Validate file size before reading.
        let metadata = std::fs::metadata(path).map_err(|e| {
            crate::skill::SkillError::InvalidInput(format!("cannot read file '{path}': {e}"))
        })?;
        if metadata.len() > MAX_FILE_SIZE {
            return Err(crate::skill::SkillError::InvalidInput(format!(
                "file too large ({} bytes, max {})",
                metadata.len(),
                MAX_FILE_SIZE,
            )));
        }
        std::fs::read_to_string(path).map_err(|e| {
            crate::skill::SkillError::InvalidInput(format!("cannot read file '{path}': {e}"))
        })?
    } else if let Some(code) = input.get("source_code").and_then(|v| v.as_str()) {
        code.to_string()
    } else {
        return Err(crate::skill::SkillError::InvalidInput(
            "provide either 'file_path' or 'source_code'".into(),
        ));
    };

    // Determine language: explicit > file extension > "unknown".
    let language = if let Some(lang) = input.get("language").and_then(|v| v.as_str()) {
        lang.to_string()
    } else if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
        detect_language(path)
    } else {
        "unknown".to_string()
    };

    Ok((source, language))
}

/// Detect language from file extension.
pub(crate) fn detect_language(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "rust",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "py" | "pyi" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cxx" | "cc" | "hpp" | "hxx" => "cpp",
        "rb" => "ruby",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "zig" => "zig",
        "lua" => "lua",
        "sh" | "bash" | "zsh" => "shell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "md" | "markdown" => "markdown",
        _ => "unknown",
    }
    .to_string()
}

/// Regex patterns for symbol extraction, keyed by language.
pub(crate) struct LanguagePatterns {
    pub function: &'static str,
    pub struct_or_class: &'static str,
    pub enum_or_interface: &'static str,
    pub constant: &'static str,
    pub import: &'static str,
    pub module_or_type: &'static str,
}

pub(crate) fn patterns_for_language(language: &str) -> LanguagePatterns {
    match language {
        "rust" => LanguagePatterns {
            function: r"(?m)^\s*(?:pub(?:\([\w:]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?fn\s+(\w+)",
            struct_or_class: r"(?m)^\s*(?:pub(?:\([\w:]+\))?\s+)?struct\s+(\w+)",
            enum_or_interface: r"(?m)^\s*(?:pub(?:\([\w:]+\))?\s+)?(?:enum|trait)\s+(\w+)",
            constant: r"(?m)^\s*(?:pub(?:\([\w:]+\))?\s+)?(?:const|static)\s+(\w+)",
            import: r"(?m)^\s*use\s+(.+);",
            module_or_type: r"(?m)^\s*(?:pub(?:\([\w:]+\))?\s+)?(?:mod|type)\s+(\w+)",
        },
        "javascript" | "jsx" => LanguagePatterns {
            function: r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)",
            struct_or_class: r"(?m)^\s*(?:export\s+)?class\s+(\w+)",
            enum_or_interface: r"(?m)^\s*(?:export\s+)?(?:const|let)\s+(\w+)\s*=\s*(?:async\s+)?\(",
            constant: r"(?m)^\s*(?:export\s+)?const\s+(\w+)\s*=\s*[^(]",
            import: r#"(?m)^\s*import\s+.+\s+from\s+['"](.+)['"]"#,
            module_or_type: r"(?m)$^", // No module keyword in JS
        },
        "typescript" | "tsx" => LanguagePatterns {
            function: r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)",
            struct_or_class: r"(?m)^\s*(?:export\s+)?class\s+(\w+)",
            enum_or_interface: r"(?m)^\s*(?:export\s+)?(?:interface|enum)\s+(\w+)",
            constant: r"(?m)^\s*(?:export\s+)?const\s+(\w+)\s*[=:]",
            import: r#"(?m)^\s*import\s+.+\s+from\s+['"](.+)['"]"#,
            module_or_type: r"(?m)^\s*(?:export\s+)?type\s+(\w+)",
        },
        "python" => LanguagePatterns {
            function: r"(?m)^(?:\s*)(?:async\s+)?def\s+(\w+)",
            struct_or_class: r"(?m)^(?:\s*)class\s+(\w+)",
            enum_or_interface: r"(?m)$^", // No enum/interface in Python
            constant: r"(?m)^([A-Z][A-Z0-9_]+)\s*=",
            import: r"(?m)^(?:from\s+(\S+)\s+import|import\s+(\S+))",
            module_or_type: r"(?m)$^",
        },
        "go" => LanguagePatterns {
            function: r"(?m)^func\s+(?:\([^)]+\)\s+)?(\w+)",
            struct_or_class: r"(?m)^type\s+(\w+)\s+struct\b",
            enum_or_interface: r"(?m)^type\s+(\w+)\s+interface\b",
            constant: r"(?m)^\s*(?:const|var)\s+(\w+)",
            import: r#"(?m)^\s*"(.+)""#,
            module_or_type: r"(?m)^type\s+(\w+)\s+(?!struct|interface)\w",
        },
        _ => LanguagePatterns {
            function: r"(?m)(?:fn|func|function|def)\s+(\w+)",
            struct_or_class: r"(?m)(?:struct|class)\s+(\w+)",
            enum_or_interface: r"(?m)(?:enum|interface|trait|protocol)\s+(\w+)",
            constant: r"(?m)(?:const|static|final)\s+(\w+)",
            import: r"(?m)(?:use|import|require|include)\s+(.+)",
            module_or_type: r"(?m)(?:mod|module|type|typedef)\s+(\w+)",
        },
    }
}
