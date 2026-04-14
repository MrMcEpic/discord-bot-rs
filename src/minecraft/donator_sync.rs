use std::collections::HashSet;

use serde::Deserialize;
use serenity::all::*;

use crate::instance_config::DonatorSyncConfig;

#[derive(Debug, Deserialize)]
pub struct DonatorListResponse {
	pub donators: Vec<DonatorInfo>,
}

#[derive(Debug, Deserialize)]
pub struct DonatorInfo {
	pub discord_id: String,
	pub tier: String,
}

pub async fn fetch_donators(
	http_client: &reqwest::Client,
	base_url: &str,
	secret: &str,
) -> Result<Vec<DonatorInfo>, String> {
	let url = format!("{}/api/donators", base_url.trim_end_matches('/'));

	let resp = http_client
		.get(&url)
		.header("Authorization", format!("Bearer {secret}"))
		.send()
		.await
		.map_err(|e| format!("Failed to reach MC server: {e}"))?;

	if !resp.status().is_success() {
		return Err(format!("MC server returned status {}", resp.status()));
	}

	let body = resp
		.json::<DonatorListResponse>()
		.await
		.map_err(|e| format!("Invalid donator response: {e}"))?;

	Ok(body.donators)
}

pub async fn sync_roles(
	http: &Http,
	guild_id: GuildId,
	donators: &[DonatorInfo],
	config: &DonatorSyncConfig,
	restricted_role: Option<RoleId>,
) -> Result<(), String> {
	let supporter_role = config
		.supporter_role
		.parse::<u64>()
		.map_err(|_| "Invalid supporter_role ID")?;
	let premium_role = config
		.premium_role
		.parse::<u64>()
		.map_err(|_| "Invalid premium_role ID")?;
	let supporter_role_id = RoleId::new(supporter_role);
	let premium_role_id = RoleId::new(premium_role);

	// Build sets of who SHOULD have each role based on MC API
	let mut should_have_supporter: HashSet<UserId> = HashSet::new();
	let mut should_have_premium: HashSet<UserId> = HashSet::new();

	for d in donators {
		if let Ok(uid) = d.discord_id.parse::<u64>() {
			let user_id = UserId::new(uid);
			match d.tier.as_str() {
				"supporter" => {
					should_have_supporter.insert(user_id);
				}
				"premium" => {
					should_have_premium.insert(user_id);
				}
				other => {
					tracing::warn!(
						"Donator sync: unknown tier '{}' for discord_id {}",
						other,
						d.discord_id
					);
				}
			}
		}
	}

	// Fetch guild members who currently have either role
	let mut after = None;
	let mut current_supporters: HashSet<UserId> = HashSet::new();
	let mut current_premium: HashSet<UserId> = HashSet::new();
	let mut restricted_users: HashSet<UserId> = HashSet::new();

	loop {
		let members = http
			.get_guild_members(guild_id, Some(1000), after)
			.await
			.map_err(|e| format!("Failed to fetch guild members: {e}"))?;

		if members.is_empty() {
			break;
		}

		for member in &members {
			if member.roles.contains(&supporter_role_id) {
				current_supporters.insert(member.user.id);
			}
			if member.roles.contains(&premium_role_id) {
				current_premium.insert(member.user.id);
			}
			if let Some(ref restricted) = restricted_role {
				if member.roles.contains(restricted) {
					restricted_users.insert(member.user.id);
				}
			}
		}

		after = members.last().map(|m| m.user.id.get());
		if members.len() < 1000 {
			break;
		}
	}

	// Add missing roles
	for &user_id in &should_have_supporter {
		if restricted_users.contains(&user_id) {
			continue;
		}
		if !current_supporters.contains(&user_id) {
			match http
				.add_member_role(
					guild_id,
					user_id,
					supporter_role_id,
					Some("Donator sync: supporter tier"),
				)
				.await
			{
				Ok(_) => tracing::info!("Donator sync: added Supporter role to {}", user_id),
				Err(e) => tracing::warn!(
					"Donator sync: failed to add Supporter role to {}: {}",
					user_id,
					e
				),
			}
		}
	}

	for &user_id in &should_have_premium {
		if restricted_users.contains(&user_id) {
			continue;
		}
		if !current_premium.contains(&user_id) {
			match http
				.add_member_role(
					guild_id,
					user_id,
					premium_role_id,
					Some("Donator sync: premium tier"),
				)
				.await
			{
				Ok(_) => tracing::info!("Donator sync: added Premium role to {}", user_id),
				Err(e) => tracing::warn!(
					"Donator sync: failed to add Premium role to {}: {}",
					user_id,
					e
				),
			}
		}
	}

	// Remove stale roles (user has role but MC says they shouldn't)
	for &user_id in &current_supporters {
		if restricted_users.contains(&user_id) {
			continue;
		}
		if !should_have_supporter.contains(&user_id) {
			match http
				.remove_member_role(
					guild_id,
					user_id,
					supporter_role_id,
					Some("Donator sync: supporter expired"),
				)
				.await
			{
				Ok(_) => tracing::info!("Donator sync: removed Supporter role from {}", user_id),
				Err(e) => tracing::warn!(
					"Donator sync: failed to remove Supporter role from {}: {}",
					user_id,
					e
				),
			}
		}
	}

	for &user_id in &current_premium {
		if restricted_users.contains(&user_id) {
			continue;
		}
		if !should_have_premium.contains(&user_id) {
			match http
				.remove_member_role(
					guild_id,
					user_id,
					premium_role_id,
					Some("Donator sync: premium expired"),
				)
				.await
			{
				Ok(_) => tracing::info!("Donator sync: removed Premium role from {}", user_id),
				Err(e) => tracing::warn!(
					"Donator sync: failed to remove Premium role from {}: {}",
					user_id,
					e
				),
			}
		}
	}

	// Handle tier changes: if someone has supporter but should be premium, swap
	for &user_id in &should_have_premium {
		if restricted_users.contains(&user_id) {
			continue;
		}
		if current_supporters.contains(&user_id) && !should_have_supporter.contains(&user_id) {
			match http
				.remove_member_role(
					guild_id,
					user_id,
					supporter_role_id,
					Some("Donator sync: upgraded to premium"),
				)
				.await
			{
				Ok(_) => tracing::info!(
					"Donator sync: removed Supporter (upgraded to Premium) for {}",
					user_id
				),
				Err(e) => tracing::warn!(
					"Donator sync: failed to remove Supporter during upgrade for {}: {}",
					user_id,
					e
				),
			}
		}
	}

	for &user_id in &should_have_supporter {
		if restricted_users.contains(&user_id) {
			continue;
		}
		if current_premium.contains(&user_id) && !should_have_premium.contains(&user_id) {
			match http
				.remove_member_role(
					guild_id,
					user_id,
					premium_role_id,
					Some("Donator sync: downgraded to supporter"),
				)
				.await
			{
				Ok(_) => tracing::info!(
					"Donator sync: removed Premium (downgraded to Supporter) for {}",
					user_id
				),
				Err(e) => tracing::warn!(
					"Donator sync: failed to remove Premium during downgrade for {}: {}",
					user_id,
					e
				),
			}
		}
	}

	Ok(())
}
