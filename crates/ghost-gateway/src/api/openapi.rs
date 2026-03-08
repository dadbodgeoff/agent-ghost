//! OpenAPI specification endpoint.
//!
//! Serves the auto-generated OpenAPI 3.1 spec at `GET /api/openapi.json`.
//! Uses `utoipa` to derive the spec from handler types and response structs.
//!
//! Ref: ADE_DESIGN_PLAN §17.3, tasks.md T-1.3.1

#![allow(dead_code)]

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use utoipa::OpenApi;

/// Root OpenAPI document.
///
/// Aggregates all endpoint paths and component schemas.
/// Handler types are referenced by module — utoipa resolves
/// the `#[utoipa::path]` annotations at compile time.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "GHOST ADE Gateway API",
        version = "0.1.0",
        description = "REST + WebSocket API for the GHOST Autonomous Development Environment.\n\n\
            Authentication: Bearer JWT or legacy token via `Authorization` header.\n\
            Rate limits: 20 req/min (unauth), 200 req/min (auth), 10 req/min (safety).",
        license(name = "MIT OR Apache-2.0"),
    ),
    servers(
        (url = "/", description = "Local development (relative — port from ghost.yml config)"),
    ),
    paths(
        health,
        ready,
        list_agents,
        create_agent,
        delete_agent,
        get_auth_session,
        list_sessions,
        get_session_events,
        list_session_bookmarks,
        create_session_bookmark,
        delete_session_bookmark,
        branch_runtime_session,
        heartbeat_runtime_session,
        get_convergence_scores,
        list_goals,
        get_goal,
        approve_goal,
        reject_goal,
        list_memories,
        get_memory_graph,
        search_memories,
        list_archived_memories,
        get_memory,
        write_memory,
        archive_memory,
        unarchive_memory,
        get_live_execution,
        get_costs,
        list_workflows,
        get_workflow,
        list_workflow_executions,
        create_workflow,
        update_workflow,
        execute_workflow,
        resume_workflow_execution,
        list_studio_sessions,
        get_studio_session,
        create_studio_session,
        delete_studio_session,
        send_studio_message,
        stream_studio_message,
        recover_studio_stream,
        studio_run,
        get_traces,
        safety_status,
        kill_all,
        pause_agent,
        resume_agent,
        quarantine_agent,
        query_audit,
        audit_aggregation,
        audit_export,
        create_backup,
        list_backups,
        export_backup_data,
        restore_backup,
        login,
        refresh,
        logout,
        list_provider_keys,
        set_provider_key,
        delete_provider_key,
        list_webhooks,
        create_webhook,
        update_webhook,
        delete_webhook,
        test_webhook,
        list_skills,
        install_skill,
        uninstall_skill,
        quarantine_skill,
        resolve_skill_quarantine,
        reverify_skill,
        execute_skill_by_name,
        get_crdt_state,
        verify_integrity_chain,
        agent_chat,
        agent_chat_stream,
        list_channels,
        create_channel,
        reconnect_channel,
        delete_channel,
        inject_channel_message,
        list_itp_events,
        list_oauth_providers,
        list_oauth_connections,
        connect_oauth_provider,
        disconnect_oauth_connection,
        oauth_callback,
        execute_oauth_api_call,
        get_mesh_trust_graph,
        get_mesh_consensus,
        list_mesh_delegations,
        list_profiles,
        create_profile,
        update_profile,
        delete_profile,
        assign_agent_profile,
        search,
        get_pc_control_status,
        update_pc_control_status,
        list_pc_control_actions,
        update_pc_control_allowed_apps,
        update_pc_control_blocked_hotkeys,
        update_pc_control_safe_zones,
        issue_ws_ticket,
        get_push_vapid_key,
        subscribe_push,
        unsubscribe_push,
        list_marketplace_agents,
        register_marketplace_agent,
        get_marketplace_agent,
        update_marketplace_agent_status,
        delist_marketplace_agent,
        list_marketplace_skills,
        publish_marketplace_skill,
        get_marketplace_skill,
        list_marketplace_contracts,
        propose_marketplace_contract,
        get_marketplace_contract,
        accept_marketplace_contract,
        reject_marketplace_contract,
        start_marketplace_contract,
        complete_marketplace_contract,
        dispute_marketplace_contract,
        cancel_marketplace_contract,
        resolve_marketplace_contract,
        get_marketplace_wallet,
        seed_marketplace_wallet,
        list_marketplace_transactions,
        submit_marketplace_review,
        list_marketplace_reviews,
        discover_marketplace_agents,
        list_safety_checks,
        register_safety_check,
        unregister_safety_check,
        send_a2a_task,
        get_a2a_task,
        list_a2a_tasks,
        stream_a2a_task,
        discover_a2a_agents,
    ),
    components(
        schemas(
            ErrorResponseSchema,
            AgentInfoSchema,
            CreateAgentRequestSchema,
            SessionResponseSchema,
            ConvergenceScoreSchema,
            SessionSchema,
            SessionEventSchema,
            SessionBookmarkSchema,
            AgentCostSchema,
            WorkflowSchema,
            WorkflowExecutionSchema,
            LiveExecutionSchema,
            ChannelSchema,
            ItpEventSchema,
            OAuthProviderSchema,
            OAuthConnectionSchema,
            ProfileSchema,
            SearchResultSchema,
            SearchResponseSchema,
            ProviderKeyInfoSchema,
            MemoryGraphNodeSchema,
            MemoryGraphEdgeSchema,
            MemoryGraphResponseSchema,
            PcControlSafeZoneSchema,
            PcControlActionBudgetSchema,
            PcControlStatusSchema,
            PcControlActionLogSchema,
            PushSubscriptionSchema,
            WebhookSchema,
            crate::skill_catalog::SkillStateDto,
            crate::skill_catalog::SkillInstallStateDto,
            crate::skill_catalog::SkillVerificationStatusDto,
            crate::skill_catalog::SkillQuarantineStateDto,
            crate::skill_catalog::SkillSummaryDto,
            crate::skill_catalog::SkillListResponseDto,
            crate::skill_catalog::SkillQuarantineRequestDto,
            crate::skill_catalog::SkillQuarantineResolutionRequestDto,
            crate::skill_catalog::ExecuteSkillRequestDto,
            crate::skill_catalog::ExecuteSkillResponseDto,
            crate::skill_catalog::definitions::SkillExecutionMode,
            crate::skill_catalog::definitions::SkillMutationKind,
            crate::skill_catalog::definitions::SkillSourceKind,
            A2ATaskSchema,
        )
    ),
    tags(
        (name = "health", description = "Liveness and readiness probes"),
        (name = "auth", description = "JWT authentication endpoints"),
        (name = "agents", description = "Agent registry management"),
        (name = "convergence", description = "Convergence score queries"),
        (name = "sessions", description = "Session listing and replay"),
        (name = "goals", description = "Proposal/goal lifecycle"),
        (name = "memory", description = "Memory store operations"),
        (name = "executions", description = "Accepted-boundary execution recovery status"),
        (name = "state", description = "CRDT and state inspection"),
        (name = "integrity", description = "Hash-chain integrity verification"),
        (name = "costs", description = "Per-agent cost tracking"),
        (name = "workflows", description = "Workflow definitions and execution"),
        (name = "studio", description = "Studio session and prompt routes"),
        (name = "traces", description = "Session trace inspection"),
        (name = "mesh", description = "Multi-agent trust graph and delegation views"),
        (name = "chat", description = "Direct agent chat execution endpoints"),
        (name = "safety", description = "Kill switch and quarantine controls"),
        (name = "audit", description = "Audit log queries and export"),
        (name = "admin", description = "Backup, restore, and administrative data operations"),
        (name = "provider-keys", description = "Provider key management"),
        (name = "webhooks", description = "Webhook configuration and testing"),
        (name = "skills", description = "Gateway-owned mixed-source skill catalog management and execution"),
        (name = "channels", description = "Channel lifecycle and reconnect operations"),
        (name = "itp", description = "ITP event inspection"),
        (name = "oauth", description = "OAuth provider and connection flows"),
        (name = "profiles", description = "Convergence profile management"),
        (name = "search", description = "Cross-domain search"),
        (name = "pc-control", description = "PC control safety settings and activity"),
        (name = "push", description = "Push notification subscription routes"),
        (name = "marketplace", description = "Agent marketplace listings, contracts, wallet, and reviews"),
        (name = "safety-checks", description = "Custom safety check registration"),
        (name = "a2a", description = "Agent-to-Agent protocol endpoints"),
    )
)]
pub struct ApiDoc;

// ── Schema types for OpenAPI (mirrors actual response structs) ──

/// Standard error response envelope.
#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ErrorResponseSchema {
    pub error: ErrorBodySchema,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ErrorBodySchema {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct AgentInfoSchema {
    pub id: String,
    pub name: String,
    pub status: String,
    pub spending_cap: f64,
}

#[derive(utoipa::ToSchema, serde::Deserialize)]
pub struct CreateAgentRequestSchema {
    pub name: String,
    pub spending_cap: Option<f64>,
    pub capabilities: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub generate_keypair: Option<bool>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ConvergenceScoreSchema {
    pub agent_id: String,
    pub agent_name: String,
    pub score: f64,
    pub level: i32,
    pub profile: String,
    pub signal_scores: serde_json::Value,
    pub computed_at: Option<String>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SessionSchema {
    pub session_id: String,
    pub started_at: String,
    pub last_event_at: String,
    pub event_count: i64,
    pub agents: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct AgentCostSchema {
    pub agent_id: String,
    pub agent_name: String,
    pub daily_total: f64,
    pub compaction_cost: f64,
    pub spending_cap: f64,
    pub cap_remaining: f64,
    pub cap_utilization_pct: f64,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SessionResponseSchema {
    pub authenticated: bool,
    pub subject: String,
    pub role: String,
    pub mode: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SessionEventSchema {
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

#[derive(utoipa::ToSchema, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBookmarkSchema {
    pub id: String,
    pub event_index: i64,
    pub label: String,
    pub created_at: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct WorkflowSchema {
    pub id: String,
    pub name: String,
    pub description: String,
    pub nodes: serde_json::Value,
    pub edges: serde_json::Value,
    pub created_by: Option<String>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct WorkflowExecutionSchema {
    pub execution_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: String,
    pub mode: String,
    pub steps: serde_json::Value,
    pub input: Option<serde_json::Value>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct LiveExecutionSchema {
    pub execution_id: String,
    pub route_kind: String,
    pub status: String,
    pub operation_id: String,
    pub accepted_response: serde_json::Value,
    pub result_status_code: Option<u16>,
    pub result_body: Option<serde_json::Value>,
    pub recovery_required: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ChannelSchema {
    pub id: String,
    pub channel_type: String,
    pub status: String,
    pub status_message: Option<String>,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub config: serde_json::Value,
    pub last_message_at: Option<String>,
    pub message_count: i64,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ItpEventSchema {
    pub id: String,
    pub event_type: String,
    pub platform: String,
    pub session_id: String,
    pub timestamp: String,
    pub source: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct OAuthProviderSchema {
    pub name: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct OAuthConnectionSchema {
    pub ref_id: String,
    pub provider: String,
    pub scopes: Vec<String>,
    pub connected_at: String,
    pub status: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ProfileSchema {
    pub name: String,
    pub description: String,
    pub is_preset: bool,
    pub weights: Vec<f64>,
    pub thresholds: Vec<f64>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SearchResultSchema {
    pub result_type: String,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct SearchResponseSchema {
    pub query: String,
    pub results: Vec<SearchResultSchema>,
    pub total: i64,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct ProviderKeyInfoSchema {
    pub provider_name: String,
    pub model: String,
    pub env_name: String,
    pub is_set: bool,
    pub preview: Option<String>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct MemoryGraphNodeSchema {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub importance: f64,
    #[serde(rename = "decayFactor")]
    pub decay_factor: f64,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct MemoryGraphEdgeSchema {
    pub source: String,
    pub target: String,
    pub relationship: String,
    pub strength: f64,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct MemoryGraphResponseSchema {
    pub nodes: Vec<MemoryGraphNodeSchema>,
    pub edges: Vec<MemoryGraphEdgeSchema>,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PcControlSafeZoneSchema {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub label: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PcControlActionBudgetSchema {
    pub max_per_minute: i64,
    pub max_per_hour: i64,
    pub used_this_minute: i64,
    pub used_this_hour: i64,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PcControlStatusSchema {
    pub enabled: bool,
    pub action_budget: PcControlActionBudgetSchema,
    pub allowed_apps: Vec<String>,
    pub safe_zone: Option<PcControlSafeZoneSchema>,
    pub safe_zones: Vec<PcControlSafeZoneSchema>,
    pub blocked_hotkeys: Vec<String>,
    pub circuit_breaker_state: String,
    pub persisted: PcControlPersistedStateSchema,
    pub runtime: PcControlRuntimeStateSchema,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PcControlPersistedStateSchema {
    pub enabled: bool,
    pub allowed_apps: Vec<String>,
    pub safe_zone: Option<PcControlSafeZoneSchema>,
    pub blocked_hotkeys: Vec<String>,
    pub action_budget: PcControlActionBudgetSchema,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PcControlRuntimeStateSchema {
    pub circuit_breaker_state: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct PcControlActionLogSchema {
    pub id: String,
    pub action_type: String,
    pub target: String,
    pub timestamp: String,
    pub result: String,
    pub input_json: String,
    pub result_json: String,
    pub target_app: Option<String>,
    pub coordinates: Option<String>,
    pub blocked: bool,
    pub block_reason: Option<String>,
    pub agent_id: String,
    pub session_id: String,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct WsAuthTicketResponseSchema {
    pub ticket: String,
    pub expires_at: String,
    pub expires_in_secs: i64,
}

#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct PushSubscriptionSchema {
    pub endpoint: String,
    pub keys: Option<serde_json::Value>,
}

// ── Path definitions ──
// These use utoipa's path macro to describe each endpoint.
// The actual handler logic lives in the respective module files.

#[utoipa::path(
    get, path = "/api/health",
    tag = "health",
    responses(
        (status = 200, description = "Gateway is alive"),
        (status = 503, description = "Gateway unavailable"),
    )
)]
async fn health() {}

#[utoipa::path(
    get, path = "/api/ready",
    tag = "health",
    responses(
        (status = 200, description = "Gateway is ready"),
        (status = 503, description = "Gateway not ready"),
    )
)]
async fn ready() {}

#[utoipa::path(
    get, path = "/api/agents",
    tag = "agents",
    responses(
        (status = 200, description = "List of registered agents", body = Vec<AgentInfoSchema>),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_agents() {}

#[utoipa::path(
    post, path = "/api/agents",
    tag = "agents",
    request_body = CreateAgentRequestSchema,
    responses(
        (status = 201, description = "Agent created"),
        (status = 409, description = "Agent name conflict"),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn create_agent() {}

#[utoipa::path(
    delete, path = "/api/agents/{id}",
    tag = "agents",
    params(("id" = String, Path, description = "Agent UUID or name")),
    responses(
        (status = 200, description = "Agent deleted"),
        (status = 404, description = "Agent not found"),
        (status = 409, description = "Cannot delete quarantined agent"),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_agent() {}

#[utoipa::path(
    get, path = "/api/sessions",
    tag = "sessions",
    params(
        ("page" = Option<u32>, Query, description = "Page number (1-based)"),
        ("page_size" = Option<u32>, Query, description = "Items per page (max 200)"),
    ),
    responses(
        (status = 200, description = "Paginated session list"),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_sessions() {}

#[utoipa::path(
    get, path = "/api/convergence/scores",
    tag = "convergence",
    responses(
        (status = 200, description = "Convergence scores per agent", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_convergence_scores() {}

#[utoipa::path(
    get, path = "/api/goals",
    tag = "goals",
    responses(
        (status = 200, description = "List of proposals/goals"),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_goals() {}

#[utoipa::path(
    post, path = "/api/goals/{id}/approve",
    tag = "goals",
    params(("id" = String, Path, description = "Goal/proposal ID")),
    responses(
        (status = 200, description = "Goal approved"),
        (status = 404, description = "Goal not found"),
        (status = 409, description = "Already resolved"),
    ),
    security(("bearer_auth" = []))
)]
async fn approve_goal() {}

#[utoipa::path(
    post, path = "/api/goals/{id}/reject",
    tag = "goals",
    params(("id" = String, Path, description = "Goal/proposal ID")),
    responses(
        (status = 200, description = "Goal rejected"),
        (status = 404, description = "Goal not found"),
        (status = 409, description = "Already resolved"),
    ),
    security(("bearer_auth" = []))
)]
async fn reject_goal() {}

#[utoipa::path(
    get, path = "/api/memory",
    tag = "memory",
    responses(
        (status = 200, description = "List of memories"),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_memories() {}

#[utoipa::path(
    get, path = "/api/memory/{id}",
    tag = "memory",
    params(("id" = String, Path, description = "Memory ID")),
    responses(
        (status = 200, description = "Memory detail"),
        (status = 404, description = "Memory not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_memory() {}

#[utoipa::path(
    get, path = "/api/memory/graph",
    tag = "memory",
    responses(
        (status = 200, description = "Derived memory graph", body = MemoryGraphResponseSchema),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_memory_graph() {}

#[utoipa::path(
    post, path = "/api/memory",
    tag = "memory",
    responses(
        (status = 201, description = "Memory created"),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn write_memory() {}

#[utoipa::path(
    get, path = "/api/costs",
    tag = "costs",
    responses(
        (status = 200, description = "Per-agent cost summary", body = Vec<AgentCostSchema>),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_costs() {}

#[utoipa::path(
    get, path = "/api/safety/status",
    tag = "safety",
    responses(
        (status = 200, description = "Kill switch and quarantine status"),
    ),
    security(("bearer_auth" = []))
)]
async fn safety_status() {}

#[utoipa::path(
    post, path = "/api/safety/kill-all",
    tag = "safety",
    responses(
        (status = 200, description = "Platform kill activated"),
    ),
    security(("bearer_auth" = []))
)]
async fn kill_all() {}

#[utoipa::path(
    post, path = "/api/safety/pause/{agent_id}",
    tag = "safety",
    params(("agent_id" = String, Path, description = "Agent UUID")),
    responses(
        (status = 200, description = "Agent paused"),
        (status = 404, description = "Agent not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn pause_agent() {}

#[utoipa::path(
    post, path = "/api/safety/resume/{agent_id}",
    tag = "safety",
    params(("agent_id" = String, Path, description = "Agent UUID")),
    responses(
        (status = 200, description = "Agent resumed"),
        (status = 404, description = "Agent not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn resume_agent() {}

#[utoipa::path(
    post, path = "/api/safety/quarantine/{agent_id}",
    tag = "safety",
    params(("agent_id" = String, Path, description = "Agent UUID")),
    responses(
        (status = 200, description = "Agent quarantined"),
        (status = 404, description = "Agent not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn quarantine_agent() {}

#[utoipa::path(
    get, path = "/api/audit",
    tag = "audit",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("page_size" = Option<u32>, Query, description = "Items per page"),
        ("agent_id" = Option<String>, Query, description = "Filter by agent"),
        ("event_type" = Option<String>, Query, description = "Filter by event type"),
        ("severity" = Option<String>, Query, description = "Filter by severity"),
        ("from" = Option<String>, Query, description = "Start timestamp (RFC3339)"),
        ("to" = Option<String>, Query, description = "End timestamp (RFC3339)"),
        ("q" = Option<String>, Query, description = "Free-text search"),
    ),
    responses(
        (status = 200, description = "Paginated audit entries"),
        (status = 500, description = "Internal error"),
    ),
    security(("bearer_auth" = []))
)]
async fn query_audit() {}

#[utoipa::path(
    get, path = "/api/audit/aggregation",
    tag = "audit",
    responses(
        (status = 200, description = "Audit aggregation data"),
    ),
    security(("bearer_auth" = []))
)]
async fn audit_aggregation() {}

#[utoipa::path(
    get, path = "/api/audit/export",
    tag = "audit",
    params(
        ("format" = Option<String>, Query, description = "Export format: json, csv, jsonl"),
    ),
    responses(
        (status = 200, description = "Exported audit data"),
    ),
    security(("bearer_auth" = []))
)]
async fn audit_export() {}

#[utoipa::path(
    post, path = "/api/admin/backup",
    tag = "admin",
    responses(
        (status = 200, description = "Backup created", body = inline(serde_json::Value)),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn create_backup() {}

#[utoipa::path(
    get, path = "/api/admin/backups",
    tag = "admin",
    responses(
        (status = 200, description = "Available backups", body = inline(serde_json::Value)),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_backups() {}

#[utoipa::path(
    get, path = "/api/admin/export",
    tag = "admin",
    params(("format" = Option<String>, Query, description = "Export format: json or jsonl")),
    responses(
        (status = 200, description = "Administrative export payload"),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn export_backup_data() {}

#[utoipa::path(
    post, path = "/api/admin/restore",
    tag = "admin",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Backup verified for restore", body = inline(serde_json::Value)),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
        (status = 404, description = "Backup file not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn restore_backup() {}

#[utoipa::path(
    post, path = "/api/auth/login",
    tag = "auth",
    responses(
        (status = 200, description = "Login successful, returns access token"),
        (status = 401, description = "Invalid credentials"),
    )
)]
async fn login() {}

#[utoipa::path(
    post, path = "/api/auth/refresh",
    tag = "auth",
    responses(
        (status = 200, description = "Token refreshed"),
        (status = 401, description = "Invalid or expired refresh token"),
    )
)]
async fn refresh() {}

#[utoipa::path(
    post, path = "/api/auth/logout",
    tag = "auth",
    responses(
        (status = 200, description = "Logged out, token revoked"),
    ),
    security(("bearer_auth" = []))
)]
async fn logout() {}

#[utoipa::path(
    get, path = "/api/auth/session",
    tag = "auth",
    responses(
        (status = 200, description = "Current authenticated session", body = SessionResponseSchema),
        (status = 401, description = "Unauthorized", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_auth_session() {}

#[utoipa::path(
    get, path = "/api/goals/{id}",
    tag = "goals",
    params(("id" = String, Path, description = "Goal/proposal ID")),
    responses(
        (status = 200, description = "Goal detail", body = inline(serde_json::Value)),
        (status = 404, description = "Goal not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_goal() {}

#[utoipa::path(
    get, path = "/api/memory/search",
    tag = "memory",
    responses(
        (status = 200, description = "Memory search results", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn search_memories() {}

#[utoipa::path(
    get, path = "/api/memory/archived",
    tag = "memory",
    responses(
        (status = 200, description = "Archived memory entries", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_archived_memories() {}

#[utoipa::path(
    get, path = "/api/sessions/{id}/events",
    tag = "sessions",
    params(("id" = String, Path, description = "Runtime session ID")),
    responses(
        (status = 200, description = "Runtime session events", body = inline(serde_json::Value)),
        (status = 404, description = "Session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_session_events() {}

#[utoipa::path(
    get, path = "/api/sessions/{id}/bookmarks",
    tag = "sessions",
    params(("id" = String, Path, description = "Runtime session ID")),
    responses(
        (status = 200, description = "Session bookmarks", body = inline(serde_json::Value)),
        (status = 404, description = "Session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_session_bookmarks() {}

#[utoipa::path(
    post, path = "/api/sessions/{id}/bookmarks",
    tag = "sessions",
    params(("id" = String, Path, description = "Runtime session ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 201, description = "Bookmark created"),
        (status = 404, description = "Session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn create_session_bookmark() {}

#[utoipa::path(
    delete, path = "/api/sessions/{id}/bookmarks/{bookmark_id}",
    tag = "sessions",
    params(
        ("id" = String, Path, description = "Runtime session ID"),
        ("bookmark_id" = String, Path, description = "Bookmark ID"),
    ),
    responses(
        (status = 200, description = "Bookmark deleted"),
        (status = 404, description = "Bookmark not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_session_bookmark() {}

#[utoipa::path(
    post, path = "/api/sessions/{id}/branch",
    tag = "sessions",
    params(("id" = String, Path, description = "Runtime session ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Session branched"),
        (status = 404, description = "Session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn branch_runtime_session() {}

#[utoipa::path(
    post, path = "/api/sessions/{id}/heartbeat",
    tag = "sessions",
    params(("id" = String, Path, description = "Runtime session ID")),
    responses(
        (status = 204, description = "Heartbeat accepted"),
        (status = 404, description = "Session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn heartbeat_runtime_session() {}

#[utoipa::path(
    get, path = "/api/live-executions/{execution_id}",
    tag = "executions",
    params(("execution_id" = String, Path, description = "Durable live execution identifier")),
    responses(
        (status = 200, description = "Accepted-boundary execution state", body = LiveExecutionSchema),
        (status = 404, description = "Execution not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_live_execution() {}

#[utoipa::path(
    get, path = "/api/workflows",
    tag = "workflows",
    responses(
        (status = 200, description = "Workflow list", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_workflows() {}

#[utoipa::path(
    get, path = "/api/workflows/{id}",
    tag = "workflows",
    params(("id" = String, Path, description = "Workflow ID")),
    responses(
        (status = 200, description = "Workflow detail", body = WorkflowSchema),
        (status = 404, description = "Workflow not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_workflow() {}

#[utoipa::path(
    get, path = "/api/workflows/{id}/executions",
    tag = "workflows",
    params(("id" = String, Path, description = "Workflow ID")),
    responses(
        (status = 200, description = "Workflow execution history", body = inline(serde_json::Value)),
        (status = 404, description = "Workflow not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_workflow_executions() {}

#[utoipa::path(
    post, path = "/api/workflows",
    tag = "workflows",
    request_body = inline(serde_json::Value),
    responses(
        (status = 201, description = "Workflow created"),
        (status = 400, description = "Invalid workflow", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn create_workflow() {}

#[utoipa::path(
    put, path = "/api/workflows/{id}",
    tag = "workflows",
    params(("id" = String, Path, description = "Workflow ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Workflow updated"),
        (status = 404, description = "Workflow not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn update_workflow() {}

#[utoipa::path(
    post, path = "/api/workflows/{id}/execute",
    tag = "workflows",
    params(("id" = String, Path, description = "Workflow ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Workflow execution started", body = WorkflowExecutionSchema),
        (status = 404, description = "Workflow not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn execute_workflow() {}

#[utoipa::path(
    post, path = "/api/workflows/{id}/resume/{execution_id}",
    tag = "workflows",
    params(
        ("id" = String, Path, description = "Workflow ID"),
        ("execution_id" = String, Path, description = "Execution ID to resume"),
    ),
    responses(
        (status = 200, description = "Workflow execution resumed", body = WorkflowExecutionSchema),
        (status = 404, description = "Workflow or execution not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn resume_workflow_execution() {}

#[utoipa::path(
    post, path = "/api/memory/{id}/archive",
    tag = "memory",
    params(("id" = String, Path, description = "Memory ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Memory archived", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid archive request", body = ErrorResponseSchema),
        (status = 404, description = "Memory not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn archive_memory() {}

#[utoipa::path(
    post, path = "/api/memory/{id}/unarchive",
    tag = "memory",
    params(("id" = String, Path, description = "Memory ID")),
    responses(
        (status = 200, description = "Memory restored from archive", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid unarchive request", body = ErrorResponseSchema),
        (status = 404, description = "Memory not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn unarchive_memory() {}

#[utoipa::path(
    get, path = "/api/studio/sessions",
    tag = "studio",
    responses(
        (status = 200, description = "Studio session list", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_studio_sessions() {}

#[utoipa::path(
    get, path = "/api/studio/sessions/{id}",
    tag = "studio",
    params(("id" = String, Path, description = "Studio session ID")),
    responses(
        (status = 200, description = "Studio session detail", body = inline(serde_json::Value)),
        (status = 404, description = "Studio session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_studio_session() {}

#[utoipa::path(
    post, path = "/api/studio/sessions",
    tag = "studio",
    request_body = inline(serde_json::Value),
    responses(
        (status = 201, description = "Studio session created"),
        (status = 400, description = "Invalid request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn create_studio_session() {}

#[utoipa::path(
    delete, path = "/api/studio/sessions/{id}",
    tag = "studio",
    params(("id" = String, Path, description = "Studio session ID")),
    responses(
        (status = 200, description = "Studio session deleted"),
        (status = 404, description = "Studio session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_studio_session() {}

#[utoipa::path(
    post, path = "/api/studio/sessions/{id}/messages",
    tag = "studio",
    params(("id" = String, Path, description = "Studio session ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Studio message completed", body = inline(serde_json::Value)),
        (status = 202, description = "Studio message accepted and requires recovery polling", body = inline(serde_json::Value)),
        (status = 404, description = "Studio session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn send_studio_message() {}

#[utoipa::path(
    post, path = "/api/studio/sessions/{id}/messages/stream",
    tag = "studio",
    params(("id" = String, Path, description = "Studio session ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Streaming studio message response"),
        (status = 404, description = "Studio session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn stream_studio_message() {}

#[utoipa::path(
    get, path = "/api/studio/sessions/{id}/stream/recover",
    tag = "studio",
    params(("id" = String, Path, description = "Studio session ID")),
    responses(
        (status = 200, description = "Recovered stream state", body = inline(serde_json::Value)),
        (status = 404, description = "Studio session not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn recover_studio_stream() {}

#[utoipa::path(
    post, path = "/api/studio/run",
    tag = "studio",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Studio run result", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid prompt request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn studio_run() {}

#[utoipa::path(
    get, path = "/api/traces/{session_id}",
    tag = "traces",
    params(("session_id" = String, Path, description = "Runtime session ID")),
    responses(
        (status = 200, description = "Session traces", body = inline(serde_json::Value)),
        (status = 404, description = "Trace data not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_traces() {}

#[utoipa::path(
    get, path = "/api/state/crdt/{agent_id}",
    tag = "state",
    params(
        ("agent_id" = String, Path, description = "Agent ID"),
        ("memory_id" = Option<String>, Query, description = "Optional memory ID filter"),
        ("limit" = Option<u32>, Query, description = "Maximum deltas to return"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "CRDT delta log snapshot", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_crdt_state() {}

#[utoipa::path(
    get, path = "/api/integrity/chain/{agent_id}",
    tag = "integrity",
    params(
        ("agent_id" = String, Path, description = "Agent ID"),
        ("chain" = Option<String>, Query, description = "Chain selector: itp, memory, or both"),
    ),
    responses(
        (status = 200, description = "Hash-chain verification report", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn verify_integrity_chain() {}

#[utoipa::path(
    post, path = "/api/agent/chat",
    tag = "chat",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Single-turn agent chat result", body = inline(serde_json::Value)),
        (status = 202, description = "Agent chat accepted and requires recovery polling", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid chat request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn agent_chat() {}

#[utoipa::path(
    post, path = "/api/agent/chat/stream",
    tag = "chat",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Streaming agent chat response"),
        (status = 400, description = "Invalid chat request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn agent_chat_stream() {}

#[utoipa::path(
    get, path = "/api/admin/provider-keys",
    tag = "provider-keys",
    responses(
        (status = 200, description = "Configured provider key status", body = inline(serde_json::Value)),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_provider_keys() {}

#[utoipa::path(
    put, path = "/api/admin/provider-keys",
    tag = "provider-keys",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Provider key saved", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid provider key request", body = ErrorResponseSchema),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn set_provider_key() {}

#[utoipa::path(
    delete, path = "/api/admin/provider-keys/{env_name}",
    tag = "provider-keys",
    params(("env_name" = String, Path, description = "Provider key environment variable name")),
    responses(
        (status = 200, description = "Provider key removed", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid provider key name", body = ErrorResponseSchema),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_provider_key() {}

#[utoipa::path(
    get, path = "/api/channels",
    tag = "channels",
    responses(
        (status = 200, description = "Configured channels and status", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_channels() {}

#[utoipa::path(
    post, path = "/api/channels",
    tag = "channels",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Channel created", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid channel request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn create_channel() {}

#[utoipa::path(
    post, path = "/api/channels/{id}/reconnect",
    tag = "channels",
    params(("id" = String, Path, description = "Channel ID")),
    responses(
        (status = 200, description = "Channel reconnected", body = inline(serde_json::Value)),
        (status = 404, description = "Channel not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn reconnect_channel() {}

#[utoipa::path(
    delete, path = "/api/channels/{id}",
    tag = "channels",
    params(("id" = String, Path, description = "Channel ID")),
    responses(
        (status = 200, description = "Channel removed", body = inline(serde_json::Value)),
        (status = 404, description = "Channel not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_channel() {}

#[utoipa::path(
    post, path = "/api/channels/{type}/inject",
    tag = "channels",
    params(("type" = String, Path, description = "Channel type to inject into")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 202, description = "Synthetic message accepted", body = inline(serde_json::Value)),
        (status = 404, description = "Target channel or agent not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn inject_channel_message() {}

#[utoipa::path(
    get, path = "/api/itp/events",
    tag = "itp",
    params(("limit" = Option<u32>, Query, description = "Maximum number of recent events to return (default 200, max 500)")),
    responses(
        (status = 200, description = "Recent ITP event snapshot", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_itp_events() {}

#[utoipa::path(
    get, path = "/api/oauth/providers",
    tag = "oauth",
    responses(
        (status = 200, description = "Configured OAuth providers", body = Vec<OAuthProviderSchema>),
    ),
    security(("bearer_auth" = []))
)]
async fn list_oauth_providers() {}

#[utoipa::path(
    get, path = "/api/oauth/connections",
    tag = "oauth",
    responses(
        (status = 200, description = "Active OAuth connections", body = Vec<OAuthConnectionSchema>),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_oauth_connections() {}

#[utoipa::path(
    post, path = "/api/oauth/connect",
    tag = "oauth",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "OAuth flow started", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid OAuth connect request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn connect_oauth_provider() {}

#[utoipa::path(
    delete, path = "/api/oauth/connections/{ref_id}",
    tag = "oauth",
    params(("ref_id" = String, Path, description = "OAuth connection reference ID")),
    responses(
        (status = 200, description = "OAuth connection removed", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid OAuth reference", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn disconnect_oauth_connection() {}

#[utoipa::path(
    get, path = "/api/oauth/callback",
    tag = "oauth",
    params(
        ("code" = String, Query, description = "OAuth authorization code"),
        ("state" = String, Query, description = "OAuth anti-CSRF state token"),
    ),
    responses(
        (status = 200, description = "OAuth connection completed", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid callback parameters", body = ErrorResponseSchema),
    )
)]
async fn oauth_callback() {}

#[utoipa::path(
    post, path = "/api/oauth/execute",
    tag = "oauth",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "OAuth-backed API call result", body = inline(serde_json::Value)),
        (status = 202, description = "OAuth-backed API call accepted but recovery is required", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid OAuth execute request", body = ErrorResponseSchema),
        (status = 401, description = "Connection token expired or revoked", body = ErrorResponseSchema),
        (status = 404, description = "OAuth connection not found", body = ErrorResponseSchema),
        (status = 409, description = "Idempotency conflict", body = ErrorResponseSchema),
        (status = 502, description = "Provider error", body = ErrorResponseSchema),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn execute_oauth_api_call() {}

#[utoipa::path(
    get, path = "/api/mesh/trust-graph",
    tag = "mesh",
    responses(
        (status = 200, description = "Current multi-agent trust graph", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_mesh_trust_graph() {}

#[utoipa::path(
    get, path = "/api/mesh/consensus",
    tag = "mesh",
    responses(
        (status = 200, description = "Consensus rounds and vote counts", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_mesh_consensus() {}

#[utoipa::path(
    get, path = "/api/mesh/delegations",
    tag = "mesh",
    responses(
        (status = 200, description = "Delegation chains and sybil metrics", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_mesh_delegations() {}

#[utoipa::path(
    get, path = "/api/profiles",
    tag = "profiles",
    responses(
        (status = 200, description = "Preset and custom convergence profiles", body = inline(serde_json::Value)),
        (status = 500, description = "Internal error", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_profiles() {}

#[utoipa::path(
    post, path = "/api/profiles",
    tag = "profiles",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Profile created", body = ProfileSchema),
        (status = 400, description = "Invalid profile request", body = ErrorResponseSchema),
        (status = 409, description = "Profile name conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn create_profile() {}

#[utoipa::path(
    put, path = "/api/profiles/{name}",
    tag = "profiles",
    params(("name" = String, Path, description = "Profile name")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Profile updated", body = ProfileSchema),
        (status = 400, description = "Invalid profile update", body = ErrorResponseSchema),
        (status = 404, description = "Profile not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn update_profile() {}

#[utoipa::path(
    delete, path = "/api/profiles/{name}",
    tag = "profiles",
    params(("name" = String, Path, description = "Profile name")),
    responses(
        (status = 200, description = "Profile deleted", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid profile delete request", body = ErrorResponseSchema),
        (status = 404, description = "Profile not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_profile() {}

#[utoipa::path(
    post, path = "/api/agents/{id}/profile",
    tag = "profiles",
    params(("id" = String, Path, description = "Agent ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Profile assigned to agent", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid assign-profile request", body = ErrorResponseSchema),
        (status = 404, description = "Agent or profile not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn assign_agent_profile() {}

#[utoipa::path(
    get, path = "/api/search",
    tag = "search",
    params(
        ("q" = String, Query, description = "Search query"),
        ("types" = Option<String>, Query, description = "Comma-separated entity types to search"),
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
    ),
    responses(
        (status = 200, description = "Search results", body = SearchResponseSchema),
        (status = 400, description = "Invalid search request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn search() {}

#[utoipa::path(
    get, path = "/api/pc-control/status",
    tag = "pc-control",
    responses(
        (status = 200, description = "Current PC control safety status", body = PcControlStatusSchema),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_pc_control_status() {}

#[utoipa::path(
    put, path = "/api/pc-control/status",
    tag = "pc-control",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "PC control status updated", body = PcControlStatusSchema),
        (status = 400, description = "Invalid PC control update", body = ErrorResponseSchema),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn update_pc_control_status() {}

#[utoipa::path(
    get, path = "/api/pc-control/actions",
    tag = "pc-control",
    params(("limit" = Option<u32>, Query, description = "Maximum number of action log entries")),
    responses(
        (status = 200, description = "Recent PC control actions", body = inline(serde_json::Value)),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn list_pc_control_actions() {}

#[utoipa::path(
    put, path = "/api/pc-control/allowed-apps",
    tag = "pc-control",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Allowed apps updated", body = PcControlStatusSchema),
        (status = 400, description = "Invalid allowed-app update", body = ErrorResponseSchema),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn update_pc_control_allowed_apps() {}

#[utoipa::path(
    put, path = "/api/pc-control/blocked-hotkeys",
    tag = "pc-control",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Blocked hotkeys updated", body = PcControlStatusSchema),
        (status = 400, description = "Invalid blocked-hotkey update", body = ErrorResponseSchema),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn update_pc_control_blocked_hotkeys() {}

#[utoipa::path(
    put, path = "/api/pc-control/safe-zones",
    tag = "pc-control",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Safe zones updated", body = PcControlStatusSchema),
        (status = 400, description = "Invalid safe-zone update", body = ErrorResponseSchema),
        (status = 403, description = "Admin role required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn update_pc_control_safe_zones() {}

#[utoipa::path(
    post, path = "/api/ws/tickets",
    tag = "health",
    responses(
        (status = 200, description = "Short-lived WebSocket upgrade ticket", body = WsAuthTicketResponseSchema),
        (status = 401, description = "Authentication required", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn issue_ws_ticket() {}

#[utoipa::path(
    get, path = "/api/push/vapid-key",
    tag = "push",
    responses(
        (status = 200, description = "Web Push VAPID public key", body = inline(serde_json::Value)),
    ),
    security(("bearer_auth" = []))
)]
async fn get_push_vapid_key() {}

#[utoipa::path(
    post, path = "/api/push/subscribe",
    tag = "push",
    request_body = PushSubscriptionSchema,
    responses(
        (status = 204, description = "Push subscription registered"),
        (status = 500, description = "Push subscription store failure", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn subscribe_push() {}

#[utoipa::path(
    post, path = "/api/push/unsubscribe",
    tag = "push",
    request_body = PushSubscriptionSchema,
    responses(
        (status = 204, description = "Push subscription removed"),
        (status = 500, description = "Push subscription store failure", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn unsubscribe_push() {}

// ── Webhook schema ──

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct WebhookSchema {
    pub id: String,
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
}

#[derive(utoipa::ToSchema, serde::Serialize)]
pub struct A2ATaskSchema {
    pub id: String,
    pub target_agent: String,
    pub method: String,
    pub status: String,
    pub created_at: String,
}

// ── Webhook paths ──

#[utoipa::path(
    get, path = "/api/webhooks",
    tag = "webhooks",
    responses(
        (status = 200, description = "List all webhooks", body = Vec<WebhookSchema>),
    ),
    security(("bearer_auth" = []))
)]
async fn list_webhooks() {}

#[utoipa::path(
    post, path = "/api/webhooks",
    tag = "webhooks",
    responses(
        (status = 201, description = "Webhook created"),
        (status = 400, description = "Invalid webhook configuration"),
    ),
    security(("bearer_auth" = []))
)]
async fn create_webhook() {}

#[utoipa::path(
    put, path = "/api/webhooks/{id}",
    tag = "webhooks",
    params(("id" = String, Path, description = "Webhook ID")),
    responses(
        (status = 200, description = "Webhook updated"),
        (status = 404, description = "Webhook not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn update_webhook() {}

#[utoipa::path(
    delete, path = "/api/webhooks/{id}",
    tag = "webhooks",
    params(("id" = String, Path, description = "Webhook ID")),
    responses(
        (status = 200, description = "Webhook deleted"),
        (status = 404, description = "Webhook not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_webhook() {}

#[utoipa::path(
    post, path = "/api/webhooks/{id}/test",
    tag = "webhooks",
    params(("id" = String, Path, description = "Webhook ID")),
    responses(
        (status = 200, description = "Test webhook fired, returns status code"),
        (status = 404, description = "Webhook not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn test_webhook() {}

// ── Skill paths ──

#[utoipa::path(
    get, path = "/api/skills",
    tag = "skills",
    responses(
        (status = 200, description = "Installed and available skills", body = crate::skill_catalog::SkillListResponseDto),
    ),
    security(("bearer_auth" = []))
)]
async fn list_skills() {}

#[utoipa::path(
    post, path = "/api/skills/{id}/install",
    tag = "skills",
    params(("id" = String, Path, description = "Skill ID")),
    responses(
        (status = 200, description = "Skill installed", body = crate::skill_catalog::SkillSummaryDto),
        (status = 404, description = "Skill not found", body = ErrorResponseSchema),
        (status = 409, description = "Skill already installed or cannot be installed", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn install_skill() {}

#[utoipa::path(
    post, path = "/api/skills/{id}/uninstall",
    tag = "skills",
    params(("id" = String, Path, description = "Skill ID")),
    responses(
        (status = 200, description = "Skill uninstalled", body = crate::skill_catalog::SkillSummaryDto),
        (status = 404, description = "Skill not found", body = ErrorResponseSchema),
        (status = 409, description = "Skill cannot be uninstalled", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn uninstall_skill() {}

#[utoipa::path(
    post, path = "/api/skills/{id}/quarantine",
    tag = "skills",
    params(("id" = String, Path, description = "Skill ID")),
    request_body = crate::skill_catalog::SkillQuarantineRequestDto,
    responses(
        (status = 200, description = "Skill quarantined", body = crate::skill_catalog::SkillSummaryDto),
        (status = 404, description = "Skill not found", body = ErrorResponseSchema),
        (status = 409, description = "Skill cannot be quarantined", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn quarantine_skill() {}

#[utoipa::path(
    post, path = "/api/skills/{id}/quarantine/resolve",
    tag = "skills",
    params(("id" = String, Path, description = "Skill ID")),
    request_body = crate::skill_catalog::SkillQuarantineResolutionRequestDto,
    responses(
        (status = 200, description = "Skill quarantine resolved", body = crate::skill_catalog::SkillSummaryDto),
        (status = 404, description = "Skill not found", body = ErrorResponseSchema),
        (status = 409, description = "Skill quarantine could not be resolved", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn resolve_skill_quarantine() {}

#[utoipa::path(
    post, path = "/api/skills/{id}/reverify",
    tag = "skills",
    params(("id" = String, Path, description = "Skill ID")),
    responses(
        (status = 200, description = "Skill reverified", body = crate::skill_catalog::SkillSummaryDto),
        (status = 404, description = "Skill not found", body = ErrorResponseSchema),
        (status = 409, description = "Skill cannot be reverified", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn reverify_skill() {}

#[utoipa::path(
    post, path = "/api/skills/{name}/execute",
    tag = "skills",
    params(("name" = String, Path, description = "Catalog skill identifier")),
    request_body = crate::skill_catalog::ExecuteSkillRequestDto,
    responses(
        (status = 200, description = "Skill execution result", body = crate::skill_catalog::ExecuteSkillResponseDto),
        (status = 400, description = "Invalid skill request", body = ErrorResponseSchema),
        (status = 403, description = "Skill blocked by policy or agent allowlist", body = ErrorResponseSchema),
        (status = 404, description = "Skill not found", body = ErrorResponseSchema),
        (status = 409, description = "Skill is disabled or unavailable", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn execute_skill_by_name() {}

#[utoipa::path(
    get, path = "/api/marketplace/agents",
    tag = "marketplace",
    params(
        ("status" = Option<String>, Query, description = "Status filter"),
        ("min_trust" = Option<f64>, Query, description = "Minimum trust score"),
        ("min_rating" = Option<f64>, Query, description = "Minimum rating"),
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Marketplace agent listings", body = inline(serde_json::Value)),
    ),
    security(("bearer_auth" = []))
)]
async fn list_marketplace_agents() {}

#[utoipa::path(
    post, path = "/api/marketplace/agents",
    tag = "marketplace",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace agent registered", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid listing request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn register_marketplace_agent() {}

#[utoipa::path(
    get, path = "/api/marketplace/agents/{id}",
    tag = "marketplace",
    params(("id" = String, Path, description = "Agent listing ID")),
    responses(
        (status = 200, description = "Marketplace agent detail", body = inline(serde_json::Value)),
        (status = 404, description = "Agent listing not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_marketplace_agent() {}

#[utoipa::path(
    put, path = "/api/marketplace/agents/{id}/status",
    tag = "marketplace",
    params(("id" = String, Path, description = "Agent listing ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace agent status updated", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid status update", body = ErrorResponseSchema),
        (status = 404, description = "Agent listing not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn update_marketplace_agent_status() {}

#[utoipa::path(
    delete, path = "/api/marketplace/agents/{id}",
    tag = "marketplace",
    params(("id" = String, Path, description = "Agent listing ID")),
    responses(
        (status = 200, description = "Marketplace agent delisted", body = inline(serde_json::Value)),
        (status = 404, description = "Agent listing not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn delist_marketplace_agent() {}

#[utoipa::path(
    get, path = "/api/marketplace/skills",
    tag = "marketplace",
    params(
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Marketplace skill listings", body = inline(serde_json::Value)),
    ),
    security(("bearer_auth" = []))
)]
async fn list_marketplace_skills() {}

#[utoipa::path(
    post, path = "/api/marketplace/skills",
    tag = "marketplace",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace skill published", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid skill publish request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn publish_marketplace_skill() {}

#[utoipa::path(
    get, path = "/api/marketplace/skills/{name}",
    tag = "marketplace",
    params(("name" = String, Path, description = "Marketplace skill name")),
    responses(
        (status = 200, description = "Marketplace skill detail", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace skill not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_marketplace_skill() {}

#[utoipa::path(
    get, path = "/api/marketplace/contracts",
    tag = "marketplace",
    params(
        ("agent_id" = Option<String>, Query, description = "Filter by hirer or worker agent"),
        ("state" = Option<String>, Query, description = "Filter by contract state"),
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Marketplace contract list", body = inline(serde_json::Value)),
    ),
    security(("bearer_auth" = []))
)]
async fn list_marketplace_contracts() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts",
    tag = "marketplace",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace contract proposed", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid contract request", body = ErrorResponseSchema),
        (status = 404, description = "Marketplace entity not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn propose_marketplace_contract() {}

#[utoipa::path(
    get, path = "/api/marketplace/contracts/{id}",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    responses(
        (status = 200, description = "Marketplace contract detail", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_marketplace_contract() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts/{id}/accept",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    responses(
        (status = 200, description = "Marketplace contract accepted", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract state conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn accept_marketplace_contract() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts/{id}/reject",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    responses(
        (status = 200, description = "Marketplace contract rejected", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract state conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn reject_marketplace_contract() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts/{id}/start",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    responses(
        (status = 200, description = "Marketplace contract started", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract state conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn start_marketplace_contract() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts/{id}/complete",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace contract completed", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract state conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn complete_marketplace_contract() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts/{id}/dispute",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    responses(
        (status = 200, description = "Marketplace contract disputed", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract state conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn dispute_marketplace_contract() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts/{id}/cancel",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    responses(
        (status = 200, description = "Marketplace contract canceled", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract state conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn cancel_marketplace_contract() {}

#[utoipa::path(
    post, path = "/api/marketplace/contracts/{id}/resolve",
    tag = "marketplace",
    params(("id" = String, Path, description = "Marketplace contract ID")),
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace contract resolved", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace contract not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace contract state conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn resolve_marketplace_contract() {}

#[utoipa::path(
    get, path = "/api/marketplace/wallet",
    tag = "marketplace",
    params(("agent_id" = String, Query, description = "Agent ID whose wallet to inspect")),
    responses(
        (status = 200, description = "Marketplace wallet balance", body = inline(serde_json::Value)),
        (status = 404, description = "Marketplace wallet not found", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn get_marketplace_wallet() {}

#[utoipa::path(
    post, path = "/api/marketplace/wallet/seed",
    tag = "marketplace",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace wallet funded", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid wallet seed request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn seed_marketplace_wallet() {}

#[utoipa::path(
    get, path = "/api/marketplace/wallet/transactions",
    tag = "marketplace",
    params(
        ("agent_id" = String, Query, description = "Agent ID whose transactions to list"),
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Marketplace transaction history", body = inline(serde_json::Value)),
    ),
    security(("bearer_auth" = []))
)]
async fn list_marketplace_transactions() {}

#[utoipa::path(
    post, path = "/api/marketplace/reviews",
    tag = "marketplace",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Marketplace review submitted", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid review request", body = ErrorResponseSchema),
        (status = 404, description = "Marketplace entity not found", body = ErrorResponseSchema),
        (status = 409, description = "Marketplace review conflict", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn submit_marketplace_review() {}

#[utoipa::path(
    get, path = "/api/marketplace/reviews/{agent_id}",
    tag = "marketplace",
    params(
        ("agent_id" = String, Path, description = "Reviewee agent ID"),
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Marketplace reviews for agent", body = inline(serde_json::Value)),
    ),
    security(("bearer_auth" = []))
)]
async fn list_marketplace_reviews() {}

#[utoipa::path(
    post, path = "/api/marketplace/discover",
    tag = "marketplace",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Capability-based marketplace matches", body = inline(serde_json::Value)),
        (status = 400, description = "Invalid discovery request", body = ErrorResponseSchema),
    ),
    security(("bearer_auth" = []))
)]
async fn discover_marketplace_agents() {}

// ── Safety check paths ──

#[utoipa::path(
    get, path = "/api/safety/checks",
    tag = "safety-checks",
    responses(
        (status = 200, description = "List registered custom safety checks"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_safety_checks() {}

#[utoipa::path(
    post, path = "/api/safety/checks",
    tag = "safety-checks",
    responses(
        (status = 201, description = "Custom safety check registered"),
        (status = 400, description = "Invalid check configuration"),
    ),
    security(("bearer_auth" = []))
)]
async fn register_safety_check() {}

#[utoipa::path(
    delete, path = "/api/safety/checks/{id}",
    tag = "safety-checks",
    params(("id" = String, Path, description = "Safety check ID")),
    responses(
        (status = 200, description = "Safety check removed"),
        (status = 404, description = "Check not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn unregister_safety_check() {}

// ── A2A paths ──

#[utoipa::path(
    post, path = "/api/a2a/tasks",
    tag = "a2a",
    responses(
        (status = 201, description = "A2A task sent", body = A2ATaskSchema),
        (status = 400, description = "Invalid task request"),
        (status = 502, description = "Target agent unreachable"),
    ),
    security(("bearer_auth" = []))
)]
async fn send_a2a_task() {}

#[utoipa::path(
    get, path = "/api/a2a/tasks/{task_id}",
    tag = "a2a",
    params(("task_id" = String, Path, description = "A2A task ID")),
    responses(
        (status = 200, description = "Task status and result", body = A2ATaskSchema),
        (status = 404, description = "Task not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_a2a_task() {}

#[utoipa::path(
    get, path = "/api/a2a/tasks",
    tag = "a2a",
    responses(
        (status = 200, description = "List of A2A tasks", body = Vec<A2ATaskSchema>),
    ),
    security(("bearer_auth" = []))
)]
async fn list_a2a_tasks() {}

#[utoipa::path(
    get, path = "/api/a2a/tasks/{task_id}/stream",
    tag = "a2a",
    params(("task_id" = String, Path, description = "A2A task ID")),
    responses(
        (status = 200, description = "SSE stream of task updates"),
        (status = 404, description = "Task not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn stream_a2a_task() {}

#[utoipa::path(
    get, path = "/api/a2a/discover",
    tag = "a2a",
    responses(
        (status = 200, description = "Discovered A2A agents"),
    ),
    security(("bearer_auth" = []))
)]
async fn discover_a2a_agents() {}

// ── Handler ──

/// GET /api/openapi.json — serve the generated OpenAPI specification.
pub async fn openapi_spec() -> impl IntoResponse {
    let doc = ApiDoc::openapi();
    (StatusCode::OK, Json(doc))
}
