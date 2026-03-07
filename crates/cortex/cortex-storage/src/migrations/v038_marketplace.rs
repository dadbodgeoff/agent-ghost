//! Migration v038: Agent marketplace tables.
//!
//! Creates tables for agent/skill listings, hiring contracts with
//! escrow-backed credits, reviews, and wallet management.

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "-- Agent listings for the marketplace
        CREATE TABLE IF NOT EXISTS marketplace_agent_listings (
            agent_id        TEXT PRIMARY KEY,
            description     TEXT NOT NULL DEFAULT '',
            capabilities    TEXT NOT NULL DEFAULT '[]',
            pricing_model   TEXT NOT NULL DEFAULT 'per_task'
                CHECK(pricing_model IN ('per_task', 'hourly', 'flat_rate', 'subscription')),
            base_price      INTEGER NOT NULL DEFAULT 100 CHECK(base_price >= 0),
            trust_score     REAL NOT NULL DEFAULT 0.0,
            total_completed INTEGER NOT NULL DEFAULT 0,
            total_failed    INTEGER NOT NULL DEFAULT 0,
            average_rating  REAL NOT NULL DEFAULT 0.0,
            total_reviews   INTEGER NOT NULL DEFAULT 0,
            status          TEXT NOT NULL DEFAULT 'active'
                CHECK(status IN ('active', 'busy', 'offline', 'suspended')),
            endpoint_url    TEXT,
            public_key      TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_mkt_listings_status
            ON marketplace_agent_listings(status);
        CREATE INDEX IF NOT EXISTS idx_mkt_listings_trust
            ON marketplace_agent_listings(trust_score DESC);
        CREATE INDEX IF NOT EXISTS idx_mkt_listings_rating
            ON marketplace_agent_listings(average_rating DESC);

        -- Skill listings
        CREATE TABLE IF NOT EXISTS marketplace_skill_listings (
            skill_name      TEXT PRIMARY KEY,
            version         TEXT NOT NULL DEFAULT '0.1.0',
            author_agent_id TEXT,
            description     TEXT NOT NULL DEFAULT '',
            signature       TEXT,
            price_credits   INTEGER NOT NULL DEFAULT 0 CHECK(price_credits >= 0),
            install_count   INTEGER NOT NULL DEFAULT 0,
            average_rating  REAL NOT NULL DEFAULT 0.0,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Hiring contracts (FSM: proposed -> accepted -> in_progress -> completed)
        CREATE TABLE IF NOT EXISTS marketplace_contracts (
            id                  TEXT PRIMARY KEY,
            hirer_agent_id      TEXT NOT NULL,
            worker_agent_id     TEXT NOT NULL,
            state               TEXT NOT NULL DEFAULT 'proposed'
                CHECK(state IN ('proposed', 'accepted', 'rejected', 'in_progress',
                                'completed', 'disputed', 'canceled', 'resolved')),
            task_description    TEXT NOT NULL DEFAULT '',
            agreed_price        INTEGER NOT NULL CHECK(agreed_price > 0),
            max_duration_secs   INTEGER CHECK(max_duration_secs IS NULL OR max_duration_secs > 0),
            delegation_id       TEXT,
            mesh_task_id        TEXT,
            escrow_id           TEXT,
            result              TEXT,
            event_hash          TEXT,
            previous_hash       TEXT,
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
            CHECK(hirer_agent_id != worker_agent_id)
        );
        CREATE INDEX IF NOT EXISTS idx_mkt_contracts_hirer
            ON marketplace_contracts(hirer_agent_id);
        CREATE INDEX IF NOT EXISTS idx_mkt_contracts_worker
            ON marketplace_contracts(worker_agent_id);
        CREATE INDEX IF NOT EXISTS idx_mkt_contracts_state
            ON marketplace_contracts(state);

        -- Credit wallets
        CREATE TABLE IF NOT EXISTS credit_wallets (
            agent_id        TEXT PRIMARY KEY,
            balance         INTEGER NOT NULL DEFAULT 0 CHECK(balance >= 0),
            escrowed        INTEGER NOT NULL DEFAULT 0 CHECK(escrowed >= 0),
            total_earned    INTEGER NOT NULL DEFAULT 0,
            total_spent     INTEGER NOT NULL DEFAULT 0,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Credit transactions (append-only)
        CREATE TABLE IF NOT EXISTS credit_transactions (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            from_agent      TEXT,
            to_agent        TEXT,
            amount          INTEGER NOT NULL CHECK(amount > 0),
            tx_type         TEXT NOT NULL,
            reference_id    TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_credit_tx_from
            ON credit_transactions(from_agent);
        CREATE INDEX IF NOT EXISTS idx_credit_tx_to
            ON credit_transactions(to_agent);

        -- Append-only trigger for credit_transactions
        CREATE TRIGGER IF NOT EXISTS credit_transactions_no_update
            BEFORE UPDATE ON credit_transactions
            BEGIN SELECT RAISE(ABORT, 'credit_transactions is append-only'); END;
        CREATE TRIGGER IF NOT EXISTS credit_transactions_no_delete
            BEFORE DELETE ON credit_transactions
            BEGIN SELECT RAISE(ABORT, 'credit_transactions is append-only'); END;

        -- Credit escrows
        CREATE TABLE IF NOT EXISTS credit_escrows (
            id              TEXT PRIMARY KEY,
            contract_id     TEXT NOT NULL REFERENCES marketplace_contracts(id),
            depositor       TEXT NOT NULL,
            beneficiary     TEXT NOT NULL,
            amount          INTEGER NOT NULL CHECK(amount > 0),
            status          TEXT NOT NULL DEFAULT 'held'
                CHECK(status IN ('held', 'released', 'refunded')),
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            CHECK(depositor != beneficiary)
        );
        CREATE INDEX IF NOT EXISTS idx_credit_escrows_contract
            ON credit_escrows(contract_id);

        -- Reviews
        CREATE TABLE IF NOT EXISTS marketplace_reviews (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            contract_id         TEXT NOT NULL REFERENCES marketplace_contracts(id),
            reviewer_agent_id   TEXT NOT NULL,
            reviewee_agent_id   TEXT NOT NULL,
            rating              INTEGER NOT NULL CHECK(rating >= 1 AND rating <= 5),
            comment             TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(contract_id, reviewer_agent_id),
            CHECK(reviewer_agent_id != reviewee_agent_id)
        );
        CREATE INDEX IF NOT EXISTS idx_mkt_reviews_reviewee
            ON marketplace_reviews(reviewee_agent_id);"
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
