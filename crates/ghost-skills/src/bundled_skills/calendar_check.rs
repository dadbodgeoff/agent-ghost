//! `calendar_check` — read calendar events via OAuth.
//!
//! Requires a valid OAuth access token for a calendar provider
//! (e.g., Google Calendar). The token must be provided in the
//! input — the caller (gateway handler or orchestrator) is
//! responsible for acquiring it via the OAuth broker.
//!
//! ## Input
//!
//! | Field           | Type   | Required | Default           | Description                        |
//! |-----------------|--------|----------|-------------------|------------------------------------|
//! | `access_token`  | string | yes      | —                 | OAuth bearer token for calendar API |
//! | `provider`      | string | no       | "google"          | Calendar provider                  |
//! | `time_min`      | string | no       | now               | ISO 8601 start of range            |
//! | `time_max`      | string | no       | now + 7 days      | ISO 8601 end of range              |
//! | `max_results`   | int    | no       | 10                | Max events to return               |
//! | `calendar_id`   | string | no       | "primary"         | Calendar identifier                |

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct CalendarCheckSkill;

/// Request timeout for calendar API calls.
const REQUEST_TIMEOUT_SECS: u64 = 10;

impl Skill for CalendarCheckSkill {
    fn name(&self) -> &str {
        "calendar_check"
    }

    fn description(&self) -> &str {
        "Read upcoming calendar events (requires OAuth access token)"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let access_token = input
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'access_token' (OAuth bearer token)".into(),
                )
            })?;

        let provider = input
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("google");

        match provider {
            "google" => self.google_calendar(access_token, input),
            other => Err(SkillError::InvalidInput(format!(
                "unsupported calendar provider '{other}'. Supported: google"
            ))),
        }
    }
}

impl CalendarCheckSkill {
    fn google_calendar(
        &self,
        access_token: &str,
        input: &serde_json::Value,
    ) -> SkillResult {
        let calendar_id = input
            .get("calendar_id")
            .and_then(|v| v.as_str())
            .unwrap_or("primary");
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);

        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();
        let time_min = input
            .get("time_min")
            .and_then(|v| v.as_str())
            .unwrap_or(&now_str);
        let default_max = (now + chrono::Duration::days(7)).to_rfc3339();
        let time_max = input
            .get("time_max")
            .and_then(|v| v.as_str())
            .unwrap_or(&default_max);

        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events\
             ?timeMin={}&timeMax={}&maxResults={}&singleEvents=true&orderBy=startTime",
            url_encode(calendar_id),
            url_encode(time_min),
            url_encode(time_max),
            max_results,
        );

        let response = ureq::get(&url)
            .set("Authorization", &format!("Bearer {access_token}"))
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .call()
            .map_err(|e| SkillError::Internal(format!("calendar API request failed: {e}")))?;

        let body: serde_json::Value = response
            .into_json()
            .map_err(|e| SkillError::Internal(format!("calendar API response parse error: {e}")))?;

        // Extract events from Google Calendar API response.
        let items = body.get("items").and_then(|v| v.as_array());
        let events: Vec<serde_json::Value> = match items {
            Some(items) => items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "id": item.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                        "summary": item.get("summary").and_then(|v| v.as_str()).unwrap_or("(no title)"),
                        "start": item.get("start"),
                        "end": item.get("end"),
                        "location": item.get("location").and_then(|v| v.as_str()),
                        "status": item.get("status").and_then(|v| v.as_str()).unwrap_or("confirmed"),
                    })
                })
                .collect(),
            None => Vec::new(),
        };

        Ok(serde_json::json!({
            "provider": "google",
            "calendar_id": calendar_id,
            "events": events,
            "count": events.len(),
            "time_min": time_min,
            "time_max": time_max,
        }))
    }
}

/// Minimal percent-encoding for URL query parameters.
fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
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
    fn rejects_missing_access_token() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = CalendarCheckSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => assert!(msg.contains("access_token")),
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn rejects_unsupported_provider() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = CalendarCheckSkill.execute(
            &ctx,
            &serde_json::json!({
                "access_token": "fake",
                "provider": "outlook",
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => assert!(msg.contains("unsupported")),
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn url_encode_preserves_safe_chars() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("primary"), "primary");
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(CalendarCheckSkill.name(), "calendar_check");
        assert!(CalendarCheckSkill.removable());
        assert_eq!(CalendarCheckSkill.source(), SkillSource::Bundled);
    }
}
