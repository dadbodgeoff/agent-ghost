//! ghost logs — live event streaming (T-2.1.1, §4.1, E.3, E.8, R12).

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::signal;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::error::CliError;

/// Arguments for `ghost logs`.
pub struct LogsArgs {
    /// Filter to events from a specific agent ID.
    pub agent: Option<String>,
    /// Filter to a specific event type (e.g. ScoreUpdate, KillSwitchActivation).
    pub event_type: Option<String>,
    /// Emit NDJSON instead of human-readable table rows.
    pub json: bool,
    /// Close connection after this many idle seconds (default: 1800).
    pub idle_timeout: u64,
    /// Gateway base URL.
    pub gateway_url: String,
    /// Bearer token for authentication.
    pub token: Option<String>,
}

/// Minimal event envelope used for filtering and display.
#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(rename = "type")]
    event_type: String,
    agent_id: Option<String>,
    #[serde(flatten)]
    rest: serde_json::Value,
}

/// Run `ghost logs`.
pub async fn run(args: LogsArgs) -> Result<(), CliError> {
    let ws_url = build_ws_url(&args.gateway_url, args.token.as_deref());

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| CliError::Http(format!("websocket connect failed: {e}")))?;

    let (mut write, mut read) = ws_stream.split();
    let idle = Duration::from_secs(args.idle_timeout);

    loop {
        let msg = tokio::select! {
            // Ctrl+C — send Close frame and exit cleanly (E.8).
            _ = signal::ctrl_c() => {
                let _ = write.send(Message::Close(None)).await;
                eprintln!("\nDisconnected.");
                return Ok(());
            }
            // Idle timeout — close cleanly (R12).
            _ = tokio::time::sleep(idle) => {
                let _ = write.send(Message::Close(None)).await;
                eprintln!(
                    "Connection idle for {}s. Re-run to reconnect.",
                    args.idle_timeout
                );
                return Ok(());
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        eprintln!("websocket error: {e}");
                        return Err(CliError::Http(e.to_string()));
                    }
                    None => {
                        eprintln!("Connection closed by server.");
                        return Ok(());
                    }
                }
            }
        };

        match msg {
            Message::Text(text) => {
                handle_message(&text, &args);
            }
            Message::Ping(data) => {
                let _ = write.send(Message::Pong(data)).await;
            }
            Message::Close(_) => {
                eprintln!("Connection closed by server.");
                return Ok(());
            }
            _ => {}
        }
    }
}

fn build_ws_url(base_url: &str, token: Option<&str>) -> String {
    let ws_base = base_url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    match token {
        Some(t) => format!("{ws_base}/api/ws?token={t}"),
        None => format!("{ws_base}/api/ws"),
    }
}

fn handle_message(text: &str, args: &LogsArgs) {
    // Skip Ping events in table mode.
    if text.contains(r#""type":"Ping""#) && !args.json {
        return;
    }

    if args.json {
        // NDJSON: apply filter then emit raw.
        match serde_json::from_str::<RawEvent>(text) {
            Ok(ev) => {
                if !passes_filter(&ev, args) {
                    return;
                }
                println!("{text}");
            }
            Err(_) => {
                // Emit unparseable lines as-is in JSON mode.
                println!("{text}");
            }
        }
    } else {
        match serde_json::from_str::<RawEvent>(text) {
            Ok(ev) => {
                if !passes_filter(&ev, args) {
                    return;
                }
                print_event_row(&ev, text);
            }
            Err(_) => {
                println!("{text}");
            }
        }
    }
}

fn passes_filter(ev: &RawEvent, args: &LogsArgs) -> bool {
    if let Some(ref filter_type) = args.event_type {
        if !ev.event_type.eq_ignore_ascii_case(filter_type) {
            return false;
        }
    }
    if let Some(ref filter_agent) = args.agent {
        match &ev.agent_id {
            Some(id) if id == filter_agent => {}
            _ => return false,
        }
    }
    true
}

fn print_event_row(ev: &RawEvent, raw: &str) {
    let ts = extract_str(raw, "timestamp")
        .or_else(|| extract_str(raw, "computed_at"))
        .unwrap_or_else(|| chrono::Utc::now().format("%H:%M:%S").to_string());

    let agent = ev
        .agent_id
        .as_deref()
        .map(|id| &id[..id.len().min(8)])
        .unwrap_or("-");

    let summary = build_summary(&ev.event_type, &ev.rest);

    println!(
        "{ts:>8}  {type_:<22}  {agent:<8}  {summary}",
        ts = ts,
        type_ = ev.event_type,
        agent = agent,
        summary = summary
    );
}

fn build_summary(event_type: &str, rest: &serde_json::Value) -> String {
    match event_type {
        "ScoreUpdate" => {
            let score = rest.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let level = rest.get("level").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("score={score:.3} level={level}")
        }
        "InterventionChange" => {
            let old = rest.get("old_level").and_then(|v| v.as_u64()).unwrap_or(0);
            let new = rest.get("new_level").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("L{old} → L{new}")
        }
        "KillSwitchActivation" => {
            let reason = rest
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("(no reason)");
            format!("reason={reason}")
        }
        "AgentStateChange" => {
            let state = rest
                .get("new_state")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("→ {state}")
        }
        "SessionEvent" => {
            let et = rest
                .get("event_type")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let seq = rest
                .get("sequence_number")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            format!("{et} seq={seq}")
        }
        "ProposalDecision" => {
            let decision = rest.get("decision").and_then(|v| v.as_str()).unwrap_or("?");
            format!("decision={decision}")
        }
        _ => String::new(),
    }
}

fn extract_str(json: &str, key: &str) -> Option<String> {
    let pattern = format!(r#""{key}":""#);
    let start = json.find(&pattern)? + pattern.len();
    let end = json[start..].find('"')? + start;
    Some(json[start..end].to_string())
}
