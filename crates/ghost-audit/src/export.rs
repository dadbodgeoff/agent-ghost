//! Audit export to JSON, CSV, JSONL formats.

use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::query_engine::AuditEntry;

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
    Jsonl,
}

/// Audit exporter.
pub struct AuditExporter;

impl AuditExporter {
    /// Export entries to the given writer in the specified format.
    pub fn export<W: Write>(
        entries: &[AuditEntry],
        format: ExportFormat,
        writer: &mut W,
    ) -> Result<(), std::io::Error> {
        match format {
            ExportFormat::Json => Self::export_json(entries, writer),
            ExportFormat::Csv => Self::export_csv(entries, writer),
            ExportFormat::Jsonl => Self::export_jsonl(entries, writer),
        }
    }

    fn export_json<W: Write>(entries: &[AuditEntry], writer: &mut W) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(entries).map_err(std::io::Error::other)?;
        writer.write_all(json.as_bytes())
    }

    fn export_csv<W: Write>(entries: &[AuditEntry], writer: &mut W) -> Result<(), std::io::Error> {
        writeln!(
            writer,
            "id,timestamp,agent_id,event_type,severity,tool_name,details,session_id"
        )?;
        for entry in entries {
            writeln!(
                writer,
                "{},{},{},{},{},{},{},{}",
                csv_escape(&entry.id),
                csv_escape(&entry.timestamp),
                csv_escape(&entry.agent_id),
                csv_escape(&entry.event_type),
                csv_escape(&entry.severity),
                csv_escape(entry.tool_name.as_deref().unwrap_or("")),
                csv_escape(&entry.details),
                csv_escape(entry.session_id.as_deref().unwrap_or("")),
            )?;
        }
        Ok(())
    }

    fn export_jsonl<W: Write>(
        entries: &[AuditEntry],
        writer: &mut W,
    ) -> Result<(), std::io::Error> {
        for entry in entries {
            let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
            writeln!(writer, "{}", line)?;
        }
        Ok(())
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
