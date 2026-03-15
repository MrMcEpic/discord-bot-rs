use serenity::all::*;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::BotError;
use crate::wordle::api;
use crate::wordle::embeds;
use crate::wordle::game::WordleGame;
use crate::Context;

/// Play Wordle
#[poise::command(
    prefix_command,
    rename = "wordle",
    aliases("w"),
    subcommands("random", "date")
)]
pub async fn wordle(ctx: Context<'_>) -> Result<(), BotError> {
    start_game(ctx, &api::today_puzzle_date()).await
}

/// Play a random Wordle
#[poise::command(prefix_command, rename = "random", aliases("rand", "r"))]
pub async fn random(ctx: Context<'_>) -> Result<(), BotError> {
    start_game(ctx, &api::random_puzzle_date()).await
}

/// Play a specific date's Wordle
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

    let puzzle = api::fetch_puzzle(&ctx.data().http_client, date)
        .await
        .map_err(BotError::Other)?;

    let mut game = WordleGame::new(
        puzzle.solution,
        puzzle.date,
        MessageId::new(1),
        channel_id,
    );

    let embed = embeds::game_embed(&game);

    let msg = ctx
        .send(poise::CreateReply::default().embed(embed))
        .await?
        .into_message()
        .await?;

    game.message_id = msg.id;

    ctx.data()
        .wordle_games
        .insert(channel_id, Arc::new(Mutex::new(game)));

    Ok(())
}
