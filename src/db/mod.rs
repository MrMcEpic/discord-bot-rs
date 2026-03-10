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

    Ok(())
}
