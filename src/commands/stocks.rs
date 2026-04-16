use rust_decimal::Decimal;
use serenity::all::*;
use std::time::Duration;

use crate::db::queries;
use crate::error::BotError;
use crate::stocks::api;
use crate::stocks::embeds::{self, HoldingWithQuote, LeaderboardEntry};
use crate::stocks::STARTING_CASH;
use crate::Context;
#[allow(unused_imports)]
use sqlx;

const RESET_CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);

// `BotError` is the project-wide error type used by every poise command in this
// crate; its largest variant (`Serenity(serenity::Error)`) trips the
// `result_large_err` lint here even though every command in the codebase returns
// the same type. Boxing the enum would be a cross-cutting refactor with no
// runtime benefit for a small helper; allow locally instead.
/// Per-user rate limit for stock commands. Returns `Ok(true)` if the user
/// was rate-limited (and we already replied), so the caller should bail out.
/// Wraps the shared `stocks` limiter (10 req / 30s) so portfolio /
/// quote-fetching network calls aren't easily spammed.
async fn stocks_rate_limit_or_reply(ctx: Context<'_>) -> Result<bool, BotError> {
	let cooldown = ctx
		.data()
		.rate_limiters
		.stocks
		.check(&ctx.author().id.to_string());
	if cooldown > 0 {
		ctx.say(format!("Slow down — try again in {cooldown}s."))
			.await?;
		return Ok(true);
	}
	Ok(false)
}

#[allow(clippy::result_large_err)]
fn require_finnhub_key(ctx: Context<'_>) -> Result<String, BotError> {
	ctx.data().config.finnhub_api_key.clone().ok_or_else(|| {
		BotError::Other(
			"Stock trading is not configured. The bot owner needs to set `FINNHUB_API_KEY`.".into(),
		)
	})
}

/// Virtual stock trading
#[poise::command(
	prefix_command,
	rename = "stock",
	aliases("stocks", "st"),
	subcommands("buy", "sell", "portfolio", "price", "leaderboard", "history", "reset")
)]
pub async fn stock(ctx: Context<'_>) -> Result<(), BotError> {
	// Bare "!m stock" -> show portfolio
	portfolio_inner(ctx, None).await
}

/// Buy shares of a stock
#[poise::command(prefix_command, rename = "buy", aliases("b"))]
pub async fn buy(
	ctx: Context<'_>,
	#[description = "Stock ticker symbol"] symbol: String,
	#[description = "Quantity or $amount"]
	#[rest]
	amount_str: String,
) -> Result<(), BotError> {
	if stocks_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let api_key = require_finnhub_key(ctx)?;
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;
	let user_id = ctx.author().id.to_string();
	let guild_id_str = guild_id.to_string();
	let symbol = symbol.to_uppercase();

	// Ensure portfolio exists
	queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;

	// Fetch current price
	let quote = api::get_quote(&ctx.data().http_client, &ctx.data().db, &api_key, &symbol)
		.await
		.map_err(BotError::Other)?;

	// Parse amount: "$500" for dollar amount, or plain number for share count.
	// Decimal has `FromStr`, so user input goes straight into exact math.
	let amount_str = amount_str.trim();
	let quantity = if let Some(dollars) = amount_str.strip_prefix('$') {
		let dollar_amount: Decimal = dollars
			.parse()
			.map_err(|_| BotError::Other("Invalid dollar amount.".into()))?;
		if dollar_amount <= Decimal::ZERO {
			return Err(BotError::Other("Amount must be positive.".into()));
		}
		// Share-count math: round to 4dp so we never try to bind a NUMERIC
		// value with more scale than the column allows.
		(dollar_amount / quote.price).round_dp(4)
	} else {
		let qty: Decimal = amount_str
			.parse()
			.map_err(|_| BotError::Other("Invalid quantity. Use a number or `$amount`.".into()))?;
		if qty <= Decimal::ZERO {
			return Err(BotError::Other("Quantity must be positive.".into()));
		}
		qty.round_dp(4)
	};

	let total = quantity * quote.price;

	match queries::buy_stock(
		&ctx.data().db,
		&guild_id_str,
		&user_id,
		&symbol,
		quantity,
		quote.price,
	)
	.await
	{
		Err(sqlx::Error::Protocol(msg)) if msg.contains("Insufficient funds") => {
			ctx.say(format!(
				"Insufficient funds. This trade costs **${}** but you don't have enough cash.",
				total.round_dp(2)
			))
			.await?;
			return Ok(());
		}
		Err(e) => return Err(e.into()),
		Ok(_) => {}
	}

	// Re-fetch portfolio for accurate balance
	let portfolio =
		queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
	let embed = embeds::buy_embed(
		&symbol,
		quantity,
		quote.price,
		total,
		portfolio.cash_balance,
	);
	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}

/// Sell shares of a stock
#[poise::command(prefix_command, rename = "sell", aliases("s"))]
pub async fn sell(
	ctx: Context<'_>,
	#[description = "Stock ticker symbol"] symbol: String,
	#[description = "Quantity or 'all'"]
	#[rest]
	amount_str: String,
) -> Result<(), BotError> {
	if stocks_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let api_key = require_finnhub_key(ctx)?;
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;
	let user_id = ctx.author().id.to_string();
	let guild_id_str = guild_id.to_string();
	let symbol = symbol.to_uppercase();

	// Ensure portfolio exists
	queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;

	// Get current holding
	let holding = queries::get_holding(&ctx.data().db, &guild_id_str, &user_id, &symbol).await?;
	let holding = match holding {
		Some(h) => h,
		None => {
			ctx.say(format!("You don't own any **{symbol}** shares."))
				.await?;
			return Ok(());
		}
	};

	// Parse quantity
	let amount_str = amount_str.trim().to_lowercase();
	let quantity = if amount_str == "all" {
		holding.quantity
	} else {
		let qty: Decimal = amount_str
			.parse()
			.map_err(|_| BotError::Other("Invalid quantity. Use a number or `all`.".into()))?;
		if qty <= Decimal::ZERO {
			return Err(BotError::Other("Quantity must be positive.".into()));
		}
		let qty = qty.round_dp(4);
		if qty > holding.quantity {
			ctx.say(format!(
				"You only have **{}** shares of **{symbol}**.",
				holding.quantity.round_dp(4)
			))
			.await?;
			return Ok(());
		}
		qty
	};

	// Fetch current price
	let quote = api::get_quote(&ctx.data().http_client, &ctx.data().db, &api_key, &symbol)
		.await
		.map_err(BotError::Other)?;

	let (total, realized_pnl) = match queries::sell_stock(
		&ctx.data().db,
		&guild_id_str,
		&user_id,
		&symbol,
		quantity,
		quote.price,
	)
	.await
	{
		Err(sqlx::Error::Protocol(msg)) if msg.contains("Insufficient shares") => {
			ctx.say(format!(
				"You don't have enough **{symbol}** shares to sell."
			))
			.await?;
			return Ok(());
		}
		Err(e) => return Err(e.into()),
		Ok(v) => v,
	};

	// Re-fetch portfolio for accurate balance
	let portfolio =
		queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
	let embed = embeds::sell_embed(
		&symbol,
		quantity,
		quote.price,
		total,
		realized_pnl,
		portfolio.cash_balance,
	);
	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}

/// View a stock portfolio
#[poise::command(prefix_command, rename = "portfolio", aliases("port", "pf", "p"))]
pub async fn portfolio(
	ctx: Context<'_>,
	#[description = "User to view (optional)"] user: Option<serenity::all::User>,
) -> Result<(), BotError> {
	portfolio_inner(ctx, user).await
}

async fn portfolio_inner(
	ctx: Context<'_>,
	user: Option<serenity::all::User>,
) -> Result<(), BotError> {
	if stocks_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let api_key = require_finnhub_key(ctx)?;
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;
	let target = user.as_ref().unwrap_or(ctx.author());
	let user_id = target.id.to_string();
	let guild_id_str = guild_id.to_string();

	let portfolio =
		queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
	let holdings = queries::get_holdings(&ctx.data().db, &guild_id_str, &user_id).await?;

	// Fetch prices for all holdings
	let mut holdings_with_quotes = Vec::new();
	for holding in &holdings {
		let price = match api::get_quote(
			&ctx.data().http_client,
			&ctx.data().db,
			&api_key,
			&holding.symbol,
		)
		.await
		{
			Ok(q) => q.price,
			Err(_) => holding.avg_cost, // fallback to avg cost if API fails
		};
		holdings_with_quotes.push(HoldingWithQuote {
			holding,
			current_price: price,
		});
	}

	let user_name = target.name.as_str();
	let embed = embeds::portfolio_embed(user_name, portfolio.cash_balance, &holdings_with_quotes);
	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}

/// Check the current price of a stock
#[poise::command(prefix_command, rename = "price", aliases("quote", "q"))]
pub async fn price(
	ctx: Context<'_>,
	#[description = "Stock ticker symbol"]
	#[rest]
	symbol: String,
) -> Result<(), BotError> {
	if stocks_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let api_key = require_finnhub_key(ctx)?;

	let quote = api::get_quote(
		&ctx.data().http_client,
		&ctx.data().db,
		&api_key,
		symbol.trim(),
	)
	.await
	.map_err(BotError::Other)?;

	let embed = embeds::price_embed(&quote);
	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}

/// Top portfolios in the server
#[poise::command(prefix_command, rename = "leaderboard", aliases("lb", "top"))]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), BotError> {
	if stocks_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let api_key = require_finnhub_key(ctx)?;
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;
	let guild_id_str = guild_id.to_string();

	let portfolios = queries::get_all_portfolios(&ctx.data().db, &guild_id_str).await?;

	let mut entries: Vec<(String, Decimal)> = Vec::new();
	for p in &portfolios {
		let holdings = queries::get_holdings(&ctx.data().db, &guild_id_str, &p.user_id).await?;
		let mut total_value = p.cash_balance;
		for h in &holdings {
			let price =
				match api::get_quote(&ctx.data().http_client, &ctx.data().db, &api_key, &h.symbol)
					.await
				{
					Ok(q) => q.price,
					Err(_) => h.avg_cost,
				};
			total_value += h.quantity * price;
		}
		entries.push((p.user_id.clone(), total_value));
	}

	// Sort by total value descending. Decimal implements `Ord`, so no
	// `partial_cmp` NaN-dance like the old f64 version needed.
	entries.sort_by_key(|b| std::cmp::Reverse(b.1));

	let leaderboard_entries: Vec<LeaderboardEntry> = entries
		.iter()
		.take(10)
		.enumerate()
		.map(|(i, (user_id, total_value))| LeaderboardEntry {
			rank: i + 1,
			user_id: user_id.clone(),
			total_value: *total_value,
			pnl: *total_value - STARTING_CASH,
		})
		.collect();

	let embed = embeds::leaderboard_embed(&leaderboard_entries);
	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}

/// Recent trade history
#[poise::command(prefix_command, rename = "history", aliases("hist", "h"))]
pub async fn history(ctx: Context<'_>) -> Result<(), BotError> {
	if stocks_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let _ = require_finnhub_key(ctx)?;
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;
	let user_id = ctx.author().id.to_string();
	let guild_id_str = guild_id.to_string();

	let transactions =
		queries::get_transactions(&ctx.data().db, &guild_id_str, &user_id, 10).await?;
	let embed = embeds::history_embed(&transactions, &ctx.author().name);
	ctx.send(poise::CreateReply::default().embed(embed)).await?;
	Ok(())
}

/// Reset your portfolio to the starting cash balance.
///
/// Wipes all holdings and trade history. The destructive action is gated behind
/// a Discord button so a stray copy-paste of the previous "type 'confirm'" form
/// can't nuke a portfolio.
#[poise::command(prefix_command, rename = "reset")]
pub async fn reset(ctx: Context<'_>) -> Result<(), BotError> {
	if stocks_rate_limit_or_reply(ctx).await? {
		return Ok(());
	}
	let _ = require_finnhub_key(ctx)?;
	let guild_id = ctx
		.guild_id()
		.ok_or(BotError::Other("Not in a guild".into()))?;
	let user_id = ctx.author().id.to_string();
	let guild_id_str = guild_id.to_string();

	// Snapshot what's about to be deleted, so the embed can show the user
	// exactly what they're agreeing to lose.
	let portfolio =
		queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
	let holdings = queries::get_holdings(&ctx.data().db, &guild_id_str, &user_id).await?;
	let transactions =
		queries::get_transactions(&ctx.data().db, &guild_id_str, &user_id, i64::MAX).await?;

	let holdings_count = holdings.len();
	let transactions_count = transactions.len();

	let confirm_id = format!(
		"stock_reset_confirm_{}",
		chrono::Utc::now().timestamp_millis()
	);
	let cancel_id = format!(
		"stock_reset_cancel_{}",
		chrono::Utc::now().timestamp_millis()
	);

	let description = format!(
		"This will permanently:\n\
         • Delete **{holdings_count}** holding{h_plural}\n\
         • Delete **{transactions_count}** trade{t_plural} of history\n\
         • Reset your cash to **${starting}** (currently ${current})\n\n\
         This cannot be undone.",
		h_plural = if holdings_count == 1 { "" } else { "s" },
		t_plural = if transactions_count == 1 { "" } else { "s" },
		starting = STARTING_CASH.round_dp(2),
		current = portfolio.cash_balance.round_dp(2),
	);

	let embed = CreateEmbed::new()
		.color(0xfee75c)
		.title("Reset Portfolio?")
		.description(&description)
		.footer(CreateEmbedFooter::new(format!(
			"Requested by {} · Expires in {}s",
			ctx.author().name,
			RESET_CONFIRM_TIMEOUT.as_secs()
		)));

	let buttons = vec![CreateActionRow::Buttons(vec![
		CreateButton::new(&confirm_id)
			.label("Confirm Reset")
			.style(ButtonStyle::Danger),
		CreateButton::new(&cancel_id)
			.label("Cancel")
			.style(ButtonStyle::Secondary),
	])];

	let reply = ctx
		.send(
			poise::CreateReply::default()
				.embed(embed)
				.components(buttons),
		)
		.await?;
	let mut confirm_msg = reply.into_message().await?;

	// Hold the shard handle across awaits so we can keep collecting interactions.
	let serenity_ctx = ctx.serenity_context();

	let interaction = confirm_msg
		.await_component_interaction(serenity_ctx.shard.clone())
		.timeout(RESET_CONFIRM_TIMEOUT)
		.author_id(ctx.author().id)
		.custom_ids(vec![confirm_id.clone(), cancel_id.clone()])
		.await;

	let Some(interaction) = interaction else {
		// Timed out — disable the buttons and tell the user.
		let timeout_embed = CreateEmbed::new()
			.color(0x95a5a6)
			.title("Confirmation timed out")
			.description(&description)
			.footer(CreateEmbedFooter::new(
				"No response — portfolio was not reset",
			));

		let _ = confirm_msg
			.edit(
				&serenity_ctx.http,
				EditMessage::new().embed(timeout_embed).components(vec![]),
			)
			.await;
		return Ok(());
	};

	let approved = interaction.data.custom_id == confirm_id;

	if approved {
		queries::reset_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;

		let done_embed = CreateEmbed::new()
			.color(0x57f287)
			.title("Portfolio reset")
			.description(format!(
				"Your portfolio has been reset to **${}**. \
                 All holdings and trade history were cleared.",
				STARTING_CASH.round_dp(2)
			))
			.footer(CreateEmbedFooter::new(format!(
				"Reset by {}",
				ctx.author().name
			)));

		interaction
			.create_response(
				&serenity_ctx.http,
				CreateInteractionResponse::UpdateMessage(
					CreateInteractionResponseMessage::new()
						.embed(done_embed)
						.components(vec![]),
				),
			)
			.await?;
	} else {
		let cancel_embed = CreateEmbed::new()
			.color(0xed4245)
			.title("Cancelled")
			.description("Your portfolio was not reset.")
			.footer(CreateEmbedFooter::new(format!(
				"Cancelled by {}",
				ctx.author().name
			)));

		interaction
			.create_response(
				&serenity_ctx.http,
				CreateInteractionResponse::UpdateMessage(
					CreateInteractionResponseMessage::new()
						.embed(cancel_embed)
						.components(vec![]),
				),
			)
			.await?;
	}

	Ok(())
}
