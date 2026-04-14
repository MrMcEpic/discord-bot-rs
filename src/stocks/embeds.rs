use serenity::builder::CreateEmbed;

use crate::db::models::{StockHolding, StockTransaction};

use super::api::StockQuote;

fn format_money(amount: f64) -> String {
	if amount >= 0.0 {
		format!("${:.2}", amount)
	} else {
		format!("-${:.2}", amount.abs())
	}
}

fn format_pnl(amount: f64) -> String {
	if amount >= 0.0 {
		format!("+${:.2}", amount)
	} else {
		format!("-${:.2}", amount.abs())
	}
}

fn pnl_color(pnl: f64) -> u32 {
	if pnl >= 0.0 {
		0x57f287 // green
	} else {
		0xed4245 // red
	}
}

pub fn price_embed(quote: &StockQuote) -> CreateEmbed {
	let change_dollar = quote.price - quote.prev_close;
	let arrow = if change_dollar >= 0.0 { "▲" } else { "▼" };
	let color = pnl_color(change_dollar);

	CreateEmbed::new()
		.color(color)
		.title(format!("{} Stock Price", quote.symbol))
		.description(format!("**${:.2}**", quote.price))
		.field(
			"Daily Change",
			format!(
				"{arrow} {:.2} ({:+.2}%)",
				change_dollar.abs(),
				quote.change_pct
			),
			true,
		)
		.field("Previous Close", format!("${:.2}", quote.prev_close), true)
}

pub fn buy_embed(
	symbol: &str,
	quantity: f64,
	price: f64,
	total: f64,
	new_cash: f64,
) -> CreateEmbed {
	CreateEmbed::new()
		.color(0x57f287)
		.title("Stock Purchased")
		.description(format!(
			"Bought **{:.4}** shares of **{symbol}** at ${price:.2}",
			quantity
		))
		.field("Total Cost", format_money(total), true)
		.field("Cash Remaining", format_money(new_cash), true)
}

pub fn sell_embed(
	symbol: &str,
	quantity: f64,
	price: f64,
	total: f64,
	realized_pnl: f64,
	new_cash: f64,
) -> CreateEmbed {
	CreateEmbed::new()
		.color(pnl_color(realized_pnl))
		.title("Stock Sold")
		.description(format!(
			"Sold **{:.4}** shares of **{symbol}** at ${price:.2}",
			quantity
		))
		.field("Total Sale", format_money(total), true)
		.field("Realized P/L", format_pnl(realized_pnl), true)
		.field("Cash Balance", format_money(new_cash), true)
}

pub struct HoldingWithQuote<'a> {
	pub holding: &'a StockHolding,
	pub current_price: f64,
}

pub fn portfolio_embed(
	user_name: &str,
	cash: f64,
	holdings: &[HoldingWithQuote<'_>],
) -> CreateEmbed {
	let mut total_value = cash;
	let mut total_cost = 0.0;
	let mut lines = Vec::new();

	for h in holdings {
		let market_value = h.holding.quantity * h.current_price;
		let cost_basis = h.holding.quantity * h.holding.avg_cost;
		let pnl = market_value - cost_basis;
		let pnl_pct = if cost_basis > 0.0 {
			(pnl / cost_basis) * 100.0
		} else {
			0.0
		};

		total_value += market_value;
		total_cost += cost_basis;

		let arrow = if pnl >= 0.0 { "📈" } else { "📉" };
		lines.push(format!(
			"{arrow} **{sym}**: {qty:.4} shares @ ${price:.2} = ${val:.2} ({pnl_pct:+.2}%)",
			sym = h.holding.symbol,
			qty = h.holding.quantity,
			price = h.current_price,
			val = market_value,
		));
	}

	let total_pnl = total_value - 1000.0;

	let mut embed = CreateEmbed::new()
		.color(pnl_color(total_pnl))
		.title(format!("Portfolio — {user_name}"))
		.description(format!("Cash: **{}**", format_money(cash)));

	if !lines.is_empty() {
		embed = embed.field("Holdings", lines.join("\n"), false);
	} else {
		embed = embed.field("Holdings", "No stocks held.", false);
	}

	embed = embed
		.field(
			"Total Value",
			format!("**{}**", format_money(total_value)),
			true,
		)
		.field(
			"Total P/L",
			format!(
				"**{}** ({:+.2}%)",
				format_pnl(total_pnl),
				if total_cost + cash > 0.0 {
					(total_pnl / 1000.0) * 100.0
				} else {
					0.0
				}
			),
			true,
		);

	embed
}

pub struct LeaderboardEntry {
	pub rank: usize,
	pub user_id: String,
	pub total_value: f64,
	pub pnl: f64,
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
			"{emoji} **{action}** {qty:.4} {sym} @ ${price:.2} (${total:.2}) — <t:{ts}:R>",
			action = t.action,
			qty = t.quantity,
			sym = t.symbol,
			price = t.price_per_share,
			total = t.total_amount,
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
