//! Workflow CRUD and execution endpoints (T-2.1.9).
//!
//! Manages saved workflow definitions (DAGs of agent, gate, and tool nodes).
//! Stored in the workflows table (v021_workflows migration).
//!
//! Ref: ADE_DESIGN_PLAN §17.11, tasks.md T-2.1.9

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation,
    execute_idempotent_json_mutation, prepare_json_operation, require_operation_context,
    start_operation_lease_heartbeat, PreparedOperation, PreparedOperationLease,
};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::provider_runtime;
use crate::runtime_safety::{RuntimeSafetyBuilder, RuntimeSafetyContext, API_SYNTHETIC_AGENT_NAME};
use crate::skill_catalog::SkillCatalogExecutor;
use crate::state::AppState;

const CREATE_WORKFLOW_ROUTE_TEMPLATE: &str = "/api/workflows";
const UPDATE_WORKFLOW_ROUTE_TEMPLATE: &str = "/api/workflows/:id";
const EXECUTE_WORKFLOW_ROUTE_TEMPLATE: &str = "/api/workflows/:id/execute";
const RESUME_WORKFLOW_ROUTE_TEMPLATE: &str = "/api/workflows/:id/resume/:execution_id";
const WORKFLOW_EXECUTION_STATE_VERSION: u32 = 2;
const SUPPORTED_WORKFLOW_NODE_TYPES: &[&str] = &[
    "llm_call",
    "tool_exec",
    "gate_check",
    "transform",
    "condition",
    "wait",
];

/// Query parameters for workflow listing.
#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkflowListParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request body for creating a workflow.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    pub nodes: Option<serde_json::Value>,
    pub edges: Option<serde_json::Value>,
}

/// Request body for updating a workflow.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub nodes: Option<serde_json::Value>,
    pub edges: Option<serde_json::Value>,
}

/// Request body for executing a workflow.
#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct ExecuteWorkflowRequest {
    /// Input payload for the first node.
    pub input: Option<serde_json::Value>,
}

/// Workflow response shape.
#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub nodes: serde_json::Value,
    pub edges: serde_json::Value,
    pub created_by: Option<String>,
    pub updated_at: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowListResponse {
    pub workflows: Vec<WorkflowResponse>,
    pub page: u32,
    pub page_size: u32,
    pub total: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowCreateResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowUpdateResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowExecutionSummary {
    pub execution_id: String,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub updated_at: String,
    pub state_version: i64,
    pub current_step_index: Option<i64>,
    pub current_node_id: Option<String>,
    pub recovery_action: Option<String>,
    pub recovery_required: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowExecutionListResponse {
    pub workflow_id: String,
    pub executions: Vec<WorkflowExecutionSummary>,
}

#[derive(Debug)]
struct StoredWorkflowRow {
    id: String,
    name: String,
    description: String,
    nodes_json: String,
    edges_json: String,
    created_by: Option<String>,
    updated_at: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowNodeState {
    node_id: String,
    node_type: String,
    status: String,
    result: serde_json::Value,
    started_at: Option<String>,
    completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowActiveStep {
    step_index: usize,
    node_id: String,
    node_type: String,
    started_at: String,
    retry_safe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowExecutionState {
    version: u32,
    execution_id: String,
    workflow_id: String,
    workflow_name: String,
    input: Option<serde_json::Value>,
    nodes: Vec<serde_json::Value>,
    edges: Vec<serde_json::Value>,
    order: Vec<String>,
    node_states: std::collections::BTreeMap<String, WorkflowNodeState>,
    node_outputs: std::collections::BTreeMap<String, serde_json::Value>,
    next_step_index: usize,
    active_step: Option<WorkflowActiveStep>,
    started_at: String,
    completed_at: Option<String>,
    final_status: Option<String>,
    final_response_status: Option<u16>,
    final_response_body: Option<serde_json::Value>,
    recovery_required: bool,
    recovery_action: Option<String>,
    recovery_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkflowExecutionBinding {
    execution_id: String,
    journal_id: String,
    operation_id: String,
    owner_token: String,
    lease_epoch: i64,
}

#[derive(Debug)]
struct WorkflowStepOutcome {
    status: String,
    result: serde_json::Value,
    output: Option<serde_json::Value>,
}

fn workflow_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

fn parse_workflow_graph_array(raw: &str, field: &str) -> Result<Vec<serde_json::Value>, ApiError> {
    serde_json::from_str(raw).map_err(|error| {
        ApiError::internal(format!("workflow {field} column is invalid JSON: {error}"))
    })
}

fn parse_workflow_graph_input(
    value: Option<&serde_json::Value>,
    field: &str,
) -> Result<Vec<serde_json::Value>, ApiError> {
    match value {
        None => Ok(Vec::new()),
        Some(serde_json::Value::Array(items)) => Ok(items.clone()),
        Some(_) => Err(ApiError::bad_request(format!(
            "workflow {field} must be an array"
        ))),
    }
}

fn validate_workflow_graph(
    nodes: &[serde_json::Value],
    edges: &[serde_json::Value],
) -> Result<(), ApiError> {
    use std::collections::HashSet;

    let mut node_ids = HashSet::new();
    for (index, node) in nodes.iter().enumerate() {
        let Some(node_obj) = node.as_object() else {
            return Err(ApiError::bad_request(format!(
                "workflow nodes[{index}] must be an object"
            )));
        };
        let Some(node_id) = node_obj.get("id").and_then(|value| value.as_str()) else {
            return Err(ApiError::bad_request(format!(
                "workflow nodes[{index}] missing string id"
            )));
        };
        if node_id.trim().is_empty() {
            return Err(ApiError::bad_request(format!(
                "workflow nodes[{index}] has empty id"
            )));
        }
        if !node_ids.insert(node_id.to_string()) {
            return Err(ApiError::bad_request(format!(
                "workflow contains duplicate node id '{node_id}'"
            )));
        }

        let Some(node_type) = node_obj.get("type").and_then(|value| value.as_str()) else {
            return Err(ApiError::bad_request(format!(
                "workflow node '{node_id}' missing string type"
            )));
        };
        if node_type.trim().is_empty() {
            return Err(ApiError::bad_request(format!(
                "workflow node '{node_id}' has empty type"
            )));
        }
    }

    for (index, edge) in edges.iter().enumerate() {
        let Some(edge_obj) = edge.as_object() else {
            return Err(ApiError::bad_request(format!(
                "workflow edges[{index}] must be an object"
            )));
        };
        let Some(source) = edge_obj.get("source").and_then(|value| value.as_str()) else {
            return Err(ApiError::bad_request(format!(
                "workflow edges[{index}] missing string source"
            )));
        };
        let Some(target) = edge_obj.get("target").and_then(|value| value.as_str()) else {
            return Err(ApiError::bad_request(format!(
                "workflow edges[{index}] missing string target"
            )));
        };
        if !node_ids.contains(source) {
            return Err(ApiError::bad_request(format!(
                "workflow edge source '{source}' does not reference a known node"
            )));
        }
        if !node_ids.contains(target) {
            return Err(ApiError::bad_request(format!(
                "workflow edge target '{target}' does not reference a known node"
            )));
        }
    }

    Ok(())
}

fn validate_workflow_supported_node_types(nodes: &[serde_json::Value]) -> Result<(), ApiError> {
    for node in nodes {
        let node_id = node
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("workflow node missing id after validation"))?;
        let node_type = node
            .get("type")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("workflow node missing type after validation"))?;
        if !SUPPORTED_WORKFLOW_NODE_TYPES.contains(&node_type) {
            return Err(ApiError::bad_request(format!(
                "workflow node '{node_id}' uses unsupported type '{node_type}'"
            )));
        }
    }
    Ok(())
}

fn workflow_node_config_value<'a>(
    node: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    node.get("config")
        .and_then(|value| value.as_object())
        .and_then(|config| config.get(key))
        .or_else(|| node.get(key))
}

fn workflow_node_config_string(node: &serde_json::Value, key: &str) -> Option<String> {
    workflow_node_config_value(node, key)
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn workflow_node_config_u64(node: &serde_json::Value, key: &str) -> Option<u64> {
    workflow_node_config_value(node, key).and_then(|value| value.as_u64())
}

fn stored_workflow_to_response(row: StoredWorkflowRow) -> Result<WorkflowResponse, ApiError> {
    let nodes = parse_workflow_graph_array(&row.nodes_json, "nodes")?;
    let edges = parse_workflow_graph_array(&row.edges_json, "edges")?;
    validate_workflow_graph(&nodes, &edges)?;

    Ok(WorkflowResponse {
        id: row.id,
        name: row.name,
        description: row.description,
        nodes: serde_json::Value::Array(nodes),
        edges: serde_json::Value::Array(edges),
        created_by: row.created_by,
        updated_at: row.updated_at,
        created_at: row.created_at,
    })
}

fn workflow_step_retry_safe(node_type: &str) -> bool {
    matches!(
        node_type,
        "condition" | "transform" | "gate" | "gate_check" | "wait"
    )
}

fn validate_workflow_execution_order(
    order: &[String],
    nodes: &[serde_json::Value],
    edges: &[serde_json::Value],
) -> Result<(), ApiError> {
    if order.len() != nodes.len() {
        return Err(ApiError::internal(
            "workflow execution order length does not match workflow nodes",
        ));
    }

    let mut positions = std::collections::HashMap::new();
    for (index, node_id) in order.iter().enumerate() {
        if positions.insert(node_id.as_str(), index).is_some() {
            return Err(ApiError::internal(format!(
                "workflow execution order contains duplicate node '{node_id}'"
            )));
        }
    }

    for node in nodes {
        let node_id = node
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("workflow execution snapshot missing node id"))?;
        if !positions.contains_key(node_id) {
            return Err(ApiError::internal(format!(
                "workflow execution order missing node '{node_id}'"
            )));
        }
    }

    for edge in edges {
        let source = edge
            .get("source")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("workflow execution snapshot edge missing source"))?;
        let target = edge
            .get("target")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("workflow execution snapshot edge missing target"))?;
        let source_index = positions.get(source).ok_or_else(|| {
            ApiError::internal(format!(
                "workflow execution order missing edge source node '{source}'"
            ))
        })?;
        let target_index = positions.get(target).ok_or_else(|| {
            ApiError::internal(format!(
                "workflow execution order missing edge target node '{target}'"
            ))
        })?;
        if source_index >= target_index {
            return Err(ApiError::internal(format!(
                "workflow execution order violates dependency {source} -> {target}"
            )));
        }
    }

    Ok(())
}

fn parse_workflow_execution_state(raw: &str) -> Result<WorkflowExecutionState, ApiError> {
    let state = serde_json::from_str::<WorkflowExecutionState>(raw).map_err(|error| {
        ApiError::internal(format!("failed to parse workflow execution state: {error}"))
    })?;
    if state.version != WORKFLOW_EXECUTION_STATE_VERSION {
        return Err(ApiError::custom(
            StatusCode::CONFLICT,
            "WORKFLOW_STATE_VERSION_UNSUPPORTED",
            format!(
                "workflow execution state version {} cannot be resumed by this binary",
                state.version
            ),
        ));
    }
    validate_workflow_graph(&state.nodes, &state.edges)?;
    validate_workflow_execution_order(&state.order, &state.nodes, &state.edges)?;

    if state.next_step_index > state.order.len() {
        return Err(ApiError::internal(format!(
            "workflow execution next_step_index {} is out of bounds",
            state.next_step_index
        )));
    }

    if let Some(active_step) = &state.active_step {
        let expected_node = state
            .order
            .get(active_step.step_index)
            .ok_or_else(|| ApiError::internal("workflow active_step points past the workflow"))?;
        if expected_node != &active_step.node_id {
            return Err(ApiError::internal(format!(
                "workflow active_step node '{}' does not match stored order '{}'",
                active_step.node_id, expected_node
            )));
        }
    }

    for (node_id, node_state) in &state.node_states {
        if !state.order.iter().any(|ordered| ordered == node_id) {
            return Err(ApiError::internal(format!(
                "workflow execution state contains unknown node state '{node_id}'"
            )));
        }
        if node_state.node_id != *node_id {
            return Err(ApiError::internal(format!(
                "workflow execution node state key '{node_id}' does not match payload '{}'",
                node_state.node_id
            )));
        }
    }

    Ok(state)
}

fn workflow_execution_current_step(state: &WorkflowExecutionState) -> (Option<i64>, Option<&str>) {
    if let Some(active_step) = &state.active_step {
        return (
            Some(active_step.step_index as i64),
            Some(active_step.node_id.as_str()),
        );
    }
    if state.next_step_index < state.order.len() {
        return (
            Some(state.next_step_index as i64),
            state
                .order
                .get(state.next_step_index)
                .map(|node_id| node_id.as_str()),
        );
    }
    (None, None)
}

fn workflow_execution_steps(state: &WorkflowExecutionState) -> Vec<serde_json::Value> {
    state
        .order
        .iter()
        .enumerate()
        .filter_map(|(index, node_id)| {
            state.node_states.get(node_id).map(|node_state| {
                serde_json::json!({
                    "step": index + 1,
                    "node_id": node_id.clone(),
                    "node_type": node_state.node_type.clone(),
                    "result": node_state.result.clone(),
                    "started_at": node_state.started_at.clone(),
                    "completed_at": node_state.completed_at.clone(),
                })
            })
        })
        .collect()
}

fn workflow_response_body(state: &WorkflowExecutionState) -> serde_json::Value {
    let (current_step_index, current_node_id) = workflow_execution_current_step(state);
    serde_json::json!({
        "execution_id": state.execution_id.clone(),
        "workflow_id": state.workflow_id.clone(),
        "workflow_name": state.workflow_name.clone(),
        "status": state.final_status.clone().unwrap_or_else(|| {
            if state.recovery_required {
                "recovery_required".to_string()
            } else {
                "running".to_string()
            }
        }),
        "mode": "dag",
        "steps": workflow_execution_steps(state),
        "input": state.input.clone(),
        "started_at": state.started_at.clone(),
        "completed_at": state.completed_at.clone(),
        "current_step_index": current_step_index,
        "current_node_id": current_node_id,
        "recovery_required": state.recovery_required,
        "recovery_action": state.recovery_action.clone(),
        "reason": state.recovery_reason.clone(),
    })
}

fn workflow_recovery_body(
    execution_id: &str,
    workflow_id: &str,
    workflow_name: Option<&str>,
    recovery_action: &str,
    reason: impl Into<String>,
) -> serde_json::Value {
    let reason = reason.into();
    serde_json::json!({
        "execution_id": execution_id,
        "workflow_id": workflow_id,
        "workflow_name": workflow_name.unwrap_or(workflow_id),
        "status": "recovery_required",
        "mode": "dag",
        "steps": [],
        "input": serde_json::Value::Null,
        "started_at": serde_json::Value::Null,
        "completed_at": serde_json::Value::Null,
        "current_step_index": serde_json::Value::Null,
        "current_node_id": serde_json::Value::Null,
        "recovery_required": true,
        "recovery_action": recovery_action,
        "reason": reason,
    })
}

/// GET /api/workflows — list saved workflows.
pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WorkflowListParams>,
) -> ApiResult<WorkflowListResponse> {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * page_size;

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_workflows", e))?;

    let total: u32 = db
        .query_row("SELECT COUNT(*) FROM workflows", [], |row| row.get(0))
        .map_err(|e| ApiError::db_error("workflow_count", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, name, description, nodes, edges, created_by, updated_at, created_at \
             FROM workflows \
             ORDER BY updated_at DESC \
             LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| ApiError::db_error("workflow_list_prepare", e))?;

    let raw_workflows: Vec<StoredWorkflowRow> = stmt
        .query_map(rusqlite::params![page_size, offset], |row| {
            Ok(StoredWorkflowRow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                nodes_json: row.get(3)?,
                edges_json: row.get(4)?,
                created_by: row.get(5)?,
                updated_at: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| ApiError::db_error("workflow_list_query", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::db_error("workflow_list_row", e))?;
    let workflows: Vec<WorkflowResponse> = raw_workflows
        .into_iter()
        .map(stored_workflow_to_response)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(WorkflowListResponse {
        workflows,
        page,
        page_size,
        total,
    }))
}

/// POST /api/workflows — create a new workflow.
pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<CreateWorkflowRequest>,
) -> Response {
    if body.name.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request("Workflow name is required"));
    }
    let graph = match parse_workflow_graph_input(body.nodes.as_ref(), "nodes").and_then(|nodes| {
        let edges = parse_workflow_graph_input(body.edges.as_ref(), "edges")?;
        validate_workflow_graph(&nodes, &edges)?;
        validate_workflow_supported_node_types(&nodes)?;
        Ok((nodes, edges))
    }) {
        Ok(graph) => graph,
        Err(error) => return error_response_with_idempotency(error),
    };

    let actor = workflow_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::to_value(&body).unwrap_or(serde_json::Value::Null);
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_WORKFLOW_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let id = operation_context
                .operation_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
            let description = body.description.clone().unwrap_or_default();
            let nodes = serde_json::to_string(&graph.0).map_err(|error| {
                ApiError::internal(format!("serialize workflow nodes: {error}"))
            })?;
            let edges = serde_json::to_string(&graph.1).map_err(|error| {
                ApiError::internal(format!("serialize workflow edges: {error}"))
            })?;

            conn.execute(
                "INSERT INTO workflows (id, name, description, nodes, edges, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![id, body.name, description, nodes, edges, actor],
            )
            .map_err(|e| ApiError::db_error("workflow_create", e))?;

            let response_id = id.clone();
            let response_name = body.name.clone();
            let response_description = description.clone();
            Ok((
                StatusCode::CREATED,
                serde_json::to_value(WorkflowCreateResponse {
                    id: response_id.clone(),
                    name: response_name.clone(),
                    description: response_description.clone(),
                    status: "created".to_string(),
                })
                .unwrap_or_else(|_| {
                    serde_json::json!({
                        "id": response_id,
                        "name": response_name,
                        "description": response_description,
                        "status": "created",
                    })
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "workflow",
                "create_workflow",
                "info",
                actor,
                "created",
                serde_json::json!({
                    "workflow_id": outcome.body["id"],
                    "name": outcome.body["name"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/workflows/:id — get a specific workflow.
pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<WorkflowResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_workflow", e))?;

    let workflow = db
        .query_row(
            "SELECT id, name, description, nodes, edges, created_by, updated_at, created_at \
             FROM workflows WHERE id = ?1",
            [&id],
            |row| {
                Ok(StoredWorkflowRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    nodes_json: row.get(3)?,
                    edges_json: row.get(4)?,
                    created_by: row.get(5)?,
                    updated_at: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::not_found(format!("Workflow {id} not found"))
            }
            _ => ApiError::db_error("workflow_get", e),
        })?;

    Ok(Json(stored_workflow_to_response(workflow)?))
}

/// PUT /api/workflows/:id — update a workflow.
pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkflowRequest>,
) -> Response {
    let validated_nodes = match parse_workflow_graph_input(body.nodes.as_ref(), "nodes") {
        Ok(nodes) => nodes,
        Err(error) => return error_response_with_idempotency(error),
    };
    let validated_edges = match parse_workflow_graph_input(body.edges.as_ref(), "edges") {
        Ok(edges) => edges,
        Err(error) => return error_response_with_idempotency(error),
    };
    if body.nodes.is_some() || body.edges.is_some() {
        let graph_check = {
            let db = match state.db.read() {
                Ok(db) => db,
                Err(error) => {
                    return error_response_with_idempotency(ApiError::db_error(
                        "workflow_update_check_graph",
                        error,
                    ));
                }
            };
            let current: Result<(String, String), rusqlite::Error> = db.query_row(
                "SELECT nodes, edges FROM workflows WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            match current {
                Ok((nodes_json, edges_json)) => {
                    let nodes = if body.nodes.is_some() {
                        validated_nodes.clone()
                    } else {
                        match parse_workflow_graph_array(&nodes_json, "nodes") {
                            Ok(nodes) => nodes,
                            Err(error) => return error_response_with_idempotency(error),
                        }
                    };
                    let edges = if body.edges.is_some() {
                        validated_edges.clone()
                    } else {
                        match parse_workflow_graph_array(&edges_json, "edges") {
                            Ok(edges) => edges,
                            Err(error) => return error_response_with_idempotency(error),
                        }
                    };
                    validate_workflow_graph(&nodes, &edges)
                        .and_then(|_| validate_workflow_supported_node_types(&nodes))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    Err(ApiError::not_found(format!("Workflow {id} not found")))
                }
                Err(error) => Err(ApiError::db_error("workflow_update_check_graph", error)),
            }
        };
        if let Err(error) = graph_check {
            return error_response_with_idempotency(error);
        }
    }

    let actor = workflow_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "workflow_id": id,
        "body": body,
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "PUT",
        UPDATE_WORKFLOW_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM workflows WHERE id = ?1",
                    [&id],
                    |row| row.get::<_, u32>(0).map(|c| c > 0),
                )
                .map_err(|e| ApiError::db_error("workflow_update_check", e))?;

            if !exists {
                return Err(ApiError::not_found(format!("Workflow {id} not found")));
            }

            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1u32;

            if let Some(ref name) = body.name {
                sets.push(format!("name = ?{idx}"));
                params.push(Box::new(name.clone()));
                idx += 1;
            }
            if let Some(ref desc) = body.description {
                sets.push(format!("description = ?{idx}"));
                params.push(Box::new(desc.clone()));
                idx += 1;
            }
            if let Some(ref nodes) = body.nodes {
                let json_str = serde_json::to_string(nodes).map_err(|error| {
                    ApiError::internal(format!("serialize workflow nodes: {error}"))
                })?;
                sets.push(format!("nodes = ?{idx}"));
                params.push(Box::new(json_str));
                idx += 1;
            }
            if let Some(ref edges) = body.edges {
                let json_str = serde_json::to_string(edges).map_err(|error| {
                    ApiError::internal(format!("serialize workflow edges: {error}"))
                })?;
                sets.push(format!("edges = ?{idx}"));
                params.push(Box::new(json_str));
                idx += 1;
            }

            if sets.is_empty() {
                return Err(ApiError::bad_request("No fields to update"));
            }

            sets.push("updated_at = datetime('now')".to_string());
            let query = format!("UPDATE workflows SET {} WHERE id = ?{idx}", sets.join(", "));
            params.push(Box::new(id.clone()));

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();

            conn.execute(&query, param_refs.as_slice())
                .map_err(|e| ApiError::db_error("workflow_update", e))?;

            Ok((
                StatusCode::OK,
                serde_json::to_value(WorkflowUpdateResponse {
                    id: id.clone(),
                    status: "updated".to_string(),
                })
                .unwrap_or_else(|_| {
                    serde_json::json!({
                        "id": id,
                        "status": "updated",
                    })
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "workflow",
                "update_workflow",
                "info",
                actor,
                "updated",
                serde_json::json!({ "workflow_id": id }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// Topological sort of workflow nodes using Kahn's algorithm.
fn topological_sort(
    nodes: &[serde_json::Value],
    edges: &[serde_json::Value],
) -> Result<Vec<String>, ApiError> {
    use std::collections::{HashMap, HashSet, VecDeque};

    let node_ids: Vec<String> = nodes
        .iter()
        .filter_map(|n| n.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let node_set: HashSet<&str> = node_ids.iter().map(|s| s.as_str()).collect();

    let mut in_degree: HashMap<&str, usize> = node_ids.iter().map(|id| (id.as_str(), 0)).collect();
    let mut adjacency: HashMap<&str, Vec<&str>> =
        node_ids.iter().map(|id| (id.as_str(), vec![])).collect();

    for edge in edges {
        let source = edge
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("workflow edge missing string source"))?;
        let target = edge
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("workflow edge missing string target"))?;
        if !node_set.contains(source) || !node_set.contains(target) {
            return Err(ApiError::bad_request(format!(
                "workflow edge references unknown node: {source} -> {target}"
            )));
        }
        adjacency.entry(source).or_default().push(target);
        *in_degree.entry(target).or_default() += 1;
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut order = Vec::new();
    while let Some(node) = queue.pop_front() {
        order.push(node.to_string());
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    if order.len() != node_ids.len() {
        return Err(ApiError::bad_request("Workflow contains a cycle"));
    }

    Ok(order)
}

enum WorkflowExecutionBootstrap {
    Drive {
        binding: WorkflowExecutionBinding,
        state: Box<WorkflowExecutionState>,
    },
    Ready {
        binding: WorkflowExecutionBinding,
        status: StatusCode,
        body: serde_json::Value,
    },
}

fn workflow_bootstrap_binding(
    execution_id: String,
    lease: &PreparedOperationLease,
    operation_id: &str,
) -> WorkflowExecutionBinding {
    WorkflowExecutionBinding {
        execution_id,
        journal_id: lease.journal_id.clone(),
        operation_id: operation_id.to_string(),
        owner_token: lease.owner_token.clone(),
        lease_epoch: lease.lease_epoch,
    }
}

fn stored_workflow_execution_response(
    status_code: Option<i64>,
    body: Option<&str>,
    context: &str,
) -> Result<Option<(StatusCode, serde_json::Value)>, ApiError> {
    let Some(raw_status) = status_code else {
        return Ok(None);
    };
    let raw_body =
        body.ok_or_else(|| ApiError::internal(format!("{context} missing final_response_body")))?;
    let status = StatusCode::from_u16(raw_status as u16).map_err(|_| {
        ApiError::internal(format!(
            "{context} has invalid final_response_status {raw_status}"
        ))
    })?;
    let parsed_body = serde_json::from_str(raw_body).map_err(|error| {
        ApiError::internal(format!(
            "{context} has invalid final_response_body: {error}"
        ))
    })?;
    Ok(Some((status, parsed_body)))
}

fn workflow_node_predecessors(
    edges: &[serde_json::Value],
) -> Result<std::collections::HashMap<String, Vec<String>>, ApiError> {
    let mut predecessors = std::collections::HashMap::new();
    for edge in edges {
        let source = edge
            .get("source")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("workflow execution edge missing source"))?;
        let target = edge
            .get("target")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("workflow execution edge missing target"))?;
        predecessors
            .entry(target.to_string())
            .or_insert_with(Vec::new)
            .push(source.to_string());
    }
    Ok(predecessors)
}

fn workflow_node_input(
    state: &WorkflowExecutionState,
    predecessors: &std::collections::HashMap<String, Vec<String>>,
    node_id: &str,
) -> Option<serde_json::Value> {
    let preds = predecessors.get(node_id).cloned().unwrap_or_default();
    if preds.len() == 1 {
        return state.node_outputs.get(&preds[0]).cloned();
    }
    if preds.len() > 1 {
        let inputs = preds
            .iter()
            .filter_map(|pred| {
                state
                    .node_outputs
                    .get(pred)
                    .map(|output| (pred.clone(), output.clone()))
            })
            .collect::<std::collections::BTreeMap<String, serde_json::Value>>();
        return Some(serde_json::json!(inputs));
    }
    state
        .node_outputs
        .get(node_id)
        .cloned()
        .or_else(|| state.input.clone())
}

fn workflow_agent_runtime_allowed(
    state: &AppState,
    execution_id: &str,
    requested_agent: Option<&str>,
) -> Result<(), String> {
    let resolved_agent = RuntimeSafetyBuilder::new(state)
        .resolve_agent(requested_agent, API_SYNTHETIC_AGENT_NAME)
        .map_err(|error| format!("runtime agent resolution failed: {error}"))?;
    let session_id = uuid::Uuid::parse_str(execution_id).unwrap_or_else(|_| uuid::Uuid::now_v7());
    let runtime_ctx = RuntimeSafetyContext::from_state(state, resolved_agent, session_id, None);
    runtime_ctx
        .ensure_execution_permitted()
        .map_err(|error| format!("{error}"))
}

async fn execute_workflow_step(
    state: &AppState,
    execution_id: &str,
    node: &serde_json::Value,
    input_val: Option<serde_json::Value>,
) -> WorkflowStepOutcome {
    let node_type = node
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    match node_type {
        "agent" | "llm_call" => {
            if let Err(error) = workflow_agent_runtime_allowed(
                state,
                execution_id,
                workflow_node_config_string(node, "agent_id").as_deref(),
            ) {
                return WorkflowStepOutcome {
                    status: "failed".to_string(),
                    result: serde_json::json!({
                        "status": "failed",
                        "error": error,
                    }),
                    output: None,
                };
            }

            let providers = provider_runtime::ordered_provider_configs(state);
            if providers.is_empty() {
                return WorkflowStepOutcome {
                    status: "failed".to_string(),
                    result: serde_json::json!({
                        "status": "failed",
                        "error": "no model providers configured",
                    }),
                    output: None,
                };
            }

            let prompt = workflow_node_config_string(node, "system_prompt")
                .or_else(|| workflow_node_config_string(node, "prompt"))
                .unwrap_or_else(|| "Process the input.".to_string());
            let input_str = input_val
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "(no input)".to_string());
            let messages = vec![
                ghost_llm::provider::ChatMessage {
                    role: ghost_llm::provider::MessageRole::System,
                    content: prompt,
                    tool_calls: None,
                    tool_call_id: None,
                },
                ghost_llm::provider::ChatMessage {
                    role: ghost_llm::provider::MessageRole::User,
                    content: input_str,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ];

            let mut fallback_chain = provider_runtime::build_fallback_chain(&providers);
            match fallback_chain.complete(&messages, &[]).await {
                Ok(result) => {
                    let text = match &result.response {
                        ghost_llm::provider::LLMResponse::Text(text) => text.clone(),
                        ghost_llm::provider::LLMResponse::Mixed { text, .. } => text.clone(),
                        _ => String::new(),
                    };
                    WorkflowStepOutcome {
                        status: "completed".to_string(),
                        result: serde_json::json!({
                            "status": "completed",
                            "tokens": result.usage.total_tokens,
                        }),
                        output: Some(serde_json::json!({ "text": text })),
                    }
                }
                Err(error) => WorkflowStepOutcome {
                    status: "failed".to_string(),
                    result: serde_json::json!({
                        "status": "failed",
                        "error": format!("{error}"),
                    }),
                    output: None,
                },
            }
        }
        "tool_exec" => {
            let Some(skill_name) = workflow_node_config_string(node, "skill_name")
                .or_else(|| workflow_node_config_string(node, "tool_name"))
            else {
                return WorkflowStepOutcome {
                    status: "failed".to_string(),
                    result: serde_json::json!({
                        "status": "failed",
                        "error": "tool_exec node is missing config.skill_name",
                    }),
                    output: None,
                };
            };
            let agent = match RuntimeSafetyBuilder::new(state).resolve_agent(
                workflow_node_config_string(node, "agent_id").as_deref(),
                API_SYNTHETIC_AGENT_NAME,
            ) {
                Ok(agent) => agent,
                Err(error) => {
                    return WorkflowStepOutcome {
                        status: "failed".to_string(),
                        result: serde_json::json!({
                            "status": "failed",
                            "error": format!("tool runtime agent resolution failed: {error}"),
                        }),
                        output: None,
                    }
                }
            };
            let session_id =
                uuid::Uuid::parse_str(execution_id).unwrap_or_else(|_| uuid::Uuid::now_v7());
            let executor = SkillCatalogExecutor::new(
                Arc::clone(&state.skill_catalog),
                Arc::clone(&state.db),
                state.convergence_profile.clone(),
            );
            let tool_input = input_val.unwrap_or(serde_json::Value::Null);
            match executor.execute(&skill_name, &agent, session_id, &tool_input) {
                Ok(result) => WorkflowStepOutcome {
                    status: "completed".to_string(),
                    result: serde_json::json!({
                        "status": "completed",
                        "skill": skill_name,
                    }),
                    output: Some(result.result),
                },
                Err(error) => WorkflowStepOutcome {
                    status: "failed".to_string(),
                    result: serde_json::json!({
                        "status": "failed",
                        "skill": skill_name,
                        "error": error.to_string(),
                    }),
                    output: None,
                },
            }
        }
        "condition" => {
            let condition_expr = workflow_node_config_string(node, "expression")
                .or_else(|| workflow_node_config_string(node, "condition"))
                .unwrap_or_else(|| "true".to_string());
            let input_str = input_val
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string());
            let passed = condition_expr == "true"
                || condition_expr == input_str
                || input_val
                    .as_ref()
                    .map(|value| !value.is_null())
                    .unwrap_or(false);
            WorkflowStepOutcome {
                status: if passed { "completed" } else { "failed" }.to_string(),
                result: serde_json::json!({
                    "status": if passed { "completed" } else { "failed" },
                    "condition": condition_expr,
                    "passed": passed,
                }),
                output: if passed { input_val } else { None },
            }
        }
        "transform" => {
            let output = input_val.unwrap_or(serde_json::Value::Null);
            WorkflowStepOutcome {
                status: "completed".to_string(),
                result: serde_json::json!({ "status": "completed" }),
                output: Some(output),
            }
        }
        "gate" | "gate_check" => WorkflowStepOutcome {
            status: "passed".to_string(),
            result: serde_json::json!({ "status": "passed" }),
            output: input_val,
        },
        "wait" => {
            let wait_ms = workflow_node_config_u64(node, "wait_ms")
                .unwrap_or(1000)
                .min(30_000);
            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
            WorkflowStepOutcome {
                status: "completed".to_string(),
                result: serde_json::json!({
                    "status": "completed",
                    "waited_ms": wait_ms,
                }),
                output: input_val,
            }
        }
        _ => WorkflowStepOutcome {
            status: "failed".to_string(),
            result: serde_json::json!({
                "status": "failed",
                "error": format!("unknown workflow node type '{node_type}'"),
            }),
            output: None,
        },
    }
}

async fn persist_workflow_execution_state(
    db: &crate::db_pool::DbPool,
    binding: &WorkflowExecutionBinding,
    state: &WorkflowExecutionState,
) -> Result<(), ApiError> {
    let state_json =
        serde_json::to_string(state).map_err(|error| ApiError::internal(error.to_string()))?;
    let final_response_body = state
        .final_response_body
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let row_status = if state.recovery_required {
        "recovery_required"
    } else {
        state.final_status.as_deref().unwrap_or("running")
    };
    let (current_step_index, current_node_id) = workflow_execution_current_step(state);
    let updated_at = chrono::Utc::now().to_rfc3339();
    let conn = db.write().await;
    let updated = cortex_storage::queries::workflow_execution_queries::update_owned_state(
        &conn,
        &binding.execution_id,
        &binding.journal_id,
        &binding.owner_token,
        binding.lease_epoch,
        &cortex_storage::queries::workflow_execution_queries::WorkflowExecutionOwnedUpdate {
            workflow_id: &state.workflow_id,
            workflow_name: &state.workflow_name,
            operation_id: &binding.operation_id,
            status: row_status,
            current_step_index,
            current_node_id,
            recovery_action: state.recovery_action.as_deref(),
            state_version: state.version as i64,
            state: &state_json,
            final_response_status: state.final_response_status.map(i64::from),
            final_response_body: final_response_body.as_deref(),
            completed_at: state.completed_at.as_deref(),
            updated_at: &updated_at,
        },
    )
    .map_err(|error| ApiError::db_error("workflow_execution_update_state", error))?;
    if !updated {
        return Err(ApiError::with_details(
            StatusCode::CONFLICT,
            "OPERATION_OWNERSHIP_LOST",
            "Workflow execution ownership was lost before state could be persisted",
            serde_json::json!({
                "execution_id": binding.execution_id,
                "journal_id": binding.journal_id,
            }),
        ));
    }
    Ok(())
}

fn load_workflow_snapshot(
    conn: &rusqlite::Connection,
    workflow_id: &str,
) -> Result<(Vec<serde_json::Value>, Vec<serde_json::Value>, String), ApiError> {
    let (nodes_json, edges_json, name): (String, String, String) = conn
        .query_row(
            "SELECT nodes, edges, name FROM workflows WHERE id = ?1",
            [workflow_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::not_found(format!("Workflow {workflow_id} not found"))
            }
            _ => ApiError::db_error("workflow_exec_load", error),
        })?;
    let nodes = parse_workflow_graph_array(&nodes_json, "nodes")?;
    let edges = parse_workflow_graph_array(&edges_json, "edges")?;
    validate_workflow_graph(&nodes, &edges)?;
    validate_workflow_supported_node_types(&nodes)?;
    Ok((nodes, edges, name))
}

fn workflow_detail_response_from_row(
    workflow_id: &str,
    row: &cortex_storage::queries::workflow_execution_queries::WorkflowExecutionRow,
) -> Result<serde_json::Value, ApiError> {
    if row.workflow_id.as_deref() != Some(workflow_id) {
        return Err(ApiError::not_found(format!(
            "Workflow execution {} not found for workflow {workflow_id}",
            row.id
        )));
    }
    if let Some((_, body)) = stored_workflow_execution_response(
        row.final_response_status,
        row.final_response_body.as_deref(),
        "workflow_executions",
    )? {
        return Ok(body);
    }
    if row.state_version != WORKFLOW_EXECUTION_STATE_VERSION as i64 {
        return Ok(workflow_recovery_body(
            &row.id,
            workflow_id,
            row.workflow_name.as_deref(),
            row.recovery_action
                .as_deref()
                .unwrap_or("state_upgrade_required"),
            format!(
                "workflow execution row uses unsupported typed state version {}",
                row.state_version
            ),
        ));
    }
    let parsed = parse_workflow_execution_state(&row.state)?;
    Ok(workflow_response_body(&parsed))
}

async fn bootstrap_workflow_execution_for_execute(
    state: &Arc<AppState>,
    workflow_id: &str,
    input: Option<serde_json::Value>,
    lease: &PreparedOperationLease,
    operation_id: &str,
) -> Result<WorkflowExecutionBootstrap, ApiError> {
    let conn = state.db.write().await;
    if let Some(existing) = cortex_storage::queries::workflow_execution_queries::get_by_journal_id(
        &conn,
        &lease.journal_id,
    )
    .map_err(|error| ApiError::db_error("workflow_execution_get_by_journal", error))?
    {
        let execution_id = existing.id.clone();
        let workflow_name = existing
            .workflow_name
            .clone()
            .or_else(|| existing.workflow_id.clone())
            .unwrap_or_else(|| workflow_id.to_string());
        let rebound = cortex_storage::queries::workflow_execution_queries::rebind_owner(
            &conn,
            &execution_id,
            workflow_id,
            &workflow_name,
            &lease.journal_id,
            operation_id,
            &lease.owner_token,
            lease.lease_epoch,
            &chrono::Utc::now().to_rfc3339(),
        )
        .map_err(|error| ApiError::db_error("workflow_execution_rebind_owner", error))?;
        if !rebound {
            return Err(ApiError::internal(format!(
                "failed to rebind workflow execution {execution_id}"
            )));
        }

        let binding = workflow_bootstrap_binding(execution_id.clone(), lease, operation_id);
        if let Some((status, body)) = stored_workflow_execution_response(
            existing.final_response_status,
            existing.final_response_body.as_deref(),
            "workflow_executions",
        )? {
            return Ok(WorkflowExecutionBootstrap::Ready {
                binding,
                status,
                body,
            });
        }
        if existing.state_version != WORKFLOW_EXECUTION_STATE_VERSION as i64 {
            return Ok(WorkflowExecutionBootstrap::Ready {
                binding,
                status: StatusCode::CONFLICT,
                body: workflow_recovery_body(
                    &execution_id,
                    workflow_id,
                    Some(&workflow_name),
                    existing
                        .recovery_action
                        .as_deref()
                        .unwrap_or("state_upgrade_required"),
                    format!(
                        "workflow execution row uses unsupported typed state version {}",
                        existing.state_version
                    ),
                ),
            });
        }
        let parsed = match parse_workflow_execution_state(&existing.state) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Ok(WorkflowExecutionBootstrap::Ready {
                    binding,
                    status: StatusCode::CONFLICT,
                    body: workflow_recovery_body(
                        &execution_id,
                        workflow_id,
                        Some(&workflow_name),
                        "invalid_persisted_state",
                        error.to_string(),
                    ),
                })
            }
        };
        return Ok(WorkflowExecutionBootstrap::Drive {
            binding,
            state: Box::new(parsed),
        });
    }

    let (nodes, edges, workflow_name) = load_workflow_snapshot(&conn, workflow_id)?;
    if nodes.is_empty() {
        return Err(ApiError::bad_request("Workflow has no nodes to execute"));
    }
    let order = topological_sort(&nodes, &edges)?;
    let execution_id = uuid::Uuid::now_v7().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();
    let execution_state = WorkflowExecutionState {
        version: WORKFLOW_EXECUTION_STATE_VERSION,
        execution_id: execution_id.clone(),
        workflow_id: workflow_id.to_string(),
        workflow_name: workflow_name.clone(),
        input,
        nodes,
        edges,
        order,
        node_states: std::collections::BTreeMap::new(),
        node_outputs: std::collections::BTreeMap::new(),
        next_step_index: 0,
        active_step: None,
        started_at: started_at.clone(),
        completed_at: None,
        final_status: None,
        final_response_status: None,
        final_response_body: None,
        recovery_required: false,
        recovery_action: None,
        recovery_reason: None,
    };
    let state_json = serde_json::to_string(&execution_state)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let current_node_id = execution_state
        .order
        .first()
        .map(|node_id| node_id.as_str());
    cortex_storage::queries::workflow_execution_queries::insert(
        &conn,
        &cortex_storage::queries::workflow_execution_queries::NewWorkflowExecutionRow {
            id: &execution_id,
            workflow_id,
            workflow_name: &workflow_name,
            journal_id: &lease.journal_id,
            operation_id,
            owner_token: &lease.owner_token,
            lease_epoch: lease.lease_epoch,
            state_version: WORKFLOW_EXECUTION_STATE_VERSION as i64,
            status: "running",
            current_step_index: Some(0),
            current_node_id,
            recovery_action: None,
            state: &state_json,
            final_response_status: None,
            final_response_body: None,
            started_at: &started_at,
            completed_at: None,
            updated_at: &started_at,
        },
    )
    .map_err(|error| ApiError::db_error("workflow_execution_insert", error))?;

    Ok(WorkflowExecutionBootstrap::Drive {
        binding: workflow_bootstrap_binding(execution_id, lease, operation_id),
        state: Box::new(execution_state),
    })
}

async fn bootstrap_workflow_execution_for_resume(
    state: &Arc<AppState>,
    workflow_id: &str,
    execution_id: &str,
    lease: &PreparedOperationLease,
    operation_id: &str,
) -> Result<WorkflowExecutionBootstrap, ApiError> {
    let conn = state.db.write().await;
    let existing =
        cortex_storage::queries::workflow_execution_queries::get_by_id(&conn, execution_id)
            .map_err(|error| ApiError::db_error("workflow_execution_get_by_id", error))?
            .ok_or_else(|| {
                ApiError::not_found(format!("Workflow execution {execution_id} not found"))
            })?;
    if existing.workflow_id.as_deref() != Some(workflow_id) {
        return Err(ApiError::not_found(format!(
            "Workflow execution {execution_id} not found for workflow {workflow_id}"
        )));
    }
    let workflow_name = existing
        .workflow_name
        .clone()
        .unwrap_or_else(|| workflow_id.to_string());
    let rebound = cortex_storage::queries::workflow_execution_queries::rebind_owner(
        &conn,
        execution_id,
        workflow_id,
        &workflow_name,
        &lease.journal_id,
        operation_id,
        &lease.owner_token,
        lease.lease_epoch,
        &chrono::Utc::now().to_rfc3339(),
    )
    .map_err(|error| ApiError::db_error("workflow_execution_rebind_owner", error))?;
    if !rebound {
        return Err(ApiError::internal(format!(
            "failed to rebind workflow execution {execution_id}"
        )));
    }

    let binding = workflow_bootstrap_binding(execution_id.to_string(), lease, operation_id);
    if let Some((status, body)) = stored_workflow_execution_response(
        existing.final_response_status,
        existing.final_response_body.as_deref(),
        "workflow_executions",
    )? {
        return Ok(WorkflowExecutionBootstrap::Ready {
            binding,
            status,
            body,
        });
    }
    if existing.state_version != WORKFLOW_EXECUTION_STATE_VERSION as i64 {
        return Ok(WorkflowExecutionBootstrap::Ready {
            binding,
            status: StatusCode::CONFLICT,
            body: workflow_recovery_body(
                execution_id,
                workflow_id,
                Some(&workflow_name),
                existing
                    .recovery_action
                    .as_deref()
                    .unwrap_or("state_upgrade_required"),
                format!(
                    "workflow execution row uses unsupported typed state version {}",
                    existing.state_version
                ),
            ),
        });
    }
    let parsed = match parse_workflow_execution_state(&existing.state) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(WorkflowExecutionBootstrap::Ready {
                binding,
                status: StatusCode::CONFLICT,
                body: workflow_recovery_body(
                    execution_id,
                    workflow_id,
                    Some(&workflow_name),
                    "invalid_persisted_state",
                    error.to_string(),
                ),
            })
        }
    };
    Ok(WorkflowExecutionBootstrap::Drive {
        binding,
        state: Box::new(parsed),
    })
}

async fn finalize_workflow_execution_failure(
    state: &Arc<AppState>,
    binding: &WorkflowExecutionBinding,
    execution_state: &mut WorkflowExecutionState,
    reason: impl Into<String>,
    recovery_action: Option<&str>,
) -> Result<(StatusCode, serde_json::Value), ApiError> {
    execution_state.completed_at = Some(chrono::Utc::now().to_rfc3339());
    execution_state.recovery_required = recovery_action.is_some();
    execution_state.recovery_action = recovery_action.map(ToString::to_string);
    execution_state.recovery_reason = Some(reason.into());
    execution_state.final_status = Some(if execution_state.recovery_required {
        "recovery_required".to_string()
    } else {
        "failed".to_string()
    });
    let (status, body) = if execution_state.recovery_required {
        (
            StatusCode::CONFLICT,
            workflow_recovery_body(
                &execution_state.execution_id,
                &execution_state.workflow_id,
                Some(&execution_state.workflow_name),
                execution_state
                    .recovery_action
                    .as_deref()
                    .unwrap_or("manual_recovery_required"),
                execution_state
                    .recovery_reason
                    .clone()
                    .unwrap_or_else(|| "workflow execution requires manual recovery".to_string()),
            ),
        )
    } else {
        (StatusCode::OK, workflow_response_body(execution_state))
    };
    execution_state.final_response_status = Some(status.as_u16());
    execution_state.final_response_body = Some(body.clone());
    persist_workflow_execution_state(&state.db, binding, execution_state).await?;
    crate::api::websocket::broadcast_event(
        state,
        if execution_state.recovery_required {
            crate::api::websocket::WsEvent::WorkflowExecutionRecoveryRequired {
                workflow_id: execution_state.workflow_id.clone(),
                execution_id: execution_state.execution_id.clone(),
                recovery_action: execution_state
                    .recovery_action
                    .clone()
                    .unwrap_or_else(|| "manual_recovery_required".to_string()),
                reason: execution_state
                    .recovery_reason
                    .clone()
                    .unwrap_or_else(|| "workflow execution requires manual recovery".to_string()),
                occurred_at: execution_state
                    .completed_at
                    .clone()
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            }
        } else {
            crate::api::websocket::WsEvent::WorkflowExecutionCompleted {
                workflow_id: execution_state.workflow_id.clone(),
                execution_id: execution_state.execution_id.clone(),
                completed_at: execution_state
                    .completed_at
                    .clone()
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                status: "failed".to_string(),
            }
        },
    );
    Ok((status, body))
}

async fn drive_workflow_execution(
    state: Arc<AppState>,
    binding: WorkflowExecutionBinding,
    mut execution_state: WorkflowExecutionState,
) -> Result<(StatusCode, serde_json::Value), ApiError> {
    let predecessors = workflow_node_predecessors(&execution_state.edges)?;
    let node_map: std::collections::HashMap<String, &serde_json::Value> = execution_state
        .nodes
        .iter()
        .filter_map(|node| {
            node.get("id")
                .and_then(|value| value.as_str())
                .map(|node_id| (node_id.to_string(), node))
        })
        .collect();

    tracing::info!(
        workflow_id = %execution_state.workflow_id,
        workflow_name = %execution_state.workflow_name,
        execution_id = %execution_state.execution_id,
        node_count = execution_state.nodes.len(),
        edge_count = execution_state.edges.len(),
        "Workflow execution started/resumed"
    );

    if let Some(active_step) = execution_state.active_step.clone() {
        if !active_step.retry_safe {
            return finalize_workflow_execution_failure(
                &state,
                &binding,
                &mut execution_state,
                format!(
                    "workflow crashed while non-retry-safe node '{}' ({}) was in progress",
                    active_step.node_id, active_step.node_type
                ),
                Some("manual_recovery_required"),
            )
            .await;
        }
        execution_state.next_step_index = active_step.step_index;
        execution_state.active_step = None;
        execution_state.recovery_required = false;
        execution_state.recovery_action = None;
        execution_state.recovery_reason = None;
        execution_state.final_status = None;
        execution_state.final_response_status = None;
        execution_state.final_response_body = None;
        execution_state.completed_at = None;
        persist_workflow_execution_state(&state.db, &binding, &execution_state).await?;
    }

    while execution_state.next_step_index < execution_state.order.len() {
        let step_index = execution_state.next_step_index;
        let node_id = execution_state.order[step_index].clone();
        let Some(node) = node_map.get(&node_id).copied() else {
            execution_state.node_states.insert(
                node_id.clone(),
                WorkflowNodeState {
                    node_id: node_id.clone(),
                    node_type: "unknown".to_string(),
                    status: "failed".to_string(),
                    result: serde_json::json!({
                        "status": "failed",
                        "error": "node missing from workflow execution snapshot",
                    }),
                    started_at: None,
                    completed_at: Some(chrono::Utc::now().to_rfc3339()),
                },
            );
            execution_state.next_step_index = step_index + 1;
            persist_workflow_execution_state(&state.db, &binding, &execution_state).await?;
            continue;
        };
        let node_type = node
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let preds = predecessors.get(&node_id).cloned().unwrap_or_default();
        let all_preds_completed = preds.iter().all(|pred| {
            execution_state
                .node_states
                .get(pred)
                .map(|node_state| node_state.status.as_str())
                .map(|status| status == "completed" || status == "passed")
                .unwrap_or(false)
        });

        if !preds.is_empty() && !all_preds_completed {
            execution_state.node_states.insert(
                node_id.clone(),
                WorkflowNodeState {
                    node_id: node_id.clone(),
                    node_type: node_type.to_string(),
                    status: "skipped".to_string(),
                    result: serde_json::json!({
                        "status": "skipped",
                        "reason": "predecessor not completed",
                    }),
                    started_at: None,
                    completed_at: Some(chrono::Utc::now().to_rfc3339()),
                },
            );
            execution_state.next_step_index = step_index + 1;
            persist_workflow_execution_state(&state.db, &binding, &execution_state).await?;
            continue;
        }

        let input_val = workflow_node_input(&execution_state, &predecessors, &node_id);
        let started_at = chrono::Utc::now().to_rfc3339();
        execution_state.active_step = Some(WorkflowActiveStep {
            step_index,
            node_id: node_id.clone(),
            node_type: node_type.to_string(),
            started_at: started_at.clone(),
            retry_safe: workflow_step_retry_safe(node_type),
        });
        persist_workflow_execution_state(&state.db, &binding, &execution_state).await?;

        crate::api::websocket::broadcast_event(
            &state,
            crate::api::websocket::WsEvent::WorkflowNodeStarted {
                workflow_id: execution_state.workflow_id.clone(),
                execution_id: execution_state.execution_id.clone(),
                node_id: node_id.clone(),
                node_type: node_type.to_string(),
                started_at: started_at.clone(),
            },
        );

        let step_outcome =
            execute_workflow_step(&state, &execution_state.execution_id, node, input_val).await;
        if let Some(output) = step_outcome.output.clone() {
            execution_state.node_outputs.insert(node_id.clone(), output);
        }
        let completed_at = chrono::Utc::now().to_rfc3339();
        execution_state.node_states.insert(
            node_id.clone(),
            WorkflowNodeState {
                node_id: node_id.clone(),
                node_type: node_type.to_string(),
                status: step_outcome.status.clone(),
                result: step_outcome.result,
                started_at: Some(started_at),
                completed_at: Some(completed_at.clone()),
            },
        );
        execution_state.active_step = None;
        execution_state.next_step_index = step_index + 1;
        persist_workflow_execution_state(&state.db, &binding, &execution_state).await?;

        crate::api::websocket::broadcast_event(
            &state,
            if step_outcome.status == "failed" {
                crate::api::websocket::WsEvent::WorkflowNodeFailed {
                    workflow_id: execution_state.workflow_id.clone(),
                    execution_id: execution_state.execution_id.clone(),
                    node_id: node_id.clone(),
                    node_type: node_type.to_string(),
                    completed_at: completed_at.clone(),
                    error_code: "WORKFLOW_NODE_FAILED".to_string(),
                    message: execution_state
                        .node_states
                        .get(&node_id)
                        .map(|node_state| node_state.result.to_string())
                        .unwrap_or_else(|| "workflow node failed".to_string()),
                    retry_safe: workflow_step_retry_safe(node_type),
                }
            } else {
                crate::api::websocket::WsEvent::WorkflowNodeCompleted {
                    workflow_id: execution_state.workflow_id.clone(),
                    execution_id: execution_state.execution_id.clone(),
                    node_id: node_id.clone(),
                    node_type: node_type.to_string(),
                    completed_at: completed_at.clone(),
                    status: step_outcome.status.clone(),
                }
            },
        );
    }

    let any_failed = execution_state
        .node_states
        .values()
        .any(|node_state| node_state.status == "failed");
    let any_skipped = execution_state
        .node_states
        .values()
        .any(|node_state| node_state.status == "skipped");
    execution_state.completed_at = Some(chrono::Utc::now().to_rfc3339());
    execution_state.recovery_required = false;
    execution_state.recovery_action = None;
    execution_state.recovery_reason = None;
    execution_state.final_status = Some(if any_failed || any_skipped {
        "failed".to_string()
    } else {
        "completed".to_string()
    });
    let body = workflow_response_body(&execution_state);
    execution_state.final_response_status = Some(StatusCode::OK.as_u16());
    execution_state.final_response_body = Some(body.clone());
    persist_workflow_execution_state(&state.db, &binding, &execution_state).await?;
    crate::api::websocket::broadcast_event(
        &state,
        crate::api::websocket::WsEvent::WorkflowExecutionCompleted {
            workflow_id: execution_state.workflow_id.clone(),
            execution_id: execution_state.execution_id.clone(),
            completed_at: execution_state
                .completed_at
                .clone()
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            status: execution_state
                .final_status
                .clone()
                .unwrap_or_else(|| "failed".to_string()),
        },
    );
    Ok((StatusCode::OK, body))
}

pub(crate) async fn execute_workflow_inner(
    state: Arc<AppState>,
    workflow_id: String,
    body: ExecuteWorkflowRequest,
    lease: &PreparedOperationLease,
    operation_id: &str,
) -> Result<(StatusCode, serde_json::Value), ApiError> {
    match bootstrap_workflow_execution_for_execute(
        &state,
        &workflow_id,
        body.input.clone(),
        lease,
        operation_id,
    )
    .await?
    {
        WorkflowExecutionBootstrap::Drive {
            binding,
            state: execution_state,
        } => {
            crate::api::websocket::broadcast_event(
                &state,
                crate::api::websocket::WsEvent::WorkflowExecutionStarted {
                    workflow_id: workflow_id.clone(),
                    execution_id: binding.execution_id.clone(),
                    started_at: execution_state.started_at.clone(),
                },
            );
            drive_workflow_execution(state, binding, *execution_state).await
        }
        WorkflowExecutionBootstrap::Ready {
            binding: _binding,
            status,
            body,
        } => Ok((status, body)),
    }
}

async fn resume_workflow_execution_inner(
    state: Arc<AppState>,
    workflow_id: String,
    execution_id: String,
    lease: &PreparedOperationLease,
    operation_id: &str,
) -> Result<(StatusCode, serde_json::Value), ApiError> {
    match bootstrap_workflow_execution_for_resume(
        &state,
        &workflow_id,
        &execution_id,
        lease,
        operation_id,
    )
    .await?
    {
        WorkflowExecutionBootstrap::Drive {
            binding,
            state: execution_state,
        } => {
            crate::api::websocket::broadcast_event(
                &state,
                crate::api::websocket::WsEvent::WorkflowExecutionResumed {
                    workflow_id: workflow_id.clone(),
                    execution_id: binding.execution_id.clone(),
                    resumed_at: chrono::Utc::now().to_rfc3339(),
                },
            );
            drive_workflow_execution(state, binding, *execution_state).await
        }
        WorkflowExecutionBootstrap::Ready {
            binding: _binding,
            status,
            body,
        } => Ok((status, body)),
    }
}

/// POST /api/workflows/:id/execute — execute a workflow DAG with durable state.
pub async fn execute_workflow(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(id): Path<String>,
    Json(body): Json<ExecuteWorkflowRequest>,
) -> Response {
    let actor = workflow_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "workflow_id": id,
        "input": body.input,
    });

    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            EXECUTE_WORKFLOW_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                &id,
                "execute_workflow",
                "medium",
                actor,
                "replayed",
                serde_json::json!({
                    "workflow_id": id,
                    "execution_id": stored.body.get("execution_id"),
                    "status": stored.body.get("status"),
                }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": EXECUTE_WORKFLOW_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            let required = match require_operation_context(&operation_context) {
                Ok(required) => required,
                Err(error) => return error_response_with_idempotency(error),
            };
            let heartbeat = start_operation_lease_heartbeat(Arc::clone(&state.db), lease.clone());
            match execute_workflow_inner(
                Arc::clone(&state),
                id.clone(),
                body.clone(),
                &lease,
                &required.operation_id,
            )
            .await
            {
                Ok((status, response_body)) => {
                    if let Err(error) = heartbeat.stop().await {
                        return error_response_with_idempotency(error);
                    }
                    let db = state.db.write().await;
                    match commit_prepared_json_operation(
                        &db,
                        &operation_context,
                        &lease,
                        status,
                        &response_body,
                    ) {
                        Ok(outcome) => {
                            write_mutation_audit_entry(
                                &db,
                                &id,
                                "execute_workflow",
                                "medium",
                                actor,
                                "executed",
                                serde_json::json!({
                                    "workflow_id": id,
                                    "execution_id": outcome.body["execution_id"],
                                    "status": outcome.body["status"],
                                }),
                                &operation_context,
                                &outcome.idempotency_status,
                            );
                            json_response_with_idempotency(
                                outcome.status,
                                outcome.body,
                                outcome.idempotency_status,
                            )
                        }
                        Err(error) => error_response_with_idempotency(error),
                    }
                }
                Err(error) => {
                    if let Err(heartbeat_error) = heartbeat.stop().await {
                        return error_response_with_idempotency(heartbeat_error);
                    }
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    error_response_with_idempotency(error)
                }
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/workflows/:id/executions — list executions for a workflow.
pub async fn list_executions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<WorkflowExecutionListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_executions", e))?;
    let rows =
        cortex_storage::queries::workflow_execution_queries::list_by_workflow_id(&db, &id, 50)
            .map_err(|error| ApiError::db_error("list_executions_query", error))?;
    let executions: Vec<WorkflowExecutionSummary> = rows
        .into_iter()
        .map(|row| WorkflowExecutionSummary {
            execution_id: row.id,
            status: row.status.clone(),
            started_at: row.started_at,
            completed_at: row.completed_at,
            updated_at: row.updated_at,
            state_version: row.state_version,
            current_step_index: row.current_step_index,
            current_node_id: row.current_node_id,
            recovery_action: row.recovery_action,
            recovery_required: row.status == "recovery_required",
        })
        .collect();

    Ok(Json(WorkflowExecutionListResponse {
        workflow_id: id,
        executions,
    }))
}

/// GET /api/workflows/:id/executions/:execution_id — get execution detail for a workflow.
pub async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path((workflow_id, execution_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_execution", e))?;
    let row = cortex_storage::queries::workflow_execution_queries::get_by_id(&db, &execution_id)
        .map_err(|error| ApiError::db_error("get_execution_query", error))?
        .ok_or_else(|| {
            ApiError::not_found(format!("Workflow execution {execution_id} not found"))
        })?;
    Ok(Json(workflow_detail_response_from_row(&workflow_id, &row)?))
}

/// POST /api/workflows/:id/resume/:execution_id — resume a failed execution.
pub async fn resume_execution(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path((workflow_id, execution_id)): Path<(String, String)>,
) -> Response {
    let actor = workflow_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "workflow_id": workflow_id,
        "resume_from_execution_id": execution_id,
    });

    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            RESUME_WORKFLOW_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                &workflow_id,
                "resume_workflow_execution",
                "medium",
                actor,
                "replayed",
                serde_json::json!({
                    "workflow_id": workflow_id,
                    "resume_from_execution_id": execution_id,
                    "execution_id": stored.body.get("execution_id"),
                }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": RESUME_WORKFLOW_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            let required = match require_operation_context(&operation_context) {
                Ok(required) => required,
                Err(error) => return error_response_with_idempotency(error),
            };
            let heartbeat = start_operation_lease_heartbeat(Arc::clone(&state.db), lease.clone());
            match resume_workflow_execution_inner(
                Arc::clone(&state),
                workflow_id.clone(),
                execution_id.clone(),
                &lease,
                &required.operation_id,
            )
            .await
            {
                Ok((status, response_body)) => {
                    if let Err(error) = heartbeat.stop().await {
                        return error_response_with_idempotency(error);
                    }
                    let db = state.db.write().await;
                    match commit_prepared_json_operation(
                        &db,
                        &operation_context,
                        &lease,
                        status,
                        &response_body,
                    ) {
                        Ok(outcome) => {
                            let audit_outcome = if outcome.status == StatusCode::OK {
                                "resumed"
                            } else {
                                "rejected"
                            };
                            write_mutation_audit_entry(
                                &db,
                                &workflow_id,
                                "resume_workflow_execution",
                                "high",
                                actor,
                                audit_outcome,
                                serde_json::json!({
                                    "workflow_id": workflow_id,
                                    "resume_from_execution_id": execution_id,
                                    "execution_id": outcome.body.get("execution_id"),
                                    "status": outcome.body.get("status"),
                                }),
                                &operation_context,
                                &outcome.idempotency_status,
                            );
                            json_response_with_idempotency(
                                outcome.status,
                                outcome.body,
                                outcome.idempotency_status,
                            )
                        }
                        Err(error) => error_response_with_idempotency(error),
                    }
                }
                Err(error) => {
                    if let Err(heartbeat_error) = heartbeat.stop().await {
                        return error_response_with_idempotency(heartbeat_error);
                    }
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    error_response_with_idempotency(error)
                }
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, node_type: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "type": node_type,
            "config": {},
        })
    }

    #[test]
    fn validate_workflow_supported_node_types_rejects_unknown_types() {
        let nodes = vec![node("n1", "llm_call"), node("n2", "parallel_branch")];

        let error = validate_workflow_supported_node_types(&nodes).expect_err("unsupported node");

        match error {
            ApiError::Validation(message) => {
                assert!(
                    message.contains("unsupported type 'parallel_branch'"),
                    "unexpected error message: {message}"
                );
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn workflow_recovery_body_is_shape_stable_for_the_dashboard() {
        let body = workflow_recovery_body(
            "exec-1",
            "wf-1",
            Some("Workflow One"),
            "manual_recovery_required",
            "test reason",
        );

        assert_eq!(
            body.get("status").and_then(|value| value.as_str()),
            Some("recovery_required")
        );
        assert_eq!(
            body.get("mode").and_then(|value| value.as_str()),
            Some("dag")
        );
        assert_eq!(
            body.get("steps")
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            body.get("recovery_action").and_then(|value| value.as_str()),
            Some("manual_recovery_required")
        );
        assert_eq!(
            body.get("reason").and_then(|value| value.as_str()),
            Some("test reason")
        );
    }

    #[test]
    fn workflow_detail_response_enforces_workflow_ownership() {
        let row = cortex_storage::queries::workflow_execution_queries::WorkflowExecutionRow {
            id: "exec-1".to_string(),
            workflow_id: Some("wf-actual".to_string()),
            workflow_name: Some("Workflow Actual".to_string()),
            journal_id: None,
            operation_id: None,
            owner_token: None,
            lease_epoch: None,
            state_version: WORKFLOW_EXECUTION_STATE_VERSION as i64,
            status: "running".to_string(),
            current_step_index: Some(0),
            current_node_id: Some("n1".to_string()),
            recovery_action: None,
            state: serde_json::json!({
                "version": WORKFLOW_EXECUTION_STATE_VERSION,
                "execution_id": "exec-1",
                "workflow_id": "wf-actual",
                "workflow_name": "Workflow Actual",
                "input": null,
                "nodes": [node("n1", "transform")],
                "edges": [],
                "order": ["n1"],
                "node_states": {},
                "node_outputs": {},
                "next_step_index": 0,
                "active_step": null,
                "started_at": "2026-03-11T00:00:00Z",
                "completed_at": null,
                "final_status": null,
                "final_response_status": null,
                "final_response_body": null,
                "recovery_required": false,
                "recovery_action": null,
                "recovery_reason": null,
            })
            .to_string(),
            final_response_status: None,
            final_response_body: None,
            started_at: "2026-03-11T00:00:00Z".to_string(),
            completed_at: None,
            updated_at: "2026-03-11T00:00:00Z".to_string(),
        };

        let error =
            workflow_detail_response_from_row("wf-requested", &row).expect_err("ownership guard");

        match error {
            ApiError::NotFound { entity, id } => {
                assert_eq!(entity, "resource");
                assert!(
                    id.contains("not found for workflow wf-requested"),
                    "unexpected not-found id/message payload: {id}"
                );
            }
            other => panic!("expected not found error, got {other:?}"),
        }
    }
}
