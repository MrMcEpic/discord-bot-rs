use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;

use super::models::{
	GuildSettings, MemberActivity, StockHolding, StockPortfolio, StockPriceCache, StockTransaction,
	Tempban,
};
use crate::stocks::STARTING_CASH;

pub async fn get_guild_settings(pool: &PgPool, guild_id: &str) -> Option<GuildSettings> {
	sqlx::query_as::<_, GuildSettings>("SELECT * FROM guild_settings WHERE guild_id = $1")
		.bind(guild_id)
		.fetch_optional(pool)
		.await
		.ok()
		.flatten()
}

const ALLOWED_COLUMNS: &[&str] = &["audit_log_channel_id", "dj_role_id", "dj_mode_enabled"];

pub async fn upsert_guild_setting(
	pool: &PgPool,
	guild_id: &str,
	key: &str,
	value: &str,
) -> Result<(), sqlx::Error> {
	if !ALLOWED_COLUMNS.contains(&key) {
		return Err(sqlx::Error::Protocol(format!("Invalid setting key: {key}")));
	}
	let query = format!(
		"INSERT INTO guild_settings (guild_id, {key}) VALUES ($1, $2) \
         ON CONFLICT (guild_id) DO UPDATE SET {key} = $2"
	);
	sqlx::query(&query)
		.bind(guild_id)
		.bind(value)
		.execute(pool)
		.await?;
	Ok(())
}

pub async fn upsert_guild_setting_bool(
	pool: &PgPool,
	guild_id: &str,
	key: &str,
	value: bool,
) -> Result<(), sqlx::Error> {
	if !ALLOWED_COLUMNS.contains(&key) {
		return Err(sqlx::Error::Protocol(format!("Invalid setting key: {key}")));
	}
	let query = format!(
		"INSERT INTO guild_settings (guild_id, {key}) VALUES ($1, $2) \
         ON CONFLICT (guild_id) DO UPDATE SET {key} = $2"
	);
	sqlx::query(&query)
		.bind(guild_id)
		.bind(value)
		.execute(pool)
		.await?;
	Ok(())
}

pub async fn create_tempban(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
	moderator_id: &str,
	duration_ms: i64,
	reason: Option<&str>,
) -> Result<DateTime<Utc>, sqlx::Error> {
	let expires_at = Utc::now() + chrono::Duration::milliseconds(duration_ms);
	sqlx::query(
		"INSERT INTO tempbans (guild_id, user_id, moderator_id, reason, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
	)
	.bind(guild_id)
	.bind(user_id)
	.bind(moderator_id)
	.bind(reason)
	.bind(expires_at)
	.execute(pool)
	.await?;
	Ok(expires_at)
}

pub async fn mark_unbanned(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
) -> Result<bool, sqlx::Error> {
	let result = sqlx::query(
		"UPDATE tempbans SET unbanned = TRUE \
         WHERE guild_id = $1 AND user_id = $2 AND unbanned = FALSE",
	)
	.bind(guild_id)
	.bind(user_id)
	.execute(pool)
	.await?;
	Ok(result.rows_affected() > 0)
}

pub async fn get_active_bans(pool: &PgPool, guild_id: &str) -> Result<Vec<Tempban>, sqlx::Error> {
	sqlx::query_as::<_, Tempban>(
		"SELECT * FROM tempbans \
         WHERE guild_id = $1 AND unbanned = FALSE AND expires_at > NOW() \
         ORDER BY expires_at ASC",
	)
	.bind(guild_id)
	.fetch_all(pool)
	.await
}

pub async fn get_expired_bans(pool: &PgPool) -> Result<Vec<Tempban>, sqlx::Error> {
	sqlx::query_as::<_, Tempban>(
		"SELECT * FROM tempbans WHERE unbanned = FALSE AND expires_at <= NOW()",
	)
	.fetch_all(pool)
	.await
}

pub async fn mark_unbanned_by_id(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
	sqlx::query("UPDATE tempbans SET unbanned = TRUE WHERE id = $1")
		.bind(id)
		.execute(pool)
		.await?;
	Ok(())
}

// ── Stock trading queries ──

pub async fn get_or_create_portfolio(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
) -> Result<StockPortfolio, sqlx::Error> {
	sqlx::query(
		"INSERT INTO stock_portfolios (guild_id, user_id) VALUES ($1, $2) \
         ON CONFLICT (guild_id, user_id) DO NOTHING",
	)
	.bind(guild_id)
	.bind(user_id)
	.execute(pool)
	.await?;

	sqlx::query_as::<_, StockPortfolio>(
		"SELECT * FROM stock_portfolios WHERE guild_id = $1 AND user_id = $2",
	)
	.bind(guild_id)
	.bind(user_id)
	.fetch_one(pool)
	.await
}

/// Ensure a portfolio row exists, then take a `FOR UPDATE` lock on it inside the
/// given transaction. All subsequent reads/writes to the user's stocks must go
/// through this lock to prevent concurrent commands (buy/sell/reset) from racing.
async fn lock_portfolio<'c>(
	tx: &mut sqlx::Transaction<'c, sqlx::Postgres>,
	guild_id: &str,
	user_id: &str,
) -> Result<StockPortfolio, sqlx::Error> {
	// Create the row if it doesn't exist yet. Done outside the lock attempt
	// because INSERT ... ON CONFLICT DO NOTHING will not return a row when the
	// row already exists, and we still need a `FOR UPDATE` lock on it.
	sqlx::query(
		"INSERT INTO stock_portfolios (guild_id, user_id) VALUES ($1, $2) \
         ON CONFLICT (guild_id, user_id) DO NOTHING",
	)
	.bind(guild_id)
	.bind(user_id)
	.execute(&mut **tx)
	.await?;

	sqlx::query_as::<_, StockPortfolio>(
		"SELECT * FROM stock_portfolios \
         WHERE guild_id = $1 AND user_id = $2 \
         FOR UPDATE",
	)
	.bind(guild_id)
	.bind(user_id)
	.fetch_one(&mut **tx)
	.await
}

pub async fn get_holdings(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
) -> Result<Vec<StockHolding>, sqlx::Error> {
	sqlx::query_as::<_, StockHolding>(
		"SELECT * FROM stock_holdings WHERE guild_id = $1 AND user_id = $2 AND quantity > 0 \
         ORDER BY symbol ASC",
	)
	.bind(guild_id)
	.bind(user_id)
	.fetch_all(pool)
	.await
}

pub async fn get_holding(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
	symbol: &str,
) -> Result<Option<StockHolding>, sqlx::Error> {
	sqlx::query_as::<_, StockHolding>(
		"SELECT * FROM stock_holdings \
         WHERE guild_id = $1 AND user_id = $2 AND symbol = $3 AND quantity > 0",
	)
	.bind(guild_id)
	.bind(user_id)
	.bind(symbol)
	.fetch_optional(pool)
	.await
}

pub async fn buy_stock(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
	symbol: &str,
	quantity: Decimal,
	price_per_share: Decimal,
) -> Result<Decimal, sqlx::Error> {
	let total = quantity * price_per_share;
	let mut tx = pool.begin().await?;

	// Take the row lock first so a concurrent reset/sell can't race with us.
	let portfolio = lock_portfolio(&mut tx, guild_id, user_id).await?;

	if portfolio.cash_balance < total {
		return Err(sqlx::Error::Protocol("Insufficient funds".into()));
	}

	// Deduct cash. The row is already locked, so this UPDATE is race-free.
	sqlx::query(
		"UPDATE stock_portfolios SET cash_balance = cash_balance - $1 \
         WHERE guild_id = $2 AND user_id = $3",
	)
	.bind(total)
	.bind(guild_id)
	.bind(user_id)
	.execute(&mut *tx)
	.await?;

	// Upsert holding with weighted average cost
	sqlx::query(
		"INSERT INTO stock_holdings (guild_id, user_id, symbol, quantity, avg_cost) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (guild_id, user_id, symbol) DO UPDATE SET \
             avg_cost = (stock_holdings.avg_cost * stock_holdings.quantity + $5 * $4) \
                        / (stock_holdings.quantity + $4), \
             quantity = stock_holdings.quantity + $4",
	)
	.bind(guild_id)
	.bind(user_id)
	.bind(symbol)
	.bind(quantity)
	.bind(price_per_share)
	.execute(&mut *tx)
	.await?;

	// Transaction log
	sqlx::query(
		"INSERT INTO stock_transactions \
         (guild_id, user_id, symbol, action, quantity, price_per_share, total_amount) \
         VALUES ($1, $2, $3, 'BUY', $4, $5, $6)",
	)
	.bind(guild_id)
	.bind(user_id)
	.bind(symbol)
	.bind(quantity)
	.bind(price_per_share)
	.bind(total)
	.execute(&mut *tx)
	.await?;

	tx.commit().await?;
	Ok(total)
}

pub async fn sell_stock(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
	symbol: &str,
	quantity: Decimal,
	price_per_share: Decimal,
) -> Result<(Decimal, Decimal), sqlx::Error> {
	// Returns (total_sale_amount, realized_pnl)
	let mut tx = pool.begin().await?;

	// Lock the portfolio row first. This serialises against buy/reset so a
	// concurrent reset can't observe a fresh $1000 balance and then have the
	// sell proceeds added on top.
	let _portfolio = lock_portfolio(&mut tx, guild_id, user_id).await?;

	// Get current holding (also locked for update to prevent concurrent sells
	// of the same position).
	let holding = sqlx::query_as::<_, StockHolding>(
		"SELECT * FROM stock_holdings \
         WHERE guild_id = $1 AND user_id = $2 AND symbol = $3 AND quantity >= $4 \
         FOR UPDATE",
	)
	.bind(guild_id)
	.bind(user_id)
	.bind(symbol)
	.bind(quantity)
	.fetch_optional(&mut *tx)
	.await?;

	let holding = match holding {
		Some(h) => h,
		None => return Err(sqlx::Error::Protocol("Insufficient shares".into())),
	};

	let total = quantity * price_per_share;
	let realized_pnl = (price_per_share - holding.avg_cost) * quantity;

	// Add cash
	sqlx::query(
		"UPDATE stock_portfolios SET cash_balance = cash_balance + $1 \
         WHERE guild_id = $2 AND user_id = $3",
	)
	.bind(total)
	.bind(guild_id)
	.bind(user_id)
	.execute(&mut *tx)
	.await?;

	// Reduce or remove holding. With Decimal the exact `is_zero()` check
	// replaces the old float-epsilon `remaining < 0.0001` guard — there is no
	// accumulated rounding error to mask.
	let remaining = holding.quantity - quantity;
	if remaining.is_zero() {
		sqlx::query(
			"DELETE FROM stock_holdings WHERE guild_id = $1 AND user_id = $2 AND symbol = $3",
		)
		.bind(guild_id)
		.bind(user_id)
		.bind(symbol)
		.execute(&mut *tx)
		.await?;
	} else {
		sqlx::query(
			"UPDATE stock_holdings SET quantity = $1 \
             WHERE guild_id = $2 AND user_id = $3 AND symbol = $4",
		)
		.bind(remaining)
		.bind(guild_id)
		.bind(user_id)
		.bind(symbol)
		.execute(&mut *tx)
		.await?;
	}

	// Transaction log
	sqlx::query(
		"INSERT INTO stock_transactions \
         (guild_id, user_id, symbol, action, quantity, price_per_share, total_amount) \
         VALUES ($1, $2, $3, 'SELL', $4, $5, $6)",
	)
	.bind(guild_id)
	.bind(user_id)
	.bind(symbol)
	.bind(quantity)
	.bind(price_per_share)
	.bind(total)
	.execute(&mut *tx)
	.await?;

	tx.commit().await?;
	Ok((total, realized_pnl))
}

pub async fn get_transactions(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
	limit: i64,
) -> Result<Vec<StockTransaction>, sqlx::Error> {
	sqlx::query_as::<_, StockTransaction>(
		"SELECT * FROM stock_transactions \
         WHERE guild_id = $1 AND user_id = $2 \
         ORDER BY created_at DESC LIMIT $3",
	)
	.bind(guild_id)
	.bind(user_id)
	.bind(limit)
	.fetch_all(pool)
	.await
}

pub async fn get_all_portfolios(
	pool: &PgPool,
	guild_id: &str,
) -> Result<Vec<StockPortfolio>, sqlx::Error> {
	sqlx::query_as::<_, StockPortfolio>("SELECT * FROM stock_portfolios WHERE guild_id = $1")
		.bind(guild_id)
		.fetch_all(pool)
		.await
}

pub async fn reset_portfolio(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
) -> Result<(), sqlx::Error> {
	let mut tx = pool.begin().await?;

	// Take the portfolio row lock before touching anything. This blocks any
	// concurrent buy/sell from running on top of a half-reset state and
	// duplicating money.
	let _portfolio = lock_portfolio(&mut tx, guild_id, user_id).await?;

	sqlx::query("DELETE FROM stock_holdings WHERE guild_id = $1 AND user_id = $2")
		.bind(guild_id)
		.bind(user_id)
		.execute(&mut *tx)
		.await?;

	sqlx::query("DELETE FROM stock_transactions WHERE guild_id = $1 AND user_id = $2")
		.bind(guild_id)
		.bind(user_id)
		.execute(&mut *tx)
		.await?;

	sqlx::query(
		"UPDATE stock_portfolios SET cash_balance = $1 WHERE guild_id = $2 AND user_id = $3",
	)
	.bind(STARTING_CASH)
	.bind(guild_id)
	.bind(user_id)
	.execute(&mut *tx)
	.await?;

	tx.commit().await?;
	Ok(())
}

pub async fn get_cached_price(
	pool: &PgPool,
	symbol: &str,
) -> Result<Option<StockPriceCache>, sqlx::Error> {
	sqlx::query_as::<_, StockPriceCache>(
		"SELECT * FROM stock_price_cache \
         WHERE symbol = $1 AND fetched_at > NOW() - INTERVAL '60 seconds'",
	)
	.bind(symbol)
	.fetch_optional(pool)
	.await
}

pub async fn upsert_cached_price(
	pool: &PgPool,
	symbol: &str,
	price: f64,
	prev_close: f64,
	change_pct: f64,
) -> Result<(), sqlx::Error> {
	sqlx::query(
		"INSERT INTO stock_price_cache (symbol, price, prev_close, change_pct, fetched_at) \
         VALUES ($1, $2, $3, $4, NOW()) \
         ON CONFLICT (symbol) DO UPDATE SET \
             price = $2, prev_close = $3, change_pct = $4, fetched_at = NOW()",
	)
	.bind(symbol)
	.bind(price)
	.bind(prev_close)
	.bind(change_pct)
	.execute(pool)
	.await?;
	Ok(())
}

// ── Auto-role queries ──

pub async fn increment_message_count(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
) -> Result<MemberActivity, sqlx::Error> {
	sqlx::query_as::<_, MemberActivity>(
        "INSERT INTO member_activity (guild_id, user_id, message_count) \
         VALUES ($1, $2, 1) \
         ON CONFLICT (guild_id, user_id) DO UPDATE SET message_count = member_activity.message_count + 1 \
         RETURNING *",
    )
    .bind(guild_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn get_unpromoted_members(
	pool: &PgPool,
	guild_id: &str,
) -> Result<Vec<MemberActivity>, sqlx::Error> {
	sqlx::query_as::<_, MemberActivity>(
		"SELECT * FROM member_activity WHERE guild_id = $1 AND promoted = FALSE",
	)
	.bind(guild_id)
	.fetch_all(pool)
	.await
}

/// Atomically claim a promotion for a member. Returns `true` if this caller
/// won the race and should perform the Discord role changes; `false` if the
/// member was already promoted (another caller — message handler or background
/// scanner — beat us to it).
pub async fn try_claim_promotion(
	pool: &PgPool,
	guild_id: &str,
	user_id: &str,
) -> Result<bool, sqlx::Error> {
	let row: Option<(String,)> = sqlx::query_as(
		"UPDATE member_activity SET promoted = TRUE \
         WHERE guild_id = $1 AND user_id = $2 AND promoted = FALSE \
         RETURNING user_id",
	)
	.bind(guild_id)
	.bind(user_id)
	.fetch_optional(pool)
	.await?;
	Ok(row.is_some())
}
