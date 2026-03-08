//! ghost convergence — convergence score queries (T-2.3.1, §4.1, Appendix C).

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

// ─── ghost convergence scores ────────────────────────────────────────────────

pub struct ConvergenceScoresArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConvergenceScoreEntry {
    pub agent_id: String,
    pub agent_name: String,
    pub score: f64,
    pub level: i32,
    pub profile: String,
    pub signal_scores: serde_json::Value,
    pub computed_at: Option<String>,
}

#[derive(Serialize)]
struct ConvergenceScoreList {
    scores: Vec<ConvergenceScoreEntry>,
}

impl TableDisplay for ConvergenceScoreList {
    fn print_table(&self) {
        if self.scores.is_empty() {
            println!("No convergence scores available.");
            return;
        }
        println!(
            "{:<12}  {:<20}  {:>7}  {:>5}  {:<16}  COMPUTED",
            "AGENT", "NAME", "SCORE", "LEVEL", "PROFILE"
        );
        println!("{}", "─".repeat(90));
        for s in &self.scores {
            let id = &s.agent_id[..s.agent_id.len().min(12)];
            let name = &s.agent_name[..s.agent_name.len().min(20)];
            let computed = s.computed_at.as_deref().unwrap_or("-");
            println!(
                "{:<12}  {:<20}  {:>7.4}  {:>5}  {:<16}  {}",
                id, name, s.score, s.level, s.profile, computed
            );
            // Print signal scores on next line if present.
            if let serde_json::Value::Object(ref signals) = s.signal_scores {
                let sig_str: Vec<String> = signals
                    .iter()
                    .map(|(k, v)| {
                        let val = v.as_f64().unwrap_or(0.0);
                        format!("{k}={val:.3}")
                    })
                    .collect();
                if !sig_str.is_empty() {
                    println!("    signals: {}", sig_str.join("  "));
                }
            }
        }
    }
}

/// Run `ghost convergence scores`.
pub async fn run_scores(args: ConvergenceScoresArgs, backend: &CliBackend) -> Result<(), CliError> {
    let scores = match backend {
        CliBackend::Http { client } => {
            let resp = client.get("/api/convergence/scores").await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse scores: {e}")))?;
            serde_json::from_value::<Vec<ConvergenceScoreEntry>>(body["scores"].clone())
                .unwrap_or_default()
        }
        CliBackend::Direct { config, .. } => {
            // Direct fallback: read convergence state files from disk.
            read_scores_from_disk(config)
        }
    };

    print_output(&ConvergenceScoreList { scores }, args.output);
    Ok(())
}

// ─── ghost convergence history ───────────────────────────────────────────────

pub struct ConvergenceHistoryArgs {
    pub agent_id: String,
    pub since: Option<String>,
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
struct HistoryEntry {
    timestamp: String,
    score: f64,
    level: i32,
}

#[derive(Serialize)]
struct HistoryList {
    agent_id: String,
    entries: Vec<HistoryEntry>,
}

impl TableDisplay for HistoryList {
    fn print_table(&self) {
        if self.entries.is_empty() {
            println!("No history found for agent {}.", self.agent_id);
            return;
        }
        println!("{:<26}  {:>7}  {:>5}", "TIMESTAMP", "SCORE", "LEVEL");
        println!("{}", "─".repeat(44));
        let mut prev_score: Option<f64> = None;
        for e in &self.entries {
            let delta = prev_score.map(|p| {
                let d = e.score - p;
                if d >= 0.0 {
                    format!("+{d:.4}")
                } else {
                    format!("{d:.4}")
                }
            });
            println!(
                "{:<26}  {:>7.4}  {:>5}  {}",
                e.timestamp,
                e.score,
                e.level,
                delta.as_deref().unwrap_or("")
            );
            prev_score = Some(e.score);
        }
    }
}

/// Run `ghost convergence history <agent_id>`.
pub async fn run_history(
    args: ConvergenceHistoryArgs,
    backend: &CliBackend,
) -> Result<(), CliError> {
    // History endpoint not yet defined in Phase 1 API; fall back to a query
    // against the convergence_scores table via HTTP or direct DB.
    let entries: Vec<HistoryEntry> = match backend {
        CliBackend::Http { client } => {
            let mut path = format!("/api/convergence/scores?agent_id={}", args.agent_id);
            if let Some(ref since) = args.since {
                path.push_str(&format!("&since={since}"));
            }
            let resp = client.get(&path).await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse history: {e}")))?;
            // The scores endpoint returns current state; history not yet available via HTTP.
            // Return what we have as a single-entry "history".
            let scores: Vec<ConvergenceScoreEntry> =
                serde_json::from_value(body["scores"].clone()).unwrap_or_default();
            scores
                .into_iter()
                .filter(|s| s.agent_id == args.agent_id)
                .map(|s| HistoryEntry {
                    timestamp: s.computed_at.unwrap_or_else(|| "-".into()),
                    score: s.score,
                    level: s.level,
                })
                .collect()
        }
        CliBackend::Direct { db, .. } => {
            let db = db.read().map_err(|e| CliError::Database(e.to_string()))?;
            query_history_direct(&db, &args.agent_id, args.since.as_deref())?
        }
    };

    print_output(
        &HistoryList {
            agent_id: args.agent_id,
            entries,
        },
        args.output,
    );
    Ok(())
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn read_scores_from_disk(config: &crate::config::GhostConfig) -> Vec<ConvergenceScoreEntry> {
    use crate::bootstrap::shellexpand_tilde;

    let base = shellexpand_tilde(&config.gateway.db_path);
    let state_dir = std::path::Path::new(&base)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("convergence_state");

    let mut scores = Vec::new();
    if let Ok(dir) = std::fs::read_dir(&state_dir) {
        for entry in dir.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = std::fs::read_to_string(entry.path()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                    let entry = ConvergenceScoreEntry {
                        agent_id: v["agent_id"].as_str().unwrap_or("").to_string(),
                        agent_name: v["agent_name"].as_str().unwrap_or("").to_string(),
                        score: v["score"].as_f64().unwrap_or(0.0),
                        level: v["level"].as_i64().unwrap_or(0) as i32,
                        profile: v["profile"].as_str().unwrap_or("default").to_string(),
                        signal_scores: v["signal_scores"].clone(),
                        computed_at: v["computed_at"].as_str().map(|s| s.to_string()),
                    };
                    scores.push(entry);
                }
            }
        }
    }
    scores
}

fn query_history_direct(
    conn: &rusqlite::Connection,
    agent_id: &str,
    since: Option<&str>,
) -> Result<Vec<HistoryEntry>, CliError> {
    let mut sql =
        "SELECT computed_at, score, level FROM convergence_scores WHERE agent_id = ?1".to_string();
    if since.is_some() {
        sql.push_str(" AND computed_at >= ?2");
    }
    sql.push_str(" ORDER BY computed_at ASC");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| CliError::Database(e.to_string()))?;

    let rows: Result<Vec<HistoryEntry>, _> = if let Some(since_val) = since {
        stmt.query_map(rusqlite::params![agent_id, since_val], |row| {
            Ok(HistoryEntry {
                timestamp: row.get(0)?,
                score: row.get(1)?,
                level: row.get(2)?,
            })
        })
        .map_err(|e| CliError::Database(e.to_string()))?
        .collect()
    } else {
        stmt.query_map(rusqlite::params![agent_id], |row| {
            Ok(HistoryEntry {
                timestamp: row.get(0)?,
                score: row.get(1)?,
                level: row.get(2)?,
            })
        })
        .map_err(|e| CliError::Database(e.to_string()))?
        .collect()
    };

    rows.map_err(|e| CliError::Database(e.to_string()))
}
