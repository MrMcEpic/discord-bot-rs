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

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Duration;

	fn activity(message_count: i32, age_days: i64) -> MemberActivity {
		MemberActivity {
			guild_id: "123".into(),
			user_id: "456".into(),
			message_count,
			first_seen: Utc::now() - Duration::days(age_days),
			promoted: false,
		}
	}

	fn config(min_age: &str, min_messages: i64, require_all: bool) -> AutoRoleConfig {
		AutoRoleConfig {
			from_role: "1".into(),
			to_role: "2".into(),
			min_age: min_age.into(),
			min_messages,
			require_all,
		}
	}

	#[test]
	fn require_all_with_both_conditions_met_returns_true() {
		let a = activity(50, 5);
		let c = config("3d", 20, true);
		assert!(meets_criteria(&a, &c));
	}

	#[test]
	fn require_all_with_only_age_met_returns_false() {
		let a = activity(5, 5);
		let c = config("3d", 20, true);
		assert!(!meets_criteria(&a, &c));
	}

	#[test]
	fn require_all_with_only_messages_met_returns_false() {
		let a = activity(50, 1);
		let c = config("3d", 20, true);
		assert!(!meets_criteria(&a, &c));
	}

	#[test]
	fn any_with_only_age_met_returns_true() {
		let a = activity(5, 5);
		let c = config("3d", 20, false);
		assert!(meets_criteria(&a, &c));
	}

	#[test]
	fn any_with_only_messages_met_returns_true() {
		let a = activity(50, 1);
		let c = config("3d", 20, false);
		assert!(meets_criteria(&a, &c));
	}

	#[test]
	fn neither_condition_met_returns_false() {
		let a = activity(5, 1);
		let c_all = config("3d", 20, true);
		let c_any = config("3d", 20, false);
		assert!(!meets_criteria(&a, &c_all));
		assert!(!meets_criteria(&a, &c_any));
	}

	#[test]
	fn invalid_min_age_falls_back_to_3d_default() {
		// `parse_duration("garbage")` is None → falls back to 3 days. So a
		// 4-day-old member with enough messages should pass.
		let a = activity(50, 4);
		let c = config("garbage", 20, true);
		assert!(meets_criteria(&a, &c));

		// A 2-day-old member with enough messages should NOT pass under default.
		let a2 = activity(50, 2);
		assert!(!meets_criteria(&a2, &c));
	}

	#[test]
	fn boundary_exact_message_count_matches() {
		let a = activity(20, 5);
		let c = config("3d", 20, true); // >= 20 messages, >= 3 days
		assert!(meets_criteria(&a, &c));
	}
}
