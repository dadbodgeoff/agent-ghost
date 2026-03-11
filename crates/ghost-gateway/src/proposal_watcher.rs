//! Proposal watcher — polls for newly created proposals and broadcasts
//! canonical proposal creation events for ADE/operator consumers.

use std::sync::Arc;

use crate::api::goals::canonical_status_from_parts;
use crate::api::websocket::{broadcast_event, WsEvent};
use crate::state::AppState;

#[derive(Clone, Debug)]
struct ProposalWatermark {
    created_at: String,
    id: String,
}

pub async fn proposal_watcher_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    let mut watermark = load_latest_watermark(&state).await;

    loop {
        interval.tick().await;

        let db = match state.db.read() {
            Ok(db) => db,
            Err(error) => {
                tracing::warn!(error = %error, "proposal_watcher: failed to acquire DB reader");
                continue;
            }
        };

        let Some(ref current) = watermark else {
            drop(db);
            watermark = load_latest_watermark(&state).await;
            continue;
        };

        let mut stmt = match db.prepare(
            "SELECT gp.id, gp.agent_id, gp.created_at, gp.decision, gp.resolved_at,
                    (SELECT to_state
                     FROM goal_proposal_transitions t
                     WHERE t.proposal_id = gp.id
                     ORDER BY rowid DESC
                     LIMIT 1),
                    v2.supersedes_proposal_id
             FROM goal_proposals gp
             LEFT JOIN goal_proposals_v2 v2 ON v2.id = gp.id
             WHERE gp.created_at > ?1
                OR (gp.created_at = ?1 AND gp.id > ?2)
             ORDER BY gp.created_at ASC, gp.id ASC",
        ) {
            Ok(stmt) => stmt,
            Err(error) => {
                tracing::warn!(error = %error, "proposal_watcher: failed to prepare query");
                continue;
            }
        };

        let rows = match stmt.query_map([&current.created_at, &current.id], |row| {
            let decision = row.get::<_, Option<String>>(3)?;
            let resolved_at = row.get::<_, Option<String>>(4)?;
            let current_state = row.get::<_, Option<String>>(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                canonical_status_from_parts(
                    current_state.as_deref(),
                    decision.as_deref(),
                    resolved_at.as_deref(),
                ),
                row.get::<_, Option<String>>(6)?,
            ))
        }) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(error = %error, "proposal_watcher: query failed");
                continue;
            }
        };

        let mut latest = watermark.clone();
        for row in rows {
            let (proposal_id, agent_id, created_at, status, supersedes_proposal_id) = match row {
                Ok(row) => row,
                Err(error) => {
                    tracing::warn!(error = %error, "proposal_watcher: skipping malformed row");
                    continue;
                }
            };

            latest = Some(ProposalWatermark {
                created_at: created_at.clone(),
                id: proposal_id.clone(),
            });

            broadcast_event(
                &state,
                WsEvent::ProposalUpdated {
                    proposal_id,
                    agent_id,
                    status,
                    change: "created".into(),
                    supersedes_proposal_id,
                },
            );
        }

        watermark = latest;
    }
}

async fn load_latest_watermark(state: &Arc<AppState>) -> Option<ProposalWatermark> {
    let db = state.db.read().ok()?;
    db.query_row(
        "SELECT id, created_at
         FROM goal_proposals
         ORDER BY created_at DESC, id DESC
         LIMIT 1",
        [],
        |row| {
            Ok(ProposalWatermark {
                id: row.get(0)?,
                created_at: row.get(1)?,
            })
        },
    )
    .ok()
}
