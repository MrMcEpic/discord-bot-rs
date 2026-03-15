use crate::error::BotError;
use crate::minecraft::api;
use crate::Context;

fn require_mc_config(ctx: Context<'_>) -> Result<(String, String), BotError> {
    let url = ctx
        .data()
        .config
        .mc_verify_url
        .clone()
        .ok_or_else(|| BotError::Other("MC verification is not configured. Set `MC_VERIFY_URL` in .env".into()))?;
    let secret = ctx
        .data()
        .config
        .mc_verify_secret
        .clone()
        .ok_or_else(|| BotError::Other("MC verification is not configured. Set `MC_VERIFY_SECRET` in .env".into()))?;
    Ok((url, secret))
}

/// Link your Discord account to Minecraft
#[poise::command(prefix_command, rename = "verify")]
pub async fn verify(
    ctx: Context<'_>,
    #[description = "Verification code from Minecraft"]
    #[rest]
    code: String,
) -> Result<(), BotError> {
    let (url, secret) = require_mc_config(ctx)?;
    let code = code.trim().to_uppercase();

    if code.is_empty() {
        ctx.say("Usage: `!m verify <code>` — get your code by running `/verify` in Minecraft.")
            .await?;
        return Ok(());
    }

    let discord_id = ctx.author().id.to_string();

    let result = api::verify(&ctx.data().http_client, &url, &secret, &code, &discord_id).await;

    match result {
        Ok(resp) if resp.success => {
            let username = resp.username.unwrap_or_else(|| "Unknown".to_string());
            ctx.say(format!("Verified! Your Discord account is now linked to **{username}** in Minecraft."))
                .await?;
        }
        Ok(resp) => {
            let error = resp.error.unwrap_or_else(|| "Unknown error".to_string());
            ctx.say(format!("Verification failed: {error}")).await?;
        }
        Err(e) => {
            ctx.say(format!("Could not reach the MC server: {e}")).await?;
        }
    }

    Ok(())
}
