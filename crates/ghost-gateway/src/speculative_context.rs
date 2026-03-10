use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use chrono::{SecondsFormat, Utc};
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
use cortex_storage::queries::context_attempt_job_queries::{
    insert_job, mark_job_failed, mark_job_running, mark_job_succeeded, select_due_jobs,
    NewContextAttemptJob,
};
use cortex_storage::queries::context_attempt_promotion_queries::{
    insert_promotion, latest_for_attempt, NewContextAttemptPromotion,
};
use cortex_storage::queries::context_attempt_queries::{
    expire_due_attempts, get_attempt, insert_attempt, update_attempt_status, NewContextAttempt,
};
use cortex_storage::queries::context_attempt_validation_queries::{
    insert_validation, NewContextAttemptValidation,
};
use cortex_storage::queries::{
    memory_audit_queries, memory_event_queries, memory_snapshot_queries,
};
use ghost_agent_loop::output_inspector::InspectionResult;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::api::runtime_execution::{inspect_text_safety, inspection_safety_status};
use crate::state::AppState;

const ATTEMPT_KIND_SUMMARY: &str = "summary";
const ATTEMPT_KIND_FACT_CANDIDATE: &str = "fact_candidate";
const JOB_TYPE_DEEP_VALIDATE: &str = "deep_validate";
const JOB_TYPE_PROMOTE: &str = "promote";
const FAST_GATE_VERSION: i64 = 1;
const MAX_RETRY_COUNT: i64 = 3;
const ATTEMPT_TTL_HOURS: i64 = 6;
const VALIDATION_BATCH_SIZE: u32 = 32;
const PROMOTION_BATCH_SIZE: u32 = 16;
const EXPIRY_BATCH_SIZE: u32 = 64;
const DEEP_VALIDATION_MEMORY_SCAN_LIMIT: u32 = 25;
const SUMMARY_MAX_CHARS: usize = 600;
const FACT_CANDIDATE_MAX_CHARS: usize = 220;
const STATUS_WINDOW_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpeculativeContextStatus {
    pub enabled: bool,
    pub available: bool,
    pub window_hours: u32,
    pub attempts_created: usize,
    pub promotions_created: usize,
    pub retrievable_rate: f64,
    pub blocked_rate: f64,
    pub expired_rate: f64,
    pub status_counts: BTreeMap<String, usize>,
    pub pending_job_depth: usize,
    pub pending_jobs_by_type: BTreeMap<String, usize>,
    pub dead_letter_jobs: usize,
    pub ttl_backlog: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompletedTurnInput {
    pub agent_id: Uuid,
    pub session_id: Uuid,
    pub turn_id: String,
    pub route_kind: &'static str,
    pub user_message: String,
    pub assistant_message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FastGateStatus {
    Retrievable,
    Flagged,
    Blocked,
}

impl FastGateStatus {
    fn as_db_status(self) -> &'static str {
        match self {
            Self::Retrievable => "retrievable",
            Self::Flagged => "flagged",
            Self::Blocked => "blocked",
        }
    }

    fn should_enqueue_validation(self) -> bool {
        !matches!(self, Self::Blocked)
    }

    fn validation_decision(self) -> &'static str {
        match self {
            Self::Retrievable => "passed",
            Self::Flagged => "flagged",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone)]
struct FastGateOutcome {
    status: FastGateStatus,
    severity: f64,
    confidence: f64,
    retrieval_weight: f64,
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct AttemptDraft {
    kind: &'static str,
    content: String,
    promotion_candidate: bool,
    retrieval_weight: f64,
}

pub async fn record_completed_turn(
    state: &Arc<AppState>,
    input: CompletedTurnInput,
) -> anyhow::Result<usize> {
    let drafts = build_attempt_drafts(&input.user_message, &input.assistant_message);
    if drafts.is_empty() {
        return Ok(0);
    }

    let agent_id = input.agent_id.to_string();
    let session_id = input.session_id.to_string();
    let now = Utc::now();
    let expires_at = (now + chrono::Duration::hours(ATTEMPT_TTL_HOURS))
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let conn = state.db.write().await;
    let mut recorded = 0;

    for draft in drafts {
        let inspection = inspect_text_safety(&draft.content, input.agent_id);
        let mut gate = fast_gate_outcome(&inspection, &draft.content);
        gate.retrieval_weight = draft.retrieval_weight;
        let attempt_id = Uuid::now_v7().to_string();
        let source_refs = json!({
            "route": input.route_kind,
            "attempt_kind": draft.kind,
            "user_message": truncate_for_summary(&input.user_message, 240),
            "assistant_message": truncate_for_summary(&input.assistant_message, 240),
            "safety_status": inspection_safety_status(&inspection),
        })
        .to_string();
        let source_hash = blake3::hash(draft.content.as_bytes());

        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: &attempt_id,
                agent_id: &agent_id,
                session_id: &session_id,
                turn_id: &input.turn_id,
                attempt_kind: draft.kind,
                content: &draft.content,
                redacted_content: None,
                status: gate.status.as_db_status(),
                severity: gate.severity,
                confidence: gate.confidence,
                retrieval_weight: gate.retrieval_weight,
                source_refs: &source_refs,
                source_hash: Some(source_hash.as_bytes()),
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: draft.promotion_candidate
                    && matches!(gate.status, FastGateStatus::Retrievable),
                expires_at: &expires_at,
            },
        )
        .with_context(|| format!("insert speculative attempt {attempt_id}"))?;

        let details_json = json!({
            "attempt_kind": draft.kind,
            "safety_status": inspection_safety_status(&inspection),
            "severity": gate.severity,
            "retrieval_weight": gate.retrieval_weight,
        })
        .to_string();
        insert_validation(
            &conn,
            &NewContextAttemptValidation {
                id: &Uuid::now_v7().to_string(),
                attempt_id: &attempt_id,
                gate_name: "fast_gate",
                decision: gate.status.validation_decision(),
                reason: gate.reason.as_deref(),
                score: Some(gate.severity),
                details_json: Some(&details_json),
            },
        )
        .with_context(|| format!("insert fast-gate validation for {attempt_id}"))?;

        if gate.status.should_enqueue_validation() {
            let run_after = now.to_rfc3339_opts(SecondsFormat::Secs, true);
            insert_job(
                &conn,
                &NewContextAttemptJob {
                    id: &Uuid::now_v7().to_string(),
                    attempt_id: &attempt_id,
                    job_type: JOB_TYPE_DEEP_VALIDATE,
                    status: "pending",
                    retry_count: 0,
                    last_error: None,
                    run_after: &run_after,
                },
            )
            .with_context(|| format!("enqueue deep validation for {attempt_id}"))?;
        }

        recorded += 1;
    }

    Ok(recorded)
}

pub async fn validation_worker_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.tick().await;

    loop {
        interval.tick().await;
        match process_due_validation_jobs(&state, VALIDATION_BATCH_SIZE).await {
            Ok(processed) if processed > 0 => {
                tracing::info!(processed, "speculative context validation batch completed");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(error = %error, "speculative context validation batch failed")
            }
        }
    }
}

pub async fn promotion_worker_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.tick().await;

    loop {
        interval.tick().await;
        match process_due_promotion_jobs(&state, PROMOTION_BATCH_SIZE).await {
            Ok(processed) if processed > 0 => {
                tracing::info!(processed, "speculative context promotion batch completed");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(error = %error, "speculative context promotion batch failed")
            }
        }
    }
}

pub async fn expiry_worker_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.tick().await;

    loop {
        interval.tick().await;
        match expire_due_attempts_batch(&state, EXPIRY_BATCH_SIZE).await {
            Ok(expired) if expired > 0 => {
                tracing::info!(expired, "speculative context expiry batch completed");
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(error = %error, "speculative context expiry batch failed"),
        }
    }
}

pub async fn status(state: &Arc<AppState>) -> SpeculativeContextStatus {
    let conn = match state.db.read() {
        Ok(conn) => conn,
        Err(error) => {
            return SpeculativeContextStatus {
                enabled: true,
                available: false,
                window_hours: STATUS_WINDOW_HOURS as u32,
                error: Some(error.to_string()),
                ..SpeculativeContextStatus::default()
            };
        }
    };

    match status_from_connection(&conn) {
        Ok(status) => status,
        Err(error) => SpeculativeContextStatus {
            enabled: true,
            available: false,
            window_hours: STATUS_WINDOW_HOURS as u32,
            error: Some(error.to_string()),
            ..SpeculativeContextStatus::default()
        },
    }
}

pub async fn process_due_validation_jobs(
    state: &Arc<AppState>,
    limit: u32,
) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let conn = state.db.write().await;
    let jobs = select_due_jobs(&conn, JOB_TYPE_DEEP_VALIDATE, &now, limit)
        .context("select speculative validation jobs")?;
    let mut processed = 0;

    for job in jobs {
        if !mark_job_running(&conn, &job.id).context("mark validation job running")? {
            continue;
        }

        match validate_attempt(&conn, &job.attempt_id) {
            Ok(()) => {
                mark_job_succeeded(&conn, &job.id).context("mark validation job succeeded")?;
                processed += 1;
            }
            Err(error) => {
                fail_job_with_backoff(&conn, &job.id, job.retry_count + 1, &error)
                    .context("mark validation job failed")?;
            }
        }
    }

    Ok(processed)
}

pub async fn process_due_promotion_jobs(
    state: &Arc<AppState>,
    limit: u32,
) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let conn = state.db.write().await;
    let jobs = select_due_jobs(&conn, JOB_TYPE_PROMOTE, &now, limit)
        .context("select speculative promotion jobs")?;
    let mut processed = 0;

    for job in jobs {
        if !mark_job_running(&conn, &job.id).context("mark promotion job running")? {
            continue;
        }

        match promote_attempt(&conn, &job.attempt_id) {
            Ok(()) => {
                mark_job_succeeded(&conn, &job.id).context("mark promotion job succeeded")?;
                processed += 1;
            }
            Err(error) => {
                fail_job_with_backoff(&conn, &job.id, job.retry_count + 1, &error)
                    .context("mark promotion job failed")?;
            }
        }
    }

    Ok(processed)
}

pub async fn expire_due_attempts_batch(state: &Arc<AppState>, limit: u32) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let conn = state.db.write().await;
    expire_due_attempts(&conn, &now, limit).context("expire speculative attempts")
}

fn validate_attempt(conn: &Connection, attempt_id: &str) -> anyhow::Result<()> {
    let Some(attempt) = get_attempt(conn, attempt_id).context("load speculative attempt")? else {
        return Ok(());
    };

    if matches!(attempt.status.as_str(), "blocked" | "expired" | "promoted") {
        return Ok(());
    }

    if let Some(memory_id) = find_duplicate_durable_memory_id(conn, &attempt)? {
        update_attempt_status(conn, &attempt.id, "flagged", Some(&memory_id))
            .context("mark duplicate speculative attempt flagged")?;
        let details = json!({
            "attempt_kind": attempt.attempt_kind,
            "memory_id": memory_id,
        })
        .to_string();
        insert_validation(
            conn,
            &NewContextAttemptValidation {
                id: &Uuid::now_v7().to_string(),
                attempt_id: &attempt.id,
                gate_name: "deep_validate",
                decision: "flagged",
                reason: Some("matching durable memory already exists"),
                score: Some(1.0),
                details_json: Some(&details),
            },
        )
        .context("insert duplicate validation record")?;
        return Ok(());
    }

    if attempt.status == "flagged" {
        insert_validation(
            conn,
            &NewContextAttemptValidation {
                id: &Uuid::now_v7().to_string(),
                attempt_id: &attempt.id,
                gate_name: "deep_validate",
                decision: "flagged",
                reason: Some("fast gate warning retained outside retrieval path"),
                score: Some(attempt.severity),
                details_json: None,
            },
        )
        .context("insert flagged validation record")?;
        return Ok(());
    }

    if attempt.status == "retrievable"
        && attempt.promotion_candidate
        && attempt.attempt_kind == ATTEMPT_KIND_FACT_CANDIDATE
    {
        let run_after = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        insert_validation(
            conn,
            &NewContextAttemptValidation {
                id: &Uuid::now_v7().to_string(),
                attempt_id: &attempt.id,
                gate_name: "deep_validate",
                decision: "passed",
                reason: Some("fact candidate cleared deep validation"),
                score: Some(attempt.confidence),
                details_json: Some(
                    &json!({
                        "attempt_kind": attempt.attempt_kind,
                        "promotion_eligible": true,
                    })
                    .to_string(),
                ),
            },
        )
        .context("insert promotion-ready validation record")?;
        insert_job(
            conn,
            &NewContextAttemptJob {
                id: &Uuid::now_v7().to_string(),
                attempt_id: &attempt.id,
                job_type: JOB_TYPE_PROMOTE,
                status: "pending",
                retry_count: 0,
                last_error: None,
                run_after: &run_after,
            },
        )
        .context("enqueue promotion job")?;
        return Ok(());
    }

    insert_validation(
        conn,
        &NewContextAttemptValidation {
            id: &Uuid::now_v7().to_string(),
            attempt_id: &attempt.id,
            gate_name: "deep_validate",
            decision: "deferred",
            reason: Some("attempt remains session-scoped speculative context"),
            score: Some(attempt.confidence),
            details_json: Some(
                &json!({
                    "attempt_kind": attempt.attempt_kind,
                    "promotion_eligible": false,
                })
                .to_string(),
            ),
        },
    )
    .context("insert retained validation record")?;

    Ok(())
}

fn promote_attempt(conn: &Connection, attempt_id: &str) -> anyhow::Result<()> {
    let Some(attempt) = get_attempt(conn, attempt_id).context("load speculative attempt")? else {
        return Ok(());
    };

    if attempt.status != "retrievable"
        || !attempt.promotion_candidate
        || attempt.attempt_kind != ATTEMPT_KIND_FACT_CANDIDATE
    {
        return Ok(());
    }

    if latest_for_attempt(conn, &attempt.id)
        .context("load prior promotion linkage")?
        .is_some()
    {
        update_attempt_status(conn, &attempt.id, "promoted", None)
            .context("re-mark speculative attempt promoted from existing linkage")?;
        return Ok(());
    }

    if let Some(memory_id) = find_duplicate_durable_memory_id(conn, &attempt)? {
        update_attempt_status(conn, &attempt.id, "flagged", Some(&memory_id))
            .context("flag duplicate attempt during promotion")?;
        let details = json!({
            "attempt_kind": attempt.attempt_kind,
            "memory_id": memory_id,
        })
        .to_string();
        insert_validation(
            conn,
            &NewContextAttemptValidation {
                id: &Uuid::now_v7().to_string(),
                attempt_id: &attempt.id,
                gate_name: "promotion",
                decision: "flagged",
                reason: Some("duplicate durable memory detected during promotion"),
                score: Some(1.0),
                details_json: Some(&details),
            },
        )
        .context("insert promotion duplicate record")?;
        return Ok(());
    }

    let memory_id = Uuid::now_v7().to_string();
    let promotion_type = "semantic_fact";
    let now = Utc::now();
    let snapshot = durable_memory_snapshot(&attempt, &memory_id, now);
    let snapshot_json = serde_json::to_string(&snapshot).context("serialize durable memory")?;
    let state_hash = blake3::hash(snapshot_json.as_bytes());
    let event_hash = blake3::hash(format!("{}:{}", attempt.id, attempt.content).as_bytes());
    let audit_details = format!(
        "speculative_attempt_id={}, kind={}, promotion_type={promotion_type}",
        attempt.id, attempt.attempt_kind
    );

    memory_event_queries::insert_event(
        conn,
        &memory_id,
        "speculative_promotion",
        &attempt.content,
        &attempt.agent_id,
        event_hash.as_bytes(),
        &[0u8; 32],
    )
    .context("insert speculative promotion memory event")?;
    memory_snapshot_queries::insert_snapshot(
        conn,
        &memory_id,
        &snapshot_json,
        Some(state_hash.as_bytes()),
    )
    .context("insert speculative promotion memory snapshot")?;
    memory_audit_queries::insert_audit(
        conn,
        &memory_id,
        "speculative_promotion",
        Some(&audit_details),
    )
    .context("insert speculative promotion memory audit")?;
    insert_promotion(
        conn,
        &NewContextAttemptPromotion {
            id: &Uuid::now_v7().to_string(),
            attempt_id: &attempt.id,
            promoted_memory_id: &memory_id,
            promotion_type,
        },
    )
    .context("insert promotion linkage")?;
    update_attempt_status(conn, &attempt.id, "promoted", None)
        .context("mark speculative attempt promoted")?;
    let details = json!({
        "attempt_kind": attempt.attempt_kind,
        "memory_id": memory_id,
        "promotion_type": promotion_type,
    })
    .to_string();
    insert_validation(
        conn,
        &NewContextAttemptValidation {
            id: &Uuid::now_v7().to_string(),
            attempt_id: &attempt.id,
            gate_name: "promotion",
            decision: "passed",
            reason: Some("fact candidate promoted into durable memory"),
            score: Some(attempt.confidence),
            details_json: Some(&details),
        },
    )
    .context("insert promotion record")?;

    tracing::info!(
        attempt_id = %attempt.id,
        memory_id = %memory_id,
        promotion_type,
        "speculative fact candidate promoted"
    );

    Ok(())
}

fn durable_memory_snapshot(
    attempt: &cortex_storage::queries::context_attempt_queries::ContextAttemptRow,
    memory_id: &str,
    created_at: chrono::DateTime<Utc>,
) -> BaseMemory {
    let (memory_type, content, summary) = match attempt.attempt_kind.as_str() {
        ATTEMPT_KIND_FACT_CANDIDATE => (
            MemoryType::Semantic,
            json!({
                "fact": attempt.content,
                "source_refs": serde_json::from_str::<serde_json::Value>(&attempt.source_refs).unwrap_or_else(|_| json!([])),
                "speculative_attempt_id": attempt.id,
                "promotion_source": "phase3",
            }),
            truncate_for_summary(&attempt.content, 160),
        ),
        _ => (
            MemoryType::Context,
            json!({
                "speculative_summary": attempt.content,
                "source_refs": serde_json::from_str::<serde_json::Value>(&attempt.source_refs).unwrap_or_else(|_| json!([])),
                "speculative_attempt_id": attempt.id,
                "promotion_source": "phase3",
            }),
            truncate_for_summary(&attempt.content, 160),
        ),
    };

    BaseMemory {
        id: Uuid::parse_str(memory_id).unwrap_or_else(|_| Uuid::now_v7()),
        memory_type,
        content,
        summary,
        importance: Importance::Normal,
        confidence: attempt.confidence,
        created_at,
        last_accessed: None,
        access_count: 0,
        tags: vec![
            "speculative_context".to_string(),
            "phase3".to_string(),
            attempt.attempt_kind.clone(),
        ],
        archived: false,
    }
}

fn find_duplicate_durable_memory_id(
    conn: &Connection,
    attempt: &cortex_storage::queries::context_attempt_queries::ContextAttemptRow,
) -> anyhow::Result<Option<String>> {
    let durable = memory_snapshot_queries::latest_for_actor(
        conn,
        &attempt.agent_id,
        DEEP_VALIDATION_MEMORY_SCAN_LIMIT,
    )
    .context("load recent durable memories")?;

    for snapshot in durable {
        if let Ok(memory) = serde_json::from_str::<BaseMemory>(&snapshot.snapshot) {
            if memory.summary == attempt.content {
                return Ok(Some(snapshot.memory_id));
            }
            let key = if attempt.attempt_kind == ATTEMPT_KIND_FACT_CANDIDATE {
                "fact"
            } else {
                "speculative_summary"
            };
            if memory.content.get(key).and_then(|value| value.as_str())
                == Some(attempt.content.as_str())
            {
                return Ok(Some(snapshot.memory_id));
            }
        }
    }

    Ok(None)
}

fn fail_job_with_backoff(
    conn: &Connection,
    job_id: &str,
    retry_count: i64,
    error: &anyhow::Error,
) -> anyhow::Result<()> {
    let next_status = if retry_count >= MAX_RETRY_COUNT {
        "failed"
    } else {
        "pending"
    };
    let run_after =
        (Utc::now() + chrono::Duration::seconds(30)).to_rfc3339_opts(SecondsFormat::Secs, true);
    mark_job_failed(
        conn,
        job_id,
        &error.to_string(),
        retry_count,
        next_status,
        &run_after,
    )
    .context("update speculative job retry state")?;
    Ok(())
}

fn build_attempt_drafts(user_message: &str, assistant_message: &str) -> Vec<AttemptDraft> {
    let mut drafts = Vec::new();
    let summary = build_attempt_summary(user_message, assistant_message);
    if !summary.is_empty() {
        drafts.push(AttemptDraft {
            kind: ATTEMPT_KIND_SUMMARY,
            content: summary,
            promotion_candidate: false,
            retrieval_weight: 0.45,
        });
    }

    if let Some(fact_candidate) = build_fact_candidate(assistant_message) {
        drafts.push(AttemptDraft {
            kind: ATTEMPT_KIND_FACT_CANDIDATE,
            content: fact_candidate,
            promotion_candidate: true,
            retrieval_weight: 0.35,
        });
    }

    drafts
}

fn build_attempt_summary(user_message: &str, assistant_message: &str) -> String {
    let user = truncate_for_summary(user_message, 220);
    let assistant = truncate_for_summary(assistant_message, SUMMARY_MAX_CHARS);
    if assistant.is_empty() {
        return String::new();
    }

    format!("User asked: {user}\nAssistant answered: {assistant}")
}

fn build_fact_candidate(assistant_message: &str) -> Option<String> {
    let first_line = assistant_message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    if first_line.contains('?') {
        return None;
    }
    if ["maybe", "might", "could", "i think", "likely", "probably"]
        .iter()
        .any(|token| first_line.to_ascii_lowercase().contains(token))
    {
        return None;
    }

    let sentence = first_line
        .split_terminator(['.', '!', '\n'])
        .next()
        .unwrap_or(first_line)
        .trim();
    let sentence = truncate_for_summary(sentence, FACT_CANDIDATE_MAX_CHARS);
    if sentence.len() < 24 || sentence.contains("User asked:") {
        return None;
    }
    Some(sentence)
}

fn truncate_for_summary(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn fast_gate_outcome(inspection: &InspectionResult, summary: &str) -> FastGateOutcome {
    match inspection {
        InspectionResult::KillAll { pattern_name, .. } => FastGateOutcome {
            status: FastGateStatus::Blocked,
            severity: 1.0,
            confidence: 0.0,
            retrieval_weight: 0.0,
            reason: Some(format!(
                "blocked by output inspection pattern {pattern_name}"
            )),
        },
        InspectionResult::Warning { pattern_name, .. } => FastGateOutcome {
            status: FastGateStatus::Flagged,
            severity: 0.75,
            confidence: bounded_confidence(summary),
            retrieval_weight: 0.0,
            reason: Some(format!(
                "warning from output inspection pattern {pattern_name}"
            )),
        },
        InspectionResult::Clean => FastGateOutcome {
            status: FastGateStatus::Retrievable,
            severity: 0.1,
            confidence: bounded_confidence(summary),
            retrieval_weight: 0.45,
            reason: None,
        },
    }
}

fn bounded_confidence(summary: &str) -> f64 {
    let len = summary.chars().count() as f64;
    (0.55 + (len / 2000.0)).clamp(0.55, 0.92)
}

fn status_from_connection(conn: &Connection) -> anyhow::Result<SpeculativeContextStatus> {
    let now = Utc::now();
    let since = (now - chrono::Duration::hours(STATUS_WINDOW_HOURS))
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let now_rfc3339 = now.to_rfc3339_opts(SecondsFormat::Secs, true);

    let status_counts = count_grouped_strings(
        conn,
        "SELECT status, COUNT(*) FROM context_attempts GROUP BY status",
        rusqlite::params![],
    )
    .context("count speculative attempts by status")?;
    let created_window = query_count(
        conn,
        "SELECT COUNT(*) FROM context_attempts WHERE created_at >= ?1",
        rusqlite::params![since.as_str()],
    )
    .context("count speculative attempts in window")?;
    let window_status_counts = count_grouped_strings(
        conn,
        "SELECT status, COUNT(*) FROM context_attempts WHERE created_at >= ?1 GROUP BY status",
        rusqlite::params![since.as_str()],
    )
    .context("count speculative window statuses")?;
    let promotions_created = query_count(
        conn,
        "SELECT COUNT(*) FROM context_attempt_promotion WHERE created_at >= ?1",
        rusqlite::params![since.as_str()],
    )
    .context("count speculative promotions in window")?;
    let pending_jobs_by_type = count_grouped_strings(
        conn,
        "SELECT job_type, COUNT(*) FROM context_attempt_jobs
         WHERE status IN ('pending', 'running')
         GROUP BY job_type",
        rusqlite::params![],
    )
    .context("count speculative pending jobs by type")?;
    let dead_letter_jobs = query_count(
        conn,
        "SELECT COUNT(*) FROM context_attempt_jobs WHERE status = 'dead_letter'",
        rusqlite::params![],
    )
    .context("count speculative dead-letter jobs")?;
    let ttl_backlog = query_count(
        conn,
        "SELECT COUNT(*) FROM context_attempts
         WHERE status IN ('pending', 'retrievable', 'flagged')
           AND expires_at <= ?1",
        rusqlite::params![now_rfc3339.as_str()],
    )
    .context("count speculative ttl backlog")?;

    let pending_job_depth = pending_jobs_by_type.values().copied().sum();

    Ok(SpeculativeContextStatus {
        enabled: true,
        available: true,
        window_hours: STATUS_WINDOW_HOURS as u32,
        attempts_created: created_window as usize,
        promotions_created: promotions_created as usize,
        retrievable_rate: rate_from_counts(window_status_counts.get("retrievable"), created_window),
        blocked_rate: rate_from_counts(window_status_counts.get("blocked"), created_window),
        expired_rate: rate_from_counts(window_status_counts.get("expired"), created_window),
        status_counts,
        pending_job_depth,
        pending_jobs_by_type,
        dead_letter_jobs: dead_letter_jobs as usize,
        ttl_backlog: ttl_backlog as usize,
        error: None,
    })
}

fn query_count<P>(conn: &Connection, sql: &str, params: P) -> anyhow::Result<i64>
where
    P: rusqlite::Params,
{
    conn.query_row(sql, params, |row| row.get(0))
        .map_err(anyhow::Error::from)
}

fn count_grouped_strings<P>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> anyhow::Result<BTreeMap<String, usize>>
where
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (key, count) = row?;
        out.insert(key, count.max(0) as usize);
    }
    Ok(out)
}

fn rate_from_counts(count: Option<&usize>, total: i64) -> f64 {
    if total <= 0 {
        0.0
    } else {
        count.copied().unwrap_or_default() as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_storage::queries::context_attempt_job_queries::{get_job, select_due_jobs};
    use cortex_storage::queries::context_attempt_promotion_queries::latest_for_attempt;
    use cortex_storage::queries::context_attempt_queries::get_attempt;
    use cortex_storage::run_all_migrations;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_all_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn attempt_drafts_include_summary_and_controlled_fact_candidate() {
        let drafts = build_attempt_drafts(
            "remember the deployment caveat",
            "The service now relies on the shared hydrator path. It also keeps route handlers thin.",
        );

        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].kind, ATTEMPT_KIND_SUMMARY);
        assert_eq!(drafts[1].kind, ATTEMPT_KIND_FACT_CANDIDATE);
        assert!(drafts[1].promotion_candidate);
    }

    #[test]
    fn deep_validation_only_enqueues_promotion_for_fact_candidates() {
        let conn = setup();
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "summary-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: ATTEMPT_KIND_SUMMARY,
                content: "User asked: status\nAssistant answered: use the hydrated path",
                redacted_content: None,
                status: "retrievable",
                severity: 0.1,
                confidence: 0.7,
                retrieval_weight: 0.45,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: false,
                expires_at: "2026-03-10T12:00:00Z",
            },
        )
        .unwrap();
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "fact-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: ATTEMPT_KIND_FACT_CANDIDATE,
                content: "The service now relies on the shared hydrator path",
                redacted_content: None,
                status: "retrievable",
                severity: 0.1,
                confidence: 0.7,
                retrieval_weight: 0.35,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: true,
                expires_at: "2026-03-10T12:00:00Z",
            },
        )
        .unwrap();

        validate_attempt(&conn, "summary-1").unwrap();
        validate_attempt(&conn, "fact-1").unwrap();

        let jobs = select_due_jobs(&conn, JOB_TYPE_PROMOTE, "9999-01-01T00:00:00Z", 10).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].attempt_id, "fact-1");
    }

    #[test]
    fn promotion_creates_linked_semantic_memory_for_fact_candidate() {
        let conn = setup();
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "fact-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: ATTEMPT_KIND_FACT_CANDIDATE,
                content: "The service now relies on the shared hydrator path",
                redacted_content: None,
                status: "retrievable",
                severity: 0.1,
                confidence: 0.7,
                retrieval_weight: 0.35,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: true,
                expires_at: "2026-03-10T12:00:00Z",
            },
        )
        .unwrap();

        promote_attempt(&conn, "fact-1").unwrap();

        let attempt = get_attempt(&conn, "fact-1").unwrap().unwrap();
        assert_eq!(attempt.status, "promoted");
        let linkage = latest_for_attempt(&conn, "fact-1").unwrap().unwrap();
        assert_eq!(linkage.promotion_type, "semantic_fact");

        let latest = memory_snapshot_queries::latest_by_memory(&conn, &linkage.promoted_memory_id)
            .unwrap()
            .unwrap();
        let memory: BaseMemory = serde_json::from_str(&latest.snapshot).unwrap();
        assert_eq!(memory.memory_type, MemoryType::Semantic);
        assert_eq!(
            memory.content["speculative_attempt_id"].as_str(),
            Some("fact-1")
        );
        assert_eq!(
            memory.content["fact"].as_str(),
            Some("The service now relies on the shared hydrator path")
        );
    }

    #[test]
    fn summary_attempts_remain_speculative_only() {
        let conn = setup();
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "summary-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: ATTEMPT_KIND_SUMMARY,
                content: "User asked: status\nAssistant answered: use the hydrated path",
                redacted_content: None,
                status: "retrievable",
                severity: 0.1,
                confidence: 0.7,
                retrieval_weight: 0.45,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: false,
                expires_at: "2026-03-10T12:00:00Z",
            },
        )
        .unwrap();

        promote_attempt(&conn, "summary-1").unwrap();

        let attempt = get_attempt(&conn, "summary-1").unwrap().unwrap();
        assert_eq!(attempt.status, "retrievable");
        assert!(latest_for_attempt(&conn, "summary-1").unwrap().is_none());
    }

    #[test]
    fn duplicate_fact_promotion_is_flagged_instead_of_reinserted() {
        let conn = setup();
        let existing_memory_id = Uuid::now_v7().to_string();
        let snapshot = serde_json::to_string(&BaseMemory {
            id: Uuid::parse_str(&existing_memory_id).unwrap(),
            memory_type: MemoryType::Semantic,
            content: json!({"fact": "The service now relies on the shared hydrator path"}),
            summary: "The service now relies on the shared hydrator path".into(),
            importance: Importance::Normal,
            confidence: 0.8,
            created_at: Utc::now(),
            last_accessed: None,
            access_count: 0,
            tags: vec![],
            archived: false,
        })
        .unwrap();
        let state_hash = blake3::hash(snapshot.as_bytes());
        memory_event_queries::insert_event(
            &conn,
            &existing_memory_id,
            "seed",
            "The service now relies on the shared hydrator path",
            "agent-a",
            blake3::hash(b"seed").as_bytes(),
            &[0u8; 32],
        )
        .unwrap();
        memory_snapshot_queries::insert_snapshot(
            &conn,
            &existing_memory_id,
            &snapshot,
            Some(state_hash.as_bytes()),
        )
        .unwrap();

        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "fact-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: ATTEMPT_KIND_FACT_CANDIDATE,
                content: "The service now relies on the shared hydrator path",
                redacted_content: None,
                status: "retrievable",
                severity: 0.1,
                confidence: 0.7,
                retrieval_weight: 0.35,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: true,
                expires_at: "2026-03-10T12:00:00Z",
            },
        )
        .unwrap();

        promote_attempt(&conn, "fact-1").unwrap();

        let attempt = get_attempt(&conn, "fact-1").unwrap().unwrap();
        assert_eq!(attempt.status, "flagged");
        assert_eq!(
            attempt.contradicted_by_memory_id.as_deref(),
            Some(existing_memory_id.as_str())
        );
    }

    #[test]
    fn fail_job_backoff_marks_terminal_after_max_retries() {
        let conn = setup();
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "fact-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: ATTEMPT_KIND_FACT_CANDIDATE,
                content: "The service now relies on the shared hydrator path",
                redacted_content: None,
                status: "retrievable",
                severity: 0.1,
                confidence: 0.7,
                retrieval_weight: 0.35,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: true,
                expires_at: "2026-03-10T12:00:00Z",
            },
        )
        .unwrap();
        insert_job(
            &conn,
            &NewContextAttemptJob {
                id: "job-1",
                attempt_id: "fact-1",
                job_type: JOB_TYPE_PROMOTE,
                status: "running",
                retry_count: 0,
                last_error: None,
                run_after: "2026-03-10T10:00:00Z",
            },
        )
        .unwrap();

        fail_job_with_backoff(&conn, "job-1", MAX_RETRY_COUNT, &anyhow::anyhow!("boom")).unwrap();

        let job = get_job(&conn, "job-1").unwrap().unwrap();
        assert_eq!(job.status, "failed");
        assert_eq!(job.retry_count, MAX_RETRY_COUNT);
    }

    #[test]
    fn status_summary_reports_quality_and_capacity_metrics() {
        let conn = setup();
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "summary-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: ATTEMPT_KIND_SUMMARY,
                content: "summary",
                redacted_content: None,
                status: "retrievable",
                severity: 0.1,
                confidence: 0.7,
                retrieval_weight: 0.45,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: false,
                expires_at: "2999-01-01T00:00:00Z",
            },
        )
        .unwrap();
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: "fact-1",
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-2",
                attempt_kind: ATTEMPT_KIND_FACT_CANDIDATE,
                content: "fact candidate",
                redacted_content: None,
                status: "blocked",
                severity: 0.9,
                confidence: 0.6,
                retrieval_weight: 0.0,
                source_refs: "[]",
                source_hash: None,
                fast_gate_version: FAST_GATE_VERSION,
                contradicted_by_memory_id: None,
                promotion_candidate: false,
                expires_at: "2000-01-01T00:00:00Z",
            },
        )
        .unwrap();
        insert_job(
            &conn,
            &NewContextAttemptJob {
                id: "job-1",
                attempt_id: "summary-1",
                job_type: JOB_TYPE_DEEP_VALIDATE,
                status: "pending",
                retry_count: 0,
                last_error: None,
                run_after: "2999-01-01T00:00:00Z",
            },
        )
        .unwrap();

        let summary = status_from_connection(&conn).unwrap();
        assert!(summary.available);
        assert_eq!(summary.attempts_created, 2);
        assert_eq!(summary.status_counts.get("retrievable"), Some(&1));
        assert_eq!(summary.status_counts.get("blocked"), Some(&1));
        assert_eq!(
            summary.pending_jobs_by_type.get(JOB_TYPE_DEEP_VALIDATE),
            Some(&1)
        );
        assert_eq!(summary.pending_job_depth, 1);
        assert!(summary.retrievable_rate > 0.0);
        assert!(summary.blocked_rate > 0.0);
    }
}
