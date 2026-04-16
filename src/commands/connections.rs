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
	let trimmed = date_str.trim();
	if chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").is_err() {
		ctx.say("Use YYYY-MM-DD format (e.g. 2024-03-15).").await?;
		return Ok(());
	}
	start_game(ctx, trimmed).await
}

async fn start_game(ctx: Context<'_>, date: &str) -> Result<(), BotError> {
	let channel_id = ctx.channel_id();

	// Refuse if a non-expired game is already active in this channel
	if let Some(existing) = ctx
		.data()
		.connections_games
		.get(&channel_id)
		.map(|e| e.value().clone())
	{
		let game = existing.lock().await;
		if !game.is_expired() {
			drop(game);
			ctx.say("A connections game is already active in this channel. Finish or wait for it to expire.").await?;
			return Ok(());
		}
	}

	// Fetch puzzle
	let puzzle = api::fetch_puzzle(&ctx.data().http_client, date)
		.await
		.map_err(BotError::Other)?;

	if puzzle.date != date {
		ctx.say(format!(
			"NYT didn't have a puzzle for **{date}**; showing **{}** instead.",
			puzzle.date
		))
		.await?;
	}

	// Create game with placeholder message ID
	let mut game = ConnectionsGame::new(puzzle, MessageId::new(1), channel_id);

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
