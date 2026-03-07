use std::time::Duration;

use ghost_agent_loop::tools::executor::{register_builtin_tools, ToolError, ToolExecutor};
use ghost_agent_loop::tools::registry::{RegisteredTool, ToolRegistry};
use ghost_agent_loop::tools::skill_bridge::ExecutionContext;
use ghost_llm::provider::{LLMToolCall, ToolSchema};
use ghost_policy::engine::{CorpPolicy, PolicyEngine};
use uuid::Uuid;

fn exec_ctx(intervention_level: u8) -> ExecutionContext {
    ExecutionContext {
        agent_id: Uuid::now_v7(),
        session_id: Uuid::now_v7(),
        intervention_level,
        session_duration: Duration::from_secs(60),
        session_reflection_count: 0,
        is_compaction_flush: false,
    }
}

fn tool_call(name: &str, arguments: serde_json::Value) -> LLMToolCall {
    LLMToolCall {
        id: format!("call-{name}"),
        name: name.into(),
        arguments,
    }
}

fn registered_tool(name: &str, capability: &str) -> RegisteredTool {
    RegisteredTool {
        name: name.into(),
        description: format!("{name} test tool"),
        schema: ToolSchema {
            name: name.into(),
            description: format!("{name} schema"),
            parameters: serde_json::json!({"type": "object"}),
        },
        capability: capability.into(),
        hidden_at_level: 5,
        timeout_secs: 1,
    }
}

#[tokio::test]
async fn policy_denied_tool_never_reaches_dispatch() {
    let workspace = tempfile::tempdir().unwrap();
    let mut executor = ToolExecutor::default();
    executor.set_workspace_root(workspace.path().to_path_buf());
    executor.set_policy_engine(PolicyEngine::new(CorpPolicy::new()));

    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry);

    let call = tool_call(
        "write_file",
        serde_json::json!({"path":"blocked.txt","content":"should-not-write"}),
    );
    let result = executor.execute(&call, &registry, &exec_ctx(0)).await;

    assert!(matches!(result, Err(ToolError::PolicyDenied(_))));
    assert!(!workspace.path().join("blocked.txt").exists());
}

#[tokio::test]
async fn capability_grant_required_for_tool_execution() {
    let mut executor = ToolExecutor::default();
    executor.set_policy_engine(PolicyEngine::new(CorpPolicy::new()));

    let mut registry = ToolRegistry::new();
    registry.register(registered_tool("send_proactive_message", "messaging"));

    let result = executor
        .execute(
            &tool_call("send_proactive_message", serde_json::json!({})),
            &registry,
            &exec_ctx(0),
        )
        .await;

    assert!(matches!(result, Err(ToolError::PolicyDenied(_))));
}

#[tokio::test]
async fn convergence_policy_tightening_blocks_expected_tools() {
    let ctx = exec_ctx(2);
    let mut executor = ToolExecutor::default();
    let mut policy = PolicyEngine::new(CorpPolicy::new());
    policy.grant_capability(ctx.agent_id, "messaging".into());
    executor.set_policy_engine(policy);

    let mut registry = ToolRegistry::new();
    registry.register(registered_tool("send_proactive_message", "messaging"));

    let result = executor
        .execute(
            &tool_call("send_proactive_message", serde_json::json!({})),
            &registry,
            &ctx,
        )
        .await;

    assert!(matches!(result, Err(ToolError::PolicyDenied(_))));
}

#[tokio::test]
async fn new_tool_without_policy_mapping_fails_closed() {
    let ctx = exec_ctx(0);
    let mut executor = ToolExecutor::default();
    let mut policy = PolicyEngine::new(CorpPolicy::new());
    policy.grant_capability(ctx.agent_id, "future_capability".into());
    executor.set_policy_engine(policy);

    let mut registry = ToolRegistry::new();
    registry.register(registered_tool("new_future_tool", "future_capability"));

    let result = executor
        .execute(
            &tool_call("new_future_tool", serde_json::json!({})),
            &registry,
            &ctx,
        )
        .await;

    assert!(matches!(result, Err(ToolError::PolicyDenied(_))));
}
