//! `csv_analyze` — basic data analysis on CSV files.
//!
//! Reads a CSV file and produces schema detection, row counts, column
//! statistics (min, max, mean, unique count for numeric/string columns),
//! and sample rows. Pure Rust — no external dependencies beyond std.
//!
//! ## Input
//!
//! | Field         | Type   | Required | Default | Description                     |
//! |---------------|--------|----------|---------|---------------------------------|
//! | `file_path`   | string | yes      | —       | Absolute path to CSV file       |
//! | `delimiter`   | string | no       | ","     | Column delimiter (single char)  |
//! | `has_header`  | bool   | no       | true    | Whether first row is a header   |
//! | `sample_rows` | int    | no       | 5       | Number of sample rows to return |
//! | `max_rows`    | int    | no       | 100000  | Max rows to analyze             |

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct CsvAnalyzeSkill;

const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
const DEFAULT_MAX_ROWS: usize = 100_000;

impl Skill for CsvAnalyzeSkill {
    fn name(&self) -> &str {
        "csv_analyze"
    }

    fn description(&self) -> &str {
        "Analyze CSV files: schema detection, statistics, and sample rows"
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
            .ok_or_else(|| SkillError::InvalidInput("missing required field 'file_path'".into()))?;

        let delimiter = input
            .get("delimiter")
            .and_then(|v| v.as_str())
            .unwrap_or(",");
        if delimiter.len() != 1 {
            return Err(SkillError::InvalidInput(
                "delimiter must be a single character".into(),
            ));
        }
        let delim_char = delimiter.chars().next().unwrap();

        let has_header = input
            .get("has_header")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let sample_rows_count = input
            .get("sample_rows")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let max_rows = input
            .get("max_rows")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_ROWS as u64) as usize;

        // Validate file.
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

        let content = std::fs::read_to_string(file_path)
            .map_err(|e| SkillError::InvalidInput(format!("cannot read file as text: {e}")))?;

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return Ok(serde_json::json!({
                "file_path": file_path,
                "error": "file is empty",
            }));
        }

        // Parse header.
        let header: Vec<String> = if has_header {
            split_csv_line(lines[0], delim_char)
        } else {
            let col_count = split_csv_line(lines[0], delim_char).len();
            (0..col_count).map(|i| format!("col_{i}")).collect()
        };

        let data_start = if has_header { 1 } else { 0 };
        let data_lines = &lines[data_start..];
        let row_count = data_lines.len().min(max_rows);

        // Collect column statistics.
        let col_count = header.len();
        let mut col_stats: Vec<ColumnStats> = (0..col_count).map(|_| ColumnStats::new()).collect();

        let mut sample_data: Vec<Vec<String>> = Vec::new();

        for (i, line) in data_lines.iter().enumerate().take(max_rows) {
            let fields = split_csv_line(line, delim_char);
            if i < sample_rows_count {
                sample_data.push(fields.clone());
            }
            for (j, field) in fields.iter().enumerate() {
                if j < col_count {
                    col_stats[j].observe(field);
                }
            }
        }

        // Build column summaries.
        let columns: Vec<serde_json::Value> = header
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let stats = &col_stats[i];
                let mut col = serde_json::json!({
                    "name": name,
                    "type": stats.inferred_type(),
                    "non_null_count": stats.non_null,
                    "null_count": stats.null_count,
                    "unique_count": stats.unique_values.len(),
                });
                if let Some(ref numeric) = stats.numeric {
                    col["min"] = serde_json::json!(numeric.min);
                    col["max"] = serde_json::json!(numeric.max);
                    col["mean"] = serde_json::json!(if numeric.count > 0 {
                        numeric.sum / numeric.count as f64
                    } else {
                        0.0
                    });
                }
                col
            })
            .collect();

        // Build sample rows.
        let samples: Vec<serde_json::Value> = sample_data
            .iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (i, val) in row.iter().enumerate() {
                    let key = header.get(i).map(|s| s.as_str()).unwrap_or("?");
                    obj.insert(key.to_string(), serde_json::json!(val));
                }
                serde_json::Value::Object(obj)
            })
            .collect();

        Ok(serde_json::json!({
            "file_path": file_path,
            "delimiter": delimiter,
            "has_header": has_header,
            "row_count": row_count,
            "column_count": col_count,
            "columns": columns,
            "sample_rows": samples,
        }))
    }
}

/// Split a CSV line by delimiter, respecting quoted fields.
fn split_csv_line(line: &str, delim: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            if in_quotes {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                in_quotes = true;
            }
        } else if c == delim && !in_quotes {
            fields.push(current.trim().to_string());
            current = String::new();
        } else {
            current.push(c);
        }
    }
    fields.push(current.trim().to_string());
    fields
}

struct ColumnStats {
    non_null: usize,
    null_count: usize,
    numeric: Option<NumericStats>,
    unique_values: std::collections::HashSet<String>,
    looks_numeric: bool,
}

struct NumericStats {
    min: f64,
    max: f64,
    sum: f64,
    count: usize,
}

impl ColumnStats {
    fn new() -> Self {
        Self {
            non_null: 0,
            null_count: 0,
            numeric: None,
            unique_values: std::collections::HashSet::new(),
            looks_numeric: true,
        }
    }

    fn observe(&mut self, value: &str) {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == "NA" || trimmed == "null" || trimmed == "NULL" {
            self.null_count += 1;
            return;
        }

        self.non_null += 1;

        // Track unique values (cap at 1000 to limit memory).
        if self.unique_values.len() < 1000 {
            self.unique_values.insert(trimmed.to_string());
        }

        // Attempt numeric parsing.
        if self.looks_numeric {
            if let Ok(num) = trimmed.parse::<f64>() {
                let stats = self.numeric.get_or_insert(NumericStats {
                    min: f64::MAX,
                    max: f64::MIN,
                    sum: 0.0,
                    count: 0,
                });
                if num < stats.min {
                    stats.min = num;
                }
                if num > stats.max {
                    stats.max = num;
                }
                stats.sum += num;
                stats.count += 1;
            } else {
                self.looks_numeric = false;
                self.numeric = None;
            }
        }
    }

    fn inferred_type(&self) -> &'static str {
        if self.non_null == 0 {
            "null"
        } else if self.numeric.is_some() {
            "numeric"
        } else {
            "string"
        }
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
    fn analyze_simple_csv() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let tmp = std::env::temp_dir().join(format!("ghost-csv-test-{}.csv", Uuid::now_v7()));
        std::fs::write(
            &tmp,
            "name,age,score\nAlice,30,95.5\nBob,25,87.3\nCharlie,,92.1\n",
        )
        .unwrap();

        let result = CsvAnalyzeSkill
            .execute(
                &ctx,
                &serde_json::json!({"file_path": tmp.to_str().unwrap()}),
            )
            .unwrap();

        assert_eq!(result["row_count"], 3);
        assert_eq!(result["column_count"], 3);

        let columns = result["columns"].as_array().unwrap();
        assert_eq!(columns[0]["name"], "name");
        assert_eq!(columns[0]["type"], "string");
        assert_eq!(columns[1]["name"], "age");
        assert_eq!(columns[1]["type"], "numeric");
        assert_eq!(columns[2]["name"], "score");
        assert_eq!(columns[2]["type"], "numeric");

        // Age column should have 1 null.
        assert_eq!(columns[1]["null_count"], 1);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn analyze_quoted_csv() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let tmp = std::env::temp_dir().join(format!("ghost-csv-quoted-{}.csv", Uuid::now_v7()));
        std::fs::write(
            &tmp,
            "name,description\nAlice,\"Has a, comma\"\nBob,\"Says \"\"hello\"\"\"\n",
        )
        .unwrap();

        let result = CsvAnalyzeSkill
            .execute(
                &ctx,
                &serde_json::json!({"file_path": tmp.to_str().unwrap()}),
            )
            .unwrap();

        let samples = result["sample_rows"].as_array().unwrap();
        assert_eq!(samples[0]["description"], "Has a, comma");
        assert_eq!(samples[1]["description"], "Says \"hello\"");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn rejects_nonexistent_file() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = CsvAnalyzeSkill.execute(
            &ctx,
            &serde_json::json!({"file_path": "/nonexistent/file.csv"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn split_csv_line_basic() {
        assert_eq!(split_csv_line("a,b,c", ','), vec!["a", "b", "c"]);
        assert_eq!(split_csv_line("\"a,b\",c,d", ','), vec!["a,b", "c", "d"]);
        assert_eq!(split_csv_line("a\tb\tc", '\t'), vec!["a", "b", "c"]);
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(CsvAnalyzeSkill.name(), "csv_analyze");
        assert!(CsvAnalyzeSkill.removable());
        assert_eq!(CsvAnalyzeSkill.source(), SkillSource::Bundled);
    }
}
