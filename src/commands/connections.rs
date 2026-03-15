use serenity::all::*;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::connections::api;
use crate::connections::embeds;
use crate::connections::game::ConnectionsGame;
use crate::error::BotError;
use crate::Context;

/// Play NYT Connections
#[poise::command(
    prefix_command,
    rename = "connections",
    aliases("conn"),
    subcommands("random", "date")
)]
pub async fn connections(ctx: Context<'_>) -> Result<(), BotError> {
    // Bare "!m connections" -> today's puzzle
    start_game(ctx, &api::today_puzzle_date()).await
}

/// Play a random Connections puzzle
#[poise::command(prefix_command, rename = "random", aliases("rand", "r"))]
pub async fn random(ctx: Context<'_>) -> Result<(), BotError> {
    start_game(ctx, &api::random_puzzle_date()).await
}

/// Play a specific date's Connections puzzle
#[poise::command(prefix_command, rename = "date", aliases("d"))]
pub async fn date(
    ctx: Context<'_>,
    #[description = "Date (YYYY-MM-DD)"]
    #[rest]
    date_str: String,
) -> Result<(), BotError> {
    start_game(ctx, date_str.trim()).await
}

async fn start_game(ctx: Context<'_>, date: &str) -> Result<(), BotError> {
    let channel_id = ctx.channel_id();

    // Fetch puzzle
    let puzzle = api::fetch_puzzle(&ctx.data().http_client, date)
        .await
        .map_err(BotError::Other)?;

    // Create game with placeholder message ID
    let mut game = ConnectionsGame::new(
        puzzle,
        MessageId::new(1),
        channel_id,
    );

    let embed = embeds::game_embed(&game);
    let buttons = embeds::game_buttons(&game);

    let msg = ctx
        .send(
            poise::CreateReply::default()
                .embed(embed)
                .components(buttons),
        )
        .await?
        .into_message()
        .await?;

    // Update with actual message ID
    game.message_id = msg.id;

    // Store in DashMap (replaces any existing game in this channel)
    ctx.data()
        .connections_games
        .insert(channel_id, Arc::new(Mutex::new(game)));

    Ok(())
}
