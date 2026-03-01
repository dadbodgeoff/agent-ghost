//! Convergence profile CRUD endpoints (T-3.3.1).
//!
//! Manages convergence profiles (weight configurations for the 8-signal scorer).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSummary {
    pub name: String,
    pub description: String,
    pub is_preset: bool,
    pub weights: [f64; 8],
    pub thresholds: [f64; 4],
}

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    pub description: Option<String>,
    pub weights: [f64; 8],
    pub thresholds: [f64; 4],
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub weights: Option<[f64; 8]>,
    pub thresholds: Option<[f64; 4]>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProfileListResponse {
    pub profiles: Vec<ProfileSummary>,
}

#[derive(Debug, Deserialize)]
pub struct AssignProfileRequest {
    pub profile_name: String,
}

#[derive(Debug, Serialize)]
pub struct AssignProfileResponse {
    pub agent_id: String,
    pub profile_name: String,
}

// ── Presets ─────────────────────────────────────────────────────────

fn preset_profiles() -> Vec<ProfileSummary> {
    vec![
        ProfileSummary {
            name: "standard".into(),
            description: "Balanced scoring across all signals".into(),
            is_preset: true,
            weights: [0.125; 8],
            thresholds: [0.3, 0.5, 0.7, 0.85],
        },
        ProfileSummary {
            name: "research".into(),
            description: "Higher thresholds — more permissive for research agents".into(),
            is_preset: true,
            weights: [0.10, 0.10, 0.10, 0.15, 0.15, 0.15, 0.10, 0.15],
            thresholds: [0.4, 0.6, 0.8, 0.9],
        },
        ProfileSummary {
            name: "companion".into(),
            description: "Lower thresholds — more sensitive to convergence patterns".into(),
            is_preset: true,
            weights: [0.15, 0.15, 0.10, 0.15, 0.15, 0.10, 0.10, 0.10],
            thresholds: [0.2, 0.4, 0.6, 0.75],
        },
        ProfileSummary {
            name: "productivity".into(),
            description: "Task-focused — prioritizes goal boundary and initiative signals".into(),
            is_preset: true,
            weights: [0.05, 0.05, 0.10, 0.10, 0.25, 0.25, 0.10, 0.10],
            thresholds: [0.3, 0.5, 0.7, 0.85],
        },
    ]
}

// ── Handlers ────────────────────────────────────────────────────────

/// GET /api/profiles — list all profiles (presets + custom from DB).
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
) -> ApiResult<ProfileListResponse> {
    let mut profiles = preset_profiles();

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Load custom profiles stored in convergence_profiles table (if exists).
    let custom: Vec<ProfileSummary> = db
        .prepare(
            "SELECT name, description, weights, thresholds FROM convergence_profiles \
             ORDER BY name",
        )
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let description: String = row.get::<_, String>(1).unwrap_or_default();
                let weights_json: String = row.get::<_, String>(2).unwrap_or_default();
                let thresholds_json: String = row.get::<_, String>(3).unwrap_or_default();
                Ok((name, description, weights_json, thresholds_json))
            })?;
            Ok(rows
                .filter_map(|r| r.ok())
                .filter_map(|(name, desc, w, t)| {
                    let weights: [f64; 8] = serde_json::from_str(&w).ok()?;
                    let thresholds: [f64; 4] = serde_json::from_str(&t).ok()?;
                    Some(ProfileSummary {
                        name,
                        description: desc,
                        is_preset: false,
                        weights,
                        thresholds,
                    })
                })
                .collect())
        })
        .unwrap_or_default();

    profiles.extend(custom);
    Ok(Json(ProfileListResponse { profiles }))
}

/// POST /api/profiles — create a custom profile.
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProfileRequest>,
) -> ApiResult<ProfileSummary> {
    // Validate weights sum to ~1.0.
    let sum: f64 = req.weights.iter().sum();
    if (sum - 1.0).abs() > 0.01 {
        return Err(ApiError::bad_request(format!(
            "Weights must sum to 1.0 (got {sum:.3})"
        )));
    }

    // Check not a preset name.
    if ["standard", "research", "companion", "productivity"].contains(&req.name.as_str()) {
        return Err(ApiError::conflict("Cannot create profile with preset name"));
    }

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Table created by migration v025_convergence_profiles.

    // Validate individual weights are non-negative.
    if req.weights.iter().any(|&w| w < 0.0) {
        return Err(ApiError::bad_request("Weights must be non-negative"));
    }

    // T-5.6.1: Validate thresholds are in [0.0, 1.0] and monotonically increasing.
    for (i, &t) in req.thresholds.iter().enumerate() {
        if !(0.0..=1.0).contains(&t) {
            return Err(ApiError::bad_request(format!(
                "Threshold[{i}] = {t} is out of range [0.0, 1.0]"
            )));
        }
        if i > 0 && t <= req.thresholds[i - 1] {
            return Err(ApiError::bad_request(format!(
                "Thresholds must be monotonically increasing: threshold[{i}]={t} <= threshold[{}]={}",
                i - 1, req.thresholds[i - 1]
            )));
        }
    }

    let weights_json = serde_json::to_string(&req.weights)
        .map_err(|e| ApiError::internal(format!("serialize weights: {e}")))?;
    let thresholds_json = serde_json::to_string(&req.thresholds)
        .map_err(|e| ApiError::internal(format!("serialize thresholds: {e}")))?;

    db.execute(
        "INSERT INTO convergence_profiles (name, description, weights, thresholds) \
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            req.name,
            req.description.as_deref().unwrap_or(""),
            weights_json,
            thresholds_json,
        ],
    )
    .map_err(|e| ApiError::db_error("insert profile", e))?;

    Ok(Json(ProfileSummary {
        name: req.name,
        description: req.description.unwrap_or_default(),
        is_preset: false,
        weights: req.weights,
        thresholds: req.thresholds,
    }))
}

/// PUT /api/profiles/:name — update a custom profile's weights and thresholds.
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<UpdateProfileRequest>,
) -> ApiResult<ProfileSummary> {
    if ["standard", "research", "companion", "productivity"].contains(&name.as_str()) {
        return Err(ApiError::bad_request("Cannot modify preset profiles"));
    }

    if let Some(weights) = &req.weights {
        let sum: f64 = weights.iter().sum();
        if (sum - 1.0).abs() > 0.01 {
            return Err(ApiError::bad_request(format!(
                "Weights must sum to 1.0 (got {sum:.3})"
            )));
        }
    }

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Get current values.
    let (cur_desc, cur_weights, cur_thresholds): (String, String, String) = db
        .query_row(
            "SELECT description, weights, thresholds FROM convergence_profiles WHERE name = ?1",
            rusqlite::params![name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| ApiError::not_found(format!("Profile '{name}' not found")))?;

    let weights: [f64; 8] = match &req.weights {
        Some(w) => *w,
        None => serde_json::from_str(&cur_weights).unwrap_or([0.125; 8]),
    };
    let thresholds: [f64; 4] = match &req.thresholds {
        Some(t) => *t,
        None => serde_json::from_str(&cur_thresholds).unwrap_or([0.3, 0.5, 0.7, 0.85]),
    };
    let description = req.description.unwrap_or(cur_desc);

    let weights_json = serde_json::to_string(&weights)
        .map_err(|e| ApiError::internal(format!("serialize weights: {e}")))?;
    let thresholds_json = serde_json::to_string(&thresholds)
        .map_err(|e| ApiError::internal(format!("serialize thresholds: {e}")))?;

    db.execute(
        "UPDATE convergence_profiles SET description = ?1, weights = ?2, thresholds = ?3 WHERE name = ?4",
        rusqlite::params![description, weights_json, thresholds_json, name],
    )
    .map_err(|e| ApiError::db_error("update profile", e))?;

    Ok(Json(ProfileSummary {
        name,
        description,
        is_preset: false,
        weights,
        thresholds,
    }))
}

/// DELETE /api/profiles/:name — delete a custom profile.
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<serde_json::Value> {
    if ["standard", "research", "companion", "productivity"].contains(&name.as_str()) {
        return Err(ApiError::bad_request("Cannot delete preset profiles"));
    }

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    let affected = db
        .execute(
            "DELETE FROM convergence_profiles WHERE name = ?1",
            rusqlite::params![name],
        )
        .map_err(|e| ApiError::db_error("delete profile", e))?;

    if affected == 0 {
        return Err(ApiError::not_found(format!("Profile '{name}' not found")));
    }

    Ok(Json(serde_json::json!({ "deleted": name })))
}

/// POST /api/agents/:id/profile — assign a profile to an agent.
pub async fn assign_profile(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(req): Json<AssignProfileRequest>,
) -> ApiResult<AssignProfileResponse> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // T-5.6.1: Verify profile exists before assignment.
    let profile_exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM convergence_profiles WHERE name = ?1",
            rusqlite::params![req.profile_name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !profile_exists {
        // Also check preset names.
        let is_preset = ["standard", "research", "companion", "productivity"]
            .contains(&req.profile_name.as_str());
        if !is_preset {
            return Err(ApiError::bad_request(format!(
                "Profile '{}' does not exist",
                req.profile_name
            )));
        }
    }

    // Update the agent's profile in convergence_scores config.
    db.execute(
        "UPDATE convergence_scores SET profile = ?1 WHERE agent_id = ?2",
        rusqlite::params![req.profile_name, agent_id],
    )
    .map_err(|e| ApiError::db_error("assign profile", e))?;

    Ok(Json(AssignProfileResponse {
        agent_id,
        profile_name: req.profile_name,
    }))
}
