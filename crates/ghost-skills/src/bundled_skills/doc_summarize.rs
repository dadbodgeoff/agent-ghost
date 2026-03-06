//! `doc_summarize` — extract document structure, statistics, and key content.
//!
//! Reads a file and produces a structured summary including metadata,
//! heading structure, word/line/paragraph counts, and key sentences.
//! Works offline — no LLM required. Supports plain text and markdown.
//!
//! ## Input
//!
//! | Field           | Type   | Required | Default | Description                       |
//! |-----------------|--------|----------|---------|-----------------------------------|
//! | `file_path`     | string | yes      | —       | Absolute path to the document     |
//! | `max_sentences` | int    | no       | 10      | Max key sentences to extract      |
//! | `format`        | string | no       | "auto"  | "auto", "text", "markdown"        |
//!
//! ## Output
//!
//! ```json
//! {
//!   "file_path": "...",
//!   "format": "markdown",
//!   "stats": { "bytes": 1234, "lines": 50, "words": 300, "paragraphs": 12 },
//!   "headings": ["# Title", "## Section 1", ...],
//!   "key_sentences": ["...", "..."],
//!   "first_paragraph": "..."
//! }
//! ```

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct DocSummarizeSkill;

/// Maximum file size to read (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

impl Skill for DocSummarizeSkill {
    fn name(&self) -> &str {
        "doc_summarize"
    }

    fn description(&self) -> &str {
        "Extract document structure, statistics, and key sentences from a file"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let file_path = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'file_path'".into())
            })?;

        let max_sentences = input
            .get("max_sentences")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let format_hint = input
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

        // Validate file exists and is within size limit.
        let metadata = std::fs::metadata(file_path).map_err(|e| {
            SkillError::InvalidInput(format!("cannot read file '{file_path}': {e}"))
        })?;

        if !metadata.is_file() {
            return Err(SkillError::InvalidInput(format!(
                "'{file_path}' is not a regular file"
            )));
        }

        if metadata.len() > MAX_FILE_SIZE {
            return Err(SkillError::InvalidInput(format!(
                "file too large ({} bytes, max {MAX_FILE_SIZE})",
                metadata.len()
            )));
        }

        let content = std::fs::read_to_string(file_path).map_err(|e| {
            SkillError::InvalidInput(format!("cannot read file as text: {e}"))
        })?;

        // Detect format.
        let detected_format = match format_hint {
            "markdown" => "markdown",
            "text" => "text",
            _ => {
                if file_path.ends_with(".md") || file_path.ends_with(".markdown") {
                    "markdown"
                } else if content.contains("# ") || content.contains("## ") {
                    "markdown"
                } else {
                    "text"
                }
            }
        };

        // Compute statistics.
        let lines: Vec<&str> = content.lines().collect();
        let words: usize = content.split_whitespace().count();
        let paragraphs = content
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count();

        // Extract headings (markdown).
        let headings: Vec<String> = if detected_format == "markdown" {
            lines
                .iter()
                .filter(|l| l.starts_with('#'))
                .map(|l| l.to_string())
                .collect()
        } else {
            Vec::new()
        };

        // Extract first non-empty paragraph.
        let first_paragraph = content
            .split("\n\n")
            .map(|p| p.trim())
            .find(|p| !p.is_empty() && !p.starts_with('#'))
            .unwrap_or("")
            .to_string();

        // Extract key sentences using simple heuristic:
        // Prefer longer sentences that don't start with common filler.
        let key_sentences = extract_key_sentences(&content, max_sentences);

        Ok(serde_json::json!({
            "file_path": file_path,
            "format": detected_format,
            "stats": {
                "bytes": metadata.len(),
                "lines": lines.len(),
                "words": words,
                "paragraphs": paragraphs,
            },
            "headings": headings,
            "key_sentences": key_sentences,
            "first_paragraph": first_paragraph,
        }))
    }
}

/// Extract key sentences from text using a simple scoring heuristic.
///
/// Scoring criteria:
/// - Longer sentences score higher (information density)
/// - Sentences with technical terms score higher
/// - Filler sentences ("In this paper", "We also") score lower
fn extract_key_sentences(text: &str, max: usize) -> Vec<String> {
    let sentences: Vec<&str> = text
        .split(|c: char| c == '.' || c == '!' || c == '?')
        .map(|s| s.trim())
        .filter(|s| {
            let word_count = s.split_whitespace().count();
            word_count >= 5 && word_count <= 60
        })
        .collect();

    if sentences.is_empty() {
        return Vec::new();
    }

    let filler_starts = [
        "in this", "we also", "the rest", "this paper",
        "the paper", "note that", "it is worth",
    ];

    let mut scored: Vec<(f64, &str)> = sentences
        .iter()
        .map(|&s| {
            let word_count = s.split_whitespace().count() as f64;
            let lower = s.to_lowercase();

            // Base score from length (diminishing returns after 20 words).
            let length_score = (word_count / 20.0).min(1.0);

            // Penalty for filler.
            let filler_penalty = if filler_starts.iter().any(|f| lower.starts_with(f)) {
                0.5
            } else {
                1.0
            };

            // Bonus for containing data-like content.
            let data_bonus = if s.contains('%')
                || s.chars().any(|c| c.is_ascii_digit())
                || s.contains("significant")
                || s.contains("result")
                || s.contains("show")
                || s.contains("demonstrate")
            {
                1.2
            } else {
                1.0
            };

            let score = length_score * filler_penalty * data_bonus;
            (score, s)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max);

    scored.into_iter().map(|(_, s)| format!("{s}.")).collect()
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
    fn summarize_text_file() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let tmp = std::env::temp_dir().join(format!("ghost-doc-test-{}.txt", Uuid::now_v7()));
        std::fs::write(
            &tmp,
            "This is the first paragraph of the document.\n\n\
             The second paragraph contains important results showing 95% accuracy.\n\n\
             A third paragraph provides additional context for the analysis.",
        )
        .unwrap();

        let result = DocSummarizeSkill
            .execute(
                &ctx,
                &serde_json::json!({"file_path": tmp.to_str().unwrap()}),
            )
            .unwrap();

        assert_eq!(result["format"], "text");
        assert!(result["stats"]["words"].as_u64().unwrap() > 10);
        assert_eq!(result["stats"]["paragraphs"], 3);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn summarize_markdown_file() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let tmp = std::env::temp_dir().join(format!("ghost-doc-test-{}.md", Uuid::now_v7()));
        std::fs::write(
            &tmp,
            "# Title\n\nIntroduction paragraph.\n\n## Section One\n\nFirst section content.\n\n## Section Two\n\nSecond section content.",
        )
        .unwrap();

        let result = DocSummarizeSkill
            .execute(
                &ctx,
                &serde_json::json!({"file_path": tmp.to_str().unwrap()}),
            )
            .unwrap();

        assert_eq!(result["format"], "markdown");
        let headings = result["headings"].as_array().unwrap();
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0], "# Title");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn rejects_nonexistent_file() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = DocSummarizeSkill.execute(
            &ctx,
            &serde_json::json!({"file_path": "/nonexistent/file.txt"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn key_sentence_extraction() {
        let text = "Short. This is a much longer sentence that contains important \
                    information about the research results showing 95% improvement. \
                    In this paper we do something. Another meaningful sentence with \
                    data and significant findings from the experiment.";
        let sentences = extract_key_sentences(text, 2);
        assert_eq!(sentences.len(), 2);
        // The longer, data-rich sentences should rank higher.
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(DocSummarizeSkill.name(), "doc_summarize");
        assert!(DocSummarizeSkill.removable());
        assert_eq!(DocSummarizeSkill.source(), SkillSource::Bundled);
    }
}
