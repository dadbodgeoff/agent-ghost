use ghost_agent_loop::context::run_context::RunContext;
use ghost_agent_loop::runner::{AgentRunner, RunError, RunResult};
use uuid::Uuid;

use crate::api::error::ApiError;
use crate::config::ProviderConfig;
use crate::provider_runtime;
use crate::runtime_safety::{
    ResolvedRuntimeAgent, RunnerBuildOptions, RuntimeSafetyBuilder, RuntimeSafetyContext,
    RuntimeSafetyError,
};
use crate::state::AppState;

pub struct PreparedRuntimeExecution {
    pub agent_id: String,
    pub runtime_ctx: RuntimeSafetyContext,
    pub runner: AgentRunner,
    pub providers: Vec<ProviderConfig>,
}

pub fn prepare_requested_runtime_execution(
    state: &AppState,
    requested_agent_id: Option<&str>,
    synthetic_name: &str,
    session_id: Uuid,
    options: RunnerBuildOptions,
) -> Result<PreparedRuntimeExecution, ApiError> {
    let agent = RuntimeSafetyBuilder::new(state)
        .resolve_agent(requested_agent_id, synthetic_name)
        .map_err(map_runtime_safety_error)?;
    prepare_runtime_execution_for_agent(state, agent, session_id, options)
}

pub fn prepare_stored_runtime_execution(
    state: &AppState,
    stored_agent_id: &str,
    synthetic_name: &str,
    session_id: Uuid,
    options: RunnerBuildOptions,
) -> Result<PreparedRuntimeExecution, ApiError> {
    let agent = RuntimeSafetyBuilder::new(state)
        .resolve_stored_agent(stored_agent_id, synthetic_name)
        .map_err(map_runtime_safety_error)?;
    prepare_runtime_execution_for_agent(state, agent, session_id, options)
}

pub fn prepare_runtime_execution_for_agent(
    state: &AppState,
    agent: ResolvedRuntimeAgent,
    session_id: Uuid,
    options: RunnerBuildOptions,
) -> Result<PreparedRuntimeExecution, ApiError> {
    let runtime_ctx = RuntimeSafetyContext::from_state(state, agent.clone(), session_id, None);
    runtime_ctx
        .ensure_execution_permitted()
        .map_err(map_runner_error)?;

    let runner = RuntimeSafetyBuilder::new(state)
        .build_live_runner(&runtime_ctx, options)
        .map_err(map_runtime_safety_error)?;
    let providers = provider_runtime::ordered_provider_configs(state);

    Ok(PreparedRuntimeExecution {
        agent_id: agent.id.to_string(),
        runtime_ctx,
        runner,
        providers,
    })
}

pub async fn pre_loop_blocking_turn(
    runner: &mut AgentRunner,
    runtime_ctx: &RuntimeSafetyContext,
    route: &'static str,
    user_message: &str,
) -> Result<RunContext, RunError> {
    runner
        .pre_loop(
            runtime_ctx.agent.id,
            runtime_ctx.session_id,
            route,
            user_message,
        )
        .await
}

pub async fn execute_blocking_turn(
    runner: &mut AgentRunner,
    ctx: &mut RunContext,
    user_message: &str,
    providers: &[ProviderConfig],
) -> Result<RunResult, RunError> {
    let mut fallback_chain = provider_runtime::build_fallback_chain(providers);
    runner
        .run_turn(ctx, &mut fallback_chain, user_message)
        .await
}

pub fn map_runtime_safety_error(error: RuntimeSafetyError) -> ApiError {
    match error {
        RuntimeSafetyError::AgentNotFound(message) => ApiError::bad_request(message),
        other => ApiError::internal(other.to_string()),
    }
}

pub fn map_runner_error(error: RunError) -> ApiError {
    match error {
        RunError::AgentPaused => ApiError::custom(
            axum::http::StatusCode::LOCKED,
            "AGENT_PAUSED",
            "Agent is paused",
        ),
        RunError::AgentQuarantined => ApiError::custom(
            axum::http::StatusCode::LOCKED,
            "AGENT_QUARANTINED",
            "Agent is quarantined",
        ),
        RunError::PlatformKilled => ApiError::KillSwitchActive,
        RunError::KillGateClosed => ApiError::custom(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "DISTRIBUTED_KILL_GATE_CLOSED",
            "Distributed kill gate is closed",
        ),
        RunError::ConvergenceProtectionDegraded(status) => ApiError::custom(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "CONVERGENCE_PROTECTION_DEGRADED",
            format!("Convergence protection is {status}"),
        ),
        other => ApiError::internal(format!("agent run failed: {other}")),
    }
}
