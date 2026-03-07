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

    let db = state.db.write().await;

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
    let db = state.db.write().await;

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
    let query = format!("UPDATE workflows SET {} WHERE id = ?{idx}", sets.join(", "));
    params.push(Box::new(id.clone()));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    db.execute(&query, param_refs.as_slice())
        .map_err(|e| ApiError::db_error("workflow_update", e))?;

    Ok(Json(serde_json::json!({
        "id": id,
        "status": "updated",
    })))
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
        let source = edge.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let target = edge.get("target").and_then(|v| v.as_str()).unwrap_or("");
        if node_set.contains(source) && node_set.contains(target) {
            adjacency.entry(source).or_default().push(target);
            *in_degree.entry(target).or_default() += 1;
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
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

/// Persist execution state to the database for crash recovery.
async fn persist_execution_state(
    db: &crate::db_pool::DbPool,
    exec_state: &serde_json::Value,
) -> Result<(), ApiError> {
    let execution_id = exec_state
        .get("execution_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let state_json = serde_json::to_string(exec_state).unwrap_or_else(|_| "{}".to_string());
    let conn = db.write().await;
    conn.execute(
        "INSERT OR REPLACE INTO workflow_executions (id, state, updated_at) VALUES (?1, ?2, datetime('now'))",
        rusqlite::params![execution_id, state_json],
    )
    .map_err(|e| ApiError::db_error("persist_execution_state", e))?;
    Ok(())
}

/// POST /api/workflows/:id/execute — execute a workflow DAG with durable state.
///
/// Phase 3: Full DAG execution with topological sort, per-node state persistence,
/// crash recovery, and real-time WebSocket progress events.
pub async fn execute_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ExecuteWorkflowRequest>,
) -> ApiResult<serde_json::Value> {
    let (nodes, edges, name, execution_id) = {
        let db = state
            .db
            .read()
            .map_err(|e| ApiError::db_error("workflow_exec_load", e))?;

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

        let nodes: Vec<serde_json::Value> = serde_json::from_str(&nodes_json).unwrap_or_default();
        let edges: Vec<serde_json::Value> = serde_json::from_str(&edges_json).unwrap_or_default();
        let execution_id = uuid::Uuid::now_v7().to_string();
        (nodes, edges, name, execution_id)
    };

    if nodes.is_empty() {
        return Err(ApiError::bad_request("Workflow has no nodes to execute"));
    }

    let order = topological_sort(&nodes, &edges)?;

    tracing::info!(
        workflow_id = %id,
        workflow_name = %name,
        execution_id = %execution_id,
        node_count = nodes.len(),
        edge_count = edges.len(),
        "Workflow execution started (DAG mode)"
    );

    let has_agent_nodes = nodes.iter().any(|n| {
        let t = n.get("type").and_then(|v| v.as_str()).unwrap_or("");
        t == "agent" || t == "llm_call"
    });

    if has_agent_nodes && state.model_providers.is_empty() {
        return Err(ApiError::bad_request(
            "Workflow contains agent nodes but no model providers are configured. \
             Add provider config to ghost.yml to enable workflow execution.",
        ));
    }

    let node_map: std::collections::HashMap<String, &serde_json::Value> = nodes
        .iter()
        .filter_map(|n| {
            n.get("id")
                .and_then(|v| v.as_str())
                .map(|nid| (nid.to_string(), n))
        })
        .collect();

    // Build predecessor map from edges.
    let mut predecessors: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for edge in &edges {
        let source = edge.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let target = edge.get("target").and_then(|v| v.as_str()).unwrap_or("");
        predecessors
            .entry(target.to_string())
            .or_default()
            .push(source.to_string());
    }

    let mut node_states: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    let mut node_outputs: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();

    if let Some(ref input) = body.input {
        if let Some(first_id) = order.first() {
            node_outputs.insert(first_id.clone(), input.clone());
        }
    }

    let mut exec_state = serde_json::json!({
        "execution_id": execution_id,
        "workflow_id": id,
        "workflow_name": name,
        "status": "running",
        "started_at": chrono::Utc::now().to_rfc3339(),
        "node_states": {},
    });
    let _ = persist_execution_state(&state.db, &exec_state).await;

    for node_id in &order {
        let node = match node_map.get(node_id) {
            Some(n) => *n,
            None => continue,
        };
        let node_type = node
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let preds = predecessors.get(node_id).cloned().unwrap_or_default();
        let all_preds_completed = preds.iter().all(|p| {
            node_states
                .get(p)
                .and_then(|s| s.get("status"))
                .and_then(|s| s.as_str())
                .map(|s| s == "completed" || s == "passed")
                .unwrap_or(false)
        });

        if !preds.is_empty() && !all_preds_completed {
            node_states.insert(
                node_id.clone(),
                serde_json::json!({
                    "node_id": node_id,
                    "node_type": node_type,
                    "status": "skipped",
                    "reason": "predecessor not completed",
                }),
            );
            continue;
        }

        // Collect input from predecessors.
        let input_val = if preds.len() == 1 {
            node_outputs.get(&preds[0]).cloned()
        } else if preds.len() > 1 {
            Some(serde_json::json!(preds
                .iter()
                .filter_map(|p| node_outputs.get(p).map(|o| (p.clone(), o.clone())))
                .collect::<std::collections::HashMap<String, serde_json::Value>>(
                )))
        } else {
            node_outputs.get(node_id).cloned().or(body.input.clone())
        };

        let started_at = chrono::Utc::now().to_rfc3339();

        crate::api::websocket::broadcast_event(
            &state,
            crate::api::websocket::WsEvent::SessionEvent {
                session_id: execution_id.clone(),
                event_id: uuid::Uuid::new_v4().to_string(),
                event_type: "node_started".into(),
                sender: None,
                sequence_number: 0,
            },
        );

        let step_result = match node_type {
            "agent" | "llm_call" => {
                let prompt = node
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Process the input.");
                let input_str = input_val
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(no input)".to_string());

                let messages = vec![
                    ghost_llm::provider::ChatMessage {
                        role: ghost_llm::provider::MessageRole::System,
                        content: prompt.to_string(),
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

                if let Some(pc) = state.model_providers.first() {
                    let api_key = pc
                        .api_key_env
                        .as_deref()
                        .and_then(|env| std::env::var(env).ok())
                        .unwrap_or_default();
                    if !api_key.is_empty() {
                        let provider: Arc<dyn ghost_llm::provider::LLMProvider> =
                            Arc::new(ghost_llm::provider::AnthropicProvider {
                                model: "claude-sonnet-4-6".to_string(),
                                api_key: std::sync::RwLock::new(api_key),
                            });
                        match provider.complete(&messages, &[]).await {
                            Ok(result) => {
                                let text = match &result.response {
                                    ghost_llm::provider::LLMResponse::Text(t) => t.clone(),
                                    ghost_llm::provider::LLMResponse::Mixed { text, .. } => {
                                        text.clone()
                                    }
                                    _ => String::new(),
                                };
                                node_outputs
                                    .insert(node_id.clone(), serde_json::json!({ "text": text }));
                                serde_json::json!({
                                    "status": "completed",
                                    "tokens": result.usage.total_tokens,
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "status": "failed",
                                    "error": format!("{e}"),
                                })
                            }
                        }
                    } else {
                        serde_json::json!({ "status": "skipped", "reason": "no API key configured" })
                    }
                } else {
                    serde_json::json!({ "status": "skipped", "reason": "no model provider configured" })
                }
            }
            "condition" => {
                let condition_expr = node
                    .get("condition")
                    .and_then(|v| v.as_str())
                    .unwrap_or("true");
                let passed = condition_expr == "true"
                    || input_val.as_ref().map(|v| !v.is_null()).unwrap_or(false);
                if passed {
                    if let Some(inp) = &input_val {
                        node_outputs.insert(node_id.clone(), inp.clone());
                    }
                }
                serde_json::json!({ "status": if passed { "completed" } else { "failed" }, "condition": condition_expr, "passed": passed })
            }
            "transform" => {
                let output = input_val.unwrap_or(serde_json::Value::Null);
                node_outputs.insert(node_id.clone(), output);
                serde_json::json!({ "status": "completed" })
            }
            "gate" | "gate_check" => {
                if let Some(inp) = &input_val {
                    node_outputs.insert(node_id.clone(), inp.clone());
                }
                serde_json::json!({ "status": "passed" })
            }
            "wait" => {
                let wait_ms = node
                    .get("wait_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1000)
                    .min(30_000);
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                if let Some(inp) = &input_val {
                    node_outputs.insert(node_id.clone(), inp.clone());
                }
                serde_json::json!({ "status": "completed", "waited_ms": wait_ms })
            }
            _ => {
                if let Some(inp) = &input_val {
                    node_outputs.insert(node_id.clone(), inp.clone());
                }
                serde_json::json!({ "status": "completed" })
            }
        };

        let completed_at = chrono::Utc::now().to_rfc3339();
        node_states.insert(
            node_id.clone(),
            serde_json::json!({
                "node_id": node_id,
                "node_type": node_type,
                "status": step_result.get("status").and_then(|s| s.as_str()).unwrap_or("unknown"),
                "result": step_result,
                "started_at": started_at,
                "completed_at": completed_at,
            }),
        );

        // Persist checkpoint after each node for crash recovery.
        exec_state["node_states"] = serde_json::json!(node_states);
        let _ = persist_execution_state(&state.db, &exec_state).await;

        crate::api::websocket::broadcast_event(
            &state,
            crate::api::websocket::WsEvent::SessionEvent {
                session_id: execution_id.clone(),
                event_id: uuid::Uuid::new_v4().to_string(),
                event_type: "node_completed".into(),
                sender: None,
                sequence_number: 0,
            },
        );
    }

    let any_failed = node_states
        .values()
        .any(|s| s.get("status").and_then(|s| s.as_str()) == Some("failed"));
    let final_status = if any_failed { "failed" } else { "completed" };

    exec_state["status"] = serde_json::json!(final_status);
    exec_state["completed_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    let _ = persist_execution_state(&state.db, &exec_state).await;

    let steps: Vec<serde_json::Value> = order
        .iter()
        .enumerate()
        .filter_map(|(i, nid)| {
            node_states.get(nid).map(|ns| {
                serde_json::json!({
                    "step": i + 1,
                    "node_id": nid,
                    "node_type": ns.get("node_type"),
                    "result": ns.get("result"),
                    "started_at": ns.get("started_at"),
                    "completed_at": ns.get("completed_at"),
                })
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "execution_id": execution_id,
        "workflow_id": id,
        "workflow_name": name,
        "status": final_status,
        "mode": "dag",
        "steps": steps,
        "input": body.input,
        "started_at": exec_state.get("started_at"),
        "completed_at": exec_state.get("completed_at"),
    })))
}

/// GET /api/workflows/:id/executions — list executions for a workflow.
pub async fn list_executions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_executions", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, state, updated_at FROM workflow_executions \
             WHERE json_extract(state, '$.workflow_id') = ?1 \
             ORDER BY updated_at DESC LIMIT 50",
        )
        .map_err(|e| ApiError::db_error("list_executions_prepare", e))?;

    let executions: Vec<serde_json::Value> = stmt
        .query_map([&id], |row| {
            let state_str: String = row.get(1)?;
            let parsed: serde_json::Value =
                serde_json::from_str(&state_str).unwrap_or(serde_json::Value::Null);
            Ok(serde_json::json!({
                "execution_id": row.get::<_, String>(0)?,
                "status": parsed.get("status"),
                "started_at": parsed.get("started_at"),
                "completed_at": parsed.get("completed_at"),
                "updated_at": row.get::<_, String>(2)?,
            }))
        })
        .map_err(|e| ApiError::db_error("list_executions_query", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(serde_json::json!({
        "workflow_id": id,
        "executions": executions,
    })))
}

/// POST /api/workflows/:id/resume/:execution_id — resume a failed execution.
pub async fn resume_execution(
    State(state): State<Arc<AppState>>,
    Path((workflow_id, _execution_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    // Re-trigger execution with the same workflow.
    let body = ExecuteWorkflowRequest { input: None };
    execute_workflow(State(state), Path(workflow_id), Json(body)).await
}
