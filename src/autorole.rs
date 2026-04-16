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
///
/// Race-safe: the DB claim runs FIRST and is atomic
/// (`UPDATE ... WHERE promoted = FALSE RETURNING user_id`). If this caller
/// loses the race (message handler vs. 60s background scanner firing within
/// ~100ms of each other), the claim returns `false` and we early-return
/// without touching Discord — no duplicate API calls, no duplicate log lines.
///
/// If a Discord call fails AFTER a successful claim, we log it but do NOT
/// unclaim. The user's roles may end up slightly inconsistent, but the bot
/// won't loop forever trying to re-promote them.
pub async fn try_promote(
	http: &Http,
	pool: &PgPool,
	guild_id: GuildId,
	user_id: UserId,
	config: &AutoRoleConfig,
) -> Result<(), String> {
	let from_role = config
		.from_role
		.parse::<u64>()
		.map_err(|_| "Invalid from_role ID".to_string())?;
	let to_role = config
		.to_role
		.parse::<u64>()
		.map_err(|_| "Invalid to_role ID".to_string())?;

	// Config sanity: from_role == to_role is a no-op promotion that would
	// remove the role we just added. Skip and warn — it's a config error.
	if from_role == to_role {
		tracing::warn!(
			"Auto-role config error: from_role == to_role ({}) for guild {} — skipping promotion",
			from_role,
			guild_id
		);
		return Ok(());
	}

	// Atomically claim the promotion. If we lose the race, another caller is
	// already handling this user — bail out without touching Discord.
	let claimed = queries::try_claim_promotion(pool, &guild_id.to_string(), &user_id.to_string())
		.await
		.map_err(|e| format!("Failed to claim promotion: {e}"))?;
	if !claimed {
		return Ok(());
	}

	// Add the new role first, then remove the old one
	http.add_member_role(
		guild_id,
		user_id,
		RoleId::new(to_role),
		Some("Auto-role promotion"),
	)
	.await
	.map_err(|e| format!("Failed to add role: {e}"))?;

	http.remove_member_role(
		guild_id,
		user_id,
		RoleId::new(from_role),
		Some("Auto-role promotion"),
	)
	.await
	.map_err(|e| format!("Failed to remove role: {e}"))?;

	tracing::info!("Auto-promoted user {} in guild {}", user_id, guild_id);

	Ok(())
}
