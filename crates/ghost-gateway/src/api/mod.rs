//! REST API and WebSocket server (Req 25).

use ghost_agent_loop::tools::builtin::http_request::HttpRequestConfig;
use ghost_agent_loop::tools::builtin::shell::ShellToolConfig;
use ghost_agent_loop::tools::builtin::web_fetch::FetchConfig;
use ghost_agent_loop::tools::builtin::web_search::{SearchBackend, WebSearchConfig};

/// Apply tool configurations from ghost.yml to an AgentRunner's ToolExecutor.
pub fn apply_tool_configs(
    executor: &mut ghost_agent_loop::tools::executor::ToolExecutor,
    tools: &crate::config::ToolsConfig,
) {
    // Web search
    let backend = match tools.web_search.backend.to_lowercase().as_str() {
        "tavily" => SearchBackend::Tavily,
        "brave" => SearchBackend::Brave,
        _ => SearchBackend::SearXNG,
    };
    executor.set_web_search_config(WebSearchConfig {
        backend,
        searxng_url: tools.web_search.searxng_url.clone(),
        tavily_api_key: tools.web_search.tavily_api_key.clone(),
        brave_api_key: tools.web_search.brave_api_key.clone(),
        max_results: tools.web_search.max_results,
        ..Default::default()
    });

    // Web fetch
    executor.set_fetch_config(FetchConfig {
        allow_http: tools.web_fetch.allow_http,
        max_body_bytes: tools.web_fetch.max_body_bytes,
        timeout_secs: tools.web_fetch.timeout_secs,
        ..Default::default()
    });

    // HTTP request
    executor.set_http_request_config(HttpRequestConfig {
        allow_http: tools.http_request.allow_http,
        allowed_domains: tools.http_request.allowed_domains.clone(),
        ..Default::default()
    });

    // Shell
    executor.set_shell_config(ShellToolConfig {
        allowed_prefixes: tools.shell.allowed_prefixes.clone(),
        timeout: std::time::Duration::from_secs(tools.shell.timeout_secs),
        ..Default::default()
    });
}

pub mod a2a;
pub mod admin;
pub mod agent_chat;
pub mod agents;
pub mod audit;
pub mod auth;
pub mod authz;
pub mod authz_policy;
pub mod autonomy;
pub mod channels;
pub mod codex_auth;
pub mod compatibility;
pub mod convergence;
pub mod costs;
pub mod error;
pub mod goals;
pub mod health;
pub mod idempotency;
pub mod integrity;
pub mod itp;
pub mod kill_fanout;
pub mod live_executions;
pub mod marketplace;
pub mod memory;
pub mod mesh_routes;
pub mod mesh_viz;
pub mod mutation;
pub mod oauth_routes;
pub mod observability;
pub mod openapi;
pub mod operation_context;
pub mod pc_control;
pub mod profiles;
pub mod provider_keys;
pub mod push_routes;
pub mod rate_limit;
pub mod rbac;
pub mod runtime_execution;
pub mod safety;
pub mod safety_checks;
pub mod sandbox_reviews;
pub mod search;
pub mod sessions;
pub mod skill_execute;
pub mod skills;
pub mod ssrf;
pub mod state;
pub mod stream_errors;
pub mod stream_runtime;
pub mod studio;
pub mod studio_sessions;
pub mod traces;
pub mod webhooks;
pub mod websocket;
pub mod workflows;
