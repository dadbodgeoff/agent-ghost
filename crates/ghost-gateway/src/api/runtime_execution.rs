use ghost_agent_loop::context::run_context::RunContext;
use ghost_agent_loop::output_inspector::{InspectionResult, OutputInspector};
use ghost_agent_loop::runner::{AgentRunner, RunError, RunResult};
use ghost_llm::provider::{ChatMessage, MessageRole};
use uuid::Uuid;

use chrono::SecondsFormat;
use cortex_core::memory::types::convergence::{
    AgentGoalContent, AgentReflectionContent, ReflectionTrigger,
};
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::BaseMemory;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HydrationRouteKind {
    AgentChat,
    Studio,
    Cli,
    Autonomy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrationRequest {
    pub agent_id: Uuid,
    pub session_id: Uuid,
    pub route_kind: HydrationRouteKind,
    pub user_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HydratedSpeculativeKind {
    Summary,
    FactCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydratedSpeculativeEntry {
    pub id: String,
    pub kind: HydratedSpeculativeKind,
    pub content: String,
    pub retrieval_weight: f64,
    pub created_at: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HydratedSnapshotInput {
    pub goals: Vec<AgentGoalContent>,
    pub reflections: Vec<AgentReflectionContent>,
    pub durable_memories: Vec<BaseMemory>,
    pub speculative_entries: Vec<HydratedSpeculativeEntry>,
    pub convergence: HydratedConvergenceState,
}

#[derive(Debug, Clone, Default)]
pub struct HydratedRuntimeContext {
    pub build_options: RunnerBuildOptions,
    pub snapshot_input: HydratedSnapshotInput,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HydratedConvergenceState {
    pub score: f64,
    pub level: u8,
}

pub trait RuntimeHydrator {
    fn hydrate_for_request(
        &self,
        request: &HydrationRequest,
    ) -> Result<HydratedRuntimeContext, ApiError>;
}

const HYDRATED_SPECULATIVE_LIMIT: u32 = 3;
const HYDRATED_DURABLE_MEMORY_LIMIT: u32 = 10;
const HYDRATED_HISTORY_EVENT_LIMIT: u32 = 64;
const HYDRATED_HISTORY_MESSAGE_LIMIT: usize = 12;
const HYDRATED_REFLECTION_LIMIT: usize = 3;

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
    let hydrated = hydrate_runtime_context_from_state(
        state,
        agent.id,
        session_id,
        HydrationRouteKind::AgentChat,
        options,
    );
    prepare_runtime_execution_for_agent_with_hydration(state, agent, session_id, hydrated)
}

pub fn prepare_requested_runtime_execution_with_hydration(
    state: &AppState,
    requested_agent_id: Option<&str>,
    synthetic_name: &str,
    session_id: Uuid,
    hydrated: HydratedRuntimeContext,
) -> Result<PreparedRuntimeExecution, ApiError> {
    let agent = RuntimeSafetyBuilder::new(state)
        .resolve_agent(requested_agent_id, synthetic_name)
        .map_err(map_runtime_safety_error)?;
    prepare_runtime_execution_for_agent_with_hydration(state, agent, session_id, hydrated)
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
    let hydrated = hydrate_runtime_context_from_state(
        state,
        agent.id,
        session_id,
        HydrationRouteKind::Studio,
        options,
    );
    prepare_runtime_execution_for_agent_with_hydration(state, agent, session_id, hydrated)
}

pub fn prepare_stored_runtime_execution_with_hydration(
    state: &AppState,
    stored_agent_id: &str,
    synthetic_name: &str,
    session_id: Uuid,
    hydrated: HydratedRuntimeContext,
) -> Result<PreparedRuntimeExecution, ApiError> {
    let agent = RuntimeSafetyBuilder::new(state)
        .resolve_stored_agent(stored_agent_id, synthetic_name)
        .map_err(map_runtime_safety_error)?;
    prepare_runtime_execution_for_agent_with_hydration(state, agent, session_id, hydrated)
}

pub fn prepare_runtime_execution_for_agent(
    state: &AppState,
    agent: ResolvedRuntimeAgent,
    session_id: Uuid,
    options: RunnerBuildOptions,
) -> Result<PreparedRuntimeExecution, ApiError> {
    prepare_runtime_execution_for_agent_with_hydration(
        state,
        agent,
        session_id,
        default_hydrated_runtime_context(options),
    )
}

pub fn prepare_runtime_execution_for_agent_with_hydration(
    state: &AppState,
    agent: ResolvedRuntimeAgent,
    session_id: Uuid,
    hydrated: HydratedRuntimeContext,
) -> Result<PreparedRuntimeExecution, ApiError> {
    let runtime_ctx = RuntimeSafetyContext::from_state(state, agent.clone(), session_id, None);
    runtime_ctx
        .ensure_execution_permitted()
        .map_err(map_runner_error)?;

    let mut runner = RuntimeSafetyBuilder::new(state)
        .build_live_runner(&runtime_ctx, hydrated.build_options)
        .map_err(map_runtime_safety_error)?;

    let snapshot_memories = hydrated_snapshot_memories_as_json(&hydrated.snapshot_input);
    if !snapshot_memories.is_empty() {
        runner
            .tool_executor
            .set_snapshot_memories(snapshot_memories);
    }

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

pub fn inspect_text_safety(text: &str, agent_id: Uuid) -> InspectionResult {
    OutputInspector::new().scan(text, agent_id)
}

pub fn inspection_safety_status(inspection: &InspectionResult) -> &'static str {
    match inspection {
        InspectionResult::Clean => "clean",
        InspectionResult::Warning { .. } => "warning",
        InspectionResult::KillAll { .. } => "blocked",
    }
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
        RunError::Cancelled => ApiError::custom(
            axum::http::StatusCode::CONFLICT,
            "EXECUTION_CANCELLED",
            "Execution cancelled by user",
        ),
        RunError::ToolLoopAborted { message, .. } => ApiError::custom(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "AGENT_TOOL_LOOP_ABORTED",
            message,
        ),
        other => ApiError::internal(format!("agent run failed: {other}")),
    }
}

fn hydrated_snapshot_memories_as_json(input: &HydratedSnapshotInput) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = input
        .goals
        .iter()
        .filter_map(|goal| {
            serde_json::to_value(goal).ok().map(|content| {
                serde_json::json!({
                    "kind": "agent_goal",
                    "content": content,
                    "hydrated": true,
                })
            })
        })
        .collect();

    out.extend(input.reflections.iter().filter_map(|reflection| {
        serde_json::to_value(reflection).ok().map(|content| {
            serde_json::json!({
                "kind": "agent_reflection",
                "content": content,
                "hydrated": true,
            })
        })
    }));

    if input.convergence.score > 0.0 || input.convergence.level > 0 {
        out.push(serde_json::json!({
            "kind": "convergence_state",
            "score": input.convergence.score,
            "level": input.convergence.level,
            "hydrated": true,
        }));
    }

    out.extend(
        input
            .durable_memories
            .iter()
            .filter_map(|memory| serde_json::to_value(memory).ok())
            .collect::<Vec<_>>(),
    );

    out.extend(input.speculative_entries.iter().map(|entry| {
        serde_json::json!({
            "id": entry.id,
            "kind": entry.kind,
            "content": entry.content,
            "retrieval_weight": entry.retrieval_weight,
            "created_at": entry.created_at,
            "source_refs": entry.source_refs,
            "speculative": true,
        })
    }));

    out
}

fn default_hydrated_runtime_context(options: RunnerBuildOptions) -> HydratedRuntimeContext {
    HydratedRuntimeContext {
        build_options: options,
        snapshot_input: HydratedSnapshotInput::default(),
    }
}

fn hydrate_runtime_context_from_state(
    state: &AppState,
    agent_id: Uuid,
    session_id: Uuid,
    route_kind: HydrationRouteKind,
    options: RunnerBuildOptions,
) -> HydratedRuntimeContext {
    let mut hydrated = default_hydrated_runtime_context(options);
    let session_id = session_id.to_string();
    let actor_id = agent_id.to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let db = match state.db.read() {
        Ok(db) => db,
        Err(error) => {
            tracing::warn!(error = %error, "runtime hydration: failed to acquire DB read handle");
            return hydrated;
        }
    };

    if hydrated.build_options.conversation_history.is_empty() {
        hydrated.build_options.conversation_history =
            hydrate_conversation_history_from_db(&db, &session_id, &route_kind);
    }

    match cortex_storage::queries::context_attempt_queries::list_retrievable_for_session(
        &db,
        &session_id,
        &now,
        HYDRATED_SPECULATIVE_LIMIT,
    ) {
        Ok(rows) => {
            hydrated.snapshot_input.speculative_entries = rows
                .into_iter()
                .map(|row| HydratedSpeculativeEntry {
                    id: row.id,
                    kind: match row.attempt_kind.as_str() {
                        "fact_candidate" => HydratedSpeculativeKind::FactCandidate,
                        _ => HydratedSpeculativeKind::Summary,
                    },
                    content: row
                        .redacted_content
                        .filter(|value| !value.is_empty())
                        .unwrap_or(row.content),
                    retrieval_weight: row.retrieval_weight,
                    created_at: row.created_at,
                    source_refs: serde_json::from_str(&row.source_refs).unwrap_or_default(),
                })
                .collect();
        }
        Err(error) => {
            tracing::warn!(error = %error, session_id = %session_id, "runtime hydration: speculative context lookup failed");
        }
    }

    if let Ok(Some(score)) =
        cortex_storage::queries::convergence_score_queries::latest_by_agent(&db, &actor_id)
    {
        hydrated.snapshot_input.convergence = HydratedConvergenceState {
            score: score.composite_score,
            level: score.level.clamp(0, u8::MAX as i32) as u8,
        };
    }

    match cortex_storage::queries::memory_snapshot_queries::latest_for_actor(
        &db,
        &actor_id,
        HYDRATED_DURABLE_MEMORY_LIMIT,
    ) {
        Ok(rows) => {
            hydrated.snapshot_input.durable_memories = rows
                .into_iter()
                .filter_map(|row| serde_json::from_str::<BaseMemory>(&row.snapshot).ok())
                .collect();
            hydrate_goal_and_reflection_layers_from_memories(&mut hydrated.snapshot_input);
        }
        Err(error) => {
            tracing::warn!(error = %error, actor_id = %actor_id, "runtime hydration: durable memory lookup failed");
        }
    }

    if hydrated.snapshot_input.reflections.is_empty() {
        match cortex_storage::queries::reflection_queries::query_by_session(&db, &session_id) {
            Ok(rows) => {
                hydrated.snapshot_input.reflections = rows
                    .into_iter()
                    .rev()
                    .take(HYDRATED_REFLECTION_LIMIT)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|row| AgentReflectionContent {
                        reflection_text: row.reflection_text,
                        trigger: reflection_trigger_from_str(&row.trigger_type),
                        depth: row.depth.clamp(0, u8::MAX as i32) as u8,
                        parent_reflection_id: None,
                    })
                    .collect();
            }
            Err(error) => {
                tracing::warn!(error = %error, session_id = %session_id, "runtime hydration: reflection lookup failed");
            }
        }
    }

    hydrated
}

fn hydrate_conversation_history_from_db(
    conn: &rusqlite::Connection,
    session_id: &str,
    route_kind: &HydrationRouteKind,
) -> Vec<ChatMessage> {
    match route_kind {
        HydrationRouteKind::Studio => {
            let studio_history = hydrate_studio_conversation_history(conn, session_id);
            if !studio_history.is_empty() {
                return studio_history;
            }
            hydrate_runtime_session_conversation_history(conn, session_id)
        }
        HydrationRouteKind::AgentChat | HydrationRouteKind::Cli | HydrationRouteKind::Autonomy => {
            let runtime_history = hydrate_runtime_session_conversation_history(conn, session_id);
            if !runtime_history.is_empty() {
                return runtime_history;
            }
            hydrate_studio_conversation_history(conn, session_id)
        }
    }
}

fn hydrate_studio_conversation_history(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Vec<ChatMessage> {
    match cortex_storage::queries::studio_chat_queries::list_messages(conn, session_id) {
        Ok(rows) => trim_conversation_history(
            rows.into_iter()
                .map(|msg| ChatMessage {
                    role: studio_role_to_message_role(&msg.role),
                    content: msg.content,
                    tool_calls: None,
                    tool_call_id: None,
                })
                .collect(),
        ),
        Err(error) => {
            tracing::warn!(error = %error, session_id = %session_id, "runtime hydration: studio history lookup failed");
            Vec::new()
        }
    }
}

fn hydrate_runtime_session_conversation_history(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Vec<ChatMessage> {
    match cortex_storage::queries::itp_event_queries::query_recent_hydration_events(
        conn,
        session_id,
        HYDRATED_HISTORY_EVENT_LIMIT,
    ) {
        Ok(rows) => trim_conversation_history(rebuild_runtime_conversation_history(&rows)),
        Err(error) => {
            tracing::warn!(error = %error, session_id = %session_id, "runtime hydration: runtime history lookup failed");
            Vec::new()
        }
    }
}

fn rebuild_runtime_conversation_history(
    rows: &[cortex_storage::queries::itp_event_queries::ITPHydrationEventRow],
) -> Vec<ChatMessage> {
    let mut history = Vec::new();
    let mut pending_stream_response = String::new();

    for row in rows {
        let attributes = serde_json::from_str::<serde_json::Value>(&row.attributes)
            .unwrap_or_else(|_| serde_json::json!({}));
        match row.event_type.as_str() {
            "stream_start" => {
                if !pending_stream_response.is_empty() {
                    push_history_message(
                        &mut history,
                        MessageRole::Assistant,
                        std::mem::take(&mut pending_stream_response),
                    );
                }

                let user_message = attributes
                    .get("message")
                    .and_then(|value| value.as_str())
                    .or_else(|| {
                        attributes
                            .get("user_message")
                            .and_then(|value| value.as_str())
                    });
                if let Some(user_message) = user_message {
                    push_history_message(&mut history, MessageRole::User, user_message.to_string());
                }
            }
            "text_chunk" => {
                if let Some(content) = attributes.get("content").and_then(|value| value.as_str()) {
                    pending_stream_response.push_str(content);
                }
            }
            "turn_complete" => {
                if let Some(user_message) =
                    attributes.get("message").and_then(|value| value.as_str())
                {
                    let last_matches = history.last().is_some_and(|message| {
                        message.role == MessageRole::User && message.content == user_message
                    });
                    if !last_matches {
                        push_history_message(
                            &mut history,
                            MessageRole::User,
                            user_message.to_string(),
                        );
                    }
                }

                let assistant_message = if !pending_stream_response.is_empty() {
                    Some(std::mem::take(&mut pending_stream_response))
                } else {
                    attributes
                        .get("content")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned)
                };

                if let Some(assistant_message) = assistant_message {
                    push_history_message(&mut history, MessageRole::Assistant, assistant_message);
                }
            }
            _ => {}
        }
    }

    if !pending_stream_response.is_empty() {
        push_history_message(
            &mut history,
            MessageRole::Assistant,
            pending_stream_response,
        );
    }

    history
}

fn trim_conversation_history(history: Vec<ChatMessage>) -> Vec<ChatMessage> {
    if history.len() <= HYDRATED_HISTORY_MESSAGE_LIMIT {
        history
    } else {
        history[history.len() - HYDRATED_HISTORY_MESSAGE_LIMIT..].to_vec()
    }
}

fn push_history_message(history: &mut Vec<ChatMessage>, role: MessageRole, content: String) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }
    history.push(ChatMessage {
        role,
        content: trimmed.to_string(),
        tool_calls: None,
        tool_call_id: None,
    });
}

fn studio_role_to_message_role(role: &str) -> MessageRole {
    match role {
        "assistant" => MessageRole::Assistant,
        "system" => MessageRole::System,
        _ => MessageRole::User,
    }
}

fn hydrate_goal_and_reflection_layers_from_memories(input: &mut HydratedSnapshotInput) {
    input.goals = input
        .durable_memories
        .iter()
        .filter(|memory| memory.memory_type == MemoryType::AgentGoal)
        .filter_map(|memory| {
            serde_json::from_value::<AgentGoalContent>(memory.content.clone()).ok()
        })
        .collect();

    if input.reflections.is_empty() {
        input.reflections = input
            .durable_memories
            .iter()
            .filter(|memory| memory.memory_type == MemoryType::AgentReflection)
            .filter_map(|memory| {
                serde_json::from_value::<AgentReflectionContent>(memory.content.clone()).ok()
            })
            .collect();
    }
}

fn reflection_trigger_from_str(trigger: &str) -> ReflectionTrigger {
    match trigger {
        "scheduled" => ReflectionTrigger::Scheduled,
        "session_end" => ReflectionTrigger::SessionEnd,
        "threshold_crossed" => ReflectionTrigger::ThresholdCrossed,
        _ => ReflectionTrigger::UserRequested,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_core::memory::types::MemoryType;
    use cortex_core::memory::Importance;

    #[test]
    fn inspect_text_safety_marks_credential_patterns_as_warning() {
        let inspection = inspect_text_safety(
            "credential sk-proj-1234567890abcdefghijklmn exposed",
            Uuid::nil(),
        );

        assert!(matches!(inspection, InspectionResult::Warning { .. }));
        assert_eq!(inspection_safety_status(&inspection), "warning");
    }

    #[test]
    fn inspection_safety_status_maps_clean_to_clean() {
        assert_eq!(inspection_safety_status(&InspectionResult::Clean), "clean");
    }

    #[test]
    fn hydrated_snapshot_memories_include_durable_and_speculative_entries() {
        let memory = BaseMemory {
            id: Uuid::nil(),
            memory_type: MemoryType::Semantic,
            content: serde_json::json!("durable"),
            summary: "durable".into(),
            importance: Importance::Normal,
            confidence: 1.0,
            created_at: chrono::Utc::now(),
            last_accessed: None,
            access_count: 0,
            tags: Vec::new(),
            archived: false,
        };

        let input = HydratedSnapshotInput {
            durable_memories: vec![memory],
            speculative_entries: vec![HydratedSpeculativeEntry {
                id: "attempt-1".into(),
                kind: HydratedSpeculativeKind::Summary,
                content: "speculative".into(),
                retrieval_weight: 0.4,
                created_at: "2026-03-10T10:00:00Z".into(),
                source_refs: vec!["msg-1".into()],
            }],
            ..Default::default()
        };

        let values = hydrated_snapshot_memories_as_json(&input);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["content"], "durable");
        assert_eq!(values[1]["speculative"], true);
        assert_eq!(values[1]["content"], "speculative");
    }

    #[test]
    fn rebuild_runtime_conversation_history_restores_blocking_turns() {
        let rows = vec![
            cortex_storage::queries::itp_event_queries::ITPHydrationEventRow {
                event_type: "turn_complete".into(),
                timestamp: "2026-03-10T10:00:00Z".into(),
                sequence_number: 1,
                attributes: serde_json::json!({
                    "message": "hello",
                    "content": "world",
                })
                .to_string(),
            },
        ];

        let history = rebuild_runtime_conversation_history(&rows);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, MessageRole::User);
        assert_eq!(history[0].content, "hello");
        assert_eq!(history[1].role, MessageRole::Assistant);
        assert_eq!(history[1].content, "world");
    }

    #[test]
    fn rebuild_runtime_conversation_history_restores_stream_turns() {
        let rows = vec![
            cortex_storage::queries::itp_event_queries::ITPHydrationEventRow {
                event_type: "stream_start".into(),
                timestamp: "2026-03-10T10:00:00Z".into(),
                sequence_number: 1,
                attributes: serde_json::json!({
                    "message": "stream this",
                    "session_id": "sess-1",
                })
                .to_string(),
            },
            cortex_storage::queries::itp_event_queries::ITPHydrationEventRow {
                event_type: "text_chunk".into(),
                timestamp: "2026-03-10T10:00:01Z".into(),
                sequence_number: 2,
                attributes: serde_json::json!({
                    "content": "partial ",
                })
                .to_string(),
            },
            cortex_storage::queries::itp_event_queries::ITPHydrationEventRow {
                event_type: "text_chunk".into(),
                timestamp: "2026-03-10T10:00:02Z".into(),
                sequence_number: 3,
                attributes: serde_json::json!({
                    "content": "response",
                })
                .to_string(),
            },
            cortex_storage::queries::itp_event_queries::ITPHydrationEventRow {
                event_type: "turn_complete".into(),
                timestamp: "2026-03-10T10:00:03Z".into(),
                sequence_number: 4,
                attributes: serde_json::json!({
                    "token_count": 42,
                })
                .to_string(),
            },
        ];

        let history = rebuild_runtime_conversation_history(&rows);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, MessageRole::User);
        assert_eq!(history[0].content, "stream this");
        assert_eq!(history[1].role, MessageRole::Assistant);
        assert_eq!(history[1].content, "partial response");
    }
}
