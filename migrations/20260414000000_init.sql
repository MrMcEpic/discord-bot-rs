-- Initial schema migration.
--
-- This migration replaces the hand-rolled DDL that used to live in
-- `src/db/mod.rs::migrate`. It is idempotent by design: every
-- CREATE uses IF NOT EXISTS so that the migration can be applied
-- cleanly to pre-existing databases that already contain the full
-- schema (production's examplebot and secondbot instances). On a fresh
-- database it creates everything; on an existing database it is a
-- no-op and sqlx records the `_sqlx_migrations` row afterwards.
--
-- The target schema is selected by the pool's `search_path`, which
-- is configured in `init_pool`'s `after_connect` hook. All objects
-- below are unqualified and therefore resolved inside that schema.

CREATE TABLE IF NOT EXISTS tempbans (
    id SERIAL PRIMARY KEY,
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    moderator_id TEXT NOT NULL,
    reason TEXT,
    banned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    unbanned BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_tempbans_active
    ON tempbans (guild_id, expires_at) WHERE unbanned = FALSE;

CREATE TABLE IF NOT EXISTS guild_settings (
    guild_id TEXT PRIMARY KEY,
    audit_log_channel_id TEXT,
    dj_role_id TEXT,
    dj_mode_enabled BOOLEAN NOT NULL DEFAULT FALSE
);

-- Stock trading tables
CREATE TABLE IF NOT EXISTS stock_portfolios (
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    cash_balance DOUBLE PRECISION NOT NULL DEFAULT 1000.0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (guild_id, user_id)
);

CREATE TABLE IF NOT EXISTS stock_holdings (
    id SERIAL PRIMARY KEY,
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    quantity DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    avg_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    UNIQUE (guild_id, user_id, symbol)
);

CREATE INDEX IF NOT EXISTS idx_stock_holdings_user
    ON stock_holdings (guild_id, user_id);

CREATE TABLE IF NOT EXISTS stock_transactions (
    id SERIAL PRIMARY KEY,
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    action TEXT NOT NULL,
    quantity DOUBLE PRECISION NOT NULL,
    price_per_share DOUBLE PRECISION NOT NULL,
    total_amount DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_stock_transactions_user
    ON stock_transactions (guild_id, user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS stock_price_cache (
    symbol TEXT PRIMARY KEY,
    price DOUBLE PRECISION NOT NULL,
    prev_close DOUBLE PRECISION NOT NULL,
    change_pct DOUBLE PRECISION NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS member_activity (
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    promoted BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (guild_id, user_id)
);
