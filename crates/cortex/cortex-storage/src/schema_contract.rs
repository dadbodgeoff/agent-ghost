use std::collections::BTreeSet;

use rusqlite::Connection;
use thiserror::Error;

use crate::migrations::{current_version, LATEST_VERSION};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaContractReport {
    pub current_version: u32,
    pub latest_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaProblem {
    pub message: String,
}

impl SchemaProblem {
    fn missing_table(table: &str) -> Self {
        Self {
            message: format!("missing table {table}"),
        }
    }

    fn missing_column(table: &str, column: &str) -> Self {
        Self {
            message: format!("table {table} missing column {column}"),
        }
    }

    fn missing_index(index: &str) -> Self {
        Self {
            message: format!("missing index {index}"),
        }
    }

    fn missing_trigger(trigger: &str) -> Self {
        Self {
            message: format!("missing trigger {trigger}"),
        }
    }
}

#[derive(Debug, Error)]
pub enum SchemaContractError {
    #[error(
        "migration required: database schema version v{current_version} is older than supported v{expected_version}"
    )]
    MigrationRequired {
        current_version: u32,
        expected_version: u32,
    },
    #[error(
        "unsupported newer schema: database schema version v{current_version} is newer than supported v{expected_version}"
    )]
    UnsupportedNewerSchema {
        current_version: u32,
        expected_version: u32,
    },
    #[error("schema verification failed: {summary}")]
    ContractViolation {
        current_version: u32,
        summary: String,
        problems: Vec<SchemaProblem>,
    },
    #[error("database integrity check failed: {0}")]
    Integrity(String),
    #[error("schema inspection failed: {0}")]
    Query(String),
}

struct TableRequirement {
    name: &'static str,
    required_columns: &'static [&'static str],
}

const REQUIRED_TABLES: &[TableRequirement] = &[
    TableRequirement {
        name: "schema_version",
        required_columns: &["version", "name", "applied_at"],
    },
    TableRequirement {
        name: "memory_events",
        required_columns: &[
            "event_id",
            "memory_id",
            "event_type",
            "delta",
            "actor_id",
            "recorded_at",
            "event_hash",
            "previous_hash",
        ],
    },
    TableRequirement {
        name: "memory_audit_log",
        required_columns: &["id", "memory_id", "operation", "timestamp", "details"],
    },
    TableRequirement {
        name: "memory_snapshots",
        required_columns: &[
            "id",
            "memory_id",
            "snapshot",
            "created_at",
            "state_hash",
            "citation_count",
        ],
    },
    TableRequirement {
        name: "itp_events",
        required_columns: &[
            "id",
            "session_id",
            "event_type",
            "sender",
            "timestamp",
            "sequence_number",
            "event_hash",
            "previous_hash",
        ],
    },
    TableRequirement {
        name: "convergence_scores",
        required_columns: &[
            "id",
            "agent_id",
            "session_id",
            "composite_score",
            "signal_scores",
            "level",
            "profile",
            "computed_at",
            "event_hash",
            "previous_hash",
        ],
    },
    TableRequirement {
        name: "intervention_history",
        required_columns: &[
            "id",
            "agent_id",
            "session_id",
            "intervention_level",
            "previous_level",
            "trigger_score",
            "trigger_signals",
            "action_type",
            "event_hash",
            "previous_hash",
        ],
    },
    TableRequirement {
        name: "goal_proposals",
        required_columns: &[
            "id",
            "agent_id",
            "session_id",
            "proposer_type",
            "operation",
            "target_type",
            "content",
            "decision",
            "resolved_at",
            "event_hash",
            "previous_hash",
        ],
    },
    TableRequirement {
        name: "reflection_entries",
        required_columns: &[
            "id",
            "session_id",
            "chain_id",
            "depth",
            "trigger_type",
            "reflection_text",
            "event_hash",
            "previous_hash",
        ],
    },
    TableRequirement {
        name: "boundary_violations",
        required_columns: &[
            "id",
            "session_id",
            "violation_type",
            "severity",
            "action_taken",
            "event_hash",
            "previous_hash",
        ],
    },
    TableRequirement {
        name: "delegation_state",
        required_columns: &[],
    },
    TableRequirement {
        name: "intervention_state",
        required_columns: &[
            "agent_id",
            "level",
            "consecutive_normal",
            "cooldown_until",
            "ack_required",
            "hysteresis_count",
            "de_escalation_credits",
            "updated_at",
        ],
    },
    TableRequirement {
        name: "audit_log",
        required_columns: &[
            "id",
            "timestamp",
            "agent_id",
            "event_type",
            "severity",
            "tool_name",
            "details",
            "session_id",
            "actor_id",
            "operation_id",
            "request_id",
            "idempotency_key",
            "idempotency_status",
        ],
    },
    TableRequirement {
        name: "workflows",
        required_columns: &[],
    },
    TableRequirement {
        name: "session_event_index",
        required_columns: &[],
    },
    TableRequirement {
        name: "otel_spans",
        required_columns: &[],
    },
    TableRequirement {
        name: "backup_manifest",
        required_columns: &[],
    },
    TableRequirement {
        name: "convergence_profiles",
        required_columns: &["name", "description", "weights", "thresholds"],
    },
    TableRequirement {
        name: "webhooks",
        required_columns: &[],
    },
    TableRequirement {
        name: "installed_skills",
        required_columns: &[],
    },
    TableRequirement {
        name: "a2a_tasks",
        required_columns: &[],
    },
    TableRequirement {
        name: "discovered_agents",
        required_columns: &[],
    },
    TableRequirement {
        name: "memory_archival_log",
        required_columns: &[],
    },
    TableRequirement {
        name: "compaction_runs",
        required_columns: &[],
    },
    TableRequirement {
        name: "compaction_event_ranges",
        required_columns: &[],
    },
    TableRequirement {
        name: "memory_fts",
        required_columns: &[],
    },
    TableRequirement {
        name: "memory_embeddings",
        required_columns: &[],
    },
    TableRequirement {
        name: "agent_notes",
        required_columns: &[],
    },
    TableRequirement {
        name: "agent_timers",
        required_columns: &[],
    },
    TableRequirement {
        name: "pc_control_actions",
        required_columns: &[],
    },
    TableRequirement {
        name: "convergence_links",
        required_columns: &[],
    },
    TableRequirement {
        name: "studio_chat_sessions",
        required_columns: &[
            "id",
            "title",
            "model",
            "system_prompt",
            "temperature",
            "max_tokens",
            "created_at",
            "updated_at",
            "last_activity_at",
            "deleted_at",
            "agent_id",
        ],
    },
    TableRequirement {
        name: "studio_chat_messages",
        required_columns: &[],
    },
    TableRequirement {
        name: "studio_chat_safety_audit",
        required_columns: &[],
    },
    TableRequirement {
        name: "marketplace_agent_listings",
        required_columns: &[],
    },
    TableRequirement {
        name: "marketplace_skill_listings",
        required_columns: &[],
    },
    TableRequirement {
        name: "marketplace_contracts",
        required_columns: &[],
    },
    TableRequirement {
        name: "credit_wallets",
        required_columns: &[],
    },
    TableRequirement {
        name: "credit_transactions",
        required_columns: &[],
    },
    TableRequirement {
        name: "credit_escrows",
        required_columns: &[],
    },
    TableRequirement {
        name: "marketplace_reviews",
        required_columns: &[],
    },
    TableRequirement {
        name: "stream_event_log",
        required_columns: &[],
    },
    TableRequirement {
        name: "workflow_executions",
        required_columns: &[],
    },
    TableRequirement {
        name: "channels",
        required_columns: &[
            "id",
            "channel_type",
            "status",
            "status_message",
            "agent_id",
            "config",
            "last_message_at",
            "message_count",
            "created_at",
            "updated_at",
        ],
    },
    TableRequirement {
        name: "session_bookmarks",
        required_columns: &[],
    },
    TableRequirement {
        name: "cost_snapshots",
        required_columns: &[],
    },
    TableRequirement {
        name: "revoked_tokens",
        required_columns: &[],
    },
    TableRequirement {
        name: "operation_journal",
        required_columns: &[
            "id",
            "actor_key",
            "method",
            "route_template",
            "operation_id",
            "idempotency_key",
            "request_fingerprint",
            "request_body",
            "status",
            "created_at",
            "last_seen_at",
        ],
    },
    TableRequirement {
        name: "live_execution_records",
        required_columns: &[
            "id",
            "journal_id",
            "operation_id",
            "route_kind",
            "actor_key",
            "status",
            "state_json",
            "created_at",
            "updated_at",
        ],
    },
    TableRequirement {
        name: "goal_proposals_v2",
        required_columns: &[],
    },
    TableRequirement {
        name: "goal_proposal_transitions",
        required_columns: &[],
    },
    TableRequirement {
        name: "goal_lineage_heads",
        required_columns: &[],
    },
    TableRequirement {
        name: "monitor_threshold_config",
        required_columns: &[
            "config_key",
            "critical_override_threshold",
            "updated_at",
            "updated_by",
            "confirmed_by",
        ],
    },
    TableRequirement {
        name: "monitor_threshold_history",
        required_columns: &[
            "id",
            "config_key",
            "previous_value",
            "new_value",
            "initiated_by",
            "confirmed_by",
            "change_mode",
            "changed_at",
        ],
    },
    TableRequirement {
        name: "agent_profile_assignments",
        required_columns: &[
            "agent_id",
            "profile_name",
            "created_at",
            "updated_at",
            "updated_by",
        ],
    },
];

const REQUIRED_INDEXES: &[&str] = &[
    "idx_memory_events_memory_id",
    "idx_itp_events_session",
    "idx_itp_events_timestamp",
    "idx_itp_events_event_type",
    "idx_convergence_scores_agent",
    "idx_intervention_agent",
    "idx_intervention_level",
    "idx_goal_proposals_agent",
    "idx_goal_proposals_pending",
    "idx_reflection_session",
    "idx_reflection_chain",
    "idx_boundary_session",
    "idx_boundary_type",
    "idx_delegation_state_delegation",
    "idx_delegation_state_sender",
    "idx_delegation_state_recipient",
    "idx_delegation_state_pending",
    "idx_audit_timestamp",
    "idx_audit_agent",
    "idx_audit_event_type",
    "idx_audit_severity",
    "idx_audit_log_actor_id",
    "idx_audit_operation_id",
    "idx_audit_idempotency_key",
    "idx_workflows_name",
    "idx_workflows_created_by",
    "idx_sei_session_seq",
    "idx_otel_session",
    "idx_otel_trace",
    "idx_webhooks_active",
    "idx_installed_skills_state",
    "idx_installed_skills_name",
    "idx_a2a_tasks_status",
    "idx_a2a_tasks_created",
    "idx_discovered_agents_trust",
    "idx_archival_memory",
    "idx_compaction_ranges_memory",
    "idx_memory_snapshots_citation_count",
    "idx_agent_notes_agent",
    "idx_agent_notes_title",
    "idx_agent_timers_agent",
    "idx_agent_timers_fire",
    "idx_pc_actions_agent",
    "idx_pc_actions_session",
    "idx_pc_actions_skill",
    "idx_pc_actions_blocked",
    "idx_convergence_links_parent",
    "idx_convergence_links_child",
    "idx_convergence_links_delegation",
    "idx_convergence_links_active",
    "idx_studio_msg_session",
    "idx_studio_safety_session",
    "idx_studio_sessions_last_activity",
    "idx_studio_sessions_deleted_at",
    "idx_studio_sessions_agent_id",
    "idx_mkt_listings_status",
    "idx_mkt_listings_trust",
    "idx_mkt_listings_rating",
    "idx_mkt_contracts_hirer",
    "idx_mkt_contracts_worker",
    "idx_mkt_contracts_state",
    "idx_credit_tx_from",
    "idx_credit_tx_to",
    "idx_credit_escrows_contract",
    "idx_mkt_reviews_reviewee",
    "idx_channels_agent",
    "idx_channels_type",
    "idx_session_bookmarks_session",
    "idx_cost_snapshots_scope_date",
    "idx_revoked_tokens_expires",
    "idx_operation_journal_actor_key_idempotency",
    "idx_operation_journal_operation_id",
    "idx_operation_journal_status_lease",
    "idx_operation_journal_fingerprint",
    "idx_goal_proposals_v2_lineage",
    "idx_goal_proposals_v2_subject",
    "idx_goal_proposal_transitions_proposal",
    "idx_goal_proposal_transitions_lineage",
    "idx_goal_proposal_single_terminal",
    "idx_monitor_threshold_history_key_changed_at",
    "idx_agent_profile_assignments_profile",
];

const REQUIRED_TRIGGERS: &[&str] = &[
    "prevent_memory_events_update",
    "prevent_memory_events_delete",
    "prevent_audit_log_update",
    "prevent_audit_log_delete",
    "prevent_snapshots_update",
    "prevent_snapshots_delete",
    "prevent_itp_events_update",
    "prevent_itp_events_delete",
    "prevent_convergence_scores_update",
    "prevent_convergence_scores_delete",
    "prevent_intervention_history_update",
    "prevent_intervention_history_delete",
    "prevent_reflection_entries_update",
    "prevent_reflection_entries_delete",
    "prevent_boundary_violations_update",
    "prevent_boundary_violations_delete",
    "goal_proposals_append_guard",
    "prevent_goal_proposals_delete",
    "delegation_state_append_guard",
    "prevent_delegation_state_delete",
    "memory_fts_insert",
    "credit_transactions_no_update",
    "credit_transactions_no_delete",
    "prevent_goal_proposals_v2_update",
    "prevent_goal_proposals_v2_delete",
    "prevent_goal_proposal_transitions_update",
    "prevent_goal_proposal_transitions_delete",
    "prevent_audit_log_row_update",
    "prevent_audit_log_row_delete",
];

pub fn require_schema_ready(
    conn: &Connection,
) -> Result<SchemaContractReport, SchemaContractError> {
    let current =
        current_version(conn).map_err(|error| SchemaContractError::Query(error.to_string()))?;

    if current < LATEST_VERSION {
        return Err(SchemaContractError::MigrationRequired {
            current_version: current,
            expected_version: LATEST_VERSION,
        });
    }
    if current > LATEST_VERSION {
        return Err(SchemaContractError::UnsupportedNewerSchema {
            current_version: current,
            expected_version: LATEST_VERSION,
        });
    }

    let tables = load_named_objects(conn, "table")?;
    let indexes = load_named_objects(conn, "index")?;
    let triggers = load_named_objects(conn, "trigger")?;

    let mut problems = Vec::new();

    for requirement in REQUIRED_TABLES {
        if !tables.contains(requirement.name) {
            problems.push(SchemaProblem::missing_table(requirement.name));
            continue;
        }

        let columns = load_table_columns(conn, requirement.name)?;
        for column in requirement.required_columns {
            if !columns.contains(*column) {
                problems.push(SchemaProblem::missing_column(requirement.name, column));
            }
        }
    }

    for index in REQUIRED_INDEXES {
        if !indexes.contains(*index) {
            problems.push(SchemaProblem::missing_index(index));
        }
    }

    for trigger in REQUIRED_TRIGGERS {
        if !triggers.contains(*trigger) {
            problems.push(SchemaProblem::missing_trigger(trigger));
        }
    }

    if !problems.is_empty() {
        let summary = problems
            .iter()
            .map(|problem| problem.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(SchemaContractError::ContractViolation {
            current_version: current,
            summary,
            problems,
        });
    }

    verify_integrity(conn)?;

    Ok(SchemaContractReport {
        current_version: current,
        latest_version: LATEST_VERSION,
    })
}

fn load_named_objects(
    conn: &Connection,
    object_type: &str,
) -> Result<BTreeSet<String>, SchemaContractError> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = ?1 ORDER BY name")
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    let rows = stmt
        .query_map([object_type], |row| row.get::<_, String>(0))
        .map_err(|error| SchemaContractError::Query(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    Ok(rows.into_iter().collect())
}

fn load_table_columns(
    conn: &Connection,
    table: &str,
) -> Result<BTreeSet<String>, SchemaContractError> {
    let sql = format!("PRAGMA table_info('{table}')");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| SchemaContractError::Query(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    Ok(rows.into_iter().collect())
}

fn verify_integrity(conn: &Connection) -> Result<(), SchemaContractError> {
    let mut stmt = conn
        .prepare("PRAGMA integrity_check")
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    let messages = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| SchemaContractError::Query(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    if messages.iter().any(|message| message != "ok") {
        return Err(SchemaContractError::Integrity(messages.join("; ")));
    }

    let mut stmt = conn
        .prepare("PRAGMA foreign_key_check")
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    let violations = stmt
        .query_map([], |_row| Ok(()))
        .map_err(|error| SchemaContractError::Query(error.to_string()))?
        .count();
    if violations != 0 {
        return Err(SchemaContractError::Integrity(format!(
            "{violations} foreign key violation(s) detected"
        )));
    }

    Ok(())
}
