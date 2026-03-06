//! `github_search` — search GitHub repositories and issues via the public API.
//!
//! Uses the GitHub REST API v3 search endpoints. No authentication required
//! for public repositories (rate-limited to 10 requests/minute unauthenticated).
//! If an `access_token` is provided, authenticated rate limits apply (30/min).
//!
//! ## Input
//!
//! | Field           | Type   | Required | Default       | Description                         |
//! |-----------------|--------|----------|---------------|-------------------------------------|
//! | `query`         | string | yes      | —             | Search query (GitHub syntax)        |
//! | `search_type`   | string | no       | "repositories" | "repositories", "issues", "code"   |
//! | `max_results`   | int    | no       | 10            | Max results to return (1-30)        |
//! | `sort`          | string | no       | "best-match"  | Sort field (type-dependent)         |
//! | `order`         | string | no       | "desc"        | "asc" or "desc"                     |
//! | `access_token`  | string | no       | —             | GitHub personal access token        |

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct GithubSearchSkill;

const GITHUB_API_URL: &str = "https://api.github.com/search";
const REQUEST_TIMEOUT_SECS: u64 = 10;
const MAX_RESULTS_CAP: u64 = 30;

impl Skill for GithubSearchSkill {
    fn name(&self) -> &str {
        "github_search"
    }

    fn description(&self) -> &str {
        "Search GitHub repositories, issues, and code"
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

        let search_type = input
            .get("search_type")
            .and_then(|v| v.as_str())
            .unwrap_or("repositories");

        let valid_types = ["repositories", "issues", "code"];
        if !valid_types.contains(&search_type) {
            return Err(SkillError::InvalidInput(format!(
                "invalid search_type '{search_type}', must be one of: {valid_types:?}"
            )));
        }

        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(MAX_RESULTS_CAP);

        let sort = input.get("sort").and_then(|v| v.as_str());
        let order = input
            .get("order")
            .and_then(|v| v.as_str())
            .unwrap_or("desc");

        let access_token = input.get("access_token").and_then(|v| v.as_str());

        let mut url = format!(
            "{GITHUB_API_URL}/{search_type}?q={}&per_page={max_results}",
            url_encode(query),
        );
        if let Some(sort_field) = sort {
            url.push_str(&format!("&sort={sort_field}"));
        }
        url.push_str(&format!("&order={order}"));

        let mut request = ureq::get(&url)
            .set("Accept", "application/vnd.github.v3+json")
            .set("User-Agent", "ghost-skills/0.1")
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS));

        if let Some(token) = access_token {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }

        let response = request
            .call()
            .map_err(|e| SkillError::Internal(format!("GitHub API request failed: {e}")))?;

        let body: serde_json::Value = response
            .into_json()
            .map_err(|e| SkillError::Internal(format!("GitHub API response parse error: {e}")))?;

        let total_count = body
            .get("total_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let items = body.get("items").and_then(|v| v.as_array());

        let results: Vec<serde_json::Value> = match (items, search_type) {
            (Some(items), "repositories") => items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "full_name": item.get("full_name").and_then(|v| v.as_str()).unwrap_or(""),
                        "description": item.get("description").and_then(|v| v.as_str()),
                        "html_url": item.get("html_url").and_then(|v| v.as_str()).unwrap_or(""),
                        "stars": item.get("stargazers_count").and_then(|v| v.as_u64()).unwrap_or(0),
                        "language": item.get("language").and_then(|v| v.as_str()),
                        "updated_at": item.get("updated_at").and_then(|v| v.as_str()),
                        "topics": item.get("topics"),
                    })
                })
                .collect(),
            (Some(items), "issues") => items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "title": item.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                        "html_url": item.get("html_url").and_then(|v| v.as_str()).unwrap_or(""),
                        "state": item.get("state").and_then(|v| v.as_str()).unwrap_or(""),
                        "user": item.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()),
                        "created_at": item.get("created_at").and_then(|v| v.as_str()),
                        "comments": item.get("comments").and_then(|v| v.as_u64()).unwrap_or(0),
                        "labels": item.get("labels").and_then(|v| v.as_array()).map(|a| {
                            a.iter().filter_map(|l| l.get("name").and_then(|n| n.as_str()).map(String::from)).collect::<Vec<_>>()
                        }),
                    })
                })
                .collect(),
            (Some(items), "code") => items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "path": item.get("path").and_then(|v| v.as_str()).unwrap_or(""),
                        "html_url": item.get("html_url").and_then(|v| v.as_str()).unwrap_or(""),
                        "repository": item.get("repository").and_then(|r| r.get("full_name")).and_then(|v| v.as_str()),
                    })
                })
                .collect(),
            _ => Vec::new(),
        };

        Ok(serde_json::json!({
            "search_type": search_type,
            "query": query,
            "total_count": total_count,
            "results": results,
            "count": results.len(),
        }))
    }
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
    fn rejects_empty_query() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GithubSearchSkill.execute(&ctx, &serde_json::json!({"query": "  "}));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_search_type() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GithubSearchSkill.execute(
            &ctx,
            &serde_json::json!({"query": "test", "search_type": "users"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GithubSearchSkill.name(), "github_search");
        assert!(GithubSearchSkill.removable());
        assert_eq!(GithubSearchSkill.source(), SkillSource::Bundled);
    }
}
