use chrono::Utc;
use serenity::all::*;
use sqlx::PgPool;

use crate::db::models::MemberActivity;
use crate::db::queries;
use crate::instance_config::AutoRoleConfig;
use crate::util::duration::parse_duration;

/// Check whether a member meets the promotion criteria.
pub fn meets_criteria(activity: &MemberActivity, config: &AutoRoleConfig) -> bool {
    let min_age_ms = parse_duration(&config.min_age).unwrap_or(259_200_000); // default 3d
    let age_ms = (Utc::now() - activity.first_seen).num_milliseconds();

    let time_ok = age_ms >= min_age_ms;
    let msgs_ok = i64::from(activity.message_count) >= config.min_messages;

    if config.require_all {
        time_ok && msgs_ok
    } else {
        time_ok || msgs_ok
    }
}

/// Promote a member: add to_role, remove from_role, mark in DB.
pub async fn try_promote(
    http: &Http,
    pool: &PgPool,
    guild_id: GuildId,
    user_id: UserId,
    config: &AutoRoleConfig,
) -> Result<(), String> {
    let from_role = config.from_role.parse::<u64>()
        .map_err(|_| "Invalid from_role ID".to_string())?;
    let to_role = config.to_role.parse::<u64>()
        .map_err(|_| "Invalid to_role ID".to_string())?;

    // Add the new role first, then remove the old one
    http.add_member_role(guild_id, user_id, RoleId::new(to_role), Some("Auto-role promotion"))
        .await
        .map_err(|e| format!("Failed to add role: {e}"))?;

    http.remove_member_role(guild_id, user_id, RoleId::new(from_role), Some("Auto-role promotion"))
        .await
        .map_err(|e| format!("Failed to remove role: {e}"))?;

    queries::mark_promoted(pool, &guild_id.to_string(), &user_id.to_string())
        .await
        .map_err(|e| format!("Failed to mark promoted: {e}"))?;

    tracing::info!(
        "Auto-promoted user {} in guild {}",
        user_id, guild_id
    );

    Ok(())
}
