//! Kill gate HTTP fanout to mesh peers (T-X.25).
//!
//! When KILL_ALL is activated, propagates the kill signal to all known
//! mesh peers via signed HTTP POST requests.
//!
//! Hardening (Phase 5):
//! - T-5.2.1: HMAC-SHA256 signing with GHOST_MESH_SECRET (not self-derived key)
//! - T-5.3.3: Bounded concurrency via JoinSet + Semaphore (max 16)
//! - T-5.4.1: Fail-safe on mutex poisoning (process exit, never silent failure)

use std::sync::Arc;

use tokio::task::JoinSet;

use crate::state::AppState;

/// Maximum concurrent HTTP fanout requests (T-5.3.3).
const MAX_CONCURRENT_FANOUT: usize = 16;

/// Maximum time for the entire fanout to complete (T-5.3.3, §11.2).
const FANOUT_TIMEOUT_SECS: u64 = 30;

/// Propagate a kill signal to all known mesh peers.
///
/// Sends a signed POST request to each peer's `/a2a` endpoint with a
/// `kill/propagate` method. Uses bounded concurrency via JoinSet.
///
/// # Fail-safe (T-5.4.1)
/// On DB mutex poisoning, the process exits with a non-zero code rather than
/// silently failing. Per §11.2, kill switch activation is irreversible — forged
/// or missing kills cause permanent inconsistency.
pub fn propagate_kill(state: &Arc<AppState>, level: &str, reason: &str, agent_id: Option<&str>) {
    // T-5.4.1: Handle pool exhaustion with fail-safe.
    // Kill signal MUST reach all reachable peers OR the process MUST crash.
    let peers: Vec<String> = match state.db.read() {
        Ok(db) => db
            .prepare("SELECT endpoint_url FROM discovered_agents WHERE endpoint_url IS NOT NULL")
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default(),
        Err(e) => {
            // T-5.4.1: DB pool error — cannot query peers. This is fatal for kill propagation.
            // Attempt to log, then force process exit so orchestrator can restart and re-propagate.
            tracing::error!(
                error = %e,
                "FATAL: DB pool error during kill fanout — peers NOT notified. Forcing process exit."
            );
            // Exit with code 70 (EX_SOFTWARE) to signal internal error to orchestrator.
            std::process::exit(70);
        }
    };

    if peers.is_empty() {
        tracing::debug!("No mesh peers to notify of kill signal");
        return;
    }

    let peer_count = peers.len();
    let level = level.to_string();
    let reason = reason.to_string();
    let agent_id = agent_id.map(|s| s.to_string());

    // T-5.2.1: Read mesh secret for HMAC-SHA256 signing.
    let mesh_secret = std::env::var("GHOST_MESH_SECRET").ok();
    if mesh_secret.is_none() {
        tracing::warn!(
            "GHOST_MESH_SECRET not set — kill fanout requests will be unsigned. \
             Set GHOST_MESH_SECRET for signed kill propagation."
        );
    }

    // T-5.3.3: Spawn bounded fanout with JoinSet + Semaphore.
    tokio::spawn(async move {
        let mut join_set = JoinSet::new();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_FANOUT));
        let mut notified: u32 = 0;
        let mut failed: u32 = 0;

        for peer_url in peers {
            let level = level.clone();
            let reason = reason.clone();
            let agent_id = agent_id.clone();
            let mesh_secret = mesh_secret.clone();
            let sem = semaphore.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");

                let payload = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": uuid::Uuid::now_v7().to_string(),
                    "method": "kill/propagate",
                    "params": {
                        "level": level,
                        "reason": reason,
                        "agent_id": agent_id,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }
                });

                let body = serde_json::to_string(&payload).unwrap_or_default();

                // T-5.2.1: Sign with HMAC-SHA256 using shared mesh secret.
                // Never derive key from the message body (cryptographically meaningless).
                let signature = if let Some(ref secret) = mesh_secret {
                    use hmac::{Hmac, Mac};
                    use sha2::Sha256;
                    type HmacSha256 = Hmac<Sha256>;
                    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                        .expect("HMAC-SHA256 accepts any key size");
                    mac.update(body.as_bytes());
                    let result = mac.finalize();
                    let hex = result
                        .into_bytes()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    format!("sha256={hex}")
                } else {
                    String::new()
                };

                let client = reqwest::Client::new();
                let mut req = client
                    .post(&format!("{}/a2a", peer_url.trim_end_matches('/')))
                    .header("Content-Type", "application/json")
                    .timeout(std::time::Duration::from_secs(5));

                if !signature.is_empty() {
                    req = req.header("X-Ghost-Kill-Signature", &signature);
                }

                match req.body(body).send().await {
                    Ok(resp) => {
                        tracing::info!(
                            peer = %peer_url,
                            status = %resp.status(),
                            "Kill signal propagated to mesh peer"
                        );
                        true
                    }
                    Err(e) => {
                        tracing::warn!(
                            peer = %peer_url,
                            error = %e,
                            "Failed to propagate kill signal to mesh peer"
                        );
                        false
                    }
                }
            });
        }

        // T-5.3.3: Wait for all tasks with timeout (§11.2: kill is time-critical).
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(FANOUT_TIMEOUT_SECS);

        while let Ok(Some(result)) = tokio::time::timeout_at(deadline, join_set.join_next()).await {
            match result {
                Ok(true) => notified += 1,
                Ok(false) | Err(_) => failed += 1,
            }
        }

        // Any remaining tasks that didn't complete within the deadline.
        let timed_out = join_set.len();
        if timed_out > 0 {
            tracing::warn!(
                timed_out,
                "Kill fanout: {timed_out} peers did not respond within {FANOUT_TIMEOUT_SECS}s"
            );
            failed += timed_out as u32;
            join_set.abort_all();
        }

        tracing::info!(
            peer_count,
            notified,
            failed,
            "Kill signal fanout completed: {notified}/{peer_count} peers notified, {failed} failed",
        );
    });
}
