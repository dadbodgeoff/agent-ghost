//! ghost session — session list, inspection, and replay (T-2.4.1, T-2.4.2, T-4.3.1, §4.1).

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

// ─── ghost session list ───────────────────────────────────────────────────────

pub struct SessionListArgs {
    pub agent: Option<String>,
    pub limit: u32,
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub started_at: String,
    pub last_event_at: String,
    pub event_count: i64,
    pub agents: String,
}

#[derive(Serialize)]
struct SessionList {
    sessions: Vec<SessionSummary>,
}

impl TableDisplay for SessionList {
    fn print_table(&self) {
        if self.sessions.is_empty() {
            println!("No sessions found.");
            return;
        }
        println!(
            "{:<36}  {:<26}  {:>6}  {}",
            "SESSION ID", "STARTED", "EVENTS", "AGENT(S)"
        );
        println!("{}", "─".repeat(100));
        for s in &self.sessions {
            println!(
                "{:<36}  {:<26}  {:>6}  {}",
                s.session_id, s.started_at, s.event_count, s.agents
            );
        }
        println!("\n{} session(s) shown.", self.sessions.len());
    }
}

/// Run `ghost session list`.
pub async fn run_list(args: SessionListArgs, backend: &CliBackend) -> Result<(), CliError> {
    let sessions = match backend {
        CliBackend::Http { client } => {
            let mut params = vec![format!("limit={}", args.limit)];
            if let Some(ref a) = args.agent {
                params.push(format!("agent_id={a}"));
            }
            let path = format!("/api/sessions?{}", params.join("&"));
            let resp = client.get(&path).await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse sessions: {e}")))?;
            serde_json::from_value::<Vec<SessionSummary>>(body["sessions"].clone())
                .unwrap_or_default()
        }
        CliBackend::Direct { db, .. } => {
            let db = db.lock().map_err(|_| CliError::Database("lock poisoned".into()))?;
            query_sessions_direct(&db, args.agent.as_deref(), args.limit)?
        }
    };

    print_output(&SessionList { sessions }, args.output);
    Ok(())
}

// ─── ghost session inspect ────────────────────────────────────────────────────

pub struct SessionInspectArgs {
    pub session_id: String,
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEvent {
    pub id: String,
    pub event_type: String,
    pub sender: Option<String>,
    pub timestamp: String,
    pub sequence_number: i64,
    pub content_hash: Option<String>,
    pub content_length: Option<i64>,
    pub privacy_level: String,
    pub latency_ms: Option<i64>,
    pub token_count: Option<i64>,
    pub event_hash: String,
    pub previous_hash: String,
    pub attributes: serde_json::Value,
}

#[derive(Serialize)]
struct SessionDetail {
    session_id: String,
    events: Vec<SessionEvent>,
    total: u32,
    chain_valid: bool,
    cumulative_cost: f64,
}

impl TableDisplay for SessionDetail {
    fn print_table(&self) {
        println!("Session: {}", self.session_id);
        println!(
            "Events: {}  |  Chain valid: {}  |  Cumulative cost: ${:.6}",
            self.total, self.chain_valid, self.cumulative_cost
        );
        println!();
        if self.events.is_empty() {
            println!("No events.");
            return;
        }
        println!(
            "{:>4}  {:<26}  {:<22}  {:<12}  {:>6}  {}",
            "SEQ", "TIMESTAMP", "TYPE", "SENDER", "TOKENS", "HASH"
        );
        println!("{}", "─".repeat(100));
        for e in &self.events {
            let sender = e.sender.as_deref().unwrap_or("-");
            let tokens = e
                .token_count
                .map(|t| t.to_string())
                .unwrap_or_else(|| "-".into());
            let hash_short = &e.event_hash[..e.event_hash.len().min(16)];
            println!(
                "{:>4}  {:<26}  {:<22}  {:<12}  {:>6}  {}",
                e.sequence_number, e.timestamp, e.event_type, sender, tokens, hash_short,
            );
        }
    }
}

/// Run `ghost session inspect <session_id>`.
pub async fn run_inspect(args: SessionInspectArgs, backend: &CliBackend) -> Result<(), CliError> {
    let detail = match backend {
        CliBackend::Http { client } => {
            let path = format!("/api/sessions/{}/events", args.session_id);
            let resp = client.get(&path).await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse session events: {e}")))?;

            let events: Vec<SessionEvent> =
                serde_json::from_value(body["events"].clone()).unwrap_or_default();
            SessionDetail {
                session_id: args.session_id,
                total: body["total"].as_u64().unwrap_or(events.len() as u64) as u32,
                chain_valid: body["chain_valid"].as_bool().unwrap_or(true),
                cumulative_cost: body["cumulative_cost"].as_f64().unwrap_or(0.0),
                events,
            }
        }
        CliBackend::Direct { db, .. } => {
            let db = db.lock().map_err(|_| CliError::Database("lock poisoned".into()))?;
            let events = query_events_direct(&db, &args.session_id)?;
            let total = events.len() as u32;
            SessionDetail {
                session_id: args.session_id,
                events,
                total,
                chain_valid: true, // verify chain separately with `ghost db verify`
                cumulative_cost: 0.0,
            }
        }
    };

    print_output(&detail, args.output);
    Ok(())
}

// ─── ghost session replay ────────────────────────────────────────────────────

pub struct SessionReplayArgs {
    pub session_id: String,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct ReplayEntry {
    seq: i64,
    timestamp: String,
    event_type: String,
    sender: String,
    content: String,
}

#[derive(Serialize)]
struct SessionReplay {
    session_id: String,
    entries: Vec<ReplayEntry>,
}

impl TableDisplay for SessionReplay {
    fn print_table(&self) {
        if self.entries.is_empty() {
            println!("No events to replay for session {}.", self.session_id);
            return;
        }
        println!("╔══ Session Replay: {} ══╗", self.session_id);
        println!();

        let mut last_gate_state: Option<String> = None;

        for entry in &self.entries {
            // Show gate state transitions inline.
            if entry.event_type == "gate_check" || entry.event_type == "gate_state_change" {
                let new_state = format!("[GATE] {}", entry.content);
                if last_gate_state.as_deref() != Some(&new_state) {
                    println!("  --- {} ---", new_state);
                    last_gate_state = Some(new_state);
                }
                continue;
            }

            let sender = if entry.sender.is_empty() {
                "system"
            } else {
                &entry.sender
            };

            // Format based on event type.
            match entry.event_type.as_str() {
                "llm_request" | "user_message" | "message" => {
                    println!(
                        "[{} | #{}] {} >",
                        entry.timestamp, entry.seq, sender
                    );
                    // Indent message content.
                    for line in entry.content.lines() {
                        println!("  {line}");
                    }
                    println!();
                }
                "llm_response" | "assistant_message" => {
                    println!(
                        "[{} | #{}] {} <",
                        entry.timestamp, entry.seq, sender
                    );
                    for line in entry.content.lines() {
                        println!("  {line}");
                    }
                    println!();
                }
                "tool_call" | "tool_execution" => {
                    println!(
                        "[{} | #{}] TOOL {}",
                        entry.timestamp, entry.seq, entry.content
                    );
                }
                "intervention" | "kill_switch" => {
                    println!(
                        "[{} | #{}] *** {} ***",
                        entry.timestamp, entry.seq, entry.content
                    );
                }
                _ => {
                    println!(
                        "[{} | #{}] ({}) {}",
                        entry.timestamp, entry.seq, entry.event_type, entry.content
                    );
                }
            }
        }

        println!();
        println!("╚══ End of replay ({} events) ══╝", self.entries.len());
    }
}

/// Run `ghost session replay <session_id>`.
pub async fn run_replay(args: SessionReplayArgs, backend: &CliBackend) -> Result<(), CliError> {
    // Fetch events using the same path as inspect.
    let events = match backend {
        CliBackend::Http { client } => {
            let path = format!("/api/sessions/{}/events", args.session_id);
            let resp = client.get(&path).await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse session events: {e}")))?;
            serde_json::from_value::<Vec<SessionEvent>>(body["events"].clone())
                .unwrap_or_default()
        }
        CliBackend::Direct { db, .. } => {
            let db = db.lock().map_err(|_| CliError::Database("lock poisoned".into()))?;
            query_events_direct(&db, &args.session_id)?
        }
    };

    // Convert events to replay entries by extracting content from attributes.
    let entries: Vec<ReplayEntry> = events
        .iter()
        .map(|e| {
            let content = extract_replay_content(e);
            ReplayEntry {
                seq: e.sequence_number,
                timestamp: e.timestamp.clone(),
                event_type: e.event_type.clone(),
                sender: e.sender.clone().unwrap_or_default(),
                content,
            }
        })
        .collect();

    print_output(
        &SessionReplay {
            session_id: args.session_id,
            entries,
        },
        args.output,
    );
    Ok(())
}

/// Extract human-readable content from a session event for replay.
fn extract_replay_content(event: &SessionEvent) -> String {
    let attrs = &event.attributes;

    // Try common attribute keys for content.
    if let Some(content) = attrs["content"].as_str() {
        return content.to_string();
    }
    if let Some(message) = attrs["message"].as_str() {
        return message.to_string();
    }
    if let Some(text) = attrs["text"].as_str() {
        return text.to_string();
    }
    if let Some(prompt) = attrs["prompt"].as_str() {
        return prompt.to_string();
    }
    if let Some(response) = attrs["response"].as_str() {
        return response.to_string();
    }

    // For tool calls, show the tool name and truncated input.
    if let Some(tool) = attrs["tool_name"].as_str() {
        let input = attrs["input"]
            .as_str()
            .or_else(|| attrs["arguments"].as_str())
            .unwrap_or("");
        let truncated = if input.len() > 100 {
            format!("{}...", &input[..100])
        } else {
            input.to_string()
        };
        return format!("{tool}({truncated})");
    }

    // For gate checks, summarize the state.
    if event.event_type == "gate_check" || event.event_type == "gate_state_change" {
        if let Some(state) = attrs["gate_state"].as_str() {
            return state.to_string();
        }
        if let Some(result) = attrs["result"].as_str() {
            return result.to_string();
        }
    }

    // For interventions, show the reason.
    if let Some(reason) = attrs["reason"].as_str() {
        return reason.to_string();
    }

    // Fallback: serialize non-null attributes as a short summary.
    if attrs.is_object() && !attrs.as_object().unwrap().is_empty() {
        let summary = serde_json::to_string(attrs).unwrap_or_default();
        if summary.len() > 120 {
            return format!("{}...", &summary[..120]);
        }
        return summary;
    }

    format!("[{} event]", event.event_type)
}

// ─── direct DB helpers ────────────────────────────────────────────────────────

fn query_sessions_direct(
    conn: &rusqlite::Connection,
    agent: Option<&str>,
    limit: u32,
) -> Result<Vec<SessionSummary>, CliError> {
    let sql: String = if agent.is_some() {
        "SELECT session_id, MIN(timestamp) as started_at, MAX(timestamp) as last_event_at, \
         COUNT(*) as event_count, GROUP_CONCAT(DISTINCT sender) as agents \
         FROM itp_events WHERE sender = ?1 GROUP BY session_id ORDER BY started_at DESC LIMIT ?2"
            .into()
    } else {
        "SELECT session_id, MIN(timestamp) as started_at, MAX(timestamp) as last_event_at, \
         COUNT(*) as event_count, GROUP_CONCAT(DISTINCT sender) as agents \
         FROM itp_events GROUP BY session_id ORDER BY started_at DESC LIMIT ?1"
            .into()
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| CliError::Database(e.to_string()))?;

    let rows: Result<Vec<SessionSummary>, _> = if let Some(a) = agent {
        stmt.query_map(rusqlite::params![a, limit], map_session_row)
            .map_err(|e| CliError::Database(e.to_string()))?
            .collect()
    } else {
        stmt.query_map(rusqlite::params![limit], map_session_row)
            .map_err(|e| CliError::Database(e.to_string()))?
            .collect()
    };

    rows.map_err(|e| CliError::Database(e.to_string()))
}

fn map_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSummary> {
    Ok(SessionSummary {
        session_id: row.get(0)?,
        started_at: row.get(1)?,
        last_event_at: row.get(2)?,
        event_count: row.get(3)?,
        agents: row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "-".into()),
    })
}

fn query_events_direct(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<SessionEvent>, CliError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, event_type, sender, timestamp, sequence_number, \
             content_hash, content_length, privacy_level, latency_ms, token_count, \
             hex(event_hash), hex(previous_hash), attributes \
             FROM itp_events WHERE session_id = ?1 ORDER BY sequence_number ASC",
        )
        .map_err(|e| CliError::Database(e.to_string()))?;

    let rows: Result<Vec<SessionEvent>, _> = stmt
        .query_map(rusqlite::params![session_id], |row| {
            let attrs_str: String =
                row.get::<_, Option<String>>(12)?.unwrap_or_else(|| "{}".into());
            let attributes = serde_json::from_str(&attrs_str)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            Ok(SessionEvent {
                id: row.get(0)?,
                event_type: row.get(1)?,
                sender: row.get(2)?,
                timestamp: row.get(3)?,
                sequence_number: row.get(4)?,
                content_hash: row.get(5)?,
                content_length: row.get(6)?,
                privacy_level: row.get(7)?,
                latency_ms: row.get(8)?,
                token_count: row.get(9)?,
                event_hash: row.get(10)?,
                previous_hash: row.get(11)?,
                attributes,
            })
        })
        .map_err(|e| CliError::Database(e.to_string()))?
        .collect();

    rows.map_err(|e| CliError::Database(e.to_string()))
}
