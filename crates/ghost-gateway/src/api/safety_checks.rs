//! Custom safety check registration API (T-4.3.2).
//!
//! Allows registering additional validation dimensions beyond the built-in
//! 7 (D1–D7). Custom checks are stored in-memory and invoked during
//! proposal validation via their webhook URL.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSafetyCheck {
    pub id: String,
    pub name: String,
    pub description: String,
    pub webhook_url: String,
    pub dimension_id: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct RegisterCheckRequest {
    pub name: String,
    pub description: Option<String>,
    pub webhook_url: String,
    pub dimension_id: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SafetyCheckListResponse {
    pub checks: Vec<CustomSafetyCheck>,
    pub builtin_dimensions: Vec<&'static str>,
}

// ── Built-in dimensions ────────────────────────────────────────────

const BUILTIN_DIMENSIONS: &[&str] = &[
    "D1: Convergence Threshold",
    "D2: Goal Boundary",
    "D3: Initiative Ratio",
    "D4: Memory Consistency",
    "D5: Temporal Coherence",
    "D6: Output Safety",
    "D7: Resource Budget",
];

// ── Handlers ───────────────────────────────────────────────────────

/// GET /api/safety/checks — list all registered safety checks.
pub async fn list_safety_checks(
    State(state): State<Arc<AppState>>,
) -> ApiResult<SafetyCheckListResponse> {
    let checks = state
        .custom_safety_checks
        .read()
        .map_err(|_| ApiError::lock_poisoned("custom_safety_checks"))?
        .clone();

    Ok(Json(SafetyCheckListResponse {
        checks,
        builtin_dimensions: BUILTIN_DIMENSIONS.to_vec(),
    }))
}

/// POST /api/safety/checks — register a custom safety check.
pub async fn register_safety_check(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterCheckRequest>,
) -> ApiResult<CustomSafetyCheck> {
    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("Check name is required"));
    }
    if req.webhook_url.trim().is_empty() {
        return Err(ApiError::bad_request("Webhook URL is required"));
    }
    // T-5.5.1: Validate webhook URL against SSRF blocklist.
    if let Err(e) = crate::api::ssrf::validate_url(&req.webhook_url) {
        return Err(ApiError::bad_request(format!("Safety check URL blocked: {e}")));
    }
    if req.dimension_id.trim().is_empty() {
        return Err(ApiError::bad_request("Dimension ID is required"));
    }

    // Dimension ID must not overlap with built-in D1–D7.
    let dim = req.dimension_id.to_uppercase();
    if matches!(dim.as_str(), "D1" | "D2" | "D3" | "D4" | "D5" | "D6" | "D7") {
        return Err(ApiError::bad_request(format!(
            "Dimension '{dim}' is a built-in dimension and cannot be overridden"
        )));
    }

    let check = CustomSafetyCheck {
        id: uuid::Uuid::now_v7().to_string(),
        name: req.name,
        description: req.description.unwrap_or_default(),
        webhook_url: req.webhook_url,
        dimension_id: req.dimension_id,
        timeout_ms: req.timeout_ms.unwrap_or(5000),
    };

    let mut checks = state
        .custom_safety_checks
        .write()
        .map_err(|_| ApiError::lock_poisoned("custom_safety_checks"))?;

    // Check for duplicate dimension ID.
    if checks.iter().any(|c| c.dimension_id == check.dimension_id) {
        return Err(ApiError::conflict(format!(
            "Dimension '{}' is already registered",
            check.dimension_id
        )));
    }

    checks.push(check.clone());
    tracing::info!(
        check_name = %check.name,
        dimension = %check.dimension_id,
        "Custom safety check registered"
    );

    Ok(Json(check))
}

/// DELETE /api/safety/checks/:id — unregister a custom safety check.
pub async fn unregister_safety_check(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut checks = state
        .custom_safety_checks
        .write()
        .map_err(|_| ApiError::lock_poisoned("custom_safety_checks"))?;

    let original_len = checks.len();
    checks.retain(|c| c.id != id);

    if checks.len() == original_len {
        return Err(ApiError::not_found(format!("Safety check '{id}' not found")));
    }

    tracing::info!(check_id = %id, "Custom safety check unregistered");
    Ok(Json(serde_json::json!({ "unregistered": id })))
}
