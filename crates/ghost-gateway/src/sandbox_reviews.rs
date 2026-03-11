use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use ghost_agent_loop::tools::executor::{
    SandboxReviewDecision, SandboxReviewRequest, SandboxReviewRequestEnvelope,
};
use ghost_audit::query_engine::{AuditEntry, AuditQueryEngine};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::api::websocket::{EventReplayBuffer, WsEnvelope, WsEvent};
use crate::db_pool::DbPool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxReviewRecord {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub execution_id: Option<String>,
    pub route_kind: Option<String>,
    pub tool_name: String,
    pub violation_reason: String,
    pub sandbox_mode: String,
    pub status: String,
    pub resolution_note: Option<String>,
    pub resolved_by: Option<String>,
    pub requested_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct AgentSandboxMetrics {
    pub pending_reviews: u32,
    pub total_reviews: u32,
    pub approved_reviews: u32,
    pub rejected_reviews: u32,
    pub expired_reviews: u32,
    pub last_requested_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxReviewListParams {
    pub status: Option<String>,
    pub agent_id: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxReviewDecisionRequest {
    pub note: Option<String>,
}

struct PendingReview {
    agent_id: String,
    tool_name: String,
    decision_tx: oneshot::Sender<SandboxReviewDecision>,
}

#[derive(Clone)]
pub struct SandboxReviewCoordinator {
    db: Arc<DbPool>,
    replay_buffer: Arc<EventReplayBuffer>,
    event_tx: tokio::sync::broadcast::Sender<WsEnvelope>,
    pending: Arc<DashMap<String, PendingReview>>,
    request_tx: mpsc::Sender<SandboxReviewRequestEnvelope>,
}

impl SandboxReviewCoordinator {
    pub fn new(
        db: Arc<DbPool>,
        replay_buffer: Arc<EventReplayBuffer>,
        event_tx: tokio::sync::broadcast::Sender<WsEnvelope>,
    ) -> Arc<Self> {
        let (request_tx, mut request_rx) = mpsc::channel::<SandboxReviewRequestEnvelope>(64);
        let coordinator = Arc::new(Self {
            db,
            replay_buffer,
            event_tx,
            pending: Arc::new(DashMap::new()),
            request_tx,
        });

        let worker = Arc::clone(&coordinator);
        tokio::spawn(async move {
            while let Some(envelope) = request_rx.recv().await {
                if let Err(error) = worker.register_request(envelope).await {
                    tracing::error!(error = %error, "failed to register sandbox review");
                }
            }
        });

        coordinator
    }

    pub fn request_sender(&self) -> mpsc::Sender<SandboxReviewRequestEnvelope> {
        self.request_tx.clone()
    }

    async fn register_request(
        self: &Arc<Self>,
        envelope: SandboxReviewRequestEnvelope,
    ) -> Result<(), String> {
        let request = envelope.request;
        self.insert_review_row(&request).await?;
        self.insert_audit(
            request.agent_id.to_string(),
            "sandbox_review_requested".to_string(),
            "warn".to_string(),
            Some(request.tool_name.clone()),
            serde_json::json!({
                "review_id": request.review_id,
                "session_id": request.session_id,
                "execution_id": request.execution_id,
                "route_kind": request.route_kind,
                "violation_reason": request.violation_reason,
                "sandbox_mode": request.sandbox_mode,
            }),
            Some("builtin_sandbox".to_string()),
        )
        .await?;

        self.pending.insert(
            request.review_id.clone(),
            PendingReview {
                agent_id: request.agent_id.to_string(),
                tool_name: request.tool_name.clone(),
                decision_tx: envelope.decision_tx,
            },
        );

        self.broadcast(WsEvent::SandboxReviewRequested {
            review_id: request.review_id.clone(),
            agent_id: request.agent_id.to_string(),
            tool_name: request.tool_name.clone(),
            status: "pending".into(),
        });

        let timer_coordinator = Arc::clone(self);
        let review_id = request.review_id.clone();
        let timeout = Duration::from_secs(request.timeout_secs.max(1));
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            let _ = timer_coordinator
                .resolve(
                    &review_id,
                    SandboxReviewDecision::Expired,
                    "system",
                    Some("review timed out".into()),
                )
                .await;
        });

        Ok(())
    }

    async fn insert_review_row(&self, request: &SandboxReviewRequest) -> Result<(), String> {
        let writer = self.db.write().await;
        writer
            .execute(
                "INSERT INTO sandbox_review_requests (
                    id, agent_id, session_id, execution_id, route_kind, tool_name,
                    violation_reason, sandbox_mode, status
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
                rusqlite::params![
                    request.review_id,
                    request.agent_id.to_string(),
                    request.session_id.to_string(),
                    request.execution_id,
                    request.route_kind,
                    request.tool_name,
                    request.violation_reason,
                    request.sandbox_mode,
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn insert_audit(
        &self,
        agent_id: String,
        event_type: String,
        severity: String,
        tool_name: Option<String>,
        details: serde_json::Value,
        actor_id: Option<String>,
    ) -> Result<(), String> {
        let writer = self.db.write().await;
        let engine = AuditQueryEngine::new(&writer);
        engine
            .insert(&AuditEntry {
                id: uuid::Uuid::now_v7().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                agent_id,
                event_type,
                severity,
                tool_name,
                details: details.to_string(),
                session_id: None,
                actor_id,
                operation_id: None,
                request_id: None,
                idempotency_key: None,
                idempotency_status: None,
            })
            .map_err(|error| error.to_string())
    }

    fn broadcast(&self, event: WsEvent) {
        if !self
            .replay_buffer
            .push_and_broadcast(event, &self.event_tx)
            .1
        {
            tracing::debug!("sandbox review event broadcast without active listeners");
        }
    }

    pub async fn resolve(
        &self,
        review_id: &str,
        decision: SandboxReviewDecision,
        actor: &str,
        note: Option<String>,
    ) -> Result<bool, String> {
        let status = match decision {
            SandboxReviewDecision::Approved => "approved",
            SandboxReviewDecision::Rejected => "rejected",
            SandboxReviewDecision::Expired => "expired",
        };

        let writer = self.db.write().await;
        let updated = writer
            .execute(
                "UPDATE sandbox_review_requests
                 SET status = ?2,
                     resolution_note = ?3,
                     resolved_by = ?4,
                     resolved_at = datetime('now')
                 WHERE id = ?1 AND status = 'pending'",
                rusqlite::params![review_id, status, note, actor],
            )
            .map_err(|error| error.to_string())?;
        drop(writer);

        if updated == 0 {
            return Ok(false);
        }

        let pending = self.pending.remove(review_id);
        let agent_id = pending
            .as_ref()
            .map(|entry| entry.1.agent_id.clone())
            .unwrap_or_default();
        let tool_name = pending
            .as_ref()
            .map(|entry| entry.1.tool_name.clone())
            .unwrap_or_else(|| "unknown".into());

        if let Some((_, pending)) = pending {
            let _ = pending.decision_tx.send(decision.clone());
        }

        self.insert_audit(
            agent_id.clone(),
            format!("sandbox_review_{}", status),
            if matches!(decision, SandboxReviewDecision::Approved) {
                "info".to_string()
            } else {
                "warn".to_string()
            },
            Some(tool_name.clone()),
            serde_json::json!({
                "review_id": review_id,
                "decision": status,
                "note": note,
            }),
            Some(actor.to_string()),
        )
        .await?;

        self.broadcast(WsEvent::SandboxReviewResolved {
            review_id: review_id.to_string(),
            agent_id,
            tool_name,
            decision: status.to_string(),
            resolved_by: actor.to_string(),
        });

        Ok(true)
    }

    pub async fn list_reviews(
        &self,
        params: &SandboxReviewListParams,
    ) -> Result<Vec<SandboxReviewRecord>, String> {
        let db = self.db.read().map_err(|error| error.to_string())?;
        let mut sql = String::from(
            "SELECT id, agent_id, session_id, execution_id, route_kind, tool_name,
                    violation_reason, sandbox_mode, status, resolution_note, resolved_by,
                    requested_at, resolved_at
             FROM sandbox_review_requests",
        );
        let mut predicates = Vec::new();
        let mut values: Vec<String> = Vec::new();

        if let Some(status) = params.status.as_ref() {
            predicates.push("status = ?".to_string());
            values.push(status.clone());
        }
        if let Some(agent_id) = params.agent_id.as_ref() {
            predicates.push("agent_id = ?".to_string());
            values.push(agent_id.clone());
        }
        if !predicates.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&predicates.join(" AND "));
        }
        sql.push_str(" ORDER BY requested_at DESC LIMIT ?");
        let limit = params.limit.unwrap_or(50).min(200).to_string();
        values.push(limit);

        let mut stmt = db.prepare(&sql).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(values.iter()), |row| {
                Ok(SandboxReviewRecord {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    session_id: row.get(2)?,
                    execution_id: row.get(3)?,
                    route_kind: row.get(4)?,
                    tool_name: row.get(5)?,
                    violation_reason: row.get(6)?,
                    sandbox_mode: row.get(7)?,
                    status: row.get(8)?,
                    resolution_note: row.get(9)?,
                    resolved_by: row.get(10)?,
                    requested_at: row.get(11)?,
                    resolved_at: row.get(12)?,
                })
            })
            .map_err(|error| error.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub async fn agent_metrics(&self) -> Result<HashMap<String, AgentSandboxMetrics>, String> {
        let db = self.db.read().map_err(|error| error.to_string())?;
        let mut stmt = db
            .prepare(
                "SELECT agent_id,
                        SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) AS pending_reviews,
                        COUNT(*) AS total_reviews,
                        SUM(CASE WHEN status = 'approved' THEN 1 ELSE 0 END) AS approved_reviews,
                        SUM(CASE WHEN status = 'rejected' THEN 1 ELSE 0 END) AS rejected_reviews,
                        SUM(CASE WHEN status = 'expired' THEN 1 ELSE 0 END) AS expired_reviews,
                        MAX(requested_at) AS last_requested_at
                 FROM sandbox_review_requests
                 GROUP BY agent_id",
            )
            .map_err(|error| error.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    AgentSandboxMetrics {
                        pending_reviews: row.get::<_, i64>(1)?.max(0) as u32,
                        total_reviews: row.get::<_, i64>(2)?.max(0) as u32,
                        approved_reviews: row.get::<_, i64>(3)?.max(0) as u32,
                        rejected_reviews: row.get::<_, i64>(4)?.max(0) as u32,
                        expired_reviews: row.get::<_, i64>(5)?.max(0) as u32,
                        last_requested_at: row.get(6)?,
                    },
                ))
            })
            .map_err(|error| error.to_string())?;

        rows.collect::<Result<HashMap<_, _>, _>>()
            .map_err(|error| error.to_string())
    }
}
