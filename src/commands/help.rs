use serenity::all::*;
use serenity::builder::CreateEmbedFooter;

use crate::error::BotError;
use crate::Context;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Show available commands
#[poise::command(prefix_command, rename = "help", aliases("h"))]
pub async fn help(ctx: Context<'_>) -> Result<(), BotError> {
	let member = match ctx.author_member().await {
		Some(m) => m.into_owned(),
		None => return Ok(()),
	};

	let perms = ctx
		.guild_id()
		.and_then(|gid| ctx.cache().guild(gid))
		.map(|guild| guild.member_permissions(&member))
		.unwrap_or(Permissions::empty());
	let has_ban = perms.contains(Permissions::BAN_MEMBERS);
	let has_manage = perms.contains(Permissions::MANAGE_MESSAGES);
	let is_admin = perms.contains(Permissions::ADMINISTRATOR);

	let mut embed = CreateEmbed::new()
        .color(0x5865f2)
        .title("Example Bot Commands")
        .description(
            "@mention me to chat, search the web, play music, trade stocks, or start games — all in plain English.",
        );

	let music_lines = [
		"`!m play <song>` — Play a song (or add to queue)",
		"`!m playlist <url>` — Queue an entire playlist",
		"`!m skip` — Skip current track",
		"`!m stop` — Stop playback and leave voice",
		"`!m pause` / `!m resume` — Pause/resume playback",
		"`!m queue` — Show the current queue",
		"`!m np` — Show what's playing now",
		"`!m loop [off|track|queue]` — Toggle loop mode",
		"`!m shuffle` — Shuffle the queue",
		"`!m remove <#>` — Remove a song from queue",
	];
	embed = embed.field("Music", music_lines.join("\n"), false);

	let game_lines = [
		"`!m connections` — Play today's NYT Connections",
		"`!m connections random` — Random puzzle",
		"`!m wordle` — Play today's Wordle",
		"`!m wordle random` — Random Wordle",
	];
	embed = embed.field("Games", game_lines.join("\n"), false);

	let stock_lines = [
		"`!m stock buy <symbol> <qty/$amt>` — Buy shares",
		"`!m stock sell <symbol> <qty/all>` — Sell shares",
		"`!m stock portfolio [@user]` — View portfolio",
		"`!m stock price <symbol>` — Check stock price",
		"`!m stock leaderboard` — Top portfolios in server",
		"`!m stock history` — Recent trades",
		"`!m stock reset confirm` — Reset to $1,000",
	];
	embed = embed.field("Stocks", stock_lines.join("\n"), false);

	if has_ban || has_manage {
		let mut mod_lines = Vec::new();
		if has_manage {
			mod_lines.push("`!m nuke <1-100>` — Bulk delete messages");
		}
		if has_ban {
			mod_lines.push("`!m ban @user <duration> [reason]` — Tempban a user");
			mod_lines.push("`!m unban @user` — Unban a user early");
			mod_lines.push("`!m banlist` — Show active tempbans");
		}
		embed = embed.field("Moderation", mod_lines.join("\n"), false);
	}

	if is_admin {
		let admin_lines = [
			"`!m setlog #channel` — Set the audit log channel",
			"`!m djrole @role` — Set the DJ role",
			"`!m djmode` — Toggle DJ-only mode for music commands",
		];
		embed = embed.field("Admin", admin_lines.join("\n"), false);
	}

	embed = embed.field(
        "Shortcuts",
        "`p` play · `pl` playlist · `s` skip · `r` resume · `q` queue · `np` now playing · `l` loop · `st` stocks · `conn` connections · `w` wordle · `h` help",
        false,
    );

	embed = embed.footer(CreateEmbedFooter::new(format!("Example Bot v{VERSION}")));

	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}
