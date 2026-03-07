//! Symbol extraction from source files via regex.
//!
//! Extracts functions, structs, traits, enums, impls, types, and constants
//! from Rust, TypeScript/JavaScript, Python, and Go source files.

use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct ExtractedSymbol {
    pub name: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: Option<usize>,
    pub signature: Option<String>,
}

/// Extract symbols from a source file based on its extension.
pub fn extract_symbols(path: &Path, content: &str) -> Vec<ExtractedSymbol> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" => extract_rust(content),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" => extract_typescript(content),
        "py" => extract_python(content),
        "go" => extract_go(content),
        _ => Vec::new(),
    }
}

// Pre-compiled regex patterns (compiled once, reused across calls).

struct CompiledPatterns {
    patterns: Vec<(Regex, &'static str)>,
}

static RUST_PATTERNS: OnceLock<CompiledPatterns> = OnceLock::new();
static TS_PATTERNS: OnceLock<CompiledPatterns> = OnceLock::new();
static PY_PATTERNS: OnceLock<CompiledPatterns> = OnceLock::new();
static GO_PATTERNS: OnceLock<CompiledPatterns> = OnceLock::new();

fn compile_patterns(specs: &[(&str, &'static str)]) -> CompiledPatterns {
    CompiledPatterns {
        patterns: specs
            .iter()
            .map(|(pat, kind)| (Regex::new(pat).expect("invalid regex pattern"), *kind))
            .collect(),
    }
}

fn extract_rust(content: &str) -> Vec<ExtractedSymbol> {
    let compiled = RUST_PATTERNS.get_or_init(|| {
        compile_patterns(&[
            (
                r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)",
                "function",
            ),
            (
                r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)",
                "struct",
            ),
            (
                r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?trait\s+(\w+)",
                "trait",
            ),
            (r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)", "enum"),
            (r"(?m)^[ \t]*impl(?:<[^>]*>)?\s+(\w+)", "impl"),
            (r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?type\s+(\w+)", "type"),
            (
                r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?const\s+(\w+)",
                "const",
            ),
            (r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)", "module"),
        ])
    });
    extract_with_compiled(content, compiled)
}

fn extract_typescript(content: &str) -> Vec<ExtractedSymbol> {
    let compiled = TS_PATTERNS.get_or_init(|| {
        compile_patterns(&[
            (
                r"(?m)^[ \t]*(?:export\s+)?(?:async\s+)?function\s+(\w+)",
                "function",
            ),
            (
                r"(?m)^[ \t]*(?:export\s+)?(?:default\s+)?class\s+(\w+)",
                "class",
            ),
            (r"(?m)^[ \t]*(?:export\s+)?interface\s+(\w+)", "interface"),
            (r"(?m)^[ \t]*(?:export\s+)?type\s+(\w+)", "type"),
            (
                r"(?m)^[ \t]*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?\(",
                "function",
            ),
            (r"(?m)^[ \t]*(?:export\s+)?enum\s+(\w+)", "enum"),
        ])
    });
    extract_with_compiled(content, compiled)
}

fn extract_python(content: &str) -> Vec<ExtractedSymbol> {
    let compiled = PY_PATTERNS.get_or_init(|| {
        compile_patterns(&[
            (r"(?m)^[ \t]*(?:async\s+)?def\s+(\w+)", "function"),
            (r"(?m)^[ \t]*class\s+(\w+)", "class"),
        ])
    });
    extract_with_compiled(content, compiled)
}

fn extract_go(content: &str) -> Vec<ExtractedSymbol> {
    let compiled = GO_PATTERNS.get_or_init(|| {
        compile_patterns(&[
            (r"(?m)^func\s+(?:\([^)]+\)\s+)?(\w+)", "function"),
            (r"(?m)^type\s+(\w+)\s+struct", "struct"),
            (r"(?m)^type\s+(\w+)\s+interface", "interface"),
        ])
    });
    extract_with_compiled(content, compiled)
}

fn extract_with_compiled(content: &str, compiled: &CompiledPatterns) -> Vec<ExtractedSymbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (re, kind) in &compiled.patterns {
        for cap in re.captures_iter(content) {
            let name = cap[1].to_string();
            let match_start = cap.get(0).unwrap().start();
            let line_start = content[..match_start].matches('\n').count() + 1;

            // Get the full line as signature
            let signature = lines
                .get(line_start.saturating_sub(1))
                .map(|l| l.trim().to_string());

            symbols.push(ExtractedSymbol {
                name,
                kind: kind.to_string(),
                line_start,
                line_end: None,
                signature,
            });
        }
    }

    symbols.sort_by_key(|s| s.line_start);
    symbols
}
