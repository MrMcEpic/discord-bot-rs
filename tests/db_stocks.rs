//! Postgres-backed integration tests for the stock-trading SQL layer.
//!
//! Each test gets a fresh database via `#[sqlx::test]`, with the bot's
//! `migrations/` directory applied automatically. Tests exercise the
//! real query functions in `discord_bot::db::queries`, not the
//! upstream Finnhub-driven command layer.
//!
//! The race-sensitive tests in this file pin down the Tier 1.2 fix:
//! `buy_stock`, `sell_stock`, and `reset_portfolio` all take a
//! `FOR UPDATE` lock on the portfolio row inside a transaction so that
//! concurrent commands cannot mint money by interleaving a sell with a
//! reset. If that lock ever regresses, `stocks_reset_sell_race_does_not_mint_money`
//! is the test that should turn red.

use rust_decimal::Decimal;
use sqlx::PgPool;
use std::str::FromStr;

use discord_bot::db::queries;
use discord_bot::stocks::STARTING_CASH;

const G: &str = "test-guild";
const U: &str = "test-user";

fn d(s: &str) -> Decimal {
	Decimal::from_str(s).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn buy_stock_decrements_cash_and_creates_holding(pool: PgPool) {
	let _ = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();

	let total = queries::buy_stock(&pool, G, U, "AAPL", d("2"), d("100"))
		.await
		.unwrap();
	assert_eq!(total, d("200"));

	let portfolio = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();
	assert_eq!(portfolio.cash_balance, STARTING_CASH - d("200"));

	let holding = queries::get_holding(&pool, G, U, "AAPL")
		.await
		.unwrap()
		.expect("holding should exist after buy");
	assert_eq!(holding.quantity, d("2"));
	assert_eq!(holding.avg_cost, d("100"));
}

#[sqlx::test(migrations = "./migrations")]
async fn buy_stock_rejects_insufficient_funds(pool: PgPool) {
	let _ = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();

	// Starting cash is $1000; try to spend $2000.
	let err = queries::buy_stock(&pool, G, U, "AAPL", d("2"), d("1000"))
		.await
		.expect_err("should be rejected");
	let msg = format!("{err}");
	assert!(
		msg.contains("Insufficient funds"),
		"unexpected error: {msg}"
	);

	// Cash should be untouched.
	let portfolio = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();
	assert_eq!(portfolio.cash_balance, STARTING_CASH);
	assert!(queries::get_holding(&pool, G, U, "AAPL")
		.await
		.unwrap()
		.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn sell_stock_partial_keeps_holding_and_full_removes_it(pool: PgPool) {
	let _ = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();
	queries::buy_stock(&pool, G, U, "AAPL", d("3"), d("50"))
		.await
		.unwrap();

	// Partial sell.
	let (proceeds, pnl) = queries::sell_stock(&pool, G, U, "AAPL", d("1"), d("75"))
		.await
		.unwrap();
	assert_eq!(proceeds, d("75"));
	assert_eq!(pnl, d("25")); // sold 1 @ 75, avg cost 50 -> +$25
	let holding = queries::get_holding(&pool, G, U, "AAPL").await.unwrap();
	assert_eq!(holding.unwrap().quantity, d("2"));

	// Full sell of remainder.
	let (proceeds2, _) = queries::sell_stock(&pool, G, U, "AAPL", d("2"), d("60"))
		.await
		.unwrap();
	assert_eq!(proceeds2, d("120"));
	assert!(queries::get_holding(&pool, G, U, "AAPL")
		.await
		.unwrap()
		.is_none());

	// Cash: 1000 - 150 (buy) + 75 (partial) + 120 (full) = 1045
	let portfolio = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();
	assert_eq!(portfolio.cash_balance, d("1045"));
}

#[sqlx::test(migrations = "./migrations")]
async fn sell_stock_rejects_insufficient_shares(pool: PgPool) {
	let _ = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();
	queries::buy_stock(&pool, G, U, "AAPL", d("1"), d("100"))
		.await
		.unwrap();

	let err = queries::sell_stock(&pool, G, U, "AAPL", d("5"), d("100"))
		.await
		.expect_err("should be rejected");
	assert!(format!("{err}").contains("Insufficient shares"));
}

#[sqlx::test(migrations = "./migrations")]
async fn reset_portfolio_wipes_holdings_and_restores_cash(pool: PgPool) {
	let _ = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();
	queries::buy_stock(&pool, G, U, "AAPL", d("2"), d("100"))
		.await
		.unwrap();
	queries::buy_stock(&pool, G, U, "MSFT", d("1"), d("250"))
		.await
		.unwrap();

	queries::reset_portfolio(&pool, G, U).await.unwrap();

	let portfolio = queries::get_or_create_portfolio(&pool, G, U).await.unwrap();
	assert_eq!(portfolio.cash_balance, STARTING_CASH);
	let holdings = queries::get_holdings(&pool, G, U).await.unwrap();
	assert!(holdings.is_empty(), "reset should wipe holdings");
	let txns = queries::get_transactions(&pool, G, U, 100).await.unwrap();
	assert!(txns.is_empty(), "reset should wipe transactions");
}

/// Tier 1.2 must-stay-fixed: concurrent reset + sell must not mint money.
///
/// Before the row-locking fix, `sell_stock` could observe a stale cash
/// balance, then `reset_portfolio` would write `STARTING_CASH`, then the
/// sell would `cash_balance = cash_balance + proceeds`, leaving the user
/// with starting cash plus the proceeds — free money.
///
/// The current implementation takes `FOR UPDATE` on the portfolio row
/// inside both transactions, so one waits for the other to commit. After
/// the dust settles the cash balance must be one of two values: the
/// reset's `STARTING_CASH` (if reset committed last and clobbered the
/// sell), or the sell's proceeds added to the post-buy balance (if sell
/// committed last). It must never be `STARTING_CASH + proceeds`.
#[sqlx::test(migrations = "./migrations")]
async fn stocks_reset_sell_race_does_not_mint_money(pool: PgPool) {
	const ITERATIONS: usize = 10;

	for i in 0..ITERATIONS {
		let user = format!("race-user-{i}");
		// Set up: $1000 cash, then buy 1 share @ $200 → $800 cash + 1 share.
		let _ = queries::get_or_create_portfolio(&pool, G, &user)
			.await
			.unwrap();
		queries::buy_stock(&pool, G, &user, "AAPL", d("1"), d("200"))
			.await
			.unwrap();

		let pool_a = pool.clone();
		let pool_b = pool.clone();
		let user_a = user.clone();
		let user_b = user.clone();

		let sell = tokio::spawn(async move {
			// Sell 1 share @ $200 → +$200 proceeds.
			queries::sell_stock(&pool_a, G, &user_a, "AAPL", d("1"), d("200")).await
		});
		let reset =
			tokio::spawn(async move { queries::reset_portfolio(&pool_b, G, &user_b).await });

		// We don't care which tx errors out — both are valid outcomes
		// (e.g. reset commits first, then sell sees zero shares and
		// returns "Insufficient shares"). The invariant under test is
		// the final cash balance, not which task succeeded.
		let _ = sell.await.unwrap();
		let _ = reset.await.unwrap();

		let final_cash = queries::get_or_create_portfolio(&pool, G, &user)
			.await
			.unwrap()
			.cash_balance;

		// Allowed final states:
		//   A) reset wins outright (sell errored or sell committed first
		//      and reset clobbered): cash = STARTING_CASH ($1000)
		//   B) sell committed after reset: cash = STARTING_CASH + 200
		//      → would be a regression (free money). FORBIDDEN.
		//   C) sell committed first, reset committed after: cash = STARTING_CASH
		//   D) sell committed first, no reset: cash = $800 + $200 = $1000
		//      (matches STARTING_CASH numerically).
		// So the only legal value is exactly STARTING_CASH ($1000). Anything
		// higher means we minted money.
		assert!(
			final_cash <= STARTING_CASH,
			"iter {i}: final cash {final_cash} exceeds STARTING_CASH {STARTING_CASH}: \
			 reset+sell race minted money"
		);
	}
}
