//! ghost audit — audit log query, export, and live tail (T-2.2.1, T-2.2.2, T-2.2.3).

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use ghost_audit::{
    export::{AuditExporter, ExportFormat},
    query_engine::{AuditEntry, AuditFilter, AuditQueryEngine},
};
use serde::{Deserialize, Serialize};
use tokio::signal;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

// ─── ghost audit query ───────────────────────────────────────────────────────

pub struct AuditQueryArgs {
    pub agent: Option<String>,
    pub severity: Option<String>,
    pub event_type: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub search: Option<String>,
    pub limit: u32,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct AuditQueryResult {
    entries: Vec<AuditEntry>,
    total: u64,
    page: u32,
    page_size: u32,
}

impl TableDisplay for AuditQueryResult {
    fn print_table(&self) {
        if self.entries.is_empty() {
            println!("No audit entries found.");
            return;
        }
        println!(
            "{:<26}  {:<12}  {:<8}  {:<20}  {}",
            "TIMESTAMP", "SEVERITY", "AGENT", "EVENT TYPE", "DETAILS"
        );
        println!("{}", "─".repeat(100));
        for e in &self.entries {
            let agent = &e.agent_id[..e.agent_id.len().min(12)];
            let et = &e.event_type[..e.event_type.len().min(20)];
            let details = &e.details[..e.details.len().min(60)];
            println!(
                "{:<26}  {:<12}  {:<8}  {:<20}  {}",
                e.timestamp, e.severity, agent, et, details
            );
        }
        println!("\n{} entries (page {}/{})", self.entries.len(), self.page, self.page_size);
    }
}

/// Run `ghost audit query`.
pub async fn run_query(args: AuditQueryArgs, backend: &CliBackend) -> Result<(), CliError> {
    match backend {
        CliBackend::Http { client } => {
            let mut params = vec![];
            if let Some(ref a) = args.agent {
                params.push(format!("agent_id={a}"));
            }
            if let Some(ref s) = args.severity {
                params.push(format!("severity={s}"));
            }
            if let Some(ref et) = args.event_type {
                params.push(format!("event_type={et}"));
            }
            if let Some(ref s) = args.since {
                params.push(format!("time_start={s}"));
            }
            if let Some(ref u) = args.until {
                params.push(format!("time_end={u}"));
            }
            if let Some(ref q) = args.search {
                params.push(format!("search={q}"));
            }
            params.push(format!("page_size={}", args.limit));

            let path = if params.is_empty() {
                "/api/audit".to_string()
            } else {
                format!("/api/audit?{}", params.join("&"))
            };

            let resp = client.get(&path).await?;
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CliError::Internal(format!("parse audit response: {e}")))?;

            let entries: Vec<AuditEntry> =
                serde_json::from_value(body["entries"].clone()).unwrap_or_default();
            let total = body["total"].as_u64().unwrap_or(entries.len() as u64);
            let page = body["page"].as_u64().unwrap_or(1) as u32;
            let page_size = body["page_size"].as_u64().unwrap_or(args.limit as u64) as u32;

            let result = AuditQueryResult { entries, total, page, page_size };
            print_output(&result, args.output);
        }
        CliBackend::Direct { db, .. } => {
            let db = db.read().map_err(|e| CliError::Database(e.to_string()))?;
            let engine = AuditQueryEngine::new(&db);
            let filter = AuditFilter {
                agent_id: args.agent,
                severity: args.severity,
                event_type: args.event_type,
                time_start: args.since,
                time_end: args.until,
                search: args.search,
                page: 1,
                page_size: args.limit,
                ..Default::default()
            };
            let paged = engine
                .query(&filter)
                .map_err(|e| CliError::Database(e.to_string()))?;
            let result = AuditQueryResult {
                total: paged.total,
                page: paged.page,
                page_size: paged.page_size,
                entries: paged.items,
            };
            print_output(&result, args.output);
        }
    }
    Ok(())
}

// ─── ghost audit export ──────────────────────────────────────────────────────

pub struct AuditExportArgs {
    pub format: String,
    pub output: Option<String>,
}

/// Run `ghost audit export`.
pub async fn run_export(args: AuditExportArgs, backend: &CliBackend) -> Result<(), CliError> {
    let export_format = match args.format.as_str() {
        "csv" => ExportFormat::Csv,
        "jsonl" | "ndjson" => ExportFormat::Jsonl,
        _ => ExportFormat::Json,
    };

    // Fetch all entries.
    let entries = match backend {
        CliBackend::Http { client } => {
            let fmt_param = match export_format {
                ExportFormat::Csv => "csv",
                ExportFormat::Jsonl => "jsonl",
                ExportFormat::Json => "json",
            };
            let resp = client
                .get(&format!("/api/audit/export?format={fmt_param}"))
                .await?;
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| CliError::Internal(format!("read export response: {e}")))?;
            write_export_output(&bytes, args.output.as_deref())?;
            return Ok(());
        }
        CliBackend::Direct { db, .. } => {
            let db = db.read().map_err(|e| CliError::Database(e.to_string()))?;
            let engine = AuditQueryEngine::new(&db);
            let filter = AuditFilter { page: 1, page_size: 10_000, ..Default::default() };
            engine
                .query(&filter)
                .map_err(|e| CliError::Database(e.to_string()))?
                .items
        }
    };

    let mut buf = Vec::new();
    AuditExporter::export(&entries, export_format, &mut buf)
        .map_err(|e| CliError::Internal(format!("export failed: {e}")))?;

    write_export_output(&buf, args.output.as_deref())?;
    Ok(())
}

fn write_export_output(data: &[u8], path: Option<&str>) -> Result<(), CliError> {
    if let Some(p) = path {
        std::fs::write(p, data).map_err(|e| CliError::Internal(format!("write {p}: {e}")))?;
        eprintln!("Exported to {p}");
    } else {
        use std::io::Write;
        std::io::stdout()
            .write_all(data)
            .map_err(|e| CliError::Internal(format!("write stdout: {e}")))?;
    }
    Ok(())
}

// ─── ghost audit tail ────────────────────────────────────────────────────────

pub struct AuditTailArgs {
    pub gateway_url: String,
    pub token: Option<String>,
}

/// Minimal deserializer for WS events to detect audit-type events.
#[derive(Debug, Deserialize)]
struct WsEventEnvelope {
    #[serde(rename = "type")]
    event_type: String,
}

/// Run `ghost audit tail` — stream live audit events.
pub async fn run_tail(args: AuditTailArgs) -> Result<(), CliError> {
    let ws_base = args
        .gateway_url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let ws_url = match args.token.as_deref() {
        Some(t) => format!("{ws_base}/api/ws?token={t}"),
        None => format!("{ws_base}/api/ws"),
    };

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| CliError::Http(format!("websocket connect failed: {e}")))?;

    let (mut write, mut read) = ws_stream.split();

    println!("Tailing audit events (Ctrl+C to stop)...");

    loop {
        let msg = tokio::select! {
            _ = signal::ctrl_c() => {
                let _ = write.send(Message::Close(None)).await;
                eprintln!("\nDisconnected.");
                return Ok(());
            }
            _ = tokio::time::sleep(Duration::from_secs(1800)) => {
                let _ = write.send(Message::Close(None)).await;
                eprintln!("Connection idle for 1800s. Re-run to reconnect.");
                return Ok(());
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(CliError::Http(e.to_string())),
                    None => {
                        eprintln!("Connection closed.");
                        return Ok(());
                    }
                }
            }
        };

        match msg {
            Message::Text(text) => {
                // Only emit audit-related events.
                if is_audit_event(&text) {
                    println!("{text}");
                }
            }
            Message::Ping(data) => {
                let _ = write.send(Message::Pong(data)).await;
            }
            Message::Close(_) => {
                eprintln!("Connection closed.");
                return Ok(());
            }
            _ => {}
        }
    }
}

fn is_audit_event(text: &str) -> bool {
    // Audit-relevant event types per the WsEvent enum.
    const AUDIT_TYPES: &[&str] = &[
        "KillSwitchActivation",
        "InterventionChange",
        "AgentStateChange",
        "ProposalDecision",
    ];
    match serde_json::from_str::<WsEventEnvelope>(text) {
        Ok(ev) => AUDIT_TYPES.contains(&ev.event_type.as_str()),
        Err(_) => false,
    }
}
