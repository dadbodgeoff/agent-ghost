use std::sync::Arc;

use axum::http::Method;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{delete, get, post, put};

use crate::api::authz::RouteId;
use crate::api::authz_policy::{route_spec_for, RouteAuthorizationSpec};
use crate::api::rbac;
use crate::state::AppState;

fn action_route(
    path: &'static str,
    method: Method,
    method_router: axum::routing::MethodRouter<Arc<AppState>>,
    spec: RouteAuthorizationSpec,
) -> axum::Router<Arc<AppState>> {
    debug_assert_eq!(
        spec,
        route_spec_for(spec.route_id, &method).expect("route spec")
    );

    let mut router = axum::Router::new()
        .route(path, method_router)
        .route_layer(from_fn(move |req, next| {
            rbac::require_route(spec, req, next)
        }));

    if spec.compatibility_required {
        router = router.route_layer(from_fn(
            crate::api::compatibility::enforce_client_compatibility_middleware,
        ));
    }

    router
}

fn spec(route_id: RouteId, method: Method) -> RouteAuthorizationSpec {
    route_spec_for(route_id, &method).expect("route spec")
}

fn live_execution_route(
    app_state: Arc<AppState>,
    path: &'static str,
    method_router: axum::routing::MethodRouter<Arc<AppState>>,
) -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(path, method_router)
        .route_layer(from_fn_with_state(
            app_state,
            rbac::require_live_execution_read_route,
        ))
}

pub fn public_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/health", get(crate::api::health::health_handler))
        .route("/api/ready", get(crate::api::health::ready_handler))
        .route(
            "/api/compatibility",
            get(crate::api::compatibility::compatibility_handler),
        )
        // WebSocket upgrade auth is handled inside the route handler because
        // browsers cannot attach standard Bearer headers during the handshake.
        .route("/api/ws", get(crate::api::websocket::ws_handler))
        .route("/api/auth/login", post(crate::api::auth::login))
        .route("/api/auth/refresh", post(crate::api::auth::refresh))
        .route("/api/auth/logout", post(crate::api::auth::logout))
        .route("/api/openapi.json", get(crate::api::openapi::openapi_spec))
}

pub fn read_routes(app_state: Arc<AppState>) -> axum::Router<Arc<AppState>> {
    action_route(
        "/api/auth/session",
        Method::GET,
        get(crate::api::auth::session),
        spec(RouteId::AuthSession, Method::GET),
    )
    .merge(action_route(
        "/api/agents",
        Method::GET,
        get(crate::api::agents::list_agents),
        spec(RouteId::Agents, Method::GET),
    ))
    .merge(action_route(
        "/api/audit",
        Method::GET,
        get(crate::api::audit::query_audit),
        spec(RouteId::Audit, Method::GET),
    ))
    .merge(action_route(
        "/api/audit/aggregation",
        Method::GET,
        get(crate::api::audit::audit_aggregation),
        spec(RouteId::AuditAggregation, Method::GET),
    ))
    .merge(action_route(
        "/api/audit/export",
        Method::GET,
        get(crate::api::audit::audit_export),
        spec(RouteId::AuditExport, Method::GET),
    ))
    .merge(action_route(
        "/api/convergence/scores",
        Method::GET,
        get(crate::api::convergence::get_scores),
        spec(RouteId::ConvergenceScores, Method::GET),
    ))
    .merge(action_route(
        "/api/convergence/history/:agent_id",
        Method::GET,
        get(crate::api::convergence::get_history),
        spec(RouteId::ConvergenceHistoryByAgentId, Method::GET),
    ))
    .merge(action_route(
        "/api/goals",
        Method::GET,
        get(crate::api::goals::list_goals),
        spec(RouteId::Goals, Method::GET),
    ))
    .merge(action_route(
        "/api/goals/:id",
        Method::GET,
        get(crate::api::goals::get_goal),
        spec(RouteId::GoalById, Method::GET),
    ))
    .merge(action_route(
        "/api/sessions",
        Method::GET,
        get(crate::api::sessions::list_sessions),
        spec(RouteId::Sessions, Method::GET),
    ))
    .merge(action_route(
        "/api/sessions/:id/events",
        Method::GET,
        get(crate::api::sessions::session_events),
        spec(RouteId::SessionEventsById, Method::GET),
    ))
    .merge(action_route(
        "/api/sessions/:id/bookmarks",
        Method::GET,
        get(crate::api::sessions::list_bookmarks),
        spec(RouteId::SessionBookmarksById, Method::GET),
    ))
    .merge(action_route(
        "/api/memory",
        Method::GET,
        get(crate::api::memory::list_memories),
        spec(RouteId::Memory, Method::GET),
    ))
    .merge(action_route(
        "/api/memory/graph",
        Method::GET,
        get(crate::api::memory::get_memory_graph),
        spec(RouteId::MemoryGraph, Method::GET),
    ))
    .merge(action_route(
        "/api/memory/search",
        Method::GET,
        get(crate::api::memory::search_memories),
        spec(RouteId::MemorySearch, Method::GET),
    ))
    .merge(action_route(
        "/api/memory/archived",
        Method::GET,
        get(crate::api::memory::list_archived),
        spec(RouteId::MemoryArchived, Method::GET),
    ))
    .merge(action_route(
        "/api/memory/:id",
        Method::GET,
        get(crate::api::memory::get_memory),
        spec(RouteId::MemoryById, Method::GET),
    ))
    .merge(live_execution_route(
        app_state,
        "/api/live-executions/:execution_id",
        get(crate::api::live_executions::get_live_execution),
    ))
    .merge(action_route(
        "/api/state/crdt/:agent_id",
        Method::GET,
        get(crate::api::state::get_crdt_state),
        spec(RouteId::StateCrdtByAgentId, Method::GET),
    ))
    .merge(action_route(
        "/api/integrity/chain/:agent_id",
        Method::GET,
        get(crate::api::integrity::verify_chain),
        spec(RouteId::IntegrityChainByAgentId, Method::GET),
    ))
    .merge(action_route(
        "/api/workflows",
        Method::GET,
        get(crate::api::workflows::list_workflows),
        spec(RouteId::Workflows, Method::GET),
    ))
    .merge(action_route(
        "/api/workflows/:id",
        Method::GET,
        get(crate::api::workflows::get_workflow),
        spec(RouteId::WorkflowById, Method::GET),
    ))
    .merge(action_route(
        "/api/workflows/:id/executions",
        Method::GET,
        get(crate::api::workflows::list_executions),
        spec(RouteId::WorkflowExecutionsById, Method::GET),
    ))
    .merge(action_route(
        "/api/studio/sessions",
        Method::GET,
        get(crate::api::studio_sessions::list_sessions),
        spec(RouteId::StudioSessions, Method::GET),
    ))
    .merge(action_route(
        "/api/studio/sessions/:id",
        Method::GET,
        get(crate::api::studio_sessions::get_session),
        spec(RouteId::StudioSessionById, Method::GET),
    ))
    .merge(action_route(
        "/api/studio/sessions/:id/stream/recover",
        Method::GET,
        get(crate::api::studio_sessions::recover_stream),
        spec(RouteId::StudioSessionRecoverStreamById, Method::GET),
    ))
    .merge(action_route(
        "/api/traces/:session_id",
        Method::GET,
        get(crate::api::traces::get_traces),
        spec(RouteId::TracesBySessionId, Method::GET),
    ))
    .merge(action_route(
        "/api/mesh/trust-graph",
        Method::GET,
        get(crate::api::mesh_viz::trust_graph),
        spec(RouteId::MeshTrustGraph, Method::GET),
    ))
    .merge(action_route(
        "/api/mesh/consensus",
        Method::GET,
        get(crate::api::mesh_viz::consensus_state),
        spec(RouteId::MeshConsensus, Method::GET),
    ))
    .merge(action_route(
        "/api/mesh/delegations",
        Method::GET,
        get(crate::api::mesh_viz::delegations),
        spec(RouteId::MeshDelegations, Method::GET),
    ))
    .merge(action_route(
        "/api/profiles",
        Method::GET,
        get(crate::api::profiles::list_profiles),
        spec(RouteId::Profiles, Method::GET),
    ))
    .merge(action_route(
        "/api/search",
        Method::GET,
        get(crate::api::search::search),
        spec(RouteId::Search, Method::GET),
    ))
    .merge(action_route(
        "/api/skills",
        Method::GET,
        get(crate::api::skills::list_skills),
        spec(RouteId::Skills, Method::GET),
    ))
    .merge(action_route(
        "/api/a2a/tasks",
        Method::GET,
        get(crate::api::a2a::list_tasks),
        spec(RouteId::A2aTasks, Method::GET),
    ))
    .merge(action_route(
        "/api/a2a/tasks/:task_id",
        Method::GET,
        get(crate::api::a2a::get_task),
        spec(RouteId::A2aTaskById, Method::GET),
    ))
    .merge(action_route(
        "/api/a2a/tasks/:task_id/stream",
        Method::GET,
        get(crate::api::a2a::stream_task),
        spec(RouteId::A2aTaskStreamById, Method::GET),
    ))
    .merge(action_route(
        "/api/a2a/discover",
        Method::GET,
        get(crate::api::a2a::discover_agents),
        spec(RouteId::A2aDiscover, Method::GET),
    ))
    .merge(action_route(
        "/api/channels",
        Method::GET,
        get(crate::api::channels::list_channels),
        spec(RouteId::Channels, Method::GET),
    ))
    .merge(action_route(
        "/api/costs",
        Method::GET,
        get(crate::api::costs::get_costs),
        spec(RouteId::Costs, Method::GET),
    ))
    .merge(action_route(
        "/api/itp/events",
        Method::GET,
        get(crate::api::itp::list_events),
        spec(RouteId::ItpEvents, Method::GET),
    ))
    .merge(action_route(
        "/api/ws/tickets",
        Method::POST,
        post(crate::api::websocket::issue_ws_ticket),
        spec(RouteId::WebSocketTickets, Method::POST),
    ))
    .merge(action_route(
        "/api/oauth/providers",
        Method::GET,
        get(crate::api::oauth_routes::list_providers),
        spec(RouteId::OAuthProviders, Method::GET),
    ))
    .merge(action_route(
        "/api/oauth/callback",
        Method::GET,
        get(crate::api::oauth_routes::callback),
        spec(RouteId::OAuthCallback, Method::GET),
    ))
    .merge(action_route(
        "/api/oauth/connections",
        Method::GET,
        get(crate::api::oauth_routes::list_connections),
        spec(RouteId::OAuthConnections, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/agents",
        Method::GET,
        get(crate::api::marketplace::list_agents),
        spec(RouteId::MarketplaceAgents, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/agents/:id",
        Method::GET,
        get(crate::api::marketplace::get_agent),
        spec(RouteId::MarketplaceAgentById, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/skills",
        Method::GET,
        get(crate::api::marketplace::list_skills),
        spec(RouteId::MarketplaceSkills, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/skills/:name",
        Method::GET,
        get(crate::api::marketplace::get_skill),
        spec(RouteId::MarketplaceSkillByName, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/contracts",
        Method::GET,
        get(crate::api::marketplace::list_contracts),
        spec(RouteId::MarketplaceContracts, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id",
        Method::GET,
        get(crate::api::marketplace::get_contract),
        spec(RouteId::MarketplaceContractById, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/wallet",
        Method::GET,
        get(crate::api::marketplace::get_wallet),
        spec(RouteId::MarketplaceWallet, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/wallet/transactions",
        Method::GET,
        get(crate::api::marketplace::list_transactions),
        spec(RouteId::MarketplaceWalletTransactions, Method::GET),
    ))
    .merge(action_route(
        "/api/marketplace/reviews/:agent_id",
        Method::GET,
        get(crate::api::marketplace::list_reviews),
        spec(RouteId::MarketplaceReviewsByAgentId, Method::GET),
    ))
}

pub fn operator_routes() -> axum::Router<Arc<AppState>> {
    action_route(
        "/api/safety/status",
        Method::GET,
        get(crate::api::safety::safety_status),
        spec(RouteId::SafetyStatus, Method::GET),
    )
    .merge(action_route(
        "/api/agents",
        Method::POST,
        post(crate::api::agents::create_agent),
        spec(RouteId::Agents, Method::POST),
    ))
    .merge(action_route(
        "/api/agents/:id",
        Method::DELETE,
        delete(crate::api::agents::delete_agent),
        spec(RouteId::AgentById, Method::DELETE),
    ))
    .merge(action_route(
        "/api/goals/:id/approve",
        Method::POST,
        post(crate::api::goals::approve_goal),
        spec(RouteId::GoalApproveById, Method::POST),
    ))
    .merge(action_route(
        "/api/goals/:id/reject",
        Method::POST,
        post(crate::api::goals::reject_goal),
        spec(RouteId::GoalRejectById, Method::POST),
    ))
    .merge(action_route(
        "/api/memory",
        Method::POST,
        post(crate::api::memory::write_memory),
        spec(RouteId::Memory, Method::POST),
    ))
    .merge(action_route(
        "/api/memory/:id/archive",
        Method::POST,
        post(crate::api::memory::archive_memory),
        spec(RouteId::MemoryArchiveById, Method::POST),
    ))
    .merge(action_route(
        "/api/memory/:id/unarchive",
        Method::POST,
        post(crate::api::memory::unarchive_memory),
        spec(RouteId::MemoryUnarchiveById, Method::POST),
    ))
    .merge(action_route(
        "/api/workflows",
        Method::POST,
        post(crate::api::workflows::create_workflow),
        spec(RouteId::Workflows, Method::POST),
    ))
    .merge(action_route(
        "/api/workflows/:id",
        Method::PUT,
        put(crate::api::workflows::update_workflow),
        spec(RouteId::WorkflowById, Method::PUT),
    ))
    .merge(action_route(
        "/api/workflows/:id/execute",
        Method::POST,
        post(crate::api::workflows::execute_workflow),
        spec(RouteId::WorkflowExecuteById, Method::POST),
    ))
    .merge(action_route(
        "/api/workflows/:id/resume/:execution_id",
        Method::POST,
        post(crate::api::workflows::resume_execution),
        spec(RouteId::WorkflowResumeExecutionById, Method::POST),
    ))
    .merge(action_route(
        "/api/sessions/:id/bookmarks",
        Method::POST,
        post(crate::api::sessions::create_bookmark),
        spec(RouteId::SessionBookmarksById, Method::POST),
    ))
    .merge(action_route(
        "/api/sessions/:id/bookmarks/:bookmark_id",
        Method::DELETE,
        delete(crate::api::sessions::delete_bookmark),
        spec(RouteId::SessionBookmarkById, Method::DELETE),
    ))
    .merge(action_route(
        "/api/sessions/:id/branch",
        Method::POST,
        post(crate::api::sessions::branch_session),
        spec(RouteId::SessionBranchById, Method::POST),
    ))
    .merge(action_route(
        "/api/sessions/:id/heartbeat",
        Method::POST,
        post(crate::api::sessions::session_heartbeat),
        spec(RouteId::SessionHeartbeatById, Method::POST),
    ))
    .merge(action_route(
        "/api/studio/run",
        Method::POST,
        post(crate::api::studio::run_prompt),
        spec(RouteId::StudioRun, Method::POST),
    ))
    .merge(action_route(
        "/api/studio/sessions",
        Method::POST,
        post(crate::api::studio_sessions::create_session),
        spec(RouteId::StudioSessions, Method::POST),
    ))
    .merge(action_route(
        "/api/studio/sessions/:id",
        Method::DELETE,
        delete(crate::api::studio_sessions::delete_session),
        spec(RouteId::StudioSessionById, Method::DELETE),
    ))
    .merge(action_route(
        "/api/studio/sessions/:id/messages",
        Method::POST,
        post(crate::api::studio_sessions::send_message),
        spec(RouteId::StudioSessionMessagesById, Method::POST),
    ))
    .merge(action_route(
        "/api/studio/sessions/:id/messages/stream",
        Method::POST,
        post(crate::api::studio_sessions::send_message_stream),
        spec(RouteId::StudioSessionMessageStreamById, Method::POST),
    ))
    .merge(action_route(
        "/api/agent/chat",
        Method::POST,
        post(crate::api::agent_chat::agent_chat),
        spec(RouteId::AgentChat, Method::POST),
    ))
    .merge(action_route(
        "/api/agent/chat/stream",
        Method::POST,
        post(crate::api::agent_chat::agent_chat_stream),
        spec(RouteId::AgentChatStream, Method::POST),
    ))
    .merge(action_route(
        "/api/profiles",
        Method::POST,
        post(crate::api::profiles::create_profile),
        spec(RouteId::Profiles, Method::POST),
    ))
    .merge(action_route(
        "/api/profiles/:name",
        Method::PUT,
        put(crate::api::profiles::update_profile),
        spec(RouteId::ProfileByName, Method::PUT),
    ))
    .merge(action_route(
        "/api/profiles/:name",
        Method::DELETE,
        delete(crate::api::profiles::delete_profile),
        spec(RouteId::ProfileByName, Method::DELETE),
    ))
    .merge(action_route(
        "/api/agents/:id/profile",
        Method::POST,
        post(crate::api::profiles::assign_profile),
        spec(RouteId::AgentProfileById, Method::POST),
    ))
    .merge(action_route(
        "/api/skills/:id/install",
        Method::POST,
        post(crate::api::skills::install_skill),
        spec(RouteId::SkillInstallById, Method::POST),
    ))
    .merge(action_route(
        "/api/skills/:id/uninstall",
        Method::POST,
        post(crate::api::skills::uninstall_skill),
        spec(RouteId::SkillUninstallById, Method::POST),
    ))
    .merge(action_route(
        "/api/skills/:id/quarantine",
        Method::POST,
        post(crate::api::skills::quarantine_skill),
        spec(RouteId::SkillQuarantineById, Method::POST),
    ))
    .merge(action_route(
        "/api/skills/:id/quarantine/resolve",
        Method::POST,
        post(crate::api::skills::resolve_skill_quarantine),
        spec(RouteId::SkillQuarantineResolveById, Method::POST),
    ))
    .merge(action_route(
        "/api/skills/:id/reverify",
        Method::POST,
        post(crate::api::skills::reverify_skill),
        spec(RouteId::SkillReverifyById, Method::POST),
    ))
    .merge(action_route(
        "/api/skills/:name/execute",
        Method::POST,
        post(crate::api::skill_execute::execute_skill),
        spec(RouteId::SkillExecuteByName, Method::POST),
    ))
    .merge(action_route(
        "/api/channels",
        Method::POST,
        post(crate::api::channels::create_channel),
        spec(RouteId::Channels, Method::POST),
    ))
    .merge(action_route(
        "/api/channels/:id/reconnect",
        Method::POST,
        post(crate::api::channels::reconnect_channel),
        spec(RouteId::ChannelReconnectById, Method::POST),
    ))
    .merge(action_route(
        "/api/channels/:id",
        Method::DELETE,
        delete(crate::api::channels::delete_channel),
        spec(RouteId::ChannelById, Method::DELETE),
    ))
    .merge(action_route(
        "/api/channels/:type/inject",
        Method::POST,
        post(crate::api::channels::inject_message),
        spec(RouteId::ChannelInjectByType, Method::POST),
    ))
    .merge(action_route(
        "/api/a2a/tasks",
        Method::POST,
        post(crate::api::a2a::send_task),
        spec(RouteId::A2aTasks, Method::POST),
    ))
    .merge(action_route(
        "/api/oauth/connect",
        Method::POST,
        post(crate::api::oauth_routes::connect),
        spec(RouteId::OAuthConnect, Method::POST),
    ))
    .merge(action_route(
        "/api/oauth/connections/:ref_id",
        Method::DELETE,
        delete(crate::api::oauth_routes::disconnect),
        spec(RouteId::OAuthConnectionByRefId, Method::DELETE),
    ))
    .merge(action_route(
        "/api/oauth/execute",
        Method::POST,
        post(crate::api::oauth_routes::execute_api_call),
        spec(RouteId::OAuthExecute, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/agents",
        Method::POST,
        post(crate::api::marketplace::register_agent),
        spec(RouteId::MarketplaceAgents, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/agents/:id",
        Method::DELETE,
        delete(crate::api::marketplace::delist_agent),
        spec(RouteId::MarketplaceAgentById, Method::DELETE),
    ))
    .merge(action_route(
        "/api/marketplace/agents/:id/status",
        Method::PUT,
        put(crate::api::marketplace::update_agent_status),
        spec(RouteId::MarketplaceAgentStatusById, Method::PUT),
    ))
    .merge(action_route(
        "/api/marketplace/skills",
        Method::POST,
        post(crate::api::marketplace::publish_skill),
        spec(RouteId::MarketplaceSkills, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts",
        Method::POST,
        post(crate::api::marketplace::propose_contract),
        spec(RouteId::MarketplaceContracts, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id/accept",
        Method::POST,
        post(crate::api::marketplace::accept_contract),
        spec(RouteId::MarketplaceContractAcceptById, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id/reject",
        Method::POST,
        post(crate::api::marketplace::reject_contract),
        spec(RouteId::MarketplaceContractRejectById, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id/start",
        Method::POST,
        post(crate::api::marketplace::start_contract),
        spec(RouteId::MarketplaceContractStartById, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id/complete",
        Method::POST,
        post(crate::api::marketplace::complete_contract),
        spec(RouteId::MarketplaceContractCompleteById, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id/dispute",
        Method::POST,
        post(crate::api::marketplace::dispute_contract),
        spec(RouteId::MarketplaceContractDisputeById, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id/cancel",
        Method::POST,
        post(crate::api::marketplace::cancel_contract),
        spec(RouteId::MarketplaceContractCancelById, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/contracts/:id/resolve",
        Method::POST,
        post(crate::api::marketplace::resolve_contract),
        spec(RouteId::MarketplaceContractResolveById, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/wallet/seed",
        Method::POST,
        post(crate::api::marketplace::seed_wallet),
        spec(RouteId::MarketplaceWalletSeed, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/reviews",
        Method::POST,
        post(crate::api::marketplace::submit_review),
        spec(RouteId::MarketplaceReviews, Method::POST),
    ))
    .merge(action_route(
        "/api/marketplace/discover",
        Method::POST,
        post(crate::api::marketplace::discover_agents),
        spec(RouteId::MarketplaceDiscover, Method::POST),
    ))
}

pub fn admin_routes() -> axum::Router<Arc<AppState>> {
    action_route(
        "/api/admin/backups",
        Method::GET,
        get(crate::api::admin::list_backups),
        spec(RouteId::AdminBackupList, Method::GET),
    )
    .merge(action_route(
        "/api/admin/export",
        Method::GET,
        get(crate::api::admin::export_data),
        spec(RouteId::AdminExport, Method::GET),
    ))
    .merge(action_route(
        "/api/admin/provider-keys",
        Method::GET,
        get(crate::api::provider_keys::list_provider_keys),
        spec(RouteId::ProviderKeys, Method::GET),
    ))
    .merge(action_route(
        "/api/pc-control/status",
        Method::GET,
        get(crate::api::pc_control::get_status),
        spec(RouteId::PcControlStatus, Method::GET),
    ))
    .merge(action_route(
        "/api/pc-control/status",
        Method::PUT,
        put(crate::api::pc_control::update_status),
        spec(RouteId::PcControlStatus, Method::PUT),
    ))
    .merge(action_route(
        "/api/pc-control/actions",
        Method::GET,
        get(crate::api::pc_control::list_actions),
        spec(RouteId::PcControlActions, Method::GET),
    ))
    .merge(action_route(
        "/api/safety/pause/:agent_id",
        Method::POST,
        post(crate::api::safety::pause_agent),
        spec(RouteId::SafetyPauseAgent, Method::POST),
    ))
    .merge(action_route(
        "/api/safety/resume/:agent_id",
        Method::POST,
        post(crate::api::safety::resume_agent),
        spec(RouteId::SafetyResumeAgent, Method::POST),
    ))
    .merge(action_route(
        "/api/safety/quarantine/:agent_id",
        Method::POST,
        post(crate::api::safety::quarantine_agent),
        spec(RouteId::SafetyQuarantineAgent, Method::POST),
    ))
    .merge(action_route(
        "/api/safety/checks",
        Method::GET,
        get(crate::api::safety_checks::list_safety_checks),
        spec(RouteId::SafetyChecks, Method::GET),
    ))
    .merge(action_route(
        "/api/safety/checks",
        Method::POST,
        post(crate::api::safety_checks::register_safety_check),
        spec(RouteId::SafetyChecks, Method::POST),
    ))
    .merge(action_route(
        "/api/safety/checks/:id",
        Method::DELETE,
        delete(crate::api::safety_checks::unregister_safety_check),
        spec(RouteId::SafetyCheckById, Method::DELETE),
    ))
    .merge(action_route(
        "/api/webhooks",
        Method::GET,
        get(crate::api::webhooks::list_webhooks),
        spec(RouteId::Webhooks, Method::GET),
    ))
    .merge(action_route(
        "/api/webhooks",
        Method::POST,
        post(crate::api::webhooks::create_webhook),
        spec(RouteId::Webhooks, Method::POST),
    ))
    .merge(action_route(
        "/api/webhooks/:id",
        Method::PUT,
        put(crate::api::webhooks::update_webhook),
        spec(RouteId::WebhookById, Method::PUT),
    ))
    .merge(action_route(
        "/api/webhooks/:id",
        Method::DELETE,
        delete(crate::api::webhooks::delete_webhook),
        spec(RouteId::WebhookById, Method::DELETE),
    ))
    .merge(action_route(
        "/api/webhooks/:id/test",
        Method::POST,
        post(crate::api::webhooks::test_webhook),
        spec(RouteId::WebhookTestById, Method::POST),
    ))
    .merge(action_route(
        "/api/admin/backup",
        Method::POST,
        post(crate::api::admin::create_backup),
        spec(RouteId::AdminBackupCreate, Method::POST),
    ))
    .merge(action_route(
        "/api/admin/provider-keys",
        Method::PUT,
        put(crate::api::provider_keys::set_provider_key),
        spec(RouteId::ProviderKeys, Method::PUT),
    ))
    .merge(action_route(
        "/api/admin/provider-keys/:env_name",
        Method::DELETE,
        delete(crate::api::provider_keys::delete_provider_key),
        spec(RouteId::ProviderKeyByEnvName, Method::DELETE),
    ))
    .merge(action_route(
        "/api/pc-control/allowed-apps",
        Method::PUT,
        put(crate::api::pc_control::update_allowed_apps),
        spec(RouteId::PcControlAllowedApps, Method::PUT),
    ))
    .merge(action_route(
        "/api/pc-control/blocked-hotkeys",
        Method::PUT,
        put(crate::api::pc_control::update_blocked_hotkeys),
        spec(RouteId::PcControlBlockedHotkeys, Method::PUT),
    ))
    .merge(action_route(
        "/api/pc-control/safe-zones",
        Method::PUT,
        put(crate::api::pc_control::update_safe_zones),
        spec(RouteId::PcControlSafeZones, Method::PUT),
    ))
}

pub fn superadmin_routes() -> axum::Router<Arc<AppState>> {
    action_route(
        "/api/admin/restore",
        Method::POST,
        post(crate::api::admin::restore_backup),
        spec(RouteId::AdminRestore, Method::POST),
    )
    .merge(action_route(
        "/api/safety/kill-all",
        Method::POST,
        post(crate::api::safety::kill_all),
        spec(RouteId::SafetyKillAll, Method::POST),
    ))
}
