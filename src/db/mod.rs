pub mod models;
pub mod queries;

use sqlx::PgPool;

pub async fn init_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPool::connect(database_url).await?;
    migrate(&pool).await?;
    tracing::info!("Database initialized.");
    Ok(pool)
}

async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tempbans (
            id SERIAL PRIMARY KEY,
            guild_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            moderator_id TEXT NOT NULL,
            reason TEXT,
            banned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ NOT NULL,
            unbanned BOOLEAN NOT NULL DEFAULT FALSE
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tempbans_active \
         ON tempbans (guild_id, expires_at) WHERE unbanned = FALSE",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS guild_settings (
            guild_id TEXT PRIMARY KEY,
            audit_log_channel_id TEXT,
            dj_role_id TEXT,
            dj_mode_enabled BOOLEAN NOT NULL DEFAULT FALSE
        )",
    )
    .execute(pool)
    .await?;

    // Stock trading tables
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stock_portfolios (
            guild_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            cash_balance DOUBLE PRECISION NOT NULL DEFAULT 1000.0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stock_holdings (
            id SERIAL PRIMARY KEY,
            guild_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            symbol TEXT NOT NULL,
            quantity DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            avg_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
            UNIQUE (guild_id, user_id, symbol)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_stock_holdings_user \
         ON stock_holdings (guild_id, user_id)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stock_transactions (
            id SERIAL PRIMARY KEY,
            guild_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            symbol TEXT NOT NULL,
            action TEXT NOT NULL,
            quantity DOUBLE PRECISION NOT NULL,
            price_per_share DOUBLE PRECISION NOT NULL,
            total_amount DOUBLE PRECISION NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_stock_transactions_user \
         ON stock_transactions (guild_id, user_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stock_price_cache (
            symbol TEXT PRIMARY KEY,
            price DOUBLE PRECISION NOT NULL,
            prev_close DOUBLE PRECISION NOT NULL,
            change_pct DOUBLE PRECISION NOT NULL,
            fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
