//! Durable proposal materialization helpers.

use crate::to_storage_err;
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

const APPROVED_STATES: &[&str] = &["approved", "auto_applied"];

#[derive(Debug, Clone)]
pub struct MaterializableProposalRow {
    pub proposal_id: String,
    pub agent_id: String,
    pub operation: String,
    pub target_type: String,
    pub content: String,
    pub lineage_id: String,
    pub subject_key: String,
    pub reviewed_revision: String,
    pub current_state: String,
}

pub fn load_materializable_proposal(
    conn: &Connection,
    proposal_id: &str,
) -> CortexResult<Option<MaterializableProposalRow>> {
    conn.query_row(
        "SELECT
             gp.id,
             gp.agent_id,
             gp.operation,
             gp.target_type,
             gpv2.content,
             gpv2.lineage_id,
             gpv2.subject_key,
             gpv2.reviewed_revision,
             (
                 SELECT to_state
                 FROM goal_proposal_transitions t
                 WHERE t.proposal_id = gp.id
                 ORDER BY rowid DESC
                 LIMIT 1
             ) AS current_state
         FROM goal_proposals gp
         JOIN goal_proposals_v2 gpv2 ON gpv2.id = gp.id
         WHERE gp.id = ?1",
        params![proposal_id],
        |row| {
            Ok(MaterializableProposalRow {
                proposal_id: row.get(0)?,
                agent_id: row.get(1)?,
                operation: row.get(2)?,
                target_type: row.get(3)?,
                content: row.get(4)?,
                lineage_id: row.get(5)?,
                subject_key: row.get(6)?,
                reviewed_revision: row.get(7)?,
                current_state: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
            })
        },
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

pub fn materialize_memory_write_in_transaction(
    conn: &Connection,
    proposal_id: &str,
) -> CortexResult<Option<String>> {
    let Some(proposal) = load_materializable_proposal(conn, proposal_id)? else {
        return Ok(None);
    };

    if proposal.operation != "MemoryWrite"
        || !APPROVED_STATES.contains(&proposal.current_state.as_str())
    {
        return Ok(None);
    }

    let mut content = serde_json::from_str::<Value>(&proposal.content)
        .map_err(|e| to_storage_err(e.to_string()))?;
    let memory_id = derive_memory_id(&proposal, &content)?;
    let existing_hash = crate::queries::memory_event_queries::latest_event_hash(conn, &memory_id)?;
    let recorded_at = chrono::Utc::now().to_rfc3339();

    if !materialization_already_present(conn, &memory_id, &proposal.proposal_id)? {
        if let Some(object) = content.as_object_mut() {
            object
                .entry("source_proposal_id".to_string())
                .or_insert_with(|| Value::String(proposal.proposal_id.clone()));
            object
                .entry("lineage_id".to_string())
                .or_insert_with(|| Value::String(proposal.lineage_id.clone()));
            object
                .entry("subject_key".to_string())
                .or_insert_with(|| Value::String(proposal.subject_key.clone()));
            object
                .entry("reviewed_revision".to_string())
                .or_insert_with(|| Value::String(proposal.reviewed_revision.clone()));
        }

        let memory = build_memory_snapshot(&proposal, &memory_id, &content, &recorded_at)?;
        let snapshot_json =
            serde_json::to_string(&memory).map_err(|e| to_storage_err(e.to_string()))?;
        let state_hash = blake3::hash(snapshot_json.as_bytes());
        let previous_hash = existing_hash
            .as_deref()
            .filter(|hash| hash.len() == 32)
            .unwrap_or(&[0u8; 32]);
        let event_hash = blake3::hash(
            format!("{}:{}:{}", proposal.proposal_id, memory_id, recorded_at).as_bytes(),
        );

        crate::queries::memory_event_queries::insert_event_at(
            conn,
            &memory_id,
            "proposal_materialized",
            &proposal.content,
            &proposal.agent_id,
            &recorded_at,
            event_hash.as_bytes(),
            previous_hash,
        )?;
        crate::queries::memory_snapshot_queries::insert_snapshot(
            conn,
            &memory_id,
            &snapshot_json,
            Some(state_hash.as_bytes()),
        )?;
        crate::queries::memory_audit_queries::insert_audit(
            conn,
            &memory_id,
            "proposal_materialized",
            Some(&format!(
                "proposal_id={}, operation={}, reviewed_revision={}",
                proposal.proposal_id, proposal.operation, proposal.reviewed_revision
            )),
        )?;
    }

    Ok(Some(memory_id))
}

fn materialization_already_present(
    conn: &Connection,
    memory_id: &str,
    proposal_id: &str,
) -> CortexResult<bool> {
    let marker = proposal_id.to_string();
    let exists = conn
        .query_row(
            "SELECT EXISTS(
                 SELECT 1
                 FROM memory_snapshots
                 WHERE memory_id = ?1
                   AND json_extract(snapshot, '$.content.source_proposal_id') = ?2
             )",
            params![memory_id, marker],
            |row| row.get(0),
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(exists)
}

fn derive_memory_id(proposal: &MaterializableProposalRow, content: &Value) -> CortexResult<String> {
    if let Some(memory_id) = content.get("memory_id").and_then(Value::as_str) {
        if Uuid::parse_str(memory_id).is_ok() {
            return Ok(memory_id.to_string());
        }
    }

    let seed = content
        .get("subject_key")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(&proposal.subject_key);
    let uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("ghost:memory:{seed}").as_bytes(),
    );
    Ok(uuid.to_string())
}

fn build_memory_snapshot(
    proposal: &MaterializableProposalRow,
    memory_id: &str,
    content: &Value,
    recorded_at: &str,
) -> CortexResult<BaseMemory> {
    let memory_type = parse_memory_type(
        content
            .get("memory_type")
            .and_then(Value::as_str)
            .unwrap_or(&proposal.target_type),
    );
    let importance = parse_importance(content.get("importance"));
    let confidence = content
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.8)
        .clamp(0.0, 1.0);
    let tags = content
        .get("tags")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(BaseMemory {
        id: Uuid::parse_str(memory_id)
            .unwrap_or_else(|_| Uuid::new_v5(&Uuid::NAMESPACE_URL, memory_id.as_bytes())),
        memory_type,
        content: content.clone(),
        summary: derive_summary(content),
        importance,
        confidence,
        created_at: chrono::DateTime::parse_from_rfc3339(recorded_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        last_accessed: None,
        access_count: 0,
        tags,
        archived: content
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn derive_summary(content: &Value) -> String {
    for key in [
        "summary",
        "goal_text",
        "goal",
        "text",
        "message",
        "description",
        "fact",
    ] {
        if let Some(text) = content.get(key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.chars().take(160).collect();
            }
        }
    }

    if let Some(object) = content.as_object() {
        if let Some((key, value)) = object.iter().find(|(_, value)| value.is_string()) {
            let text = value.as_str().unwrap_or_default().trim();
            if !text.is_empty() {
                return format!("{key}: {}", text.chars().take(140).collect::<String>());
            }
        }
    }

    "Materialized memory".to_string()
}

fn parse_memory_type(raw: &str) -> MemoryType {
    serde_json::from_str::<MemoryType>(&format!("\"{raw}\"")).unwrap_or(MemoryType::Semantic)
}

fn parse_importance(raw: Option<&Value>) -> Importance {
    let Some(raw) = raw.and_then(Value::as_str) else {
        return Importance::Normal;
    };

    match raw.to_ascii_lowercase().as_str() {
        "critical" => Importance::Critical,
        "high" => Importance::High,
        "low" => Importance::Low,
        "trivial" => Importance::Trivial,
        "medium" | "normal" => Importance::Normal,
        _ => Importance::Normal,
    }
}
