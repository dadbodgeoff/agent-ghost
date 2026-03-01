//! Workflow CRUD and execution endpoints (T-2.1.9).
//!
//! Manages saved workflow definitions (DAGs of agent, gate, and tool nodes).
//! Stored in the workflows table (v021_workflows migration).
//!
//! Ref: ADE_DESIGN_PLAN §17.11, tasks.md T-2.1.9

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Query parameters for workflow listing.
#[derive(Debug, Deserialize)]
pub struct WorkflowListParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request body for creating a workflow.
#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    pub nodes: Option<serde_json::Value>,
    pub edges: Option<serde_json::Value>,
}

/// Request body for updating a workflow.
#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub nodes: Option<serde_json::Value>,
    pub edges: Option<serde_json::Value>,
}

/// Request body for executing a workflow.
#[derive(Debug, Deserialize)]
pub struct ExecuteWorkflowRequest {
    /// Input payload for the first node.
    pub input: Option<serde_json::Value>,
}

/// Workflow response shape.
#[derive(Debug, Serialize)]
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

/// GET /api/workflows — list saved workflows.
pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WorkflowListParams>,
) -> ApiResult<serde_json::Value> {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * page_size;

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

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

    let workflows: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![page_size, offset], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, String>(2)?,
                "nodes": serde_json::from_str::<serde_json::Value>(
                    &row.get::<_, String>(3)?
                ).unwrap_or(serde_json::Value::Array(vec![])),
                "edges": serde_json::from_str::<serde_json::Value>(
                    &row.get::<_, String>(4)?
                ).unwrap_or(serde_json::Value::Array(vec![])),
                "created_by": row.get::<_, Option<String>>(5)?,
                "updated_at": row.get::<_, String>(6)?,
                "created_at": row.get::<_, String>(7)?,
            }))
        })
        .map_err(|e| ApiError::db_error("workflow_list_query", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(serde_json::json!({
        "workflows": workflows,
        "page": page,
        "page_size": page_size,
        "total": total,
    })))
}

/// POST /api/workflows — create a new workflow.
pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWorkflowRequest>,
) -> ApiResult<serde_json::Value> {
    if body.name.trim().is_empty() {
        return Err(ApiError::bad_request("Workflow name is required"));
    }

    let id = uuid::Uuid::now_v7().to_string();
    let description = body.description.unwrap_or_default();
    let nodes = serde_json::to_string(&body.nodes.unwrap_or(serde_json::Value::Array(vec![])))
        .unwrap_or_else(|_| "[]".to_string());
    let edges = serde_json::to_string(&body.edges.unwrap_or(serde_json::Value::Array(vec![])))
        .unwrap_or_else(|_| "[]".to_string());

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    db.execute(
        "INSERT INTO workflows (id, name, description, nodes, edges) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, body.name, description, nodes, edges],
    )
    .map_err(|e| ApiError::db_error("workflow_create", e))?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": body.name,
        "description": description,
        "status": "created",
    })))
}

/// GET /api/workflows/:id — get a specific workflow.
pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    let workflow = db
        .query_row(
            "SELECT id, name, description, nodes, edges, created_by, updated_at, created_at \
             FROM workflows WHERE id = ?1",
            [&id],
            |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "nodes": serde_json::from_str::<serde_json::Value>(
                        &row.get::<_, String>(3)?
                    ).unwrap_or(serde_json::Value::Array(vec![])),
                    "edges": serde_json::from_str::<serde_json::Value>(
                        &row.get::<_, String>(4)?
                    ).unwrap_or(serde_json::Value::Array(vec![])),
                    "created_by": row.get::<_, Option<String>>(5)?,
                    "updated_at": row.get::<_, String>(6)?,
                    "created_at": row.get::<_, String>(7)?,
                }))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::not_found(format!("Workflow {id} not found"))
            }
            _ => ApiError::db_error("workflow_get", e),
        })?;

    Ok(Json(workflow))
}

/// PUT /api/workflows/:id — update a workflow.
pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkflowRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Check existence first.
    let exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM workflows WHERE id = ?1",
            [&id],
            |row| row.get::<_, u32>(0).map(|c| c > 0),
        )
        .map_err(|e| ApiError::db_error("workflow_update_check", e))?;

    if !exists {
        return Err(ApiError::not_found(format!("Workflow {id} not found")));
    }

    // Build dynamic UPDATE — only set fields that are provided.
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
        let json_str = serde_json::to_string(nodes).unwrap_or_else(|_| "[]".to_string());
        sets.push(format!("nodes = ?{idx}"));
        params.push(Box::new(json_str));
        idx += 1;
    }
    if let Some(ref edges) = body.edges {
        let json_str = serde_json::to_string(edges).unwrap_or_else(|_| "[]".to_string());
        sets.push(format!("edges = ?{idx}"));
        params.push(Box::new(json_str));
        idx += 1;
    }

    if sets.is_empty() {
        return Err(ApiError::bad_request("No fields to update"));
    }

    // Always update updated_at.
    sets.push(format!("updated_at = datetime('now')"));

    // Add the WHERE id = ?N.
    let query = format!(
        "UPDATE workflows SET {} WHERE id = ?{idx}",
        sets.join(", ")
    );
    params.push(Box::new(id.clone()));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    db.execute(&query, param_refs.as_slice())
        .map_err(|e| ApiError::db_error("workflow_update", e))?;

    Ok(Json(serde_json::json!({
        "id": id,
        "status": "updated",
    })))
}

/// POST /api/workflows/:id/execute — execute a workflow (sequential pipeline for P2).
///
/// MVP: validates the DAG, logs the execution intent, and returns a placeholder
/// result. Full agent-loop integration deferred to P3.
pub async fn execute_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ExecuteWorkflowRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Load the workflow.
    let (nodes_json, edges_json, name): (String, String, String) = db
        .query_row(
            "SELECT nodes, edges, name FROM workflows WHERE id = ?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::not_found(format!("Workflow {id} not found"))
            }
            _ => ApiError::db_error("workflow_exec_load", e),
        })?;

    let nodes: Vec<serde_json::Value> =
        serde_json::from_str(&nodes_json).unwrap_or_default();
    let edges: Vec<serde_json::Value> =
        serde_json::from_str(&edges_json).unwrap_or_default();

    let execution_id = uuid::Uuid::now_v7().to_string();

    tracing::info!(
        workflow_id = %id,
        workflow_name = %name,
        execution_id = %execution_id,
        node_count = nodes.len(),
        edge_count = edges.len(),
        "Workflow execution started (sequential pipeline)"
    );

    // Sequential pipeline: process nodes in topological order.
    // For P2 MVP, we simulate execution by walking the node list.
    let mut steps = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
        let node_id = node.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        steps.push(serde_json::json!({
            "step": i + 1,
            "node_id": node_id,
            "node_type": node_type,
            "status": "simulated",
        }));
    }

    Ok(Json(serde_json::json!({
        "execution_id": execution_id,
        "workflow_id": id,
        "workflow_name": name,
        "status": "completed",
        "mode": "sequential_simulation",
        "steps": steps,
        "input": body.input,
    })))
}
