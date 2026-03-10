use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::models::{GuildSettings, Tempban};

pub async fn get_guild_settings(pool: &PgPool, guild_id: &str) -> Option<GuildSettings> {
    sqlx::query_as::<_, GuildSettings>("SELECT * FROM guild_settings WHERE guild_id = $1")
        .bind(guild_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

const ALLOWED_COLUMNS: &[&str] = &["audit_log_channel_id", "dj_role_id", "dj_mode_enabled"];

pub async fn upsert_guild_setting(
    pool: &PgPool,
    guild_id: &str,
    key: &str,
    value: &str,
) -> Result<(), sqlx::Error> {
    if !ALLOWED_COLUMNS.contains(&key) {
        return Err(sqlx::Error::Protocol(format!("Invalid setting key: {key}")));
    }
    let query = format!(
        "INSERT INTO guild_settings (guild_id, {key}) VALUES ($1, $2) \
         ON CONFLICT (guild_id) DO UPDATE SET {key} = $2"
    );
    sqlx::query(&query).bind(guild_id).bind(value).execute(pool).await?;
    Ok(())
}

pub async fn upsert_guild_setting_bool(
    pool: &PgPool,
    guild_id: &str,
    key: &str,
    value: bool,
) -> Result<(), sqlx::Error> {
    if !ALLOWED_COLUMNS.contains(&key) {
        return Err(sqlx::Error::Protocol(format!("Invalid setting key: {key}")));
    }
    let query = format!(
        "INSERT INTO guild_settings (guild_id, {key}) VALUES ($1, $2) \
         ON CONFLICT (guild_id) DO UPDATE SET {key} = $2"
    );
    sqlx::query(&query).bind(guild_id).bind(value).execute(pool).await?;
    Ok(())
}

pub async fn create_tempban(
    pool: &PgPool,
    guild_id: &str,
    user_id: &str,
    moderator_id: &str,
    duration_ms: i64,
    reason: Option<&str>,
) -> Result<DateTime<Utc>, sqlx::Error> {
    let expires_at = Utc::now() + chrono::Duration::milliseconds(duration_ms);
    sqlx::query(
        "INSERT INTO tempbans (guild_id, user_id, moderator_id, reason, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(guild_id)
    .bind(user_id)
    .bind(moderator_id)
    .bind(reason)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(expires_at)
}

pub async fn mark_unbanned(pool: &PgPool, guild_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE tempbans SET unbanned = TRUE \
         WHERE guild_id = $1 AND user_id = $2 AND unbanned = FALSE",
    )
    .bind(guild_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_active_bans(pool: &PgPool, guild_id: &str) -> Result<Vec<Tempban>, sqlx::Error> {
    sqlx::query_as::<_, Tempban>(
        "SELECT * FROM tempbans \
         WHERE guild_id = $1 AND unbanned = FALSE AND expires_at > NOW() \
         ORDER BY expires_at ASC",
    )
    .bind(guild_id)
    .fetch_all(pool)
    .await
}

pub async fn get_expired_bans(pool: &PgPool) -> Result<Vec<Tempban>, sqlx::Error> {
    sqlx::query_as::<_, Tempban>(
        "SELECT * FROM tempbans WHERE unbanned = FALSE AND expires_at <= NOW()",
    )
    .fetch_all(pool)
    .await
}

pub async fn mark_unbanned_by_id(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE tempbans SET unbanned = TRUE WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
