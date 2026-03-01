//! OpenAPI specification endpoint.
//!
//! Serves the auto-generated OpenAPI 3.1 spec at `GET /api/openapi.json`.
//! Uses `utoipa` to derive the spec from handler types and response structs.
//!
//! Ref: ADE_DESIGN_PLAN §17.3, tasks.md T-1.3.1

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
        (url = "http://localhost:18789", description = "Local development"),
    ),
    paths(
        health,
        ready,
        list_agents,
        create_agent,
        delete_agent,
        list_sessions,
        get_convergence_scores,
        list_goals,
        approve_goal,
        reject_goal,
        list_memories,
        get_memory,
        write_memory,
        get_costs,
        safety_status,
        kill_all,
        pause_agent,
        resume_agent,
        quarantine_agent,
        query_audit,
        audit_aggregation,
        audit_export,
        login,
        refresh,
        logout,
        list_webhooks,
        create_webhook,
        update_webhook,
        delete_webhook,
        test_webhook,
        list_skills,
        install_skill,
        uninstall_skill,
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
            ConvergenceScoreSchema,
            SessionSchema,
            AgentCostSchema,
            WebhookSchema,
            SkillSchema,
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
        (name = "costs", description = "Per-agent cost tracking"),
        (name = "safety", description = "Kill switch and quarantine controls"),
        (name = "audit", description = "Audit log queries and export"),
        (name = "webhooks", description = "Webhook configuration and testing"),
        (name = "skills", description = "Skill marketplace management"),
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
pub struct SkillSchema {
    pub id: String,
    pub skill_name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub source: String,
    pub state: String,
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
        (status = 200, description = "Installed and available skills"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_skills() {}

#[utoipa::path(
    post, path = "/api/skills/{id}/install",
    tag = "skills",
    params(("id" = String, Path, description = "Skill ID")),
    responses(
        (status = 200, description = "Skill installed"),
        (status = 404, description = "Skill not found"),
        (status = 409, description = "Skill already installed"),
    ),
    security(("bearer_auth" = []))
)]
async fn install_skill() {}

#[utoipa::path(
    post, path = "/api/skills/{id}/uninstall",
    tag = "skills",
    params(("id" = String, Path, description = "Skill ID")),
    responses(
        (status = 200, description = "Skill uninstalled"),
        (status = 404, description = "Skill not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn uninstall_skill() {}

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
