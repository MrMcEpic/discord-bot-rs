use rust_decimal::Decimal;
use serenity::builder::CreateEmbed;

use crate::db::models::{StockHolding, StockTransaction};

use super::api::StockQuote;
use super::STARTING_CASH;

/// Format a `Decimal` as a signed dollar string at 2dp, with the sign on the
/// `$` side (e.g. `-$1.23`). Mirrors the float version's behaviour verbatim.
fn format_money(amount: Decimal) -> String {
	let rounded = amount.round_dp(2);
	if rounded >= Decimal::ZERO {
		format!("${}", rounded)
	} else {
		format!("-${}", rounded.abs())
	}
}

/// Format a `Decimal` as a signed P/L string at 2dp (`+$1.23` / `-$0.50`).
fn format_pnl(amount: Decimal) -> String {
	let rounded = amount.round_dp(2);
	if rounded >= Decimal::ZERO {
		format!("+${}", rounded)
	} else {
		format!("-${}", rounded.abs())
	}
}

fn pnl_color(pnl: Decimal) -> u32 {
	if pnl >= Decimal::ZERO {
		0x57f287 // green
	} else {
		0xed4245 // red
	}
}

pub fn price_embed(quote: &StockQuote) -> CreateEmbed {
	let change_dollar = quote.price - quote.prev_close;
	let arrow = if change_dollar >= Decimal::ZERO {
		"▲"
	} else {
		"▼"
	};
	let color = pnl_color(change_dollar);

	CreateEmbed::new()
		.color(color)
		.title(format!("{} Stock Price", quote.symbol))
		.description(format!("**${}**", quote.price.round_dp(2)))
		.field(
			"Daily Change",
			format!(
				"{arrow} {} ({:+}%)",
				change_dollar.abs().round_dp(2),
				quote.change_pct.round_dp(2)
			),
			true,
		)
		.field(
			"Previous Close",
			format!("${}", quote.prev_close.round_dp(2)),
			true,
		)
}

pub fn buy_embed(
	symbol: &str,
	quantity: Decimal,
	price: Decimal,
	total: Decimal,
	new_cash: Decimal,
) -> CreateEmbed {
	CreateEmbed::new()
		.color(0x57f287)
		.title("Stock Purchased")
		.description(format!(
			"Bought **{}** shares of **{symbol}** at ${}",
			quantity.round_dp(4),
			price.round_dp(2),
		))
		.field("Total Cost", format_money(total), true)
		.field("Cash Remaining", format_money(new_cash), true)
}

pub fn sell_embed(
	symbol: &str,
	quantity: Decimal,
	price: Decimal,
	total: Decimal,
	realized_pnl: Decimal,
	new_cash: Decimal,
) -> CreateEmbed {
	CreateEmbed::new()
		.color(pnl_color(realized_pnl))
		.title("Stock Sold")
		.description(format!(
			"Sold **{}** shares of **{symbol}** at ${}",
			quantity.round_dp(4),
			price.round_dp(2),
		))
		.field("Total Sale", format_money(total), true)
		.field("Realized P/L", format_pnl(realized_pnl), true)
		.field("Cash Balance", format_money(new_cash), true)
}

pub struct HoldingWithQuote<'a> {
	pub holding: &'a StockHolding,
	pub current_price: Decimal,
}

pub fn portfolio_embed(
	user_name: &str,
	cash: Decimal,
	holdings: &[HoldingWithQuote<'_>],
) -> CreateEmbed {
	let mut total_value = cash;
	let mut total_cost = Decimal::ZERO;
	let mut lines = Vec::new();

	for h in holdings {
		let market_value = h.holding.quantity * h.current_price;
		let cost_basis = h.holding.quantity * h.holding.avg_cost;
		let pnl = market_value - cost_basis;
		let pnl_pct = if cost_basis > Decimal::ZERO {
			(pnl / cost_basis) * Decimal::ONE_HUNDRED
		} else {
			Decimal::ZERO
		};

		total_value += market_value;
		total_cost += cost_basis;

		let arrow = if pnl >= Decimal::ZERO { "📈" } else { "📉" };
		lines.push(format!(
			"{arrow} **{sym}**: {qty} shares @ ${price} = ${val} ({pnl_pct:+}%)",
			sym = h.holding.symbol,
			qty = h.holding.quantity.round_dp(4),
			price = h.current_price.round_dp(2),
			val = market_value.round_dp(2),
			pnl_pct = pnl_pct.round_dp(2),
		));
	}

	let total_pnl = total_value - STARTING_CASH;

	let mut embed = CreateEmbed::new()
		.color(pnl_color(total_pnl))
		.title(format!("Portfolio — {user_name}"))
		.description(format!("Cash: **{}**", format_money(cash)));

	if !lines.is_empty() {
		embed = embed.field("Holdings", lines.join("\n"), false);
	} else {
		embed = embed.field("Holdings", "No stocks held.", false);
	}

	let total_pct = if total_cost + cash > Decimal::ZERO {
		((total_pnl / STARTING_CASH) * Decimal::ONE_HUNDRED).round_dp(2)
	} else {
		Decimal::ZERO
	};

	embed = embed
		.field(
			"Total Value",
			format!("**{}**", format_money(total_value)),
			true,
		)
		.field(
			"Total P/L",
			format!("**{}** ({:+}%)", format_pnl(total_pnl), total_pct),
			true,
		);

	embed
}

pub struct LeaderboardEntry {
	pub rank: usize,
	pub user_id: String,
	pub total_value: Decimal,
	pub pnl: Decimal,
}

pub fn leaderboard_embed(entries: &[LeaderboardEntry]) -> CreateEmbed {
	let mut lines = Vec::new();
	for entry in entries {
		let medal = match entry.rank {
			1 => "🥇",
			2 => "🥈",
			3 => "🥉",
			_ => "▫️",
		};
		lines.push(format!(
			"{medal} **#{rank}** <@{uid}> — {val} ({pnl})",
			rank = entry.rank,
			uid = entry.user_id,
			val = format_money(entry.total_value),
			pnl = format_pnl(entry.pnl),
		));
	}

	let description = if lines.is_empty() {
		"No portfolios found. Use `!m stock buy` to get started!".to_string()
	} else {
		lines.join("\n")
	};

	CreateEmbed::new()
		.color(0xf1c40f)
		.title("Stock Leaderboard")
		.description(description)
}

pub fn history_embed(transactions: &[StockTransaction], user_name: &str) -> CreateEmbed {
	let mut lines = Vec::new();
	for t in transactions {
		let emoji = if t.action == "BUY" { "🟢" } else { "🔴" };
		let ts = t.created_at.timestamp();
		lines.push(format!(
			"{emoji} **{action}** {qty} {sym} @ ${price} (${total}) — <t:{ts}:R>",
			action = t.action,
			qty = t.quantity.round_dp(4),
			sym = t.symbol,
			price = t.price_per_share.round_dp(2),
			total = t.total_amount.round_dp(2),
		));
	}

	let description = if lines.is_empty() {
		"No trades yet.".to_string()
	} else {
		lines.join("\n")
	};

	CreateEmbed::new()
		.color(0x5865f2)
		.title(format!("Trade History — {user_name}"))
		.description(description)
}
