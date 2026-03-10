use serenity::all::*;

use crate::error::BotError;
use crate::Context;

/// Show available commands
#[poise::command(prefix_command, rename = "help", aliases("h"))]
pub async fn help(ctx: Context<'_>) -> Result<(), BotError> {
    let member = match ctx.author_member().await {
        Some(m) => m.into_owned(),
        None => return Ok(()),
    };

    let perms = member
        .permissions(ctx.cache())
        .unwrap_or(Permissions::empty());
    let has_ban = perms.contains(Permissions::BAN_MEMBERS);
    let has_manage = perms.contains(Permissions::MANAGE_MESSAGES);
    let is_admin = perms.contains(Permissions::ADMINISTRATOR);

    let mut embed = CreateEmbed::new()
        .color(0x5865f2)
        .title("Example Bot Commands")
        .description(
            "You can also @mention me to chat, ask questions, search the web, or control music in plain English.",
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
        "`p` play · `pl` playlist · `s` skip · `r` resume · `q` queue · `np` now playing · `l` loop · `h` help",
        false,
    );

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
