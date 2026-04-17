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

	let bot_name = &ctx.data().bot_name;
	// Pre-rendered prefix from instance_config; "!m " by default, "!" for
	// flat-command setups, "!bot " etc. for renamed parent commands.
	let p = &ctx.data().cmd_prefix;

	let mut embed = CreateEmbed::new()
        .color(0x5865f2)
        .title(format!("{bot_name} Commands"))
        .description(
            "@mention me to chat, search the web, play music, trade stocks, or start games — all in plain English.",
        );

	let music_lines = [
		format!("`{p}play <song>` — Play a song (or add to queue)"),
		format!("`{p}playlist <url>` — Queue an entire playlist"),
		format!("`{p}skip` — Skip current track"),
		format!("`{p}stop` — Stop playback and leave voice"),
		format!("`{p}pause` / `{p}resume` — Pause/resume playback"),
		format!("`{p}queue` — Show the current queue"),
		format!("`{p}np` — Show what's playing now"),
		format!("`{p}loop [off|track|queue]` — Toggle loop mode"),
		format!("`{p}shuffle` — Shuffle the queue"),
		format!("`{p}remove <#>` — Remove a song from queue"),
	];
	embed = embed.field("Music", music_lines.join("\n"), false);

	let game_lines = [
		format!("`{p}connections` — Play today's NYT Connections"),
		format!("`{p}connections random` — Random puzzle"),
		format!("`{p}wordle` — Play today's Wordle"),
		format!("`{p}wordle random` — Random Wordle"),
	];
	embed = embed.field("Games", game_lines.join("\n"), false);

	let stock_lines = [
		format!("`{p}stock buy <symbol> <qty/$amt>` — Buy shares"),
		format!("`{p}stock sell <symbol> <qty/all>` — Sell shares"),
		format!("`{p}stock portfolio [@user]` — View portfolio"),
		format!("`{p}stock price <symbol>` — Check stock price"),
		format!("`{p}stock leaderboard` — Top portfolios in server"),
		format!("`{p}stock history` — Recent trades"),
		format!("`{p}stock reset confirm` — Reset to $1,000"),
	];
	embed = embed.field("Stocks", stock_lines.join("\n"), false);

	if has_ban || has_manage {
		let mut mod_lines: Vec<String> = Vec::new();
		if has_manage {
			mod_lines.push(format!("`{p}nuke <1-100>` — Bulk delete messages"));
		}
		if has_ban {
			mod_lines.push(format!(
				"`{p}ban @user <duration> [reason]` — Tempban a user"
			));
			mod_lines.push(format!("`{p}unban @user` — Unban a user early"));
			mod_lines.push(format!("`{p}banlist` — Show active tempbans"));
		}
		embed = embed.field("Moderation", mod_lines.join("\n"), false);
	}

	if is_admin {
		let admin_lines = [
			format!("`{p}setlog #channel` — Set the audit log channel"),
			format!("`{p}djrole @role` — Set the DJ role"),
			format!("`{p}djmode` — Toggle DJ-only mode for music commands"),
		];
		embed = embed.field("Admin", admin_lines.join("\n"), false);
	}

	embed = embed.field(
        "Shortcuts",
        "`p` play · `pl` playlist · `s` skip · `r` resume · `q` queue · `np` now playing · `l` loop · `st` stocks · `conn` connections · `w` wordle · `h` help",
        false,
    );

	embed = embed.footer(CreateEmbedFooter::new(format!("{bot_name} v{VERSION}")));

	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}
