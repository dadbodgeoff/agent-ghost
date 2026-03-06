//! Convergence propagation for delegation hierarchies.
//!
//! When a child agent's convergence score changes, the effect propagates
//! upward to its parent:
//!
//! - Parent adjusted: `max(parent_score, child_score * 0.5)`
//! - Boundary violation: child quarantined, parent += 0.1 penalty
//! - Recursion terminates via monotonic `max()` on parent score
//!
//! This module is called by:
//! - The convergence watcher (background task) on periodic score updates
//! - The boundary violation handler when a child transgresses

use uuid::Uuid;

/// Result of a convergence propagation operation.
#[derive(Debug, Clone)]
pub struct PropagationResult {
    /// Parent agent affected (if any).
    pub parent_agent_id: Option<String>,
    /// Parent's score before propagation.
    pub parent_old_score: f64,
    /// Parent's score after propagation.
    pub parent_new_score: f64,
    /// Number of children re-evaluated.
    pub children_re_evaluated: u32,
    /// Agent IDs quarantined during this propagation.
    pub quarantined: Vec<String>,
}

/// Propagate a child's convergence score change upward to its parent.
///
/// Returns `Ok(result)` with propagation details, or `Ok` with no parent
/// if the child has no active convergence link.
pub fn propagate_convergence(
    conn: &rusqlite::Connection,
    child_agent_id: &str,
    new_child_score: f64,
) -> Result<PropagationResult, String> {
    // Find the parent link
    let link = cortex_storage::queries::convergence_propagation_queries::get_parent(
        conn,
        child_agent_id,
    )
    .map_err(|e| format!("query parent link: {e}"))?;

    let link = match link {
        Some(l) => l,
        None => {
            return Ok(PropagationResult {
                parent_agent_id: None,
                parent_old_score: 0.0,
                parent_new_score: 0.0,
                children_re_evaluated: 0,
                quarantined: vec![],
            });
        }
    };

    // Rule: child convergence can only go UP from inherited score
    // (We don't reject the new score — the child's actual score may legitimately
    // be below inherited due to fresh computation. We just don't propagate
    // downward pressure to the parent.)
    if new_child_score <= link.inherited_score {
        return Ok(PropagationResult {
            parent_agent_id: Some(link.parent_agent_id),
            parent_old_score: 0.0,
            parent_new_score: 0.0,
            children_re_evaluated: 0,
            quarantined: vec![],
        });
    }

    // Get parent's current score
    let parent_score_row =
        cortex_storage::queries::convergence_score_queries::latest_by_agent(
            conn,
            &link.parent_agent_id,
        )
        .map_err(|e| format!("query parent convergence: {e}"))?;

    let parent_old_score = parent_score_row
        .as_ref()
        .map(|r| r.composite_score)
        .unwrap_or(0.0);

    // Rule: parent adjusted = max(parent_score, child_score * 0.5)
    let child_impact = new_child_score * 0.5;
    let parent_new_score = parent_old_score.max(child_impact);

    // Only insert a new score if it actually changed
    if (parent_new_score - parent_old_score).abs() < 0.001 {
        return Ok(PropagationResult {
            parent_agent_id: Some(link.parent_agent_id),
            parent_old_score,
            parent_new_score: parent_old_score,
            children_re_evaluated: 0,
            quarantined: vec![],
        });
    }

    // Insert new convergence score for parent
    let score_id = Uuid::now_v7().to_string();
    let profile = parent_score_row
        .as_ref()
        .map(|r| r.profile.clone())
        .unwrap_or_else(|| "standard".to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let hash = crate::delegation_skills::compute_event_hash(
        format!("propagation:{child_agent_id}:{new_child_score}:{now}").as_bytes(),
    );
    let prev_hash = vec![0u8; 32];

    let signal_scores = serde_json::json!({
        "source": "delegation_propagation",
        "child_agent_id": child_agent_id,
        "child_score": new_child_score,
    })
    .to_string();

    let level = if parent_new_score >= 0.7 {
        3
    } else if parent_new_score >= 0.5 {
        2
    } else if parent_new_score >= 0.3 {
        1
    } else {
        0
    };

    cortex_storage::queries::convergence_score_queries::insert_score(
        conn,
        &score_id,
        &link.parent_agent_id,
        None,
        parent_new_score,
        &signal_scores,
        level,
        &profile,
        &now,
        &hash,
        &prev_hash,
    )
    .map_err(|e| format!("insert parent score: {e}"))?;

    Ok(PropagationResult {
        parent_agent_id: Some(link.parent_agent_id),
        parent_old_score,
        parent_new_score,
        children_re_evaluated: 0,
        quarantined: vec![],
    })
}

/// Handle a boundary violation by a child agent.
///
/// - Quarantines the child in the convergence link table
/// - Applies a +0.1 penalty to the parent's convergence score
pub fn handle_boundary_violation(
    conn: &rusqlite::Connection,
    child_agent_id: &str,
    reason: &str,
) -> Result<PropagationResult, String> {
    // Quarantine the child
    cortex_storage::queries::convergence_propagation_queries::quarantine_child(
        conn,
        child_agent_id,
        reason,
    )
    .map_err(|e| format!("quarantine child: {e}"))?;

    // Find parent
    // (Note: quarantine_child sets status='quarantined', so get_parent won't
    //  find active links. We need to query before quarantine or use a broader query.
    //  Since quarantine already happened, let's query directly.)
    let parent_link: Option<cortex_storage::queries::convergence_propagation_queries::ConvergenceLinkRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, parent_agent_id, child_agent_id, delegation_id,
                        inherited_score, inherited_level, status, created_at
                 FROM convergence_links
                 WHERE child_agent_id = ?1
                 ORDER BY created_at DESC LIMIT 1",
            )
            .map_err(|e| format!("prepare parent query: {e}"))?;

        let rows: Vec<_> = stmt
            .query_map(rusqlite::params![child_agent_id], |row| {
                Ok(cortex_storage::queries::convergence_propagation_queries::ConvergenceLinkRow {
                    id: row.get(0)?,
                    parent_agent_id: row.get(1)?,
                    child_agent_id: row.get(2)?,
                    delegation_id: row.get(3)?,
                    inherited_score: row.get(4)?,
                    inherited_level: row.get(5)?,
                    status: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| format!("query parent: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect parent rows: {e}"))?;

        rows.into_iter().next()
    };

    let link = match parent_link {
        Some(l) => l,
        None => {
            return Ok(PropagationResult {
                parent_agent_id: None,
                parent_old_score: 0.0,
                parent_new_score: 0.0,
                children_re_evaluated: 0,
                quarantined: vec![child_agent_id.to_string()],
            });
        }
    };

    // Apply +0.1 penalty to parent
    let parent_score_row =
        cortex_storage::queries::convergence_score_queries::latest_by_agent(
            conn,
            &link.parent_agent_id,
        )
        .map_err(|e| format!("query parent convergence: {e}"))?;

    let parent_old_score = parent_score_row
        .as_ref()
        .map(|r| r.composite_score)
        .unwrap_or(0.0);

    let parent_new_score = (parent_old_score + 0.1).min(1.0);

    let score_id = Uuid::now_v7().to_string();
    let profile = parent_score_row
        .as_ref()
        .map(|r| r.profile.clone())
        .unwrap_or_else(|| "standard".to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let hash = crate::delegation_skills::compute_event_hash(
        format!("violation_penalty:{child_agent_id}:{reason}:{now}").as_bytes(),
    );
    let prev_hash = vec![0u8; 32];

    let signal_scores = serde_json::json!({
        "source": "boundary_violation_penalty",
        "child_agent_id": child_agent_id,
        "reason": reason,
        "penalty": 0.1,
    })
    .to_string();

    let level = if parent_new_score >= 0.7 {
        3
    } else if parent_new_score >= 0.5 {
        2
    } else if parent_new_score >= 0.3 {
        1
    } else {
        0
    };

    cortex_storage::queries::convergence_score_queries::insert_score(
        conn,
        &score_id,
        &link.parent_agent_id,
        None,
        parent_new_score,
        &signal_scores,
        level,
        &profile,
        &now,
        &hash,
        &prev_hash,
    )
    .map_err(|e| format!("insert penalty score: {e}"))?;

    Ok(PropagationResult {
        parent_agent_id: Some(link.parent_agent_id),
        parent_old_score,
        parent_new_score,
        children_re_evaluated: 0,
        quarantined: vec![child_agent_id.to_string()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn seed_parent_score(db: &rusqlite::Connection, agent_id: &str, score: f64) {
        let hash = vec![0u8; 32];
        cortex_storage::queries::convergence_score_queries::insert_score(
            db,
            &Uuid::now_v7().to_string(),
            agent_id,
            None,
            score,
            "{}",
            0,
            "standard",
            &chrono::Utc::now().to_rfc3339(),
            &hash,
            &hash,
        )
        .unwrap();
    }

    fn link_agents(
        db: &rusqlite::Connection,
        parent_id: &str,
        child_id: &str,
        inherited_score: f64,
    ) -> String {
        let delegation_id = Uuid::now_v7().to_string();
        cortex_storage::queries::convergence_propagation_queries::link_parent_child(
            db,
            &Uuid::now_v7().to_string(),
            parent_id,
            child_id,
            &delegation_id,
            inherited_score,
            0,
        )
        .unwrap();
        delegation_id
    }

    #[test]
    fn child_score_rise_propagates_to_parent() {
        let db = test_db();
        let parent = Uuid::now_v7().to_string();
        let child = Uuid::now_v7().to_string();

        seed_parent_score(&db, &parent, 0.2);
        link_agents(&db, &parent, &child, 0.2);

        // Child score rises to 0.6 → parent should get max(0.2, 0.6 * 0.5) = 0.3
        let result = propagate_convergence(&db, &child, 0.6).unwrap();

        assert_eq!(result.parent_agent_id.as_deref(), Some(parent.as_str()));
        assert!((result.parent_old_score - 0.2).abs() < 0.001);
        assert!((result.parent_new_score - 0.3).abs() < 0.001);
    }

    #[test]
    fn no_propagation_when_child_below_inherited() {
        let db = test_db();
        let parent = Uuid::now_v7().to_string();
        let child = Uuid::now_v7().to_string();

        seed_parent_score(&db, &parent, 0.2);
        link_agents(&db, &parent, &child, 0.2);

        // Child score at 0.1 (below inherited 0.2) → no propagation
        let result = propagate_convergence(&db, &child, 0.1).unwrap();

        assert_eq!(result.parent_new_score, 0.0); // No change attempted
    }

    #[test]
    fn no_propagation_without_parent_link() {
        let db = test_db();
        let orphan = Uuid::now_v7().to_string();

        let result = propagate_convergence(&db, &orphan, 0.5).unwrap();
        assert!(result.parent_agent_id.is_none());
    }

    #[test]
    fn boundary_violation_quarantines_and_penalizes() {
        let db = test_db();
        let parent = Uuid::now_v7().to_string();
        let child = Uuid::now_v7().to_string();

        seed_parent_score(&db, &parent, 0.2);
        link_agents(&db, &parent, &child, 0.2);

        let result =
            handle_boundary_violation(&db, &child, "attempted blocked action").unwrap();

        assert_eq!(result.quarantined, vec![child.clone()]);
        assert!((result.parent_old_score - 0.2).abs() < 0.001);
        assert!((result.parent_new_score - 0.3).abs() < 0.001); // 0.2 + 0.1

        // Verify child is quarantined in DB
        let link =
            cortex_storage::queries::convergence_propagation_queries::get_children(
                &db, &parent,
            )
            .unwrap();
        // get_children only returns active links, so should be empty
        assert!(link.is_empty());
    }

    #[test]
    fn penalty_caps_at_one() {
        let db = test_db();
        let parent = Uuid::now_v7().to_string();
        let child = Uuid::now_v7().to_string();

        seed_parent_score(&db, &parent, 0.95);
        link_agents(&db, &parent, &child, 0.1);

        let result = handle_boundary_violation(&db, &child, "test").unwrap();
        assert!((result.parent_new_score - 1.0).abs() < 0.001); // Capped at 1.0
    }
}
