//! Marketplace queries (v038 marketplace tables).

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

// ── Agent Listings ──

pub fn upsert_agent_listing(
    conn: &Connection,
    agent_id: &str,
    description: &str,
    capabilities: &str,
    pricing_model: &str,
    base_price: i64,
    endpoint_url: Option<&str>,
    public_key: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO marketplace_agent_listings
            (agent_id, description, capabilities, pricing_model, base_price, endpoint_url, public_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(agent_id) DO UPDATE SET
            description = ?2, capabilities = ?3, pricing_model = ?4,
            base_price = ?5, endpoint_url = COALESCE(?6, endpoint_url),
            public_key = COALESCE(?7, public_key),
            updated_at = datetime('now')",
        params![agent_id, description, capabilities, pricing_model, base_price, endpoint_url, public_key],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn update_agent_listing_status(
    conn: &Connection,
    agent_id: &str,
    status: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE marketplace_agent_listings SET status = ?2, updated_at = datetime('now')
             WHERE agent_id = ?1",
            params![agent_id, status],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn get_agent_listing(
    conn: &Connection,
    agent_id: &str,
) -> CortexResult<Option<AgentListingRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT agent_id, description, capabilities, pricing_model, base_price,
                    trust_score, total_completed, total_failed, average_rating,
                    total_reviews, status, endpoint_url, public_key, created_at, updated_at
             FROM marketplace_agent_listings WHERE agent_id = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![agent_id], map_agent_listing_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows.pop())
}

pub fn list_agent_listings(
    conn: &Connection,
    status: Option<&str>,
    min_trust: Option<f64>,
    min_rating: Option<f64>,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<AgentListingRow>> {
    let mut sql = String::from(
        "SELECT agent_id, description, capabilities, pricing_model, base_price,
                trust_score, total_completed, total_failed, average_rating,
                total_reviews, status, endpoint_url, public_key, created_at, updated_at
         FROM marketplace_agent_listings WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(s) = status {
        sql.push_str(&format!(" AND status = ?{idx}"));
        param_values.push(Box::new(s.to_string()));
        idx += 1;
    }
    if let Some(t) = min_trust {
        sql.push_str(&format!(" AND trust_score >= ?{idx}"));
        param_values.push(Box::new(t));
        idx += 1;
    }
    if let Some(r) = min_rating {
        sql.push_str(&format!(" AND average_rating >= ?{idx}"));
        param_values.push(Box::new(r));
        idx += 1;
    }
    sql.push_str(&format!(" ORDER BY trust_score DESC LIMIT ?{idx} OFFSET ?{}", idx + 1));
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params_refs.as_slice(), map_agent_listing_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn update_agent_listing_stats(
    conn: &Connection,
    agent_id: &str,
    trust_score: f64,
    total_completed: i64,
    total_failed: i64,
    average_rating: f64,
    total_reviews: i64,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE marketplace_agent_listings SET
                trust_score = ?2, total_completed = ?3, total_failed = ?4,
                average_rating = ?5, total_reviews = ?6, updated_at = datetime('now')
             WHERE agent_id = ?1",
            params![agent_id, trust_score, total_completed, total_failed, average_rating, total_reviews],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

// ── Skill Listings ──

pub fn upsert_skill_listing(
    conn: &Connection,
    skill_name: &str,
    version: &str,
    author_agent_id: Option<&str>,
    description: &str,
    signature: Option<&str>,
    price_credits: i64,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO marketplace_skill_listings
            (skill_name, version, author_agent_id, description, signature, price_credits)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(skill_name) DO UPDATE SET
            version = ?2, description = ?4, signature = COALESCE(?5, signature),
            price_credits = ?6",
        params![skill_name, version, author_agent_id, description, signature, price_credits],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn list_skill_listings(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<SkillListingRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT skill_name, version, author_agent_id, description, signature,
                    price_credits, install_count, average_rating, created_at
             FROM marketplace_skill_listings
             ORDER BY install_count DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![limit, offset], |row| {
            Ok(SkillListingRow {
                skill_name: row.get(0)?,
                version: row.get(1)?,
                author_agent_id: row.get(2)?,
                description: row.get(3)?,
                signature: row.get(4)?,
                price_credits: row.get(5)?,
                install_count: row.get(6)?,
                average_rating: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn get_skill_listing(
    conn: &Connection,
    skill_name: &str,
) -> CortexResult<Option<SkillListingRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT skill_name, version, author_agent_id, description, signature,
                    price_credits, install_count, average_rating, created_at
             FROM marketplace_skill_listings WHERE skill_name = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![skill_name], |row| {
            Ok(SkillListingRow {
                skill_name: row.get(0)?,
                version: row.get(1)?,
                author_agent_id: row.get(2)?,
                description: row.get(3)?,
                signature: row.get(4)?,
                price_credits: row.get(5)?,
                install_count: row.get(6)?,
                average_rating: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows.pop())
}

// ── Contracts ──

pub fn insert_contract(
    conn: &Connection,
    id: &str,
    hirer_agent_id: &str,
    worker_agent_id: &str,
    task_description: &str,
    agreed_price: i64,
    max_duration_secs: Option<i64>,
    escrow_id: Option<&str>,
    event_hash: Option<&str>,
    previous_hash: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO marketplace_contracts
            (id, hirer_agent_id, worker_agent_id, task_description, agreed_price,
             max_duration_secs, escrow_id, event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id, hirer_agent_id, worker_agent_id, task_description, agreed_price,
            max_duration_secs, escrow_id, event_hash, previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn transition_contract(
    conn: &Connection,
    id: &str,
    new_state: &str,
    delegation_id: Option<&str>,
    mesh_task_id: Option<&str>,
    result: Option<&str>,
    event_hash: Option<&str>,
    previous_hash: Option<&str>,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE marketplace_contracts SET
                state = ?2,
                delegation_id = COALESCE(?3, delegation_id),
                mesh_task_id = COALESCE(?4, mesh_task_id),
                result = COALESCE(?5, result),
                event_hash = COALESCE(?6, event_hash),
                previous_hash = COALESCE(?7, previous_hash),
                updated_at = datetime('now')
             WHERE id = ?1",
            params![id, new_state, delegation_id, mesh_task_id, result, event_hash, previous_hash],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn get_contract(
    conn: &Connection,
    id: &str,
) -> CortexResult<Option<ContractRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, hirer_agent_id, worker_agent_id, state, task_description,
                    agreed_price, max_duration_secs, delegation_id, mesh_task_id,
                    escrow_id, result, event_hash, previous_hash, created_at, updated_at
             FROM marketplace_contracts WHERE id = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![id], map_contract_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows.pop())
}

pub fn list_contracts(
    conn: &Connection,
    agent_id: Option<&str>,
    state: Option<&str>,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<ContractRow>> {
    let mut sql = String::from(
        "SELECT id, hirer_agent_id, worker_agent_id, state, task_description,
                agreed_price, max_duration_secs, delegation_id, mesh_task_id,
                escrow_id, result, event_hash, previous_hash, created_at, updated_at
         FROM marketplace_contracts WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(a) = agent_id {
        sql.push_str(&format!(" AND (hirer_agent_id = ?{idx} OR worker_agent_id = ?{idx})"));
        param_values.push(Box::new(a.to_string()));
        idx += 1;
    }
    if let Some(s) = state {
        sql.push_str(&format!(" AND state = ?{idx}"));
        param_values.push(Box::new(s.to_string()));
        idx += 1;
    }
    sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{idx} OFFSET ?{}", idx + 1));
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params_refs.as_slice(), map_contract_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

// ── Credit Wallets ──

pub fn ensure_wallet(conn: &Connection, agent_id: &str) -> CortexResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO credit_wallets (agent_id) VALUES (?1)",
        params![agent_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_wallet(conn: &Connection, agent_id: &str) -> CortexResult<Option<WalletRow>> {
    let result = conn.query_row(
        "SELECT agent_id, balance, escrowed, total_earned, total_spent, created_at
         FROM credit_wallets WHERE agent_id = ?1",
        params![agent_id],
        |row| {
            Ok(WalletRow {
                agent_id: row.get(0)?,
                balance: row.get(1)?,
                escrowed: row.get(2)?,
                total_earned: row.get(3)?,
                total_spent: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    );
    match result {
        Ok(w) => Ok(Some(w)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(to_storage_err(e.to_string())),
    }
}

pub fn seed_wallet(conn: &Connection, agent_id: &str, amount: i64) -> CortexResult<()> {
    if amount <= 0 {
        return Err(to_storage_err("seed amount must be positive".to_string()));
    }
    conn.execute("BEGIN IMMEDIATE", []).map_err(|e| to_storage_err(e.to_string()))?;
    let result = (|| {
        ensure_wallet(conn, agent_id)?;
        conn.execute(
            "UPDATE credit_wallets SET balance = balance + ?2, total_earned = total_earned + ?2
             WHERE agent_id = ?1",
            params![agent_id, amount],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
        conn.execute(
            "INSERT INTO credit_transactions (to_agent, amount, tx_type, reference_id)
             VALUES (?1, ?2, 'seed', 'alpha_seed')",
            params![agent_id, amount],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
        Ok(())
    })();
    match &result {
        Ok(()) => { conn.execute("COMMIT", []).map_err(|e| to_storage_err(e.to_string()))?; }
        Err(_) => { let _ = conn.execute("ROLLBACK", []); }
    }
    result
}

pub fn create_escrow(
    conn: &Connection,
    escrow_id: &str,
    contract_id: &str,
    depositor: &str,
    beneficiary: &str,
    amount: i64,
) -> CortexResult<()> {
    if amount <= 0 {
        return Err(to_storage_err("escrow amount must be positive".to_string()));
    }
    conn.execute("BEGIN IMMEDIATE", []).map_err(|e| to_storage_err(e.to_string()))?;
    let result = (|| {
        // Debit depositor's available balance
        let updated = conn
            .execute(
                "UPDATE credit_wallets SET balance = balance - ?2, escrowed = escrowed + ?2
                 WHERE agent_id = ?1 AND balance >= ?2",
                params![depositor, amount],
            )
            .map_err(|e| to_storage_err(e.to_string()))?;

        if updated == 0 {
            return Err(to_storage_err("insufficient balance for escrow".to_string()));
        }

        conn.execute(
            "INSERT INTO credit_escrows (id, contract_id, depositor, beneficiary, amount)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![escrow_id, contract_id, depositor, beneficiary, amount],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "INSERT INTO credit_transactions (from_agent, amount, tx_type, reference_id)
             VALUES (?1, ?2, 'escrow_hold', ?3)",
            params![depositor, amount, contract_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        Ok(())
    })();
    match &result {
        Ok(()) => { conn.execute("COMMIT", []).map_err(|e| to_storage_err(e.to_string()))?; }
        Err(_) => { let _ = conn.execute("ROLLBACK", []); }
    }
    result
}

pub fn release_escrow(conn: &Connection, escrow_id: &str) -> CortexResult<()> {
    conn.execute("BEGIN IMMEDIATE", []).map_err(|e| to_storage_err(e.to_string()))?;
    let result = (|| {
        let (depositor, beneficiary, amount): (String, String, i64) = conn
            .query_row(
                "SELECT depositor, beneficiary, amount FROM credit_escrows
                 WHERE id = ?1 AND status = 'held'",
                params![escrow_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| to_storage_err(e.to_string()))?;

        // Release funds to beneficiary
        conn.execute(
            "UPDATE credit_wallets SET escrowed = escrowed - ?2 WHERE agent_id = ?1",
            params![depositor, amount],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        ensure_wallet(conn, &beneficiary)?;
        conn.execute(
            "UPDATE credit_wallets SET balance = balance + ?2, total_earned = total_earned + ?2
             WHERE agent_id = ?1",
            params![beneficiary, amount],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "UPDATE credit_wallets SET total_spent = total_spent + ?2 WHERE agent_id = ?1",
            params![depositor, amount],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "UPDATE credit_escrows SET status = 'released' WHERE id = ?1",
            params![escrow_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "INSERT INTO credit_transactions (from_agent, to_agent, amount, tx_type, reference_id)
             VALUES (?1, ?2, ?3, 'escrow_release', ?4)",
            params![depositor, beneficiary, amount, escrow_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        Ok(())
    })();
    match &result {
        Ok(()) => { conn.execute("COMMIT", []).map_err(|e| to_storage_err(e.to_string()))?; }
        Err(_) => { let _ = conn.execute("ROLLBACK", []); }
    }
    result
}

pub fn refund_escrow(conn: &Connection, escrow_id: &str) -> CortexResult<()> {
    conn.execute("BEGIN IMMEDIATE", []).map_err(|e| to_storage_err(e.to_string()))?;
    let result = (|| {
        let (depositor, amount): (String, i64) = conn
            .query_row(
                "SELECT depositor, amount FROM credit_escrows
                 WHERE id = ?1 AND status = 'held'",
                params![escrow_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "UPDATE credit_wallets SET balance = balance + ?2, escrowed = escrowed - ?2
             WHERE agent_id = ?1",
            params![depositor, amount],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "UPDATE credit_escrows SET status = 'refunded' WHERE id = ?1",
            params![escrow_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "INSERT INTO credit_transactions (to_agent, amount, tx_type, reference_id)
             VALUES (?1, ?2, 'escrow_refund', ?3)",
            params![depositor, amount, escrow_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        Ok(())
    })();
    match &result {
        Ok(()) => { conn.execute("COMMIT", []).map_err(|e| to_storage_err(e.to_string()))?; }
        Err(_) => { let _ = conn.execute("ROLLBACK", []); }
    }
    result
}

pub fn list_transactions(
    conn: &Connection,
    agent_id: &str,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<TransactionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, from_agent, to_agent, amount, tx_type, reference_id, created_at
             FROM credit_transactions
             WHERE from_agent = ?1 OR to_agent = ?1
             ORDER BY id DESC LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id, limit, offset], |row| {
            Ok(TransactionRow {
                id: row.get(0)?,
                from_agent: row.get(1)?,
                to_agent: row.get(2)?,
                amount: row.get(3)?,
                tx_type: row.get(4)?,
                reference_id: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

// ── Listing Stats Helpers ──

pub fn increment_completed(conn: &Connection, agent_id: &str) -> CortexResult<()> {
    conn.execute(
        "UPDATE marketplace_agent_listings SET total_completed = total_completed + 1,
         updated_at = datetime('now') WHERE agent_id = ?1",
        params![agent_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn increment_failed(conn: &Connection, agent_id: &str) -> CortexResult<()> {
    conn.execute(
        "UPDATE marketplace_agent_listings SET total_failed = total_failed + 1,
         updated_at = datetime('now') WHERE agent_id = ?1",
        params![agent_id],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

// ── Reviews ──

pub fn insert_review(
    conn: &Connection,
    contract_id: &str,
    reviewer_agent_id: &str,
    reviewee_agent_id: &str,
    rating: i32,
    comment: &str,
) -> CortexResult<()> {
    conn.execute("BEGIN IMMEDIATE", []).map_err(|e| to_storage_err(e.to_string()))?;
    let result = (|| {
        conn.execute(
            "INSERT INTO marketplace_reviews
                (contract_id, reviewer_agent_id, reviewee_agent_id, rating, comment)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![contract_id, reviewer_agent_id, reviewee_agent_id, rating, comment],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        // Update agent listing average rating
        conn.execute(
            "UPDATE marketplace_agent_listings SET
                average_rating = (
                    SELECT AVG(CAST(rating AS REAL))
                    FROM marketplace_reviews WHERE reviewee_agent_id = ?1
                ),
                total_reviews = (
                    SELECT COUNT(*) FROM marketplace_reviews WHERE reviewee_agent_id = ?1
                ),
                updated_at = datetime('now')
             WHERE agent_id = ?1",
            params![reviewee_agent_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        Ok(())
    })();
    match &result {
        Ok(()) => { conn.execute("COMMIT", []).map_err(|e| to_storage_err(e.to_string()))?; }
        Err(_) => { let _ = conn.execute("ROLLBACK", []); }
    }
    result
}

pub fn list_reviews_for_agent(
    conn: &Connection,
    agent_id: &str,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<ReviewRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, contract_id, reviewer_agent_id, reviewee_agent_id, rating, comment, created_at
             FROM marketplace_reviews WHERE reviewee_agent_id = ?1
             ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id, limit, offset], |row| {
            Ok(ReviewRow {
                id: row.get(0)?,
                contract_id: row.get(1)?,
                reviewer_agent_id: row.get(2)?,
                reviewee_agent_id: row.get(3)?,
                rating: row.get(4)?,
                comment: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

// ── Discovery ──

pub fn discover_agents(
    conn: &Connection,
    capabilities: &[String],
    min_trust: f64,
    min_rating: f64,
    max_price: Option<i64>,
    limit: u32,
) -> CortexResult<Vec<AgentListingRow>> {
    let mut sql = String::from(
        "SELECT agent_id, description, capabilities, pricing_model, base_price,
                trust_score, total_completed, total_failed, average_rating,
                total_reviews, status, endpoint_url, public_key, created_at, updated_at
         FROM marketplace_agent_listings
         WHERE status = 'active' AND trust_score >= ?1 AND average_rating >= ?2",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(min_trust));
    param_values.push(Box::new(min_rating));
    let mut idx = 3;

    // Each capability gets its own LIKE clause matching the JSON-quoted element.
    // e.g. capabilities LIKE '%"code_review"%' matches '["code_review","summarization"]'
    for cap in capabilities {
        if !cap.is_empty() {
            sql.push_str(&format!(" AND capabilities LIKE ?{idx}"));
            param_values.push(Box::new(format!("%\"{cap}\"%")));
            idx += 1;
        }
    }
    if let Some(mp) = max_price {
        sql.push_str(&format!(" AND base_price <= ?{idx}"));
        param_values.push(Box::new(mp));
        idx += 1;
    }

    // Composite score: trust * 0.4 + rating/5 * 0.3 + completed/(completed+failed+1) * 0.2 + (1 - base_price/10000) * 0.1
    sql.push_str(
        " ORDER BY (trust_score * 0.4 + average_rating / 5.0 * 0.3 +
            CAST(total_completed AS REAL) / (total_completed + total_failed + 1) * 0.2 +
            (1.0 - MIN(CAST(base_price AS REAL) / 10000.0, 1.0)) * 0.1) DESC",
    );
    sql.push_str(&format!(" LIMIT ?{idx}"));
    param_values.push(Box::new(limit));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params_refs.as_slice(), map_agent_listing_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

// ── Row Types ──

fn map_agent_listing_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentListingRow> {
    Ok(AgentListingRow {
        agent_id: row.get(0)?,
        description: row.get(1)?,
        capabilities: row.get(2)?,
        pricing_model: row.get(3)?,
        base_price: row.get(4)?,
        trust_score: row.get(5)?,
        total_completed: row.get(6)?,
        total_failed: row.get(7)?,
        average_rating: row.get(8)?,
        total_reviews: row.get(9)?,
        status: row.get(10)?,
        endpoint_url: row.get(11)?,
        public_key: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn map_contract_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContractRow> {
    Ok(ContractRow {
        id: row.get(0)?,
        hirer_agent_id: row.get(1)?,
        worker_agent_id: row.get(2)?,
        state: row.get(3)?,
        task_description: row.get(4)?,
        agreed_price: row.get(5)?,
        max_duration_secs: row.get(6)?,
        delegation_id: row.get(7)?,
        mesh_task_id: row.get(8)?,
        escrow_id: row.get(9)?,
        result: row.get(10)?,
        event_hash: row.get(11)?,
        previous_hash: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

#[derive(Debug, Clone)]
pub struct AgentListingRow {
    pub agent_id: String,
    pub description: String,
    pub capabilities: String,
    pub pricing_model: String,
    pub base_price: i64,
    pub trust_score: f64,
    pub total_completed: i64,
    pub total_failed: i64,
    pub average_rating: f64,
    pub total_reviews: i64,
    pub status: String,
    pub endpoint_url: Option<String>,
    pub public_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct SkillListingRow {
    pub skill_name: String,
    pub version: String,
    pub author_agent_id: Option<String>,
    pub description: String,
    pub signature: Option<String>,
    pub price_credits: i64,
    pub install_count: i64,
    pub average_rating: f64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ContractRow {
    pub id: String,
    pub hirer_agent_id: String,
    pub worker_agent_id: String,
    pub state: String,
    pub task_description: String,
    pub agreed_price: i64,
    pub max_duration_secs: Option<i64>,
    pub delegation_id: Option<String>,
    pub mesh_task_id: Option<String>,
    pub escrow_id: Option<String>,
    pub result: Option<String>,
    pub event_hash: Option<String>,
    pub previous_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct WalletRow {
    pub agent_id: String,
    pub balance: i64,
    pub escrowed: i64,
    pub total_earned: i64,
    pub total_spent: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct TransactionRow {
    pub id: i64,
    pub from_agent: Option<String>,
    pub to_agent: Option<String>,
    pub amount: i64,
    pub tx_type: String,
    pub reference_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ReviewRow {
    pub id: i64,
    pub contract_id: String,
    pub reviewer_agent_id: String,
    pub reviewee_agent_id: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: String,
}
