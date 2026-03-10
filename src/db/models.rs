use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct Tempban {
    pub id: i32,
    pub guild_id: String,
    pub user_id: String,
    pub moderator_id: String,
    pub reason: Option<String>,
    pub banned_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub unbanned: bool,
}

#[derive(Debug, FromRow)]
pub struct GuildSettings {
    pub guild_id: String,
    pub audit_log_channel_id: Option<String>,
    pub dj_role_id: Option<String>,
    pub dj_mode_enabled: bool,
}
