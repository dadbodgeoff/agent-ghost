use std::time::Duration;

use ghost_agent_loop::runner::{AgentRunner, AgentStreamEvent, RunError, RunResult};

use crate::api::stream_errors::StreamFailure;
use crate::config::ProviderConfig;
use crate::provider_runtime;
use crate::runtime_safety::RuntimeSafetyContext;

const STREAMING_TURN_TIMEOUT: Duration = Duration::from_secs(300);

pub async fn execute_streaming_turn(
    tx: &tokio::sync::mpsc::Sender<AgentStreamEvent>,
    runner: &mut AgentRunner,
    runtime_ctx: &RuntimeSafetyContext,
    route: &'static str,
    user_message: &str,
    providers: &[ProviderConfig],
    no_providers_message: &str,
    timeout_message: &str,
) -> Option<RunResult> {
    if providers.is_empty() {
        let _ = tx
            .send(AgentStreamEvent::terminal_error(no_providers_message))
            .await;
        return None;
    }

    let turn_result = tokio::time::timeout(STREAMING_TURN_TIMEOUT, async {
        let mut ctx = match runner
            .pre_loop(
                runtime_ctx.agent.id,
                runtime_ctx.session_id,
                route,
                user_message,
            )
            .await
        {
            Ok(ctx) => ctx,
            Err(error) => {
                let _ = tx
                    .send(AgentStreamEvent::terminal_error(format!(
                        "agent pre-loop failed: {error}"
                    )))
                    .await;
                return None;
            }
        };

        let mut result = Err(RunError::llm_error("no providers configured"));
        let mut last_failed_provider: Option<String> = None;

        for (provider_idx, provider_config) in providers.iter().enumerate() {
            let provider = provider_config.clone();
            let get_stream = move |messages: Vec<ghost_llm::provider::ChatMessage>,
                                   tools: Vec<ghost_llm::provider::ToolSchema>|
                  -> ghost_llm::streaming::StreamChunkStream {
                provider_runtime::build_provider_stream(&provider, messages, tools)
            };

            tracing::info!(
                route,
                provider = %provider_config.name,
                index = provider_idx,
                "attempting streaming with provider"
            );

            match runner
                .run_turn_streaming(&mut ctx, user_message, tx.clone(), get_stream)
                .await
            {
                Ok(run_result) => {
                    if provider_idx > 0 {
                        tracing::info!(
                            route,
                            provider = %provider_config.name,
                            index = provider_idx,
                            "streaming succeeded via fallback provider"
                        );
                    }
                    result = Ok(run_result);
                    break;
                }
                Err(error) => {
                    let failure = StreamFailure::from_run_error(&error);
                    let has_fallback = provider_idx + 1 < providers.len();
                    let can_fallback = failure.can_fallback(has_fallback);
                    tracing::warn!(
                        route,
                        provider = %provider_config.name,
                        index = provider_idx,
                        error = %failure.message,
                        partial_output = failure.partial_output,
                        can_fallback,
                        "streaming provider failed"
                    );
                    last_failed_provider = Some(provider_config.name.clone());
                    result = Err(error);
                    if can_fallback {
                        let _ = tx
                            .send(failure.as_stream_error(
                                Some(provider_config.name.clone()),
                                true,
                                false,
                            ))
                            .await;
                        ctx.recursion_depth = 0;
                        continue;
                    }
                    ctx.recursion_depth = 0;
                    break;
                }
            }
        }

        match result {
            Ok(run_result) => Some(run_result),
            Err(error) => {
                let failure = StreamFailure::from_run_error(&error);
                let _ = tx
                    .send(failure.as_stream_error(last_failed_provider, false, true))
                    .await;
                None
            }
        }
    })
    .await;

    match turn_result {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!(route, "agent turn timed out after 5 minutes");
            let _ = tx
                .send(AgentStreamEvent::terminal_error(timeout_message))
                .await;
            None
        }
    }
}
