//! `arxiv_search` — search arXiv papers via the public Atom API.
//!
//! Read-only skill. Uses the arXiv public API (no authentication).
//! Rate-limited to respect arXiv's usage guidelines (3-second minimum
//! between requests is enforced server-side by arXiv).
//!
//! ## Input
//!
//! | Field          | Type   | Required | Default | Description                    |
//! |----------------|--------|----------|---------|--------------------------------|
//! | `query`        | string | yes      | —       | Search query (arXiv syntax)    |
//! | `max_results`  | int    | no       | 10      | Max papers to return (1-50)    |
//! | `sort_by`      | string | no       | "relevance" | "relevance", "lastUpdatedDate", "submittedDate" |
//! | `sort_order`   | string | no       | "descending" | "ascending", "descending"  |
//!
//! ## Output
//!
//! ```json
//! {
//!   "papers": [{ "id": "...", "title": "...", "summary": "...", "authors": [...], "published": "...", "pdf_url": "..." }],
//!   "count": 5,
//!   "query": "..."
//! }
//! ```

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct ArxivSearchSkill;

const ARXIV_API_URL: &str = "https://export.arxiv.org/api/query";
const REQUEST_TIMEOUT_SECS: u64 = 15;
const MAX_RESULTS_CAP: u64 = 50;

impl Skill for ArxivSearchSkill {
    fn name(&self) -> &str {
        "arxiv_search"
    }

    fn description(&self) -> &str {
        "Search arXiv papers via the public API"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidInput("missing required field 'query'".into()))?;

        if query.trim().is_empty() {
            return Err(SkillError::InvalidInput("query must not be empty".into()));
        }

        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(MAX_RESULTS_CAP);

        let sort_by = input
            .get("sort_by")
            .and_then(|v| v.as_str())
            .unwrap_or("relevance");
        let sort_order = input
            .get("sort_order")
            .and_then(|v| v.as_str())
            .unwrap_or("descending");

        // Validate sort parameters.
        let valid_sort_by = ["relevance", "lastUpdatedDate", "submittedDate"];
        if !valid_sort_by.contains(&sort_by) {
            return Err(SkillError::InvalidInput(format!(
                "invalid sort_by '{sort_by}', must be one of: {valid_sort_by:?}"
            )));
        }
        let valid_sort_order = ["ascending", "descending"];
        if !valid_sort_order.contains(&sort_order) {
            return Err(SkillError::InvalidInput(format!(
                "invalid sort_order '{sort_order}', must be one of: {valid_sort_order:?}"
            )));
        }

        let url = format!(
            "{ARXIV_API_URL}?search_query={}&start=0&max_results={max_results}\
             &sortBy={sort_by}&sortOrder={sort_order}",
            url_encode(query),
        );

        let response = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .call()
            .map_err(|e| SkillError::Internal(format!("arXiv API request failed: {e}")))?;

        let xml = response
            .into_string()
            .map_err(|e| SkillError::Internal(format!("arXiv API response read error: {e}")))?;

        let papers = parse_arxiv_atom(&xml);

        Ok(serde_json::json!({
            "papers": papers,
            "count": papers.len(),
            "query": query,
            "sort_by": sort_by,
            "sort_order": sort_order,
        }))
    }
}

/// Parse arXiv Atom XML feed into structured paper entries.
///
/// Uses simple string parsing to avoid adding an XML crate dependency.
/// arXiv's Atom format is stable and well-defined.
fn parse_arxiv_atom(xml: &str) -> Vec<serde_json::Value> {
    let mut papers = Vec::new();

    // Split by <entry> tags.
    for entry_block in xml.split("<entry>").skip(1) {
        let entry_end = match entry_block.find("</entry>") {
            Some(pos) => pos,
            None => continue,
        };
        let entry = &entry_block[..entry_end];

        let id = extract_tag(entry, "id").unwrap_or_default();
        let title = extract_tag(entry, "title")
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let summary = extract_tag(entry, "summary")
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let published = extract_tag(entry, "published").unwrap_or_default();
        let updated = extract_tag(entry, "updated").unwrap_or_default();

        // Extract authors.
        let authors: Vec<String> = entry
            .split("<author>")
            .skip(1)
            .filter_map(|a| extract_tag(a, "name"))
            .collect();

        // Extract PDF link.
        let pdf_url = entry
            .split("<link")
            .find(|l| l.contains("title=\"pdf\""))
            .and_then(|l| l.split("href=\"").nth(1).and_then(|h| h.split('"').next()))
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Extract categories.
        let categories: Vec<String> = entry
            .split("<category")
            .skip(1)
            .filter_map(|c| {
                c.split("term=\"")
                    .nth(1)
                    .and_then(|t| t.split('"').next())
                    .map(|s| s.to_string())
            })
            .collect();

        papers.push(serde_json::json!({
            "id": id,
            "title": title,
            "summary": summary,
            "authors": authors,
            "published": published,
            "updated": updated,
            "pdf_url": pdf_url,
            "categories": categories,
        }));
    }

    papers
}

/// Extract text content between `<tag>` and `</tag>`.
fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_string())
}

fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u32),
        })
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
    fn parse_arxiv_atom_extracts_papers() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<entry>
<id>http://arxiv.org/abs/2301.00001v1</id>
<title>Test Paper Title</title>
<summary>This is a test abstract.</summary>
<published>2023-01-01T00:00:00Z</published>
<updated>2023-01-02T00:00:00Z</updated>
<author><name>Alice Author</name></author>
<author><name>Bob Researcher</name></author>
<link href="http://arxiv.org/pdf/2301.00001v1" title="pdf" type="application/pdf"/>
<category term="cs.AI"/>
</entry>
</feed>"#;

        let papers = parse_arxiv_atom(xml);
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0]["title"], "Test Paper Title");
        assert_eq!(papers[0]["authors"][0], "Alice Author");
        assert_eq!(papers[0]["authors"][1], "Bob Researcher");
        assert_eq!(papers[0]["categories"][0], "cs.AI");
    }

    #[test]
    fn rejects_empty_query() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ArxivSearchSkill.execute(&ctx, &serde_json::json!({"query": ""}));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_sort() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ArxivSearchSkill.execute(
            &ctx,
            &serde_json::json!({"query": "test", "sort_by": "invalid"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(ArxivSearchSkill.name(), "arxiv_search");
        assert!(ArxivSearchSkill.removable());
        assert_eq!(ArxivSearchSkill.source(), SkillSource::Bundled);
    }
}
