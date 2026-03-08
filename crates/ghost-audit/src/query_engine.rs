//! Paginated audit query engine (Req 30 AC1).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("query error: {0}")]
    Query(String),
}

pub type AuditResult<T> = Result<T, AuditError>;

fn to_audit_err(msg: String) -> AuditError {
    AuditError::Storage(msg)
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub agent_id: String,
    pub event_type: String,
    pub severity: String,
    pub tool_name: Option<String>,
    pub details: String,
    pub session_id: Option<String>,
    pub actor_id: Option<String>,
    pub operation_id: Option<String>,
    pub request_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub idempotency_status: Option<String>,
}

/// Filter criteria for audit queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditFilter {
    pub time_start: Option<String>,
    pub time_end: Option<String>,
    pub agent_id: Option<String>,
    pub event_type: Option<String>,
    pub severity: Option<String>,
    pub tool_name: Option<String>,
    pub search: Option<String>,
    pub operation_id: Option<String>,
    pub page: u32,
    pub page_size: u32,
}

impl AuditFilter {
    pub fn new() -> Self {
        Self {
            page: 1,
            page_size: 50,
            ..Default::default()
        }
    }
}

/// Paginated query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedResult<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

/// The main audit query engine.
pub struct AuditQueryEngine<'a> {
    conn: &'a Connection,
}

impl<'a> AuditQueryEngine<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert an audit entry.
    pub fn insert(&self, entry: &AuditEntry) -> AuditResult<()> {
        self.conn
            .execute(
                "INSERT INTO audit_log (
                    id, timestamp, agent_id, event_type, severity, tool_name,
                    details, session_id, actor_id, operation_id, request_id,
                    idempotency_key, idempotency_status
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    entry.id,
                    entry.timestamp,
                    entry.agent_id,
                    entry.event_type,
                    entry.severity,
                    entry.tool_name,
                    entry.details,
                    entry.session_id,
                    entry.actor_id,
                    entry.operation_id,
                    entry.request_id,
                    entry.idempotency_key,
                    entry.idempotency_status,
                ],
            )
            .map_err(|e| to_audit_err(e.to_string()))?;
        Ok(())
    }

    /// Query audit entries with filtering and pagination.
    pub fn query(&self, filter: &AuditFilter) -> AuditResult<PagedResult<AuditEntry>> {
        let mut conditions = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref start) = filter.time_start {
            conditions.push(format!("timestamp >= ?{}", param_values.len() + 1));
            param_values.push(Box::new(start.clone()));
        }
        if let Some(ref end) = filter.time_end {
            conditions.push(format!("timestamp <= ?{}", param_values.len() + 1));
            param_values.push(Box::new(end.clone()));
        }
        if let Some(ref agent) = filter.agent_id {
            conditions.push(format!("agent_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(agent.clone()));
        }
        if let Some(ref et) = filter.event_type {
            conditions.push(format!("event_type = ?{}", param_values.len() + 1));
            param_values.push(Box::new(et.clone()));
        }
        if let Some(ref sev) = filter.severity {
            conditions.push(format!("severity = ?{}", param_values.len() + 1));
            param_values.push(Box::new(sev.clone()));
        }
        if let Some(ref tool) = filter.tool_name {
            conditions.push(format!("tool_name = ?{}", param_values.len() + 1));
            param_values.push(Box::new(tool.clone()));
        }
        if let Some(ref operation_id) = filter.operation_id {
            conditions.push(format!("operation_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(operation_id.clone()));
        }
        if let Some(ref search) = filter.search {
            conditions.push(format!(
                "details LIKE ?{} ESCAPE '\\'",
                param_values.len() + 1
            ));
            // Escape LIKE metacharacters in user input. Backslash must be
            // escaped first to avoid double-escaping the others.
            let escaped = search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            param_values.push(Box::new(format!("%{}%", escaped)));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Count total
        let count_sql = format!("SELECT COUNT(*) FROM audit_log {}", where_clause);
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let total: u64 = self
            .conn
            .query_row(&count_sql, params_ref.as_slice(), |row| row.get(0))
            .map_err(|e| to_audit_err(e.to_string()))?;

        // Fetch page
        let page = filter.page.max(1);
        let page_size = filter.page_size.max(1).min(1000);
        let offset = (page - 1) * page_size;

        let select_sql = format!(
            "SELECT id, timestamp, agent_id, event_type, severity, tool_name, details, session_id,
                    actor_id, operation_id, request_id, idempotency_key, idempotency_status
             FROM audit_log {} ORDER BY timestamp DESC LIMIT ?{} OFFSET ?{}",
            where_clause,
            param_values.len() + 1,
            param_values.len() + 2,
        );

        let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = param_values;
        all_params.push(Box::new(page_size));
        all_params.push(Box::new(offset));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            all_params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self
            .conn
            .prepare(&select_sql)
            .map_err(|e| to_audit_err(e.to_string()))?;

        let items = stmt
            .query_map(params_ref.as_slice(), |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    agent_id: row.get(2)?,
                    event_type: row.get(3)?,
                    severity: row.get(4)?,
                    tool_name: row.get(5)?,
                    details: row.get(6)?,
                    session_id: row.get(7)?,
                    actor_id: row.get(8)?,
                    operation_id: row.get(9)?,
                    request_id: row.get(10)?,
                    idempotency_key: row.get(11)?,
                    idempotency_status: row.get(12)?,
                })
            })
            .map_err(|e| to_audit_err(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| to_audit_err(e.to_string()))?;

        Ok(PagedResult {
            items,
            page,
            page_size,
            total,
        })
    }
}
