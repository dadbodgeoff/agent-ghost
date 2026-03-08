use std::sync::Arc;

use axum::routing::{delete, get, post, put};

use crate::api::rbac;
use crate::state::AppState;

pub fn public_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/health", get(crate::api::health::health_handler))
        .route("/api/ready", get(crate::api::health::ready_handler))
        .route(
            "/api/compatibility",
            get(crate::api::compatibility::compatibility_handler),
        )
        .route("/api/auth/login", post(crate::api::auth::login))
        .route("/api/auth/refresh", post(crate::api::auth::refresh))
        .route("/api/auth/logout", post(crate::api::auth::logout))
        .route("/api/openapi.json", get(crate::api::openapi::openapi_spec))
}

pub fn read_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/auth/session", get(crate::api::auth::session))
        .route("/api/agents", get(crate::api::agents::list_agents))
        .route("/api/audit", get(crate::api::audit::query_audit))
        .route(
            "/api/audit/aggregation",
            get(crate::api::audit::audit_aggregation),
        )
        .route("/api/audit/export", get(crate::api::audit::audit_export))
        .route(
            "/api/convergence/scores",
            get(crate::api::convergence::get_scores),
        )
        .route("/api/goals", get(crate::api::goals::list_goals))
        .route("/api/goals/:id", get(crate::api::goals::get_goal))
        .route("/api/sessions", get(crate::api::sessions::list_sessions))
        .route(
            "/api/sessions/:id/events",
            get(crate::api::sessions::session_events),
        )
        .route(
            "/api/sessions/:id/bookmarks",
            get(crate::api::sessions::list_bookmarks),
        )
        .route("/api/memory", get(crate::api::memory::list_memories))
        .route(
            "/api/memory/graph",
            get(crate::api::memory::get_memory_graph),
        )
        .route(
            "/api/memory/search",
            get(crate::api::memory::search_memories),
        )
        .route(
            "/api/memory/archived",
            get(crate::api::memory::list_archived),
        )
        .route("/api/memory/:id", get(crate::api::memory::get_memory))
        .route(
            "/api/live-executions/:execution_id",
            get(crate::api::live_executions::get_live_execution),
        )
        .route(
            "/api/state/crdt/:agent_id",
            get(crate::api::state::get_crdt_state),
        )
        .route(
            "/api/integrity/chain/:agent_id",
            get(crate::api::integrity::verify_chain),
        )
        .route("/api/workflows", get(crate::api::workflows::list_workflows))
        .route(
            "/api/workflows/:id",
            get(crate::api::workflows::get_workflow),
        )
        .route(
            "/api/workflows/:id/executions",
            get(crate::api::workflows::list_executions),
        )
        .route(
            "/api/studio/sessions",
            get(crate::api::studio_sessions::list_sessions),
        )
        .route(
            "/api/studio/sessions/:id",
            get(crate::api::studio_sessions::get_session),
        )
        .route(
            "/api/studio/sessions/:id/stream/recover",
            get(crate::api::studio_sessions::recover_stream),
        )
        .route(
            "/api/traces/:session_id",
            get(crate::api::traces::get_traces),
        )
        .route(
            "/api/mesh/trust-graph",
            get(crate::api::mesh_viz::trust_graph),
        )
        .route(
            "/api/mesh/consensus",
            get(crate::api::mesh_viz::consensus_state),
        )
        .route(
            "/api/mesh/delegations",
            get(crate::api::mesh_viz::delegations),
        )
        .route("/api/profiles", get(crate::api::profiles::list_profiles))
        .route("/api/search", get(crate::api::search::search))
        .route("/api/skills", get(crate::api::skills::list_skills))
        .route("/api/a2a/tasks", get(crate::api::a2a::list_tasks))
        .route("/api/a2a/tasks/:task_id", get(crate::api::a2a::get_task))
        .route(
            "/api/a2a/tasks/:task_id/stream",
            get(crate::api::a2a::stream_task),
        )
        .route("/api/a2a/discover", get(crate::api::a2a::discover_agents))
        .route("/api/channels", get(crate::api::channels::list_channels))
        .route("/api/costs", get(crate::api::costs::get_costs))
        .route("/api/itp/events", get(crate::api::itp::list_events))
        .route("/api/ws", get(crate::api::websocket::ws_handler))
        .route(
            "/api/ws/tickets",
            post(crate::api::websocket::issue_ws_ticket),
        )
        .route(
            "/api/oauth/providers",
            get(crate::api::oauth_routes::list_providers),
        )
        .route(
            "/api/oauth/callback",
            get(crate::api::oauth_routes::callback),
        )
        .route(
            "/api/oauth/connections",
            get(crate::api::oauth_routes::list_connections),
        )
        .route(
            "/api/marketplace/agents",
            get(crate::api::marketplace::list_agents),
        )
        .route(
            "/api/marketplace/agents/:id",
            get(crate::api::marketplace::get_agent),
        )
        .route(
            "/api/marketplace/skills",
            get(crate::api::marketplace::list_skills),
        )
        .route(
            "/api/marketplace/skills/:name",
            get(crate::api::marketplace::get_skill),
        )
        .route(
            "/api/marketplace/contracts",
            get(crate::api::marketplace::list_contracts),
        )
        .route(
            "/api/marketplace/contracts/:id",
            get(crate::api::marketplace::get_contract),
        )
        .route(
            "/api/marketplace/wallet",
            get(crate::api::marketplace::get_wallet),
        )
        .route(
            "/api/marketplace/wallet/transactions",
            get(crate::api::marketplace::list_transactions),
        )
        .route(
            "/api/marketplace/reviews/:agent_id",
            get(crate::api::marketplace::list_reviews),
        )
}

pub fn operator_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/agents", post(crate::api::agents::create_agent))
        .route("/api/agents/:id", delete(crate::api::agents::delete_agent))
        .route(
            "/api/goals/:id/approve",
            post(crate::api::goals::approve_goal),
        )
        .route(
            "/api/goals/:id/reject",
            post(crate::api::goals::reject_goal),
        )
        .route("/api/memory", post(crate::api::memory::write_memory))
        .route(
            "/api/memory/:id/archive",
            post(crate::api::memory::archive_memory),
        )
        .route(
            "/api/memory/:id/unarchive",
            post(crate::api::memory::unarchive_memory),
        )
        .route(
            "/api/workflows",
            post(crate::api::workflows::create_workflow),
        )
        .route(
            "/api/workflows/:id",
            put(crate::api::workflows::update_workflow),
        )
        .route(
            "/api/workflows/:id/execute",
            post(crate::api::workflows::execute_workflow),
        )
        .route(
            "/api/workflows/:id/resume/:execution_id",
            post(crate::api::workflows::resume_execution),
        )
        .route(
            "/api/sessions/:id/bookmarks",
            post(crate::api::sessions::create_bookmark),
        )
        .route(
            "/api/sessions/:id/bookmarks/:bookmark_id",
            delete(crate::api::sessions::delete_bookmark),
        )
        .route(
            "/api/sessions/:id/branch",
            post(crate::api::sessions::branch_session),
        )
        .route(
            "/api/sessions/:id/heartbeat",
            post(crate::api::sessions::session_heartbeat),
        )
        .route("/api/studio/run", post(crate::api::studio::run_prompt))
        .route(
            "/api/studio/sessions",
            post(crate::api::studio_sessions::create_session),
        )
        .route(
            "/api/studio/sessions/:id",
            delete(crate::api::studio_sessions::delete_session),
        )
        .route(
            "/api/studio/sessions/:id/messages",
            post(crate::api::studio_sessions::send_message),
        )
        .route(
            "/api/studio/sessions/:id/messages/stream",
            post(crate::api::studio_sessions::send_message_stream),
        )
        .route("/api/agent/chat", post(crate::api::agent_chat::agent_chat))
        .route(
            "/api/agent/chat/stream",
            post(crate::api::agent_chat::agent_chat_stream),
        )
        .route("/api/profiles", post(crate::api::profiles::create_profile))
        .route(
            "/api/profiles/:name",
            put(crate::api::profiles::update_profile).delete(crate::api::profiles::delete_profile),
        )
        .route(
            "/api/agents/:id/profile",
            post(crate::api::profiles::assign_profile),
        )
        .route(
            "/api/skills/:id/install",
            post(crate::api::skills::install_skill),
        )
        .route(
            "/api/skills/:id/uninstall",
            post(crate::api::skills::uninstall_skill),
        )
        .route(
            "/api/skills/:name/execute",
            post(crate::api::skill_execute::execute_skill),
        )
        .route("/api/channels", post(crate::api::channels::create_channel))
        .route(
            "/api/channels/:id/reconnect",
            post(crate::api::channels::reconnect_channel),
        )
        .route(
            "/api/channels/:id",
            delete(crate::api::channels::delete_channel),
        )
        .route(
            "/api/channels/:type/inject",
            post(crate::api::channels::inject_message),
        )
        .route("/api/a2a/tasks", post(crate::api::a2a::send_task))
        .route(
            "/api/oauth/connect",
            post(crate::api::oauth_routes::connect),
        )
        .route(
            "/api/oauth/connections/:ref_id",
            delete(crate::api::oauth_routes::disconnect),
        )
        .route(
            "/api/oauth/execute",
            post(crate::api::oauth_routes::execute_api_call),
        )
        .route(
            "/api/marketplace/agents",
            post(crate::api::marketplace::register_agent),
        )
        .route(
            "/api/marketplace/agents/:id",
            delete(crate::api::marketplace::delist_agent),
        )
        .route(
            "/api/marketplace/agents/:id/status",
            put(crate::api::marketplace::update_agent_status),
        )
        .route(
            "/api/marketplace/skills",
            post(crate::api::marketplace::publish_skill),
        )
        .route(
            "/api/marketplace/contracts",
            post(crate::api::marketplace::propose_contract),
        )
        .route(
            "/api/marketplace/contracts/:id/accept",
            post(crate::api::marketplace::accept_contract),
        )
        .route(
            "/api/marketplace/contracts/:id/reject",
            post(crate::api::marketplace::reject_contract),
        )
        .route(
            "/api/marketplace/contracts/:id/start",
            post(crate::api::marketplace::start_contract),
        )
        .route(
            "/api/marketplace/contracts/:id/complete",
            post(crate::api::marketplace::complete_contract),
        )
        .route(
            "/api/marketplace/contracts/:id/dispute",
            post(crate::api::marketplace::dispute_contract),
        )
        .route(
            "/api/marketplace/contracts/:id/cancel",
            post(crate::api::marketplace::cancel_contract),
        )
        .route(
            "/api/marketplace/contracts/:id/resolve",
            post(crate::api::marketplace::resolve_contract),
        )
        .route(
            "/api/marketplace/wallet/seed",
            post(crate::api::marketplace::seed_wallet),
        )
        .route(
            "/api/marketplace/reviews",
            post(crate::api::marketplace::submit_review),
        )
        .route(
            "/api/marketplace/discover",
            post(crate::api::marketplace::discover_agents),
        )
        .route_layer(axum::middleware::from_fn(
            crate::api::compatibility::enforce_client_compatibility_middleware,
        ))
        .route_layer(axum::middleware::from_fn(rbac::operator))
}

pub fn admin_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/safety/status", get(crate::api::safety::safety_status))
        .route(
            "/api/safety/checks",
            get(crate::api::safety_checks::list_safety_checks),
        )
        .route("/api/admin/backups", get(crate::api::admin::list_backups))
        .route("/api/admin/export", get(crate::api::admin::export_data))
        .route(
            "/api/admin/provider-keys",
            get(crate::api::provider_keys::list_provider_keys),
        )
        .route(
            "/api/pc-control/status",
            get(crate::api::pc_control::get_status).put(crate::api::pc_control::update_status),
        )
        .route(
            "/api/pc-control/actions",
            get(crate::api::pc_control::list_actions),
        )
        .route(
            "/api/safety/pause/:agent_id",
            post(crate::api::safety::pause_agent),
        )
        .route(
            "/api/safety/resume/:agent_id",
            post(crate::api::safety::resume_agent),
        )
        .route(
            "/api/safety/quarantine/:agent_id",
            post(crate::api::safety::quarantine_agent),
        )
        .route(
            "/api/safety/checks",
            post(crate::api::safety_checks::register_safety_check),
        )
        .route(
            "/api/safety/checks/:id",
            delete(crate::api::safety_checks::unregister_safety_check),
        )
        .route(
            "/api/webhooks",
            get(crate::api::webhooks::list_webhooks).post(crate::api::webhooks::create_webhook),
        )
        .route(
            "/api/webhooks/:id",
            put(crate::api::webhooks::update_webhook).delete(crate::api::webhooks::delete_webhook),
        )
        .route(
            "/api/webhooks/:id/test",
            post(crate::api::webhooks::test_webhook),
        )
        .route("/api/admin/backup", post(crate::api::admin::create_backup))
        .route(
            "/api/admin/provider-keys",
            put(crate::api::provider_keys::set_provider_key),
        )
        .route(
            "/api/admin/provider-keys/:env_name",
            delete(crate::api::provider_keys::delete_provider_key),
        )
        .route(
            "/api/pc-control/allowed-apps",
            put(crate::api::pc_control::update_allowed_apps),
        )
        .route(
            "/api/pc-control/blocked-hotkeys",
            put(crate::api::pc_control::update_blocked_hotkeys),
        )
        .route(
            "/api/pc-control/safe-zones",
            put(crate::api::pc_control::update_safe_zones),
        )
        .route_layer(axum::middleware::from_fn(
            crate::api::compatibility::enforce_client_compatibility_middleware,
        ))
        .route_layer(axum::middleware::from_fn(rbac::admin))
}

pub fn superadmin_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/safety/kill-all", post(crate::api::safety::kill_all))
        .route(
            "/api/admin/restore",
            post(crate::api::admin::restore_backup),
        )
        .route_layer(axum::middleware::from_fn(
            crate::api::compatibility::enforce_client_compatibility_middleware,
        ))
        .route_layer(axum::middleware::from_fn(rbac::superadmin))
}
