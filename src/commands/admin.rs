use serenity::all::*;

use crate::db::queries::{get_guild_settings, upsert_guild_setting, upsert_guild_setting_bool};
use crate::error::BotError;
use crate::Context;

/// Set the audit log channel
#[poise::command(prefix_command, rename = "setlog", required_permissions = "ADMINISTRATOR")]
pub async fn setlog(
    ctx: Context<'_>,
    #[description = "The channel for audit logs"] channel: serenity::all::Channel,
) -> Result<(), BotError> {
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;

    upsert_guild_setting(
        &ctx.data().db,
        &guild_id.to_string(),
        "audit_log_channel_id",
        &channel.id().to_string(),
    )
    .await
    .map_err(|e| BotError::Other(format!("Database error: {e}")))?;

    ctx.say(format!("Audit log channel set to <#{}>.", channel.id()))
        .await?;
    Ok(())
}

/// Set the DJ role
#[poise::command(prefix_command, rename = "djrole", required_permissions = "ADMINISTRATOR")]
pub async fn djrole(
    ctx: Context<'_>,
    #[description = "The DJ role"] role: serenity::all::Role,
) -> Result<(), BotError> {
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;

    upsert_guild_setting(
        &ctx.data().db,
        &guild_id.to_string(),
        "dj_role_id",
        &role.id.to_string(),
    )
    .await
    .map_err(|e| BotError::Other(format!("Database error: {e}")))?;

    ctx.say(format!("DJ role set to **{}**.", role.name)).await?;
    Ok(())
}

/// Toggle DJ-only mode for music commands
#[poise::command(prefix_command, rename = "djmode", required_permissions = "ADMINISTRATOR")]
pub async fn djmode(ctx: Context<'_>) -> Result<(), BotError> {
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;

    let settings = get_guild_settings(&ctx.data().db, &guild_id.to_string()).await;
    let current = settings.as_ref().map_or(false, |s| s.dj_mode_enabled);
    let new_value = !current;

    if new_value && settings.as_ref().and_then(|s| s.dj_role_id.as_ref()).is_none() {
        ctx.say("Set a DJ role first with `!m djrole @role`.").await?;
        return Ok(());
    }

    upsert_guild_setting_bool(
        &ctx.data().db,
        &guild_id.to_string(),
        "dj_mode_enabled",
        new_value,
    )
    .await
    .map_err(|e| BotError::Other(format!("Database error: {e}")))?;

    ctx.say(if new_value {
        "DJ mode **enabled**. Only users with the DJ role (or admins) can use music commands."
    } else {
        "DJ mode **disabled**. Everyone can use music commands."
    })
    .await?;
    Ok(())
}
