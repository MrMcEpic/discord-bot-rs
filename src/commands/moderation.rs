use serenity::all::*;

use crate::db::queries::{create_tempban, get_active_bans, get_guild_settings, mark_unbanned};
use crate::error::BotError;
use crate::util::duration::{format_duration_ms, parse_duration};
use crate::Context;

/// Per-user rate limit for moderation commands. Returns `Ok(true)` if the
/// user was rate-limited (and we already replied), so the caller should bail
/// out. The AI tool path already enforces this same limiter; the prefix
/// commands have permission gates but we still cap absurd spam.
async fn moderation_rate_limit_or_reply(ctx: Context<'_>) -> Result<bool, BotError> {
	let cooldown = ctx
		.data()
		.rate_limiters
		.moderation
		.check(&ctx.author().id.to_string());
	if cooldown > 0 {
		ctx.say(format!("Slow down — try again in {cooldown}s."))
			.await?;
		return Ok(true);
	}
	Ok(false)
}

/// Temporarily ban a user
#[poise::command(prefix_command, rename = "ban", required_permissions = "BAN_MEMBERS")]
pub async fn ban(
	ctx: Context<'_>,
	#[description = "User to ban"] target: serenity::all::Member,
	#[description = "Duration (e.g. 3d, 2h, 1w)"] duration_str: String,
	#[description = "Reason"]
	#[rest]
	reason: Option<String>,
) -> Result<(), BotError> {
	if moderation_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;

	let duration_ms = match parse_duration(&duration_str) {
		Some(ms) => ms,
		None => {
			ctx.say("Invalid duration. Use: `30s`, `5m`, `2h`, `3d`, `1w`")
				.await?;
			return Ok(());
		}
	};

	let reason_ref = reason.as_deref();

	let expires_at = create_tempban(
		&ctx.data().db,
		&guild_id.to_string(),
		&target.user.id.to_string(),
		&ctx.author().id.to_string(),
		duration_ms,
		reason_ref,
	)
	.await
	.map_err(|e| BotError::Other(format!("Database error: {e}")))?;

	let ban_reason = format!(
		"Tempban by {} ({}){}",
		ctx.author().name,
		format_duration_ms(duration_ms),
		reason_ref.map_or(String::new(), |r| format!(": {r}"))
	);

	guild_id
		.ban_with_reason(&ctx.http(), target.user.id, 0, &ban_reason)
		.await?;

	let expires_ts = expires_at.timestamp();
	ctx.say(format!(
		"Banned **{}** for **{}**. Expires <t:{expires_ts}:R>.{}",
		target.display_name(),
		format_duration_ms(duration_ms),
		reason_ref.map_or(String::new(), |r| format!("\nReason: {r}"))
	))
	.await?;

	send_audit_log(
		ctx,
		"Tempban",
		&[
			(
				"User",
				&format!("{} ({})", target.display_name(), target.user.id),
				true,
			),
			("Moderator", &ctx.author().name, true),
			("Duration", &format_duration_ms(duration_ms), true),
		],
	)
	.await;

	Ok(())
}

/// Unban a user early
#[poise::command(prefix_command, rename = "unban", required_permissions = "BAN_MEMBERS")]
pub async fn unban(
	ctx: Context<'_>,
	#[description = "User to unban"] user: serenity::all::User,
) -> Result<(), BotError> {
	if moderation_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;

	let had = mark_unbanned(&ctx.data().db, &guild_id.to_string(), &user.id.to_string())
		.await
		.unwrap_or(false);

	guild_id.unban(&ctx.http(), user.id).await?;

	ctx.say(format!(
		"Unbanned **{}**.{}",
		user.name,
		if had {
			""
		} else {
			" (No active tempban was found in the database.)"
		}
	))
	.await?;

	send_audit_log(
		ctx,
		"Unban",
		&[
			("User", &format!("{} ({})", user.name, user.id), true),
			("Moderator", &ctx.author().name, true),
		],
	)
	.await;

	Ok(())
}

/// Show active tempbans
#[poise::command(
	prefix_command,
	rename = "banlist",
	aliases("bans"),
	required_permissions = "BAN_MEMBERS"
)]
pub async fn banlist(ctx: Context<'_>) -> Result<(), BotError> {
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;

	let bans = get_active_bans(&ctx.data().db, &guild_id.to_string())
		.await
		.map_err(|e| BotError::Other(format!("Database error: {e}")))?;

	if bans.is_empty() {
		ctx.say("No active tempbans.").await?;
		return Ok(());
	}

	let lines: Vec<String> = bans
		.iter()
		.map(|b| {
			let expires = b.expires_at.timestamp();
			format!(
				"<@{}> — expires <t:{expires}:R> (by <@{}>{})",
				b.user_id,
				b.moderator_id,
				b.reason
					.as_ref()
					.map_or(String::new(), |r| format!(", reason: {r}"))
			)
		})
		.collect();

	let embed = CreateEmbed::new()
		.color(0xed4245)
		.title("Active Tempbans")
		.description(lines.join("\n"));

	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}

/// Bulk delete messages
#[poise::command(
	prefix_command,
	rename = "nuke",
	required_permissions = "MANAGE_MESSAGES"
)]
pub async fn nuke(
	ctx: Context<'_>,
	#[description = "Number of messages (1-100)"] count: u8,
) -> Result<(), BotError> {
	if moderation_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	if !(1..=100).contains(&count) {
		ctx.say("Usage: `!m nuke <1-100>`").await?;
		return Ok(());
	}

	let channel_id = ctx.channel_id();

	// +1 to include the command message itself
	let messages = channel_id
		.messages(&ctx.http(), GetMessages::new().limit(count + 1))
		.await?;

	let msg_ids: Vec<MessageId> = messages.iter().map(|m| m.id).collect();
	let actual = msg_ids.len().saturating_sub(1); // Don't count command message

	if !msg_ids.is_empty() {
		channel_id.delete_messages(&ctx.http(), &msg_ids).await?;
	}

	let text = if actual < count as usize {
		format!("Deleted **{actual}/{count}** messages (messages older than 14 days can't be bulk-deleted).")
	} else {
		format!("Deleted **{actual}** messages.")
	};

	let notice = channel_id.say(&ctx.http(), &text).await?;

	let http = ctx.serenity_context().http.clone();
	let notice_id = notice.id;
	tokio::spawn(async move {
		tokio::time::sleep(std::time::Duration::from_secs(3)).await;
		let _ = http.delete_message(channel_id, notice_id, None).await;
	});

	send_audit_log(
		ctx,
		"Nuke",
		&[
			("Channel", &format!("<#{channel_id}>"), true),
			("Moderator", &ctx.author().name, true),
			("Messages Deleted", &actual.to_string(), true),
		],
	)
	.await;

	Ok(())
}

async fn send_audit_log(ctx: Context<'_>, action: &str, fields: &[(&str, &str, bool)]) {
	let guild_id = match ctx.guild_id() {
		Some(id) => id,
		None => return,
	};

	let settings = match get_guild_settings(&ctx.data().db, &guild_id.to_string()).await {
		Some(s) => s,
		None => return,
	};

	let channel_id_str = match &settings.audit_log_channel_id {
		Some(id) => id,
		None => return,
	};

	let channel_id: ChannelId = match channel_id_str.parse::<u64>() {
		Ok(id) => ChannelId::new(id),
		Err(_) => return,
	};

	let color = if action == "Unban" {
		0x57f287
	} else {
		0xed4245
	};

	let mut embed = CreateEmbed::new()
		.color(color)
		.title(format!("Mod Action: {action}"))
		.timestamp(chrono::Utc::now());

	for (name, value, inline) in fields {
		embed = embed.field(*name, *value, *inline);
	}

	let _ = channel_id
		.send_message(&ctx.http(), CreateMessage::new().embed(embed))
		.await;
}
