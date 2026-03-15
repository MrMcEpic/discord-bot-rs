use serenity::all::*;

use crate::db::queries;
use crate::error::BotError;
#[allow(unused_imports)]
use sqlx;
use crate::stocks::api;
use crate::stocks::embeds::{self, HoldingWithQuote, LeaderboardEntry};
use crate::Context;

fn require_finnhub_key(ctx: Context<'_>) -> Result<String, BotError> {
    ctx.data()
        .config
        .finnhub_api_key
        .clone()
        .ok_or_else(|| BotError::Other("Stock trading is not configured. The bot owner needs to set `FINNHUB_API_KEY`.".into()))
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
    let api_key = require_finnhub_key(ctx)?;
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let user_id = ctx.author().id.to_string();
    let guild_id_str = guild_id.to_string();
    let symbol = symbol.to_uppercase();

    // Ensure portfolio exists
    queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;

    // Fetch current price
    let quote = api::get_quote(&ctx.data().http_client, &ctx.data().db, &api_key, &symbol)
        .await
        .map_err(BotError::Other)?;

    // Parse amount: "$500" for dollar amount, or plain number for share count
    let amount_str = amount_str.trim();
    let quantity = if let Some(dollars) = amount_str.strip_prefix('$') {
        let dollar_amount: f64 = dollars
            .parse()
            .map_err(|_| BotError::Other("Invalid dollar amount.".into()))?;
        if dollar_amount <= 0.0 {
            return Err(BotError::Other("Amount must be positive.".into()));
        }
        dollar_amount / quote.price
    } else {
        let qty: f64 = amount_str
            .parse()
            .map_err(|_| BotError::Other("Invalid quantity. Use a number or `$amount`.".into()))?;
        if qty <= 0.0 {
            return Err(BotError::Other("Quantity must be positive.".into()));
        }
        qty
    };

    let total = quantity * quote.price;

    match queries::buy_stock(&ctx.data().db, &guild_id_str, &user_id, &symbol, quantity, quote.price).await {
        Err(sqlx::Error::Protocol(msg)) if msg.contains("Insufficient funds") => {
            ctx.say(format!(
                "Insufficient funds. This trade costs **${:.2}** but you don't have enough cash.",
                total
            )).await?;
            return Ok(());
        }
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }

    // Re-fetch portfolio for accurate balance
    let portfolio = queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
    let embed = embeds::buy_embed(&symbol, quantity, quote.price, total, portfolio.cash_balance);
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
    let api_key = require_finnhub_key(ctx)?;
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
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
            ctx.say(format!("You don't own any **{symbol}** shares.")).await?;
            return Ok(());
        }
    };

    // Parse quantity
    let amount_str = amount_str.trim().to_lowercase();
    let quantity = if amount_str == "all" {
        holding.quantity
    } else {
        let qty: f64 = amount_str
            .parse()
            .map_err(|_| BotError::Other("Invalid quantity. Use a number or `all`.".into()))?;
        if qty <= 0.0 {
            return Err(BotError::Other("Quantity must be positive.".into()));
        }
        if qty > holding.quantity {
            ctx.say(format!(
                "You only have **{:.4}** shares of **{symbol}**.",
                holding.quantity
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
        &ctx.data().db, &guild_id_str, &user_id, &symbol, quantity, quote.price,
    ).await {
        Err(sqlx::Error::Protocol(msg)) if msg.contains("Insufficient shares") => {
            ctx.say(format!("You don't have enough **{symbol}** shares to sell.")).await?;
            return Ok(());
        }
        Err(e) => return Err(e.into()),
        Ok(v) => v,
    };

    // Re-fetch portfolio for accurate balance
    let portfolio = queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
    let embed = embeds::sell_embed(&symbol, quantity, quote.price, total, realized_pnl, portfolio.cash_balance);
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

async fn portfolio_inner(ctx: Context<'_>, user: Option<serenity::all::User>) -> Result<(), BotError> {
    let api_key = require_finnhub_key(ctx)?;
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let target = user.as_ref().unwrap_or(ctx.author());
    let user_id = target.id.to_string();
    let guild_id_str = guild_id.to_string();

    let portfolio = queries::get_or_create_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
    let holdings = queries::get_holdings(&ctx.data().db, &guild_id_str, &user_id).await?;

    // Fetch prices for all holdings
    let mut holdings_with_quotes = Vec::new();
    for holding in &holdings {
        let price = match api::get_quote(&ctx.data().http_client, &ctx.data().db, &api_key, &holding.symbol).await {
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
    let api_key = require_finnhub_key(ctx)?;
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let guild_id_str = guild_id.to_string();

    let portfolios = queries::get_all_portfolios(&ctx.data().db, &guild_id_str).await?;

    let mut entries: Vec<(String, f64)> = Vec::new();
    for p in &portfolios {
        let holdings = queries::get_holdings(&ctx.data().db, &guild_id_str, &p.user_id).await?;
        let mut total_value = p.cash_balance;
        for h in &holdings {
            let price = match api::get_quote(&ctx.data().http_client, &ctx.data().db, &api_key, &h.symbol).await {
                Ok(q) => q.price,
                Err(_) => h.avg_cost,
            };
            total_value += h.quantity * price;
        }
        entries.push((p.user_id.clone(), total_value));
    }

    // Sort by total value descending
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let leaderboard_entries: Vec<LeaderboardEntry> = entries
        .iter()
        .take(10)
        .enumerate()
        .map(|(i, (user_id, total_value))| LeaderboardEntry {
            rank: i + 1,
            user_id: user_id.clone(),
            total_value: *total_value,
            pnl: *total_value - 1000.0,
        })
        .collect();

    let embed = embeds::leaderboard_embed(&leaderboard_entries);
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Recent trade history
#[poise::command(prefix_command, rename = "history", aliases("hist", "h"))]
pub async fn history(ctx: Context<'_>) -> Result<(), BotError> {
    let _ = require_finnhub_key(ctx)?;
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let user_id = ctx.author().id.to_string();
    let guild_id_str = guild_id.to_string();

    let transactions = queries::get_transactions(&ctx.data().db, &guild_id_str, &user_id, 10).await?;
    let embed = embeds::history_embed(&transactions, &ctx.author().name);
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Reset your portfolio to $1,000
#[poise::command(prefix_command, rename = "reset")]
pub async fn reset(
    ctx: Context<'_>,
    #[description = "Type 'confirm' to reset"]
    #[rest]
    confirmation: Option<String>,
) -> Result<(), BotError> {
    let _ = require_finnhub_key(ctx)?;
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let user_id = ctx.author().id.to_string();
    let guild_id_str = guild_id.to_string();

    match confirmation.as_deref().map(str::trim) {
        Some("confirm") => {
            queries::reset_portfolio(&ctx.data().db, &guild_id_str, &user_id).await?;
            ctx.say("Portfolio reset to **$1,000.00**. All holdings and history cleared.")
                .await?;
        }
        _ => {
            ctx.say("This will **delete all your holdings and trade history** and reset your cash to $1,000.\n\nTo confirm, run: `!m stock reset confirm`")
                .await?;
        }
    }
    Ok(())
}
