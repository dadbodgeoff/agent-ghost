//! REST API and WebSocket server (Req 25).

use ghost_agent_loop::tools::builtin::web_search::{WebSearchConfig, SearchBackend};
use ghost_agent_loop::tools::builtin::web_fetch::FetchConfig;
use ghost_agent_loop::tools::builtin::http_request::HttpRequestConfig;
use ghost_agent_loop::tools::builtin::shell::ShellToolConfig;

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
pub mod channels;
pub mod convergence;
pub mod costs;
pub mod error;
pub mod goals;
pub mod health;
pub mod integrity;
pub mod kill_fanout;
pub mod memory;
pub mod mesh_routes;
pub mod mesh_viz;
pub mod oauth_routes;
pub mod openapi;
pub mod profiles;
pub mod provider_keys;
pub mod push_routes;
pub mod rate_limit;
pub mod safety;
pub mod safety_checks;
pub mod ssrf;
pub mod search;
pub mod sessions;
pub mod skill_execute;
pub mod skills;
pub mod state;
pub mod studio;
pub mod studio_sessions;
pub mod traces;
pub mod webhooks;
pub mod websocket;
pub mod workflows;
