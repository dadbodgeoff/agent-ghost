//! `json_transform` — JSON path queries and transformations.
//!
//! Provides operations on JSON data: extract fields by path, flatten
//! nested structures, filter arrays, pick/omit keys, and merge objects.
//! All operations are pure and produce new JSON values.
//!
//! ## Input
//!
//! | Field       | Type   | Required | Description                               |
//! |-------------|--------|----------|-------------------------------------------|
//! | `action`    | string | yes      | "get", "flatten", "filter", "pick", "omit", "merge", "keys", "values" |
//! | `data`      | any    | yes      | JSON data to operate on                   |
//! | `path`      | string | get      | Dot-separated path (e.g., "a.b.c", "a.0.b") |
//! | `keys`      | array  | pick/omit | Keys to pick or omit                     |
//! | `condition` | object | filter   | `{"key": "...", "op": "eq|ne|gt|lt|contains", "value": ...}` |
//! | `other`     | any    | merge    | Second object to merge with data          |

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct JsonTransformSkill;

impl Skill for JsonTransformSkill {
    fn name(&self) -> &str {
        "json_transform"
    }

    fn description(&self) -> &str {
        "JSON path queries and transformations (get, flatten, filter, pick, omit, merge)"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'action' \
                     (get|flatten|filter|pick|omit|merge|keys|values)"
                        .into(),
                )
            })?;

        let data = input
            .get("data")
            .ok_or_else(|| SkillError::InvalidInput("missing required field 'data'".into()))?;

        match action {
            "get" => {
                let path = input
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'path'".into())
                    })?;

                let result = json_get(data, path);
                Ok(serde_json::json!({
                    "path": path,
                    "value": result,
                    "found": !result.is_null(),
                }))
            }
            "flatten" => {
                let flat = json_flatten(data, "");
                Ok(serde_json::json!({
                    "result": flat,
                    "key_count": flat.as_object().map(|m| m.len()).unwrap_or(0),
                }))
            }
            "filter" => {
                let arr = data.as_array().ok_or_else(|| {
                    SkillError::InvalidInput("'data' must be an array for filter action".into())
                })?;
                let condition = input.get("condition").ok_or_else(|| {
                    SkillError::InvalidInput("missing required field 'condition'".into())
                })?;

                let key = condition
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("condition missing 'key'".into())
                    })?;
                let op = condition
                    .get("op")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("condition missing 'op'".into())
                    })?;
                let value = condition.get("value").ok_or_else(|| {
                    SkillError::InvalidInput("condition missing 'value'".into())
                })?;

                let filtered: Vec<&serde_json::Value> = arr
                    .iter()
                    .filter(|item| json_compare(item.get(key), op, value))
                    .collect();

                Ok(serde_json::json!({
                    "result": filtered,
                    "count": filtered.len(),
                    "original_count": arr.len(),
                }))
            }
            "pick" => {
                let obj = data.as_object().ok_or_else(|| {
                    SkillError::InvalidInput("'data' must be an object for pick action".into())
                })?;
                let keys = input
                    .get("keys")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'keys' (array)".into())
                    })?;

                let key_strs: Vec<&str> = keys
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect();

                let picked: serde_json::Map<String, serde_json::Value> = obj
                    .iter()
                    .filter(|(k, _)| key_strs.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                Ok(serde_json::json!({
                    "result": serde_json::Value::Object(picked),
                }))
            }
            "omit" => {
                let obj = data.as_object().ok_or_else(|| {
                    SkillError::InvalidInput("'data' must be an object for omit action".into())
                })?;
                let keys = input
                    .get("keys")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'keys' (array)".into())
                    })?;

                let key_strs: Vec<&str> = keys
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect();

                let omitted: serde_json::Map<String, serde_json::Value> = obj
                    .iter()
                    .filter(|(k, _)| !key_strs.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                Ok(serde_json::json!({
                    "result": serde_json::Value::Object(omitted),
                }))
            }
            "merge" => {
                let base = data.as_object().ok_or_else(|| {
                    SkillError::InvalidInput("'data' must be an object for merge action".into())
                })?;
                let other = input
                    .get("other")
                    .and_then(|v| v.as_object())
                    .ok_or_else(|| {
                        SkillError::InvalidInput(
                            "missing required field 'other' (object to merge)".into(),
                        )
                    })?;

                let mut merged = base.clone();
                for (k, v) in other {
                    merged.insert(k.clone(), v.clone());
                }

                Ok(serde_json::json!({
                    "result": serde_json::Value::Object(merged),
                }))
            }
            "keys" => {
                let obj = data.as_object().ok_or_else(|| {
                    SkillError::InvalidInput("'data' must be an object for keys action".into())
                })?;
                let keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
                Ok(serde_json::json!({
                    "result": keys,
                    "count": keys.len(),
                }))
            }
            "values" => {
                let obj = data.as_object().ok_or_else(|| {
                    SkillError::InvalidInput("'data' must be an object for values action".into())
                })?;
                let values: Vec<&serde_json::Value> = obj.values().collect();
                Ok(serde_json::json!({
                    "result": values,
                    "count": values.len(),
                }))
            }
            other => Err(SkillError::InvalidInput(format!(
                "unknown action '{other}', must be one of: get, flatten, filter, pick, omit, merge, keys, values"
            ))),
        }
    }
}

/// Navigate a JSON value using a dot-separated path.
/// Supports array indexing via numeric segments (e.g., "items.0.name").
fn json_get<'a>(data: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    let mut current = data;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        current = if let Ok(idx) = segment.parse::<usize>() {
            current.get(idx).unwrap_or(&serde_json::Value::Null)
        } else {
            current.get(segment).unwrap_or(&serde_json::Value::Null)
        };
    }
    current
}

/// Flatten a nested JSON structure into a single-level object with
/// dot-separated keys (e.g., `{"a": {"b": 1}}` → `{"a.b": 1}`).
fn json_flatten(data: &serde_json::Value, prefix: &str) -> serde_json::Value {
    let mut result = serde_json::Map::new();

    match data {
        serde_json::Value::Object(obj) => {
            for (k, v) in obj {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                let flat = json_flatten(v, &key);
                if let serde_json::Value::Object(inner) = flat {
                    for (ik, iv) in inner {
                        result.insert(ik, iv);
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let key = if prefix.is_empty() {
                    i.to_string()
                } else {
                    format!("{prefix}.{i}")
                };
                let flat = json_flatten(v, &key);
                if let serde_json::Value::Object(inner) = flat {
                    for (ik, iv) in inner {
                        result.insert(ik, iv);
                    }
                }
            }
        }
        _ => {
            if !prefix.is_empty() {
                result.insert(prefix.to_string(), data.clone());
            }
        }
    }

    serde_json::Value::Object(result)
}

/// Compare a JSON value against a condition.
fn json_compare(field: Option<&serde_json::Value>, op: &str, expected: &serde_json::Value) -> bool {
    let field = match field {
        Some(v) => v,
        None => return false,
    };

    match op {
        "eq" => field == expected,
        "ne" => field != expected,
        "gt" => field
            .as_f64()
            .zip(expected.as_f64())
            .map(|(a, b)| a > b)
            .unwrap_or(false),
        "lt" => field
            .as_f64()
            .zip(expected.as_f64())
            .map(|(a, b)| a < b)
            .unwrap_or(false),
        "gte" => field
            .as_f64()
            .zip(expected.as_f64())
            .map(|(a, b)| a >= b)
            .unwrap_or(false),
        "lte" => field
            .as_f64()
            .zip(expected.as_f64())
            .map(|(a, b)| a <= b)
            .unwrap_or(false),
        "contains" => {
            let needle = expected.as_str().unwrap_or("");
            field.as_str().map(|s| s.contains(needle)).unwrap_or(false)
        }
        _ => false,
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
    fn get_nested_path() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = JsonTransformSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "get",
                    "data": {"a": {"b": {"c": 42}}},
                    "path": "a.b.c",
                }),
            )
            .unwrap();
        assert_eq!(result["value"], 42);
        assert_eq!(result["found"], true);
    }

    #[test]
    fn get_array_index() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = JsonTransformSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "get",
                    "data": {"items": [{"name": "first"}, {"name": "second"}]},
                    "path": "items.1.name",
                }),
            )
            .unwrap();
        assert_eq!(result["value"], "second");
    }

    #[test]
    fn flatten_nested_object() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = JsonTransformSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "flatten",
                    "data": {"a": {"b": 1, "c": {"d": 2}}},
                }),
            )
            .unwrap();
        assert_eq!(result["result"]["a.b"], 1);
        assert_eq!(result["result"]["a.c.d"], 2);
    }

    #[test]
    fn filter_array() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = JsonTransformSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "filter",
                    "data": [
                        {"name": "Alice", "age": 30},
                        {"name": "Bob", "age": 25},
                        {"name": "Charlie", "age": 35},
                    ],
                    "condition": {"key": "age", "op": "gt", "value": 28},
                }),
            )
            .unwrap();
        assert_eq!(result["count"], 2);
    }

    #[test]
    fn pick_keys() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = JsonTransformSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "pick",
                    "data": {"a": 1, "b": 2, "c": 3},
                    "keys": ["a", "c"],
                }),
            )
            .unwrap();
        let obj = result["result"].as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["a"], 1);
        assert_eq!(obj["c"], 3);
    }

    #[test]
    fn omit_keys() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = JsonTransformSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "omit",
                    "data": {"a": 1, "b": 2, "c": 3},
                    "keys": ["b"],
                }),
            )
            .unwrap();
        let obj = result["result"].as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert!(!obj.contains_key("b"));
    }

    #[test]
    fn merge_objects() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = JsonTransformSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "merge",
                    "data": {"a": 1, "b": 2},
                    "other": {"b": 3, "c": 4},
                }),
            )
            .unwrap();
        let obj = result["result"].as_object().unwrap();
        assert_eq!(obj["a"], 1);
        assert_eq!(obj["b"], 3); // Overwritten by other
        assert_eq!(obj["c"], 4);
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(JsonTransformSkill.name(), "json_transform");
        assert!(JsonTransformSkill.removable());
        assert_eq!(JsonTransformSkill.source(), SkillSource::Bundled);
    }
}
