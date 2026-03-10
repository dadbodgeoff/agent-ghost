use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use rusqlite::{Connection, OptionalExtension};
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

    fn column_type_mismatch(
        table: &str,
        column: &str,
        expected: ColumnAffinity,
        actual: ColumnAffinity,
    ) -> Self {
        Self {
            message: format!(
                "table {table} column {column} has type {}, expected {}",
                actual.as_str(),
                expected.as_str()
            ),
        }
    }

    fn missing_index(index: &str) -> Self {
        Self {
            message: format!("missing index {index}"),
        }
    }

    fn missing_index_contract(index: &str, detail: &str) -> Self {
        Self {
            message: format!("index {index} missing contract {detail}"),
        }
    }

    fn missing_trigger(trigger: &str) -> Self {
        Self {
            message: format!("missing trigger {trigger}"),
        }
    }

    fn missing_table_contract(table: &str, detail: &str) -> Self {
        Self {
            message: format!("table {table} missing contract {detail}"),
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

#[allow(dead_code)]
struct TableRequirement {
    name: &'static str,
    required_columns: &'static [&'static str],
}

struct TableSqlRequirement {
    table: &'static str,
    description: &'static str,
    pattern: &'static str,
}

struct IndexSqlRequirement {
    index: &'static str,
    description: &'static str,
    pattern: &'static str,
}

#[derive(Debug, Clone)]
struct DerivedTableRequirement {
    name: String,
    required_columns: BTreeMap<String, ColumnAffinity>,
}

#[derive(Debug, Clone)]
struct DerivedSchema {
    tables: Vec<DerivedTableRequirement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnAffinity {
    Text,
    Integer,
    Real,
    Blob,
    Numeric,
}

impl ColumnAffinity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "TEXT",
            Self::Integer => "INTEGER",
            Self::Real => "REAL",
            Self::Blob => "BLOB",
            Self::Numeric => "NUMERIC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
struct ColumnTypeRequirement {
    table: &'static str,
    column: &'static str,
    affinity: ColumnAffinity,
}

#[allow(dead_code)]
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
        name: "skill_install_state",
        required_columns: &["skill_name", "state", "updated_at", "updated_by"],
    },
    TableRequirement {
        name: "skill_signers",
        required_columns: &[
            "key_id",
            "publisher",
            "public_key",
            "state",
            "updated_at",
            "updated_by",
            "revocation_reason",
        ],
    },
    TableRequirement {
        name: "external_skill_artifacts",
        required_columns: &[
            "artifact_digest",
            "artifact_schema_version",
            "skill_name",
            "skill_version",
            "publisher",
            "description",
            "source_kind",
            "execution_mode",
            "entrypoint",
            "source_uri",
            "managed_artifact_path",
            "managed_entrypoint_path",
            "manifest_json",
            "requested_capabilities",
            "declared_privileges",
            "signer_key_id",
            "artifact_size_bytes",
            "ingested_at",
        ],
    },
    TableRequirement {
        name: "external_skill_verifications",
        required_columns: &[
            "artifact_digest",
            "status",
            "signer_key_id",
            "signer_publisher",
            "details_json",
            "verified_at",
        ],
    },
    TableRequirement {
        name: "external_skill_quarantine",
        required_columns: &[
            "artifact_digest",
            "state",
            "reason_code",
            "reason_detail",
            "revision",
            "updated_at",
            "updated_by",
        ],
    },
    TableRequirement {
        name: "external_skill_install_state",
        required_columns: &[
            "artifact_digest",
            "skill_name",
            "skill_version",
            "state",
            "updated_at",
            "updated_by",
        ],
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
        required_columns: &[
            "id",
            "session_id",
            "message_id",
            "event_type",
            "payload",
            "created_at",
        ],
    },
    TableRequirement {
        name: "workflow_executions",
        required_columns: &[
            "id",
            "workflow_id",
            "workflow_name",
            "journal_id",
            "operation_id",
            "owner_token",
            "lease_epoch",
            "state_version",
            "status",
            "current_step_index",
            "current_node_id",
            "recovery_action",
            "state",
            "final_response_status",
            "final_response_body",
            "started_at",
            "completed_at",
            "updated_at",
        ],
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
            "request_id",
            "response_status_code",
            "response_body",
            "response_content_type",
            "committed_at",
            "lease_expires_at",
            "owner_token",
            "lease_epoch",
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
            "state_version",
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
    TableRequirement {
        name: "autonomy_jobs",
        required_columns: &[
            "id",
            "job_type",
            "agent_id",
            "tenant_key",
            "workflow_id",
            "policy_scope",
            "payload_version",
            "payload_json",
            "schedule_version",
            "schedule_json",
            "overlap_policy",
            "missed_run_policy",
            "retry_policy_json",
            "initiative_mode",
            "approval_policy",
            "state",
            "current_run_id",
            "next_run_at",
            "last_due_at",
            "last_enqueued_at",
            "last_started_at",
            "last_finished_at",
            "last_success_at",
            "last_failure_at",
            "last_heartbeat_at",
            "pause_reason",
            "quarantine_reason",
            "terminal_reason",
            "manual_review_required",
            "retry_count",
            "retry_after",
            "created_at",
            "updated_at",
        ],
    },
    TableRequirement {
        name: "autonomy_runs",
        required_columns: &[
            "id",
            "job_id",
            "attempt",
            "trigger_source",
            "triggered_at",
            "due_at",
            "started_at",
            "completed_at",
            "state",
            "why_now_json",
            "payload_version",
            "payload_json",
            "initiative_mode",
            "approval_state",
            "approval_proposal_id",
            "approval_expires_at",
            "owner_identity",
            "owner_token",
            "lease_epoch",
            "side_effect_correlation_key",
            "side_effect_status",
            "result_json",
            "error_class",
            "error_message",
            "waiting_until",
            "terminal_reason",
            "manual_review_required",
            "created_at",
            "updated_at",
        ],
    },
    TableRequirement {
        name: "autonomy_leases",
        required_columns: &[
            "job_id",
            "run_id",
            "owner_identity",
            "owner_token",
            "lease_epoch",
            "leased_at",
            "last_seen_at",
            "lease_expires_at",
        ],
    },
    TableRequirement {
        name: "autonomy_suppressions",
        required_columns: &[
            "id",
            "scope_kind",
            "scope_key",
            "fingerprint",
            "reason",
            "created_by",
            "created_at",
            "expires_at",
            "active",
            "policy_version",
            "metadata_json",
        ],
    },
    TableRequirement {
        name: "autonomy_policies",
        required_columns: &[
            "id",
            "scope_kind",
            "scope_key",
            "policy_version",
            "policy_json",
            "created_at",
            "updated_at",
        ],
    },
    TableRequirement {
        name: "autonomy_notifications",
        required_columns: &[
            "id",
            "run_id",
            "job_id",
            "delivery_state",
            "channel",
            "correlation_key",
            "payload_json",
            "approval_proposal_id",
            "last_error",
            "created_at",
            "updated_at",
        ],
    },
];

fn expected_schema() -> Result<&'static DerivedSchema, SchemaContractError> {
    static EXPECTED_SCHEMA: OnceLock<Result<DerivedSchema, String>> = OnceLock::new();
    match EXPECTED_SCHEMA.get_or_init(|| build_expected_schema().map_err(|error| error.to_string()))
    {
        Ok(schema) => Ok(schema),
        Err(error) => Err(SchemaContractError::Query(error.clone())),
    }
}

fn build_expected_schema() -> Result<DerivedSchema, SchemaContractError> {
    let conn = Connection::open_in_memory()
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    crate::migrations::materialize_latest_schema_reference(&conn).map_err(|error| {
        SchemaContractError::Query(format!("materialize reference schema: {error}"))
    })?;

    Ok(DerivedSchema {
        tables: load_reference_tables(&conn)?,
    })
}

fn load_reference_tables(
    conn: &Connection,
) -> Result<Vec<DerivedTableRequirement>, SchemaContractError> {
    let entries = load_explicit_table_sql(conn)?;
    let virtual_table_prefixes = entries
        .iter()
        .filter_map(|(name, sql)| {
            normalize_schema_sql(sql)
                .starts_with("create virtual table")
                .then_some(name.clone())
        })
        .collect::<Vec<_>>();

    let mut tables = Vec::new();
    for (name, _sql) in entries {
        if name.starts_with("sqlite_") {
            continue;
        }
        if virtual_table_prefixes.iter().any(|prefix| {
            let shadow_prefix = format!("{prefix}_");
            name != *prefix && name.starts_with(&shadow_prefix)
        }) {
            continue;
        }

        let required_columns = load_table_columns(conn, &name)?
            .into_iter()
            .map(|(column, declared_type)| (column, normalize_declared_type(&declared_type)))
            .collect();
        tables.push(DerivedTableRequirement {
            name,
            required_columns,
        });
    }
    tables.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(tables)
}

fn load_explicit_table_sql(
    conn: &Connection,
) -> Result<Vec<(String, String)>, SchemaContractError> {
    let mut stmt = conn
        .prepare(
            "SELECT name, sql
             FROM sqlite_master
             WHERE type = 'table' AND sql IS NOT NULL
             ORDER BY name",
        )
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| SchemaContractError::Query(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    Ok(rows)
}

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
    "idx_skill_install_state_state",
    "idx_skill_signers_state",
    "idx_external_skill_artifacts_name",
    "idx_external_skill_artifacts_source_kind",
    "idx_external_skill_quarantine_state",
    "idx_external_skill_install_state_name",
    "idx_external_skill_install_state_state",
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
    "idx_stream_event_recovery",
    "idx_live_execution_route_status",
    "idx_live_execution_actor_operation",
    "idx_channels_agent",
    "idx_channels_type",
    "idx_session_bookmarks_session",
    "idx_cost_snapshots_scope_date",
    "idx_revoked_tokens_expires",
    "idx_operation_journal_actor_key_idempotency",
    "idx_operation_journal_operation_id",
    "idx_operation_journal_status_lease",
    "idx_operation_journal_fingerprint",
    "idx_workflow_executions_journal_id",
    "idx_workflow_executions_operation_id",
    "idx_workflow_executions_workflow_status",
    "idx_goal_proposals_v2_lineage",
    "idx_goal_proposals_v2_subject",
    "idx_goal_proposal_transitions_proposal",
    "idx_goal_proposal_transitions_lineage",
    "idx_goal_proposal_single_terminal",
    "idx_monitor_threshold_history_key_changed_at",
    "idx_agent_profile_assignments_profile",
    "idx_autonomy_jobs_due_state",
    "idx_autonomy_jobs_agent_state",
    "idx_autonomy_jobs_manual_review",
    "idx_autonomy_runs_job_created",
    "idx_autonomy_runs_side_effect",
    "idx_autonomy_runs_state_waiting",
    "idx_autonomy_leases_run_id",
    "idx_autonomy_leases_expiry",
    "idx_autonomy_suppressions_scope_active",
    "idx_autonomy_notifications_run_state",
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
    "prevent_operation_journal_delete",
    "operation_journal_commit_requires_current_request",
];

const REQUIRED_TABLE_SQL_PATTERNS: &[TableSqlRequirement] = &[
    TableSqlRequirement {
        table: "operation_journal",
        description: "aborted journal status",
        pattern: "CHECK(status IN ('in_progress', 'committed', 'aborted'))",
    },
    TableSqlRequirement {
        table: "operation_journal",
        description: "owner token non-empty check",
        pattern: "owner_token TEXT NOT NULL DEFAULT '' CHECK(length(owner_token) > 0)",
    },
    TableSqlRequirement {
        table: "operation_journal",
        description: "lease epoch non-negative check",
        pattern: "lease_epoch INTEGER NOT NULL DEFAULT 0 CHECK(lease_epoch >= 0)",
    },
    TableSqlRequirement {
        table: "live_execution_records",
        description: "journal_id unique contract",
        pattern: "journal_id   TEXT NOT NULL UNIQUE",
    },
    TableSqlRequirement {
        table: "live_execution_records",
        description: "operation_id unique contract",
        pattern: "operation_id TEXT NOT NULL UNIQUE",
    },
    TableSqlRequirement {
        table: "live_execution_records",
        description: "recovery-required execution status",
        pattern: "CHECK(status IN ('accepted', 'running', 'completed', 'recovery_required', 'cancelled'))",
    },
    TableSqlRequirement {
        table: "live_execution_records",
        description: "live execution state version contract",
        pattern: "state_version INTEGER NOT NULL DEFAULT 0 CHECK(state_version >= 0)",
    },
    TableSqlRequirement {
        table: "workflow_executions",
        description: "workflow state default json contract",
        pattern: "state TEXT NOT NULL DEFAULT '{}'",
    },
    TableSqlRequirement {
        table: "workflow_executions",
        description: "workflow execution version contract",
        pattern: "state_version INTEGER NOT NULL DEFAULT 0 CHECK(state_version >= 0)",
    },
    TableSqlRequirement {
        table: "workflow_executions",
        description: "workflow execution status contract",
        pattern:
            "status TEXT NOT NULL DEFAULT 'recovery_required' CHECK(status IN ('running', 'completed', 'failed', 'recovery_required'))",
    },
    TableSqlRequirement {
        table: "autonomy_jobs",
        description: "autonomy job payload version contract",
        pattern: "payload_version        INTEGER NOT NULL CHECK(payload_version > 0)",
    },
    TableSqlRequirement {
        table: "autonomy_jobs",
        description: "autonomy job schedule version contract",
        pattern: "schedule_version       INTEGER NOT NULL CHECK(schedule_version > 0)",
    },
    TableSqlRequirement {
        table: "autonomy_jobs",
        description: "autonomy job state contract",
        pattern:
            "state                  TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued', 'leased', 'running', 'waiting', 'succeeded', 'failed', 'paused', 'quarantined', 'aborted'))",
    },
    TableSqlRequirement {
        table: "autonomy_runs",
        description: "autonomy run payload version contract",
        pattern: "payload_version           INTEGER NOT NULL CHECK(payload_version > 0)",
    },
    TableSqlRequirement {
        table: "autonomy_runs",
        description: "autonomy run state contract",
        pattern:
            "state                     TEXT NOT NULL CHECK(state IN ('queued', 'leased', 'running', 'waiting', 'succeeded', 'failed', 'paused', 'quarantined', 'aborted'))",
    },
    TableSqlRequirement {
        table: "autonomy_leases",
        description: "autonomy lease owner token contract",
        pattern: "owner_token      TEXT NOT NULL DEFAULT '' CHECK(length(owner_token) > 0)",
    },
    TableSqlRequirement {
        table: "autonomy_leases",
        description: "autonomy lease epoch contract",
        pattern: "lease_epoch      INTEGER NOT NULL DEFAULT 0 CHECK(lease_epoch >= 0)",
    },
];

const REQUIRED_INDEX_SQL_PATTERNS: &[IndexSqlRequirement] = &[
    IndexSqlRequirement {
        index: "idx_workflow_executions_journal_id",
        description: "workflow execution journal uniqueness",
        pattern:
            "CREATE UNIQUE INDEX idx_workflow_executions_journal_id ON workflow_executions(journal_id) WHERE journal_id IS NOT NULL",
    },
    IndexSqlRequirement {
        index: "idx_workflow_executions_operation_id",
        description: "workflow execution operation uniqueness",
        pattern:
            "CREATE UNIQUE INDEX idx_workflow_executions_operation_id ON workflow_executions(operation_id) WHERE operation_id IS NOT NULL",
    },
    IndexSqlRequirement {
        index: "idx_autonomy_runs_side_effect",
        description: "autonomy side effect correlation uniqueness",
        pattern:
            "CREATE UNIQUE INDEX idx_autonomy_runs_side_effect ON autonomy_runs(side_effect_correlation_key) WHERE side_effect_correlation_key IS NOT NULL",
    },
];

#[allow(dead_code)]
const REQUIRED_COLUMN_TYPES: &[ColumnTypeRequirement] = &[
    ColumnTypeRequirement {
        table: "schema_version",
        column: "version",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "memory_events",
        column: "event_id",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "memory_events",
        column: "event_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "memory_events",
        column: "previous_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "memory_snapshots",
        column: "id",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "memory_snapshots",
        column: "state_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "memory_snapshots",
        column: "citation_count",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "itp_events",
        column: "sequence_number",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "itp_events",
        column: "event_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "itp_events",
        column: "previous_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "convergence_scores",
        column: "composite_score",
        affinity: ColumnAffinity::Real,
    },
    ColumnTypeRequirement {
        table: "convergence_scores",
        column: "level",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "convergence_scores",
        column: "event_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "convergence_scores",
        column: "previous_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "intervention_history",
        column: "intervention_level",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "intervention_history",
        column: "previous_level",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "intervention_history",
        column: "trigger_score",
        affinity: ColumnAffinity::Real,
    },
    ColumnTypeRequirement {
        table: "intervention_history",
        column: "event_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "intervention_history",
        column: "previous_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "goal_proposals",
        column: "event_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "goal_proposals",
        column: "previous_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "reflection_entries",
        column: "depth",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "reflection_entries",
        column: "event_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "reflection_entries",
        column: "previous_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "boundary_violations",
        column: "severity",
        affinity: ColumnAffinity::Real,
    },
    ColumnTypeRequirement {
        table: "boundary_violations",
        column: "event_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "boundary_violations",
        column: "previous_hash",
        affinity: ColumnAffinity::Blob,
    },
    ColumnTypeRequirement {
        table: "intervention_state",
        column: "level",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "intervention_state",
        column: "consecutive_normal",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "intervention_state",
        column: "ack_required",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "intervention_state",
        column: "hysteresis_count",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "intervention_state",
        column: "de_escalation_credits",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "studio_chat_sessions",
        column: "temperature",
        affinity: ColumnAffinity::Real,
    },
    ColumnTypeRequirement {
        table: "studio_chat_sessions",
        column: "max_tokens",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "channels",
        column: "message_count",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "monitor_threshold_config",
        column: "critical_override_threshold",
        affinity: ColumnAffinity::Real,
    },
    ColumnTypeRequirement {
        table: "monitor_threshold_history",
        column: "previous_value",
        affinity: ColumnAffinity::Real,
    },
    ColumnTypeRequirement {
        table: "monitor_threshold_history",
        column: "new_value",
        affinity: ColumnAffinity::Real,
    },
    ColumnTypeRequirement {
        table: "stream_event_log",
        column: "id",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "workflow_executions",
        column: "lease_epoch",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "live_execution_records",
        column: "state_version",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "workflow_executions",
        column: "state_version",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "workflow_executions",
        column: "current_step_index",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "workflow_executions",
        column: "final_response_status",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "operation_journal",
        column: "response_status_code",
        affinity: ColumnAffinity::Integer,
    },
    ColumnTypeRequirement {
        table: "operation_journal",
        column: "lease_epoch",
        affinity: ColumnAffinity::Integer,
    },
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
    let expected_schema = expected_schema()?;

    let mut problems = Vec::new();

    for requirement in &expected_schema.tables {
        if !tables.contains(requirement.name.as_str()) {
            problems.push(SchemaProblem::missing_table(&requirement.name));
            continue;
        }

        let columns = load_table_columns(conn, &requirement.name)?;
        for (column, expected_affinity) in &requirement.required_columns {
            let Some(actual_type) = columns.get(column.as_str()) else {
                problems.push(SchemaProblem::missing_column(&requirement.name, column));
                continue;
            };

            let actual_affinity = normalize_declared_type(actual_type);
            if actual_affinity != *expected_affinity {
                problems.push(SchemaProblem::column_type_mismatch(
                    &requirement.name,
                    column,
                    *expected_affinity,
                    actual_affinity,
                ));
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

    for requirement in REQUIRED_TABLE_SQL_PATTERNS {
        let Some(sql) = load_table_sql(conn, requirement.table)? else {
            continue;
        };
        if !normalize_schema_sql(&sql).contains(&normalize_schema_sql(requirement.pattern)) {
            problems.push(SchemaProblem::missing_table_contract(
                requirement.table,
                requirement.description,
            ));
        }
    }

    for requirement in REQUIRED_INDEX_SQL_PATTERNS {
        let Some(sql) = load_index_sql(conn, requirement.index)? else {
            continue;
        };
        if !normalize_schema_sql(&sql).contains(&normalize_schema_sql(requirement.pattern)) {
            problems.push(SchemaProblem::missing_index_contract(
                requirement.index,
                requirement.description,
            ));
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

fn load_table_sql(conn: &Connection, table: &str) -> Result<Option<String>, SchemaContractError> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| SchemaContractError::Query(error.to_string()))
}

fn load_index_sql(conn: &Connection, index: &str) -> Result<Option<String>, SchemaContractError> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [index],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| SchemaContractError::Query(error.to_string()))
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
) -> Result<BTreeMap<String, String>, SchemaContractError> {
    let sql = format!("PRAGMA table_info('{table}')");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(|error| SchemaContractError::Query(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SchemaContractError::Query(error.to_string()))?;
    Ok(rows.into_iter().collect())
}

fn normalize_declared_type(declared_type: &str) -> ColumnAffinity {
    let normalized = declared_type.trim().to_ascii_uppercase();
    if normalized.contains("INT") {
        ColumnAffinity::Integer
    } else if normalized.contains("CHAR")
        || normalized.contains("CLOB")
        || normalized.contains("TEXT")
    {
        ColumnAffinity::Text
    } else if normalized.contains("BLOB") || normalized.is_empty() {
        ColumnAffinity::Blob
    } else if normalized.contains("REAL")
        || normalized.contains("FLOA")
        || normalized.contains("DOUB")
    {
        ColumnAffinity::Real
    } else {
        ColumnAffinity::Numeric
    }
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
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
